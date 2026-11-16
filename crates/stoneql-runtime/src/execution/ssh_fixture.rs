//! Owned-fixture evidence for job cancellation through an SSH tunnel (Contract 5.2). Run only
//! through `npm run test:ssh`: disposable OpenSSH/database containers and generated secrets;
//! prints only timings and codes.
//!
//! Read-write over SSH stays refused by both production gates (D025-9). This in-crate test
//! (compiled only under `cfg(test)`) connects read-only, then marks its own registry entry
//! read-write so the job/lease/cancel path can run through a real tunnel.
use super::*;
use serde_json::json;
use std::path::Path;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is set by the owning fixture runner"))
}

/// A raw stream to the tunnel endpoint, kept only if the tunnel gave it a forward: a stream
/// without a slot is dropped by the tunnel (immediate EOF). MySQL/MariaDB send their greeting
/// first; PostgreSQL answers an `SSLRequest` with one byte.
async fn probe(socket: &Path, postgres: bool) -> Option<UnixStream> {
    let mut stream = UnixStream::connect(socket).await.ok()?;
    if postgres {
        stream
            .write_all(&[0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f])
            .await
            .ok()?;
    }
    let mut byte = [0_u8; 1];
    match tokio::time::timeout(Duration::from_secs(3), stream.read(&mut byte)).await {
        Ok(Ok(1)) => Some(stream),
        _ => None,
    }
}
/// Occupies every free forward; the first refused stream proves the tunnel is exhausted.
async fn exhaust(socket: &Path, postgres: bool) -> Vec<UnixStream> {
    let mut held = Vec::new();
    while let Some(stream) = probe(socket, postgres).await {
        held.push(stream);
        assert!(held.len() <= 8, "the tunnel never refused a stream");
    }
    held
}
async fn free_forwards(socket: &Path, postgres: bool) -> usize {
    let held = exhaust(socket, postgres).await;
    let count = held.len();
    drop(held);
    // Lets the tunnel observe the closed probes before the next measurement.
    tokio::time::sleep(Duration::from_millis(300)).await;
    count
}

