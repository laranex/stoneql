//! Owning-fixture SSH transport evidence. Run only through `npm run test:ssh`, which creates
//! disposable OpenSSH/database containers, generated keys and a private agent. Never reads
//! `~/.ssh`, the user's agent or saved profiles; prints only synthetic values and codes.
use serde_json::{Value, json};
use std::{
    env,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use stoneql_core::Engine;
use stoneql_runtime::{
    ConnectionId, Connections, DirectServer, ENDPOINT_PREFIX, LifecycleError, OwnerId, QueryLimits,
    SshAuth, SshConfig, endpoint_root,
};

struct Fixture {
    engine: Engine,
    database_port: u16,
    direct_port: u16,
    ssh_port: u16,
    rotated_port: u16,
    ssh_user: String,
    ssh_password: String,
    host_key: String,
    rotated_key: String,
    plain_key: PathBuf,
    locked_key: PathBuf,
    passphrase: String,
    other_key: PathBuf,
    ecdsa_key: PathBuf,
    agent: PathBuf,
    db_password: String,
    ca: Vec<u8>,
    bad_ca: Vec<u8>,
    full: bool,
}
fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is set by the owning fixture runner"))
}
impl Fixture {
    fn load() -> Self {
        let engine = match var("STONEQL_SSH_ENGINE").as_str() {
            "mysql" => Engine::MySql,
            "mariadb" => Engine::MariaDb,
            "postgresql" => Engine::PostgreSql,
            _ => panic!("unsupported fixture engine"),
        };
        assert_eq!(var("STONEQL_SPIKE_FIXTURE_ID").len(), 36);
        Self {
            engine,
            database_port: var("STONEQL_SSH_DB_PORT").parse().unwrap(),
            direct_port: var("STONEQL_LIFECYCLE_PORT").parse().unwrap(),
            ssh_port: var("STONEQL_SSH_PORT").parse().unwrap(),
            rotated_port: var("STONEQL_SSH_ROTATED_PORT").parse().unwrap(),
            ssh_user: var("STONEQL_SSH_USER"),
            ssh_password: var("STONEQL_SSH_PASSWORD"),
            host_key: var("STONEQL_SSH_HOST_KEY"),
            rotated_key: var("STONEQL_SSH_ROTATED_HOST_KEY"),
            plain_key: var("STONEQL_SSH_KEY_PLAIN").into(),
            locked_key: var("STONEQL_SSH_KEY_LOCKED").into(),
            passphrase: var("STONEQL_SSH_KEY_PASSPHRASE"),
            other_key: var("STONEQL_SSH_KEY_OTHER").into(),
            ecdsa_key: var("STONEQL_SSH_KEY_ECDSA").into(),
            agent: var("STONEQL_SSH_AGENT").into(),
            db_password: var("STONEQL_FIXTURE_PASSWORD"),
            ca: std::fs::read(var("STONEQL_LIFECYCLE_CA")).unwrap(),
            bad_ca: std::fs::read(var("STONEQL_LIFECYCLE_BAD_CA")).unwrap(),
            full: var("STONEQL_SSH_SCOPE") == "full",
        }
    }
    fn database(&self, host: &str) -> DirectServer {
        DirectServer {
            engine: self.engine,
            host: host.into(),
            port: self.database_port,
            username: "stoneql_reader".into(),
            password: self.db_password.clone(),
            database: "stoneql_m001_fixture".into(),
            root_certificate_pem: Some(self.ca.clone()),
            tls: true,
            rsa_key: stoneql_runtime::PasswordRsaKey::None,
        }
    }
    fn direct(&self) -> DirectServer {
        DirectServer {
            host: "127.0.0.1".into(),
            port: self.direct_port,
            ..self.database("127.0.0.1")
        }
    }
    fn ssh(&self, auth: SshAuth) -> SshConfig {
        SshConfig {
            host: "127.0.0.1".into(),
            port: self.ssh_port,
            username: self.ssh_user.clone(),
            host_key: Some(self.host_key.clone()),
            auth,
        }
    }
    fn password(&self) -> SshConfig {
        self.ssh(SshAuth::Password(self.ssh_password.clone()))
    }
}
const DB_HOST: &str = "db.stoneql.test";
const WRONG_HOST: &str = "wrong.stoneql.test";

