//! Explicitly opted-in IPC integration for SSH tunnels and OS secret references. Run only
//! through `npm run test:ssh`: disposable OpenSSH/database containers, generated keys, a
//! private agent and a per-run Keychain service whose items are deleted before exit.
mod common;
use serde_json::{Value, json};
use std::{env, sync::Arc};
use stoneql_desktop::{HostServices, OsSecretStore, SecretError, SecretStore};
use tauri::{
    WebviewUrl, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};
/// The app's own origin: Tauri serves the frontend from `http://tauri.localhost` on Windows.
const LOCAL: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is set by the owning fixture runner"))
}
/// Deletes every reference this test created, even when an assertion fails.
struct Created(Arc<OsSecretStore>, std::sync::Mutex<Vec<String>>);
impl Drop for Created {
    fn drop(&mut self) {
        for reference in self.1.lock().unwrap().drain(..) {
            let _ = self.0.delete(&reference);
        }
    }
}

#[test]
#[ignore = "run through npm run test:ssh with disposable OpenSSH and database fixtures"]
fn ssh_commands_use_owned_fixture() {
    assert_eq!(var("STONEQL_SPIKE_FIXTURE_ID").len(), 36);
    let engine = var("STONEQL_SSH_ENGINE");
    let service = var("STONEQL_KEYCHAIN_SERVICE");
    assert!(service.starts_with("me.thixpin.stoneql.test-"));
    let ssh_port: u16 = var("STONEQL_SSH_PORT").parse().unwrap();
    let rotated_port: u16 = var("STONEQL_SSH_ROTATED_PORT").parse().unwrap();
    let database_port: u16 = var("STONEQL_SSH_DB_PORT").parse().unwrap();
    let host_key = var("STONEQL_SSH_HOST_KEY");
    let ssh_password = var("STONEQL_SSH_PASSWORD");
    let passphrase = var("STONEQL_SSH_KEY_PASSPHRASE");
    let db_password = var("STONEQL_FIXTURE_PASSWORD");
    let ca = std::fs::read_to_string(var("STONEQL_LIFECYCLE_CA")).unwrap();
    let secrets = [
        ssh_password.clone(),
        passphrase.clone(),
        db_password.clone(),
    ];
    let keychain = Arc::new(OsSecretStore::new(&service));
    let created = Created(keychain.clone(), Default::default());
    let journal = common::TempJournal::new();
    let host_keys = Arc::new(stoneql_desktop::MemoryHostKeyStore::default());
    let rotated_key = var("STONEQL_SSH_ROTATED_HOST_KEY");
    let app = stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        HostServices {
            secrets: keychain.clone(),
            agent_socket: Some(var("STONEQL_SSH_AGENT").into()),
            // The shipped gate (A15): MySQL/MariaDB read-write over SSH is exercised below.
            read_write_sql: stoneql_desktop::READ_WRITE_SQL_ENABLED,
            outcome_journal: Some(journal.dir()),
            secret_deadline: std::time::Duration::from_secs(60),
            host_keys: Some(host_keys.clone()),
        },
    )
    .build(tauri::generate_context!())
    .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let raw = |command: &str, payload: Value| -> Result<Value, Value> {
        let reply = get_ipc_response(&window,InvokeRequest{cmd:command.into(),callback:CallbackFn(0),error:CallbackFn(1),url:LOCAL.parse().unwrap(),body:InvokeBody::Json(json!({"message":json!({"version":1,"requestId":"fixture_request","payload":payload}).to_string()})),headers:Default::default(),invoke_key:INVOKE_KEY.into()}).map(|body|body.deserialize::<Value>().unwrap());
        let text = format!("{reply:?}");
        assert!(
            secrets.iter().all(|secret| !text.contains(secret.as_str())),
            "{command} reply contained a secret"
        );
        reply
    };
    let bootstrap = |nonce: &str| {
        raw("host_info", json!({"nonce":nonce})).unwrap()["payload"]["sessionId"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let session = bootstrap("fixture_nonce");
    let call = |command: &str, mut payload: Value| {
        payload["sessionId"] = json!(session);
        raw(command, payload)
    };
    let code =
        |reply: Result<Value, Value>| reply.unwrap_err()["code"].as_str().unwrap().to_owned();
    let mut report = json!({"engine": engine});

    // Enrollment input only: scanning returns the presented key; nothing is trusted.
    let scanned = call("ssh_host_key", json!({"host":"127.0.0.1","port":ssh_port})).unwrap();
    assert_eq!(scanned["payload"]["publicKey"], json!(host_key));
    assert!(
        scanned["payload"]["fingerprint"]
            .as_str()
            .unwrap()
            .starts_with("SHA256:")
    );
    let endpoint = json!({"host":"127.0.0.1","port":ssh_port,"username":"stoneql"});
    let database = |host: &str, ssh: Value, extra: Value| {
        let mut payload = json!({"engine":engine,"host":host,"port":database_port,"username":"stoneql_reader","password":"","database":"stoneql_m001_fixture","rootCertificatePem":ca,"ssh":ssh});
        for (key, value) in extra.as_object().unwrap() {
            payload[key] = value.clone();
        }
        payload
    };
    let tunnel = |auth: Value| {
        let mut ssh = endpoint.clone();
        ssh["auth"] = auth;
        ssh
    };
    assert_eq!(
        code(call(
            "connect_direct",
            database(
                "db.stoneql.test",
                tunnel(json!({"method":"agent"})),
                json!({})
            )
        )),
        "SSH_HOST_KEY_UNKNOWN"
    );
    // F-01: enrollment binds to the native scan: a wrong fingerprint or another endpoint is
    // refused and nothing is stored; the scanned fingerprint enrolls (the native dialog is the
    // separate GUI gate, bypassed here through the trusted-caller seam).
    let fingerprint = scanned["payload"]["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let commit = |port: u16, print: &str| {
        stoneql_desktop::commit_ssh_host_key(app.handle(), "127.0.0.1", port, print)
            .map_err(|error| serde_json::to_value(error).unwrap()["code"].clone())
    };
    assert_eq!(
        commit(ssh_port, "SHA256:wrong").unwrap_err(),
        "INVALID_REQUEST"
    );
    assert_eq!(
        commit(rotated_port, &fingerprint).unwrap_err(),
        "INVALID_REQUEST"
    );
    assert_eq!(
        stoneql_desktop::HostKeyStore::get(host_keys.as_ref(), "127.0.0.1", ssh_port).unwrap(),
        None
    );
    let enrolled = commit(ssh_port, &fingerprint).unwrap();
    assert_eq!(enrolled.trusted.unwrap().fingerprint, fingerprint);
    assert_eq!(
        call(
            "ssh_host_key_status",
            json!({"host":"127.0.0.1","port":ssh_port})
        )
        .unwrap()["payload"]["trusted"]["fingerprint"],
        json!(fingerprint)
    );

    // OS secret references: saved once, bound to target+route, never returned over IPC.
    let target = json!({"engine":engine,"host":"db.stoneql.test","port":database_port,"username":"stoneql_reader","ssh":endpoint});
    let store = |purpose: &str, secret: &str| {
        let reply = call(
            "store_secret",
            json!({"purpose":purpose,"target":target,"secret":secret}),
        );
        // macOS Keychain; Linux disposable Secret Service (test:ssh starts it).
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let reference = reply.unwrap()["payload"]["reference"]
                .as_str()
                .unwrap()
                .to_owned();
            created.1.lock().unwrap().push(reference.clone());
            Some(reference)
        } else {
            assert_eq!(code(reply), "UNSUPPORTED_CAPABILITY");
            None
        }
    };
    let db_ref = store("database_password", &db_password);
    let ssh_ref = store("ssh_password", &ssh_password);
    let passphrase_ref = store("ssh_passphrase", &passphrase);
    let password_auth = match &ssh_ref {
        Some(reference) => json!({"method":"password","passwordRef":reference}),
        None => json!({"method":"password","password":ssh_password}),
    };
    let db_secret = match &db_ref {
        Some(reference) => json!({"passwordRef":reference}),
        None => json!({"password":db_password}),
    };
    if let Some(reference) = &db_ref {
        // The keychain item holds the bound scope beside the value; the profile never does.
        let item: Value = serde_json::from_slice(&keychain.get(reference).unwrap()).unwrap();
        assert!(
            item["scope"]
                .as_str()
                .unwrap()
                .contains("database_password")
        );
        assert!(
            item["scope"]
                .as_str()
                .unwrap()
                .contains("via|stoneql@127.0.0.1")
        );
    }
    let connected = |payload: Value, context: &str| -> String {
        let reply =
            call("connect_direct", payload).unwrap_or_else(|error| panic!("{context}: {error}"));
        reply["payload"]["handle"].as_str().unwrap().to_owned()
    };
    let query = |handle: &str| {
        call(
            "read_only_query",
            json!({"handle":handle,"sql":"SELECT 1 AS one"}),
        )
        .unwrap()["payload"]["rows"][0][0]["text"]
            .clone()
    };
    let handle = connected(
        database(
            "db.stoneql.test",
            tunnel(password_auth.clone()),
            db_secret.clone(),
        ),
        "password references",
    );
    assert_eq!(query(&handle), json!("1"));
    assert_eq!(
        call("database_status", json!({"handle":handle})).unwrap()["payload"]["connected"],
        json!(true)
    );
    call("disconnect_database", json!({"handle":handle})).unwrap();
    assert_eq!(
        call("database_status", json!({"handle":handle})).unwrap()["payload"]["connected"],
        json!(false)
    );
    report["secretReferences"] = json!(db_ref.is_some());

    // A15: MySQL/MariaDB read-write through the SSH tunnel with a write-capable account: DDL and
    // DML jobs commit, a transaction rolls back, and the reader account stays read-only.
    if engine != "postgresql" {
        let mut writer = database(
            "db.stoneql.test",
            tunnel(password_auth.clone()),
            json!({"password": db_password, "access": "read_write"}),
        );
        writer["username"] = json!("stoneql_writer");
        let handle = connected(writer, "SSH read-write");
        let run = |sql: &str, lease: Option<&Value>| -> Value {
            let mut payload = json!({"handle": handle, "sql": sql});
            if let Some(lease) = lease {
                payload["leaseId"] = lease.clone();
            }
            let job = call("submit_job", payload).unwrap()["payload"]["jobId"].clone();
            loop {
                let update =
                    call("fetch_job", json!({"jobId": job, "waitMs": 1000})).unwrap()["payload"]
                        .clone();
                if update["done"] == json!(true) {
                    return update;
                }
            }
        };
        let count = || {
            call(
                "read_only_query",
                json!({"handle":handle,"sql":"SELECT COUNT(*) AS n FROM ssh_rw"}),
            )
            .unwrap()["payload"]["rows"][0][0]["text"]
                .clone()
        };
        let done = run(
            "CREATE TABLE ssh_rw (id INT PRIMARY KEY, v VARCHAR(10)); INSERT INTO ssh_rw VALUES (1, 'a'), (2, 'b'); UPDATE ssh_rw SET v = 'c' WHERE id = 2; DELETE FROM ssh_rw WHERE id = 1",
            None,
        );
        assert_eq!(done["outcome"], "succeeded", "SSH write job: {done}");
        assert_eq!(count(), json!("1"));
        let lease =
            call("begin_transaction", json!({"handle": handle})).unwrap()["payload"]["leaseId"]
                .clone();
        let inside = run("INSERT INTO ssh_rw VALUES (3, 'x')", Some(&lease));
        assert_eq!(
            inside["outcome"], "succeeded",
            "SSH job in transaction: {inside}"
        );
        let ended = call(
            "end_transaction",
            json!({"leaseId": lease, "action": "rollback"}),
        )
        .unwrap()["payload"]
            .clone();
        assert_eq!(ended["outcome"], "rolled_back", "{ended}");
        assert_eq!(count(), json!("1"));
        assert_eq!(run("DROP TABLE ssh_rw", None)["outcome"], "succeeded");
        call("disconnect_database", json!({"handle": handle})).unwrap();
        report["sshReadWrite"] = json!({"jobs": true, "transactionRollback": true});
    }

    // A stored DB password cannot be replayed against another target/route.
    if let Some(reference) = &db_ref {
        assert_eq!(
            code(call(
                "connect_direct",
                json!({"engine":engine,"host":"127.0.0.1","port":var("STONEQL_LIFECYCLE_PORT").parse::<u16>().unwrap(),"username":"stoneql_reader","password":"","passwordRef":reference,"database":"stoneql_m001_fixture","rootCertificatePem":ca})
            )),
            "SECRET_UNAVAILABLE"
        );
    }

    // Key file via trusted native grant (the picker path) plus a passphrase reference.
    let grant = stoneql_desktop::authorize_ssh_key_file(
        app.handle(),
        std::path::Path::new(&var("STONEQL_SSH_KEY_LOCKED")),
    )
    .unwrap();
    assert_eq!(grant.file_name.as_deref(), Some("locked"));
    let key_auth = match &passphrase_ref {
        Some(reference) => {
            json!({"method":"key_file","grant":grant.grant,"passphraseRef":reference})
        }
        None => json!({"method":"key_file","grant":grant.grant,"passphrase":passphrase}),
    };
    let handle = connected(
        database("db.stoneql.test", tunnel(key_auth), db_secret.clone()),
        "passphrase key file",
    );
    assert_eq!(query(&handle), json!("1"));
    call("disconnect_database", json!({"handle":handle})).unwrap();
    assert_eq!(
        code(call(
            "connect_direct",
            database(
                "db.stoneql.test",
                tunnel(json!({"method":"key_file","grant":grant.grant})),
                db_secret.clone()
            )
        )),
        "SSH_AUTHENTICATION_FAILED"
    );

    // Host agent (injected private fixture agent).
    let handle = connected(
        database(
            "db.stoneql.test",
            tunnel(json!({"method":"agent"})),
            db_secret.clone(),
        ),
        "agent",
    );
    assert_eq!(query(&handle), json!("1"));
    call("disconnect_database", json!({"handle":handle})).unwrap();

    // F-01 swapped key: a same-user tamper replaces the stored key; the saved SSH password is
    // bound to the original fingerprint, so it no longer resolves and is never sent.
    let mut swapped_key = None;
    if let Some(reference) = &ssh_ref {
        stoneql_desktop::HostKeyStore::trust(
            host_keys.as_ref(),
            "127.0.0.1",
            ssh_port,
            &rotated_key,
        )
        .unwrap();
        assert_eq!(
            code(call(
                "connect_direct",
                database(
                    "db.stoneql.test",
                    tunnel(json!({"method":"password","passwordRef":reference})),
                    db_secret.clone()
                )
            )),
            "SECRET_UNAVAILABLE"
        );
        stoneql_desktop::HostKeyStore::trust(host_keys.as_ref(), "127.0.0.1", ssh_port, &host_key)
            .unwrap();
        swapped_key = Some("SECRET_UNAVAILABLE");
    }
    // A rotated server: with the old key enrolled for its endpoint, connecting is refused as
    // changed, and enrolling the new scan over it is refused too (rotation needs a forget).
    stoneql_desktop::HostKeyStore::trust(host_keys.as_ref(), "127.0.0.1", rotated_port, &host_key)
        .unwrap();
    let rescanned = call(
        "ssh_host_key",
        json!({"host":"127.0.0.1","port":rotated_port}),
    )
    .unwrap();
    let rotated_print = rescanned["payload"]["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        commit(rotated_port, &rotated_print).unwrap_err(),
        "SSH_HOST_KEY_CHANGED"
    );
    // Failure codes surfaced to the renderer.
    // Session-only password here: the saved one is bound to the enrolled endpoint's port.
    let mut rotated = tunnel(json!({"method":"password","password":ssh_password}));
    rotated["port"] = json!(rotated_port);
    assert_eq!(
        code(call(
            "connect_direct",
            database("db.stoneql.test", rotated, json!({"password":db_password}))
        )),
        "SSH_HOST_KEY_CHANGED"
    );
    assert_eq!(
        code(call(
            "connect_direct",
            database(
                "db.stoneql.test",
                tunnel(json!({"method":"password","password":"wrong-fixture-password"})),
                json!({"password":db_password})
            )
        )),
        "SSH_AUTHENTICATION_FAILED"
    );
    assert_eq!(
        code(call(
            "connect_direct",
            database(
                "wrong.stoneql.test",
                tunnel(password_auth.clone()),
                json!({"password":db_password})
            )
        )),
        "CONNECTION_UNAVAILABLE"
    );

    // Explicit setup cancellation is exercised in the runtime fixture: Tauri's mock IPC test
    // helper cannot service two concurrent invocations on one window.
    // Renderer reload (new bootstrap nonce) closes tunnel-backed handles.
    let handle = connected(
        database(
            "db.stoneql.test",
            tunnel(password_auth.clone()),
            db_secret.clone(),
        ),
        "before reload",
    );
    let reloaded = bootstrap("fixture_reload");
    assert_ne!(reloaded, session);
    let stale = raw(
        "database_status",
        json!({"sessionId":reloaded,"handle":handle}),
    )
    .unwrap();
    assert_eq!(stale["payload"]["connected"], json!(false));

    // Forget every reference; the items are gone from the dedicated service.
    for reference in created.1.lock().unwrap().iter() {
        raw(
            "delete_secret",
            json!({"sessionId":reloaded,"reference":reference}),
        )
        .unwrap();
        assert_eq!(keychain.get(reference), Err(SecretError::NotFound));
    }
    report["keychainItemsDeleted"] = json!(created.1.lock().unwrap().len());
    created.1.lock().unwrap().clear();
    report["ipc"] = json!({"scan": true, "unknownHostKey": "SSH_HOST_KEY_UNKNOWN", "changedHostKey": "SSH_HOST_KEY_CHANGED", "wrongPassword": "SSH_AUTHENTICATION_FAILED", "tlsMismatch": "CONNECTION_UNAVAILABLE", "keyFileGrant": true, "agent": true, "reloadClosesTunnel": true, "secretsInReplies": 0, "nativeEnrollment": true, "rotatedCommitRefused": "SSH_HOST_KEY_CHANGED", "swappedKeySecret": swapped_key});
    println!("STONEQL_NATIVE_RESULT {report}");
}