struct Run {
    connections: Connections,
    owner: OwnerId,
    handle: ConnectionId,
    sleep: &'static str,
}
impl Run {
    async fn submit(&self, lease: Option<LeaseId>) -> JobId {
        let mut options = JobOptions::new(self.sleep);
        options.lease = lease;
        let (id, _) = self
            .connections
            .submit_job(self.owner, self.handle, options)
            .unwrap();
        // The statement is sent and running server-side before the cancel.
        tokio::time::sleep(Duration::from_millis(500)).await;
        id
    }
    /// Milliseconds from the cancel request to the terminal state, and the final update.
    async fn cancel(&self, id: JobId) -> (f64, JobUpdate) {
        let requested = Instant::now();
        assert!(self.connections.cancel_job(self.owner, id).unwrap());
        let mut terminal = None;
        loop {
            let update = tokio::time::timeout(
                Duration::from_secs(10),
                self.connections
                    .fetch_job(self.owner, id, Duration::from_millis(20)),
            )
            .await
            .expect("fetch never hangs")
            .unwrap();
            if terminal.is_none() && update.state == JobStateDto::Finished {
                terminal = Some(requested.elapsed().as_secs_f64() * 1000.0);
            }
            if update.done {
                return (terminal.expect("terminal before done"), update);
            }
            assert!(
                requested.elapsed() < Duration::from_secs(10),
                "cancelled job hung"
            );
        }
    }
    /// Acknowledges every unknown outcome so later samples are not refused by the bound.
    fn acknowledge_all(&self) -> usize {
        let outcomes = self.connections.unresolved_outcomes(self.owner).outcomes;
        for outcome in &outcomes {
            self.connections
                .acknowledge_outcome(self.owner, OutcomeId::parse(&outcome.outcome_id).unwrap())
                .unwrap();
        }
        outcomes.len()
    }
}
fn max(samples: &[f64]) -> f64 {
    samples.iter().copied().fold(0.0, f64::max)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run through npm run test:ssh with disposable OpenSSH and database fixtures"]
async fn ssh_cancellation_uses_owned_fixture() {
    let engine = match var("STONEQL_SSH_ENGINE").as_str() {
        "mysql" => stoneql_core::Engine::MySql,
        "mariadb" => stoneql_core::Engine::MariaDb,
        "postgresql" => stoneql_core::Engine::PostgreSql,
        _ => panic!("unsupported fixture engine"),
    };
    let postgres = engine == stoneql_core::Engine::PostgreSql;
    let connections =
        Connections::new(4, Duration::from_secs(5), Duration::from_secs(600)).unwrap();
    let owner = OwnerId::new();
    let handle = connections
        .connect_with(
            owner,
            crate::DirectServer {
                engine,
                host: "db.stoneql.test".into(),
                port: var("STONEQL_SSH_DB_PORT").parse().unwrap(),
                username: "stoneql_reader".into(),
                password: var("STONEQL_FIXTURE_PASSWORD"),
                database: "stoneql_m001_fixture".into(),
                root_certificate_pem: Some(std::fs::read(var("STONEQL_LIFECYCLE_CA")).unwrap()),
                tls: true,
            },
            Some(crate::SshConfig {
                host: "127.0.0.1".into(),
                port: var("STONEQL_SSH_PORT").parse().unwrap(),
                username: var("STONEQL_SSH_USER"),
                host_key: Some(var("STONEQL_SSH_HOST_KEY")),
                auth: crate::SshAuth::Password(var("STONEQL_SSH_PASSWORD")),
            }),
            None,
        )
        .await
        .unwrap();
    let socket = {
        let mut entries = connections.0.entries.lock().unwrap();
        let entry = entries.get_mut(&handle).unwrap();
        // Test-only: the production gates never produce an SSH read-write handle.
        entry.access = Access::ReadWrite;
        entry.tunnel.as_ref().unwrap().endpoint().socket.clone()
    };
    let run = Run {
        connections,
        owner,
        handle,
        sleep: if postgres {
            "SELECT pg_sleep(20)"
        } else {
            "SELECT SLEEP(20)"
        },
    };
    let window = ExecutionLimits::CANCEL_GRACE + Duration::from_secs(1);
    let mut report = json!({"engine": var("STONEQL_SSH_ENGINE"), "windowMs": window.as_millis()});

    // The pool's idle connection holds one of the four forwards.
    let baseline = free_forwards(&socket, postgres).await;
    assert_eq!(baseline, 3, "free forwards beside the pool connection");

    // A free forward: the cancel connection fits and the server interrupt is delivered.
    let mut confirmed = Vec::new();
    for _ in 0..3 {
        let id = run.submit(None).await;
        let (ms, update) = run.cancel(id).await;
        assert_eq!(update.cancellation, CancellationDto::Confirmed);
        assert!(update.server_cancel_sent);
        assert!(
            ms <= window.as_secs_f64() * 1000.0,
            "free-slot cancel {ms} ms"
        );
        confirmed.push(ms);
        run.acknowledge_all();
    }

    // Every forward busy (pool, the lease's connection and held streams): the cancel
    // connection is refused at once, the statement is not observed to stop within the grace
    // period, the outcome stays unknown and the lease's connection is discarded.
    let mut exhausted = Vec::new();
    let mut held_streams = 0;
    for _ in 0..3 {
        let lease = run
            .connections
            .begin_transaction(run.owner, run.handle)
            .await
            .unwrap();
        let id = run.submit(Some(lease)).await;
        let held = exhaust(&socket, postgres).await;
        held_streams = held.len();
        assert_eq!(
            held_streams,
            baseline - 1,
            "pool + lease + held = 4 forwards"
        );
        let (ms, update) = run.cancel(id).await;
        drop(held);
        assert_eq!(update.cancellation, CancellationDto::Requested);
        assert!(!update.server_cancel_sent);
        assert_eq!(
            update.outcome,
            Some(stoneql_contracts::v1::ExecutionOutcomeDto::Unknown)
        );
        assert_eq!(update.lease, Some(LeaseStateDto::Ended));
        assert!(
            ms <= window.as_secs_f64() * 1000.0,
            "exhausted cancel {ms} ms"
        );
        exhausted.push(ms);
        // The discarded lease: its end is recorded unknown (never rolled back by assumption).
        assert!(run.acknowledge_all() >= 1);
        // The discarded connection's statement still runs server-side, but its forward is
        // released at once, so the tunnel is not left exhausted for the next cancel.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            free_forwards(&socket, postgres).await,
            baseline,
            "a discarded connection still holds a forward"
        );
    }

    // Recovered: the next cancel is delivered again.
    let id = run.submit(None).await;
    let (ms, update) = run.cancel(id).await;
    assert_eq!(update.cancellation, CancellationDto::Confirmed);
    assert!(update.server_cancel_sent);
    run.acknowledge_all();

    run.connections
        .disconnect(run.owner, run.handle)
        .await
        .unwrap();
    report["freeForwardsBesidePool"] = json!(baseline);
    report["heldStreamsUnderExhaustion"] = json!(held_streams);
    report["freeSlotCancelMs"] = json!({"samples": confirmed, "maxMs": max(&confirmed)});
    report["exhaustedCancelMs"] = json!({"samples": exhausted, "maxMs": max(&exhausted)});
    report["recoveredCancelMs"] = json!(ms);
    println!("STONEQL_SSH_CANCEL_RESULT {report}");
}