fn endpoint_directories() -> usize {
    std::fs::read_dir(endpoint_root())
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(ENDPOINT_PREFIX)
                })
                .count()
        })
        .unwrap_or(0)
}
fn open_descriptors() -> usize {
    std::fs::read_dir("/dev/fd")
        .map(Iterator::count)
        .unwrap_or(0)
}
fn limits() -> QueryLimits {
    QueryLimits {
        rows: 10,
        bytes: 65_536,
        deadline: Duration::from_secs(3),
    }
}
async fn select_one(connections: &Connections, owner: OwnerId, id: ConnectionId) -> String {
    let result = connections
        .read_only(owner, id, "SELECT 1 AS one", limits())
        .await
        .map_err(|error| format!("{error:?}"))
        .expect("query through tunnel");
    serde_json::to_value(&result.rows[0][0]).unwrap()["text"]
        .as_str()
        .unwrap()
        .to_owned()
}
/// No registered tunnel, no pending setup and no endpoint directory remains.
fn assert_clean(connections: &Connections, context: &str) {
    assert_eq!(
        connections.open_tunnels(),
        0,
        "{context}: registered tunnels"
    );
    assert_eq!(connections.pending_setups(), 0, "{context}: pending setups");
    assert_eq!(endpoint_directories(), 0, "{context}: endpoint directories");
}
fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    let index = ((samples.len() as f64 * fraction).ceil() as usize).saturating_sub(1);
    (samples[index] * 1000.0).round() / 1000.0
}
fn summary(mut samples: Vec<f64>) -> Value {
    json!({"samples": samples.len(), "p50Ms": percentile(&mut samples, 0.5), "p95Ms": percentile(&mut samples, 0.95), "rawMs": samples})
}
fn registry(lifetime: Duration) -> Connections {
    Connections::new(8, Duration::from_secs(3), lifetime).unwrap()
}
async fn expect_error(
    connections: &Connections,
    owner: OwnerId,
    database: DirectServer,
    ssh: SshConfig,
) -> LifecycleError {
    let error = connections
        .connect_with(owner, database, Some(ssh), None)
        .await
        .expect_err("connection must fail");
    assert_clean(connections, &format!("{error:?}"));
    error
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run through npm run test:ssh with disposable OpenSSH and database fixtures"]
async fn ssh_tunnel_uses_owned_fixture() {
    let fixture = Fixture::load();
    let connections = registry(Duration::from_secs(3600));
    let owner = OwnerId::new();
    let mut report =
        json!({"engine": var("STONEQL_SSH_ENGINE"), "scope": var("STONEQL_SSH_SCOPE")});
    assert_eq!(endpoint_directories(), 0);

    // Enrollment input: scanning reports the key but never trusts it.
    let scanned = connections
        .scan_ssh_host_key("127.0.0.1", fixture.ssh_port)
        .await
        .unwrap();
    assert_eq!(scanned.public_key, fixture.host_key);
    let rotated = connections
        .scan_ssh_host_key("127.0.0.1", fixture.rotated_port)
        .await
        .unwrap();
    assert_eq!(rotated.public_key, fixture.rotated_key);
    assert_ne!(scanned.fingerprint, rotated.fingerprint);
    report["scan"] =
        json!({"algorithm": scanned.algorithm, "fingerprintPrefix": &scanned.fingerprint[..7]});

    // Unknown host key: refused before any network access.
    let mut unknown = fixture.password();
    unknown.host_key = None;
    assert_eq!(
        expect_error(&connections, owner, fixture.database(DB_HOST), unknown).await,
        LifecycleError::HostKeyUnknown
    );
    // Changed host key: the enrolled key does not match the rotated server. Hard failure.
    let mut changed = fixture.password();
    changed.port = fixture.rotated_port;
    assert_eq!(
        expect_error(&connections, owner, fixture.database(DB_HOST), changed).await,
        LifecycleError::HostKeyChanged
    );

    // Enrolled key, password auth, verified TLS to the original hostname through the tunnel.
    let id = connections
        .connect_with(
            owner,
            fixture.database(DB_HOST),
            Some(fixture.password()),
            None,
        )
        .await
        .map_err(|error| format!("{error:?}"))
        .expect("enrolled SSH password tunnel");
    assert_eq!(connections.open_tunnels(), 1);
    assert_eq!(endpoint_directories(), 1);
    // Several queries: each discards its physical connection, so each crosses a new channel.
    for _ in 0..3 {
        assert_eq!(select_one(&connections, owner, id).await, "1");
    }
    let version = connections.engine_version(owner, id).unwrap();
    report["engineVersion"] = json!(version);
    // Another session cannot see, query or close this tunnel-backed handle.
    let other = OwnerId::new();
    assert!(!connections.is_connected(other, id));
    assert_eq!(
        connections.disconnect(other, id).await,
        Err(LifecycleError::Forbidden)
    );
    connections.disconnect(owner, id).await.unwrap();
    assert_clean(&connections, "after disconnect");

    // TLS identity is preserved: same tunnel, alias outside the certificate SAN is refused.
    assert_eq!(
        expect_error(
            &connections,
            owner,
            fixture.database(WRONG_HOST),
            fixture.password()
        )
        .await,
        LifecycleError::Unavailable
    );
    let mut untrusted = fixture.database(DB_HOST);
    untrusted.root_certificate_pem = Some(fixture.bad_ca.clone());
    assert_eq!(
        expect_error(&connections, owner, untrusted, fixture.password()).await,
        LifecycleError::Unavailable
    );
    // Wrong SSH password.
    assert_eq!(
        expect_error(
            &connections,
            owner,
            fixture.database(DB_HOST),
            fixture.ssh(SshAuth::Password("wrong-fixture-password".into()))
        )
        .await,
        LifecycleError::SshAuthentication
    );
    report["parity"] = json!({"passwordTunnel": true, "queriesThroughTunnel": 3, "tlsHostnameMismatchRefused": true, "untrustedCaRefused": true, "wrongPasswordRefused": true, "unknownHostKeyRefused": true, "changedHostKeyRefused": true});

    if fixture.full {
        report["full"] = full_matrix(&fixture, &connections, owner).await;
    }
    connections.shutdown().await;
    assert_clean(&connections, "after shutdown");
    println!("STONEQL_SSH_RESULT {report}");
}

async fn key_grant(connections: &Connections, owner: OwnerId, path: &Path) -> String {
    connections
        .grant_ssh_key_file(owner, &std::fs::canonicalize(path).unwrap())
        .unwrap()
}

async fn full_matrix(fixture: &Fixture, connections: &Connections, owner: OwnerId) -> Value {
    let mut evidence = json!({});
    let plain = key_grant(connections, owner, &fixture.plain_key).await;
    let locked = key_grant(connections, owner, &fixture.locked_key).await;
    let unauthorized = key_grant(connections, owner, &fixture.other_key).await;
    let ecdsa = key_grant(connections, owner, &fixture.ecdsa_key).await;

    // Key file auth: plain Ed25519, ECDSA P-256, passphrase-protected key, missing/wrong
    // passphrase and a key the server does not authorize.
    for (name, grant, passphrase) in [
        ("plainKey", &plain, None),
        ("ecdsaP256Key", &ecdsa, None),
        ("passphraseKey", &locked, Some(fixture.passphrase.clone())),
    ] {
        let id = connections
            .connect_with(
                owner,
                fixture.database(DB_HOST),
                Some(fixture.ssh(SshAuth::KeyFile {
                    grant: grant.clone(),
                    passphrase,
                })),
                None,
            )
            .await
            .map_err(|error| format!("{name}: {error:?}"))
            .unwrap();
        assert_eq!(select_one(connections, owner, id).await, "1");
        connections.disconnect(owner, id).await.unwrap();
        assert_clean(connections, name);
        evidence[name] = json!(true);
    }
    for (name, grant, passphrase) in [
        ("missingPassphrase", &locked, None),
        (
            "wrongPassphrase",
            &locked,
            Some("not-the-passphrase".to_owned()),
        ),
        ("unauthorizedKey", &unauthorized, None),
    ] {
        let error = expect_error(
            connections,
            owner,
            fixture.database(DB_HOST),
            fixture.ssh(SshAuth::KeyFile {
                grant: grant.clone(),
                passphrase,
            }),
        )
        .await;
        assert_eq!(error, LifecycleError::SshAuthentication, "{name}");
        evidence[name] = json!("SshAuthentication");
    }
    // A key grant is owner-scoped: another session cannot name it.
    let stranger = OwnerId::new();
    assert_eq!(
        connections
            .connect_with(
                stranger,
                fixture.database(DB_HOST),
                Some(fixture.ssh(SshAuth::KeyFile {
                    grant: plain.clone(),
                    passphrase: None
                })),
                None
            )
            .await,
        Err(LifecycleError::Forbidden)
    );
    assert!(connections.ssh_key_file_name(stranger, &plain).is_err());
    evidence["foreignKeyGrantRefused"] = json!(true);

    // Agent: private fixture agent works; absent or dead agent is a distinct failure.
    let id = connections
        .connect_with(
            owner,
            fixture.database(DB_HOST),
            Some(fixture.ssh(SshAuth::Agent(Some(fixture.agent.clone())))),
            None,
        )
        .await
        .map_err(|error| format!("agent: {error:?}"))
        .unwrap();
    assert_eq!(select_one(connections, owner, id).await, "1");
    connections.disconnect(owner, id).await.unwrap();
    assert_clean(connections, "agent");
    for socket in [None, Some(PathBuf::from("/nonexistent/stoneql-agent.sock"))] {
        assert_eq!(
            expect_error(
                connections,
                owner,
                fixture.database(DB_HOST),
                fixture.ssh(SshAuth::Agent(socket))
            )
            .await,
            LifecycleError::AgentUnavailable
        );
    }
    evidence["agent"] =
        json!({"authenticated": true, "absentRefused": true, "deadSocketRefused": true});

    // Credential isolation: concurrent handles with different credentials never share a tunnel.
    let peer = OwnerId::new();
    let first = connections
        .connect_with(
            owner,
            fixture.database(DB_HOST),
            Some(fixture.password()),
            None,
        )
        .await
        .unwrap();
    let peer_grant = key_grant(connections, peer, &fixture.plain_key).await;
    let second = connections
        .connect_with(
            peer,
            fixture.database(DB_HOST),
            Some(fixture.ssh(SshAuth::KeyFile {
                grant: peer_grant,
                passphrase: None,
            })),
            None,
        )
        .await
        .unwrap();
    assert_eq!(connections.open_tunnels(), 2);
    assert_eq!(endpoint_directories(), 2);
    assert!(!connections.is_connected(peer, first) && !connections.is_connected(owner, second));
    connections.disconnect(peer, second).await.unwrap();
    assert!(connections.is_connected(owner, first));
    assert_eq!(select_one(connections, owner, first).await, "1");
    connections.disconnect(owner, first).await.unwrap();
    assert_clean(connections, "isolation");
    evidence["isolation"] =
        json!({"separateTunnels": 2, "crossOwnerRefused": true, "peerDisconnectIndependent": true});

    // Explicit cancellation of a setup stalled before the SSH banner.
    let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stalled_port = stalled.local_addr().unwrap().port();
    let hold = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = stalled.accept().await {
            held.push(socket);
        }
    });
    let mut stalled_ssh = fixture.password();
    stalled_ssh.port = stalled_port;
    let started = Instant::now();
    let pending = {
        let connections = connections.clone();
        let database = fixture.database(DB_HOST);
        tokio::spawn(async move {
            connections
                .connect_with(owner, database, Some(stalled_ssh), Some("attempt_1"))
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(connections.pending_setups(), 1);
    assert!(!connections.cancel_connect(OwnerId::new(), "attempt_1"));
    assert!(connections.cancel_connect(owner, "attempt_1"));
    assert_eq!(pending.await.unwrap(), Err(LifecycleError::Cancelled));
    let cancel_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_clean(connections, "explicit cancel");
    hold.abort();
    // Dropped setup futures at many points (SSH kex, auth, endpoint, TLS, DB handshake).
    let mut completed_before_drop = 0;
    for step in 0..20u64 {
        let attempt = tokio::time::timeout(
            // 0.5–19.5 ms: drops land in TCP connect, kex, auth, endpoint, TLS and DB handshake.
            Duration::from_micros(500 + step * 1000),
            connections.connect_with(
                owner,
                fixture.database(DB_HOST),
                Some(fixture.password()),
                None,
            ),
        )
        .await;
        if let Ok(Ok(id)) = attempt {
            completed_before_drop += 1;
            connections.disconnect(owner, id).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_clean(connections, &format!("dropped setup {step}"));
    }
    evidence["cancellation"] = json!({"explicitCancelMs": (cancel_ms * 1000.0).round() / 1000.0, "foreignCancelRefused": true, "droppedSetups": 20, "completedBeforeDrop": completed_before_drop});

    // Reset (renderer reload) closes registered tunnels; expiry closes them without polling.
    let id = connections
        .connect_with(
            owner,
            fixture.database(DB_HOST),
            Some(fixture.password()),
            None,
        )
        .await
        .unwrap();
    connections.reset().await;
    assert!(!connections.is_connected(owner, id));
    assert_clean(connections, "reset");
    let short = registry(Duration::from_secs(1));
    let id = short
        .connect_with(
            owner,
            fixture.database(DB_HOST),
            Some(fixture.password()),
            None,
        )
        .await
        .unwrap();
    assert_eq!(short.open_tunnels(), 1);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!short.is_connected(owner, id));
    assert_clean(&short, "expiry");
    short.shutdown().await;
    evidence["resetAndExpiry"] = json!(true);

    // 100 setup/connect/disconnect/failure cycles: zero orphan tunnels, directories or handles.
    let descriptors_before = open_descriptors();
    completed_before_drop = 0;
    let mut outcomes = json!({"connected": 0, "wrongPassword": 0, "changedHostKey": 0, "tlsMismatch": 0, "cancelled": 0});
    for cycle in 0..100 {
        let kind = [
            "connected",
            "wrongPassword",
            "changedHostKey",
            "tlsMismatch",
            "cancelled",
        ][cycle % 5];
        match kind {
            "connected" => {
                let id = connections
                    .connect_with(
                        owner,
                        fixture.database(DB_HOST),
                        Some(fixture.password()),
                        None,
                    )
                    .await
                    .unwrap();
                assert_eq!(select_one(connections, owner, id).await, "1");
                connections.disconnect(owner, id).await.unwrap();
            }
            "wrongPassword" => {
                let error = connections
                    .connect_with(
                        owner,
                        fixture.database(DB_HOST),
                        Some(fixture.ssh(SshAuth::Password("wrong-fixture-password".into()))),
                        None,
                    )
                    .await;
                assert_eq!(error, Err(LifecycleError::SshAuthentication));
            }
            "changedHostKey" => {
                let mut changed = fixture.password();
                changed.port = fixture.rotated_port;
                let error = connections
                    .connect_with(owner, fixture.database(DB_HOST), Some(changed), None)
                    .await;
                assert_eq!(error, Err(LifecycleError::HostKeyChanged));
            }
            "tlsMismatch" => {
                let error = connections
                    .connect_with(
                        owner,
                        fixture.database(WRONG_HOST),
                        Some(fixture.password()),
                        None,
                    )
                    .await;
                assert_eq!(error, Err(LifecycleError::Unavailable));
            }
            _ => {
                let attempt = tokio::time::timeout(
                    Duration::from_millis(cycle as u64 / 5 + 1),
                    connections.connect_with(
                        owner,
                        fixture.database(DB_HOST),
                        Some(fixture.password()),
                        None,
                    ),
                )
                .await;
                if let Ok(Ok(id)) = attempt {
                    connections.disconnect(owner, id).await.unwrap();
                    completed_before_drop += 1;
                }
            }
        }
        outcomes[kind] = json!(outcomes[kind].as_u64().unwrap() + 1);
        assert_eq!(connections.open_tunnels(), 0, "cycle {cycle}");
        assert_eq!(connections.pending_setups(), 0, "cycle {cycle}");
    }
    // Aborted forwarding tasks and SSH session tasks finish asynchronously; allow them to drain.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_clean(connections, "100 cycles");
    let descriptors_after = open_descriptors();
    assert!(
        descriptors_after <= descriptors_before,
        "descriptor growth {descriptors_before} -> {descriptors_after}"
    );
    evidence["cycles"] = json!({"total": 100, "outcomes": outcomes, "cancelCyclesCompletedFirst": completed_before_drop, "descriptorsBefore": descriptors_before, "descriptorsAfter": descriptors_after, "registeredTunnelsAfter": 0, "endpointDirectoriesAfter": endpoint_directories()});

    // Characterization only: SSH overhead against the Direct loopback baseline.
    evidence["overhead"] = overhead(fixture, connections, owner).await;
    evidence
}

async fn overhead(fixture: &Fixture, connections: &Connections, owner: OwnerId) -> Value {
    let mut result = json!({"note": "Characterization only; Docker Desktop loopback, debug build. Not an approved transport budget."});
    for (name, ssh) in [("direct", None), ("ssh", Some(()))] {
        let mut connects = Vec::new();
        let mut queries = Vec::new();
        for _ in 0..20 {
            let started = Instant::now();
            let id = match ssh {
                None => connections.connect(owner, fixture.direct()).await.unwrap(),
                Some(()) => connections
                    .connect_with(
                        owner,
                        fixture.database(DB_HOST),
                        Some(fixture.password()),
                        None,
                    )
                    .await
                    .unwrap(),
            };
            connects.push(started.elapsed().as_secs_f64() * 1000.0);
            for _ in 0..5 {
                let started = Instant::now();
                select_one(connections, owner, id).await;
                queries.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            connections.disconnect(owner, id).await.unwrap();
        }
        result[name] = json!({"connect": summary(connects), "selectOne": summary(queries)});
    }
    assert_clean(connections, "overhead");
    result
}

/// Runs a docker CLI action against the dedicated `ssh-dead` container (owned by the runner).
fn dead_container(action: &str) {
    let container = var("STONEQL_SSH_DEAD_CONTAINER");
    let status = std::process::Command::new("docker")
        .args([action, container.as_str()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("docker CLI");
    assert!(status.success(), "docker {action} failed");
}
/// Waits until the handle and its tunnel are gone; returns the elapsed time.
async fn released(
    registry: &Connections,
    owner: OwnerId,
    id: ConnectionId,
    bound: Duration,
) -> Duration {
    let started = Instant::now();
    while registry.is_connected(owner, id) || registry.open_tunnels() > 0 {
        assert!(
            started.elapsed() < bound,
            "dead tunnel not released within {bound:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    started.elapsed()
}
/// M005 F-03 (T3): a tunnel whose SSH server dies releases its handle, slot, endpoint
/// directory and descriptors automatically: within 5 s when the server is killed and within
/// keepalive (3 × 30 s) + 10 s when the path silently stops (`docker pause`, MySQL only).
#[tokio::test]
#[ignore = "owning fixture: npm run test:ssh"]
async fn dead_tunnels_release_their_handles() {
    let fixture = Fixture::load();
    let dead_port: u16 = var("STONEQL_SSH_DEAD_PORT").parse().unwrap();
    let registry = Connections::new(8, Duration::from_secs(3), Duration::from_secs(3600)).unwrap();
    let owner = OwnerId::new();
    // Warm up so lazily created runtime descriptors are part of the baseline.
    let warm = registry.connect(owner, fixture.direct()).await.unwrap();
    registry.disconnect(owner, warm).await.unwrap();
    let baseline = open_descriptors();
    let ssh = || SshConfig {
        port: dead_port,
        ..fixture.ssh(SshAuth::Agent(Some(fixture.agent.clone())))
    };
    let mut report = json!({"engine": var("STONEQL_SSH_ENGINE")});
    if fixture.full {
        let id = registry
            .connect_with(owner, fixture.database(DB_HOST), Some(ssh()), None)
            .await
            .unwrap();
        assert_eq!(registry.open_tunnels(), 1);
        dead_container("pause");
        let elapsed = released(&registry, owner, id, Duration::from_secs(130)).await;
        dead_container("unpause");
        report["pausedReleaseMs"] = json!(elapsed.as_millis());
    }
    let id = registry
        .connect_with(owner, fixture.database(DB_HOST), Some(ssh()), None)
        .await
        .unwrap();
    assert_eq!(registry.open_tunnels(), 1);
    dead_container("kill");
    let elapsed = released(&registry, owner, id, Duration::from_secs(5)).await;
    report["killedReleaseMs"] = json!(elapsed.as_millis());
    assert_eq!(endpoint_directories(), 0, "endpoint directory left");
    // Every slot is free again: 8 direct handles fit.
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(registry.connect(owner, fixture.direct()).await.unwrap());
    }
    for handle in handles {
        registry.disconnect(owner, handle).await.unwrap();
    }
    let descriptors = open_descriptors();
    report["descriptorsBaseline"] = json!(baseline);
    report["descriptorsAfter"] = json!(descriptors);
    assert!(
        descriptors <= baseline,
        "descriptors leaked: {baseline} -> {descriptors}"
    );
    registry.shutdown().await;
    println!("STONEQL_SSH_DEAD_RESULT {report}");
}
