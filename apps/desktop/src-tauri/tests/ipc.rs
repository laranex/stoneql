mod common;
use serde_json::{Value, json};
use stoneql_contracts::v1::{CellValue, Envelope, HostInfo, ProbeResult};
use tauri::{
    WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};
const ED25519: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
/// The only `generate_context!` expansion in this test binary (it embeds Info.plist once).
fn build(builder: tauri::Builder<MockRuntime>) -> tauri::App<MockRuntime> {
    builder.build(tauri::generate_context!()).unwrap()
}
fn app() -> (tauri::App<MockRuntime>, common::TempJournal) {
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        journal.services(),
    ));
    (app, journal)
}
fn window(app: &tauri::App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    // Configured windows are created only by the real event loop; mock tests create theirs explicitly.
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .build()
        .unwrap()
}
fn invoke(
    window: &WebviewWindow<MockRuntime>,
    command: &str,
    payload: Value,
    origin: &str,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: origin.parse().unwrap(),
            body: InvokeBody::Json(json!({"message":payload.to_string()})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
    .map(|body| body.deserialize::<Value>().unwrap())
}
fn bootstrap(window: &WebviewWindow<MockRuntime>) -> Envelope<HostInfo> {
    serde_json::from_value(
        invoke(
            window,
            "host_info",
            json!({"version":1,"requestId":"r","payload":{"nonce":"nonce_1"}}),
            "tauri://localhost",
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn actual_ipc_preserves_values_and_rejects_foreign_sessions() {
    let (app, _journal) = app();
    let webview = window(&app, "main");
    let info = bootstrap(&webview);
    assert!(info.payload.simulated);
    assert_eq!(info.payload.nonce, "nonce_1");
    let values = json!([{"kind":"integer","text":"18446744073709551615"},{"kind":"decimal","text":"12345678901234567890.4500"},{"kind":"null"},{"kind":"binary","hex":"00ff0180"}]);
    let mut request = json!({"version":1,"requestId":"roundtrip","payload":{"sessionId":info.payload.session_id,"action":"echo","values":values}});
    let reply: Envelope<ProbeResult> = serde_json::from_value(
        invoke(
            &webview,
            "contract_probe",
            request.clone(),
            "tauri://localhost",
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(serde_json::to_value(&reply.payload.values).unwrap(), values);
    assert_eq!(
        reply.payload.values[0],
        CellValue::Integer {
            text: "18446744073709551615".into()
        }
    );
    request["payload"]["sessionId"] = json!("forged");
    assert_eq!(
        invoke(&webview, "contract_probe", request, "tauri://localhost").unwrap_err()["code"],
        "FORBIDDEN"
    );
}
#[test]
fn acl_denies_auxiliary_windows_remote_origins_and_ungranted_commands() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let other = window(&app, "auxiliary");
    let request = json!({"version":1,"requestId":"r","payload":{"nonce":"nonce_1"}});
    assert!(invoke(&other, "host_info", request.clone(), "tauri://localhost").is_err());
    assert!(
        invoke(
            &main,
            "host_info",
            request.clone(),
            "https://untrusted.invalid"
        )
        .is_err()
    );
    assert!(
        invoke(
            &main,
            "plugin:fs|read_file",
            request.clone(),
            "tauri://localhost"
        )
        .is_err()
    );
    assert!(
        invoke(
            &main,
            "plugin:dialog|open",
            request.clone(),
            "tauri://localhost"
        )
        .is_err()
    );
    assert!(invoke(&main, "plugin:shell|execute", request, "tauri://localhost").is_err());
}
#[test]
fn actual_commands_reject_versions_unknown_fields_and_unready_acknowledgements() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    assert_eq!(
        invoke(
            &main,
            "host_info",
            json!({"version":2,"requestId":"r","payload":{"nonce":"n"}}),
            "tauri://localhost"
        )
        .unwrap_err()["code"],
        "UNSUPPORTED_VERSION"
    );
    assert_eq!(
        invoke(
            &main,
            "host_info",
            json!({"version":1,"requestId":"r","payload":{"nonce":"n","extra":true}}),
            "tauri://localhost"
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    let session = bootstrap(&main).payload.session_id;
    assert_eq!(invoke(&main,"contract_probe",json!({"version":1,"requestId":"r","payload":{"sessionId":session,"action":"ready","values":[]}}),"tauri://localhost").unwrap_err()["code"],"INVALID_REQUEST");
}

#[test]
fn native_close_requires_a_pending_os_request_and_preserves_cancelled_sessions() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let session = bootstrap(&main).payload.session_id;
    let mut message = json!({"version":1,"requestId":"close_1","payload":{"sessionId":session,"action":"cancel"}});
    assert_eq!(
        invoke(&main, "resolve_close", message.clone(), "tauri://localhost").unwrap_err()["code"],
        "FORBIDDEN"
    );
    stoneql_desktop::request_close(app.handle()).unwrap();
    message["payload"]["action"] = json!("prompted");
    assert_eq!(
        invoke(&main, "resolve_close", message.clone(), "tauri://localhost").unwrap()["payload"]["closed"],
        false
    );
    message["payload"]["action"] = json!("cancel");
    assert_eq!(
        invoke(&main, "resolve_close", message.clone(), "tauri://localhost").unwrap()["payload"]["closed"],
        false
    );
    assert_eq!(bootstrap(&main).payload.session_id, session);
    assert_eq!(
        invoke(&main, "resolve_close", message, "tauri://localhost").unwrap_err()["code"],
        "FORBIDDEN"
    );
}

#[test]
fn database_commands_enforce_session_acl_engine_and_secret_boundaries_before_network() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let other = window(&app, "auxiliary");
    let session = bootstrap(&main).payload.session_id;
    let config = json!({"sessionId":session,"engine":"mysql","host":"127.0.0.1","port":3306,"username":"fixture","password":"synthetic-secret","database":"fixture","rootCertificatePem":null});
    for command in [
        "connect_direct",
        "read_only_query",
        "disconnect_database",
        "database_status",
        "switch_database",
        "pick_sqlite_file",
        "connect_sqlite",
    ] {
        let payload = json!({"version":1,"requestId":"denied","payload":config});
        assert!(invoke(&other, command, payload.clone(), "tauri://localhost").is_err());
        assert!(invoke(&main, command, payload, "https://untrusted.invalid").is_err());
    }
    let mut payload = json!({"version":1,"requestId":"connect","payload":config});
    payload["payload"]["sessionId"] = json!("forged");
    let error = invoke(
        &main,
        "connect_direct",
        payload.clone(),
        "tauri://localhost",
    )
    .unwrap_err();
    assert_eq!(error["code"], "FORBIDDEN");
    assert!(!error.to_string().contains("synthetic-secret"));
    // Switching databases: a forged session, then an unknown handle, are refused before I/O.
    let switch = |session: &str| {
        invoke(
            &main,
            "switch_database",
            json!({"version":1,"requestId":"switch","payload":{"sessionId":session,"handle":"00000000-0000-4000-8000-000000000000","database":"other"}}),
            "tauri://localhost",
        )
        .unwrap_err()["code"]
            .clone()
    };
    assert_eq!(switch("forged"), "FORBIDDEN");
    assert_eq!(switch(&session), "FORBIDDEN");
    payload["payload"]["sessionId"] = json!(session);
    payload["payload"]["engine"] = json!("sqlite");
    assert_eq!(
        invoke(
            &main,
            "connect_direct",
            payload.clone(),
            "tauri://localhost"
        )
        .unwrap_err()["code"],
        "UNSUPPORTED_CAPABILITY"
    );
    // A forged session is refused before any native picker is shown.
    assert_eq!(
        invoke(
            &main,
            "pick_sqlite_file",
            json!({"version":1,"requestId":"pick","payload":{"sessionId":"forged"}}),
            "tauri://localhost"
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    payload["payload"]["engine"] = json!("mysql");
    payload["payload"]["rootCertificatePem"] = json!("-----BEGIN PRIVATE KEY-----synthetic");
    assert_eq!(
        invoke(&main, "connect_direct", payload, "tauri://localhost").unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    let handle = "00000000-0000-0000-0000-000000000001";
    let payload = json!({"version":1,"requestId":"read","payload":{"sessionId":session,"handle":handle,"sql":"SELECT 1","maxRows":10,"maxBytes":1024}});
    assert_eq!(
        invoke(&main, "read_only_query", payload, "tauri://localhost").unwrap_err()["code"],
        "FORBIDDEN"
    );
    // Omitted limits take the runtime maximums; explicit out-of-range or null limits are refused.
    let defaults = json!({"version":1,"requestId":"read","payload":{"sessionId":session,"handle":handle,"sql":"SELECT 1"}});
    assert_eq!(
        invoke(
            &main,
            "read_only_query",
            defaults.clone(),
            "tauri://localhost"
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    for (field, value) in [
        ("maxRows", json!(1001)),
        ("maxRows", json!(0)),
        ("maxBytes", json!(1_048_577)),
        ("maxRows", json!(null)),
        ("maxBytes", json!(null)),
    ] {
        let mut payload = defaults.clone();
        payload["payload"][field] = value;
        assert_eq!(
            invoke(&main, "read_only_query", payload, "tauri://localhost").unwrap_err()["code"],
            "INVALID_REQUEST"
        );
    }
}

#[test]
fn renderer_bootstrap_rotates_session_and_rejects_stale_probes() {
    let (app, _journal) = app();
    let webview = window(&app, "main");
    let first = bootstrap(&webview);
    let retry = bootstrap(&webview);
    assert_eq!(first.payload.session_id, retry.payload.session_id);
    let next = invoke(
        &webview,
        "host_info",
        json!({"version":1,"requestId":"reload","payload":{"nonce":"second_renderer"}}),
        "tauri://localhost",
    )
    .unwrap();
    assert_ne!(next["payload"]["sessionId"], first.payload.session_id);
    let stale = json!({"version":1,"requestId":"old_probe","payload":{"sessionId":first.payload.session_id,"action":"echo","values":[]}});
    assert_eq!(
        invoke(&webview, "contract_probe", stale, "tauri://localhost").unwrap_err()["code"],
        "FORBIDDEN"
    );
}
/// In-memory stand-in for the OS store: default tests never touch the user's keychain.
#[derive(Default)]
struct MemoryStore(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);
impl stoneql_desktop::SecretStore for MemoryStore {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), stoneql_desktop::SecretError> {
        self.0.lock().unwrap().insert(account.into(), value.into());
        Ok(())
    }
    fn get(&self, account: &str) -> Result<Vec<u8>, stoneql_desktop::SecretError> {
        self.0
            .lock()
            .unwrap()
            .get(account)
            .cloned()
            .ok_or(stoneql_desktop::SecretError::NotFound)
    }
    fn delete(&self, account: &str) -> Result<(), stoneql_desktop::SecretError> {
        self.0
            .lock()
            .unwrap()
            .remove(account)
            .map(|_| ())
            .ok_or(stoneql_desktop::SecretError::NotFound)
    }
}
/// Stand-in for the Linux master-password vault: locked until unlocked over IPC.
#[derive(Default)]
struct VaultDouble {
    items: MemoryStore,
    password: std::sync::Mutex<Option<String>>,
    unlocked: std::sync::atomic::AtomicBool,
}
impl stoneql_desktop::SecretStore for VaultDouble {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), stoneql_desktop::SecretError> {
        self.check()?;
        self.items.set(account, value)
    }
    fn get(&self, account: &str) -> Result<Vec<u8>, stoneql_desktop::SecretError> {
        self.check()?;
        self.items.get(account)
    }
    fn delete(&self, account: &str) -> Result<(), stoneql_desktop::SecretError> {
        self.items.delete(account)
    }
    fn status(&self) -> stoneql_contracts::v1::SecretStoreStatus {
        stoneql_contracts::v1::SecretStoreStatus {
            kind: stoneql_contracts::v1::SecretStoreKindDto::EncryptedFile,
            vault_exists: self.password.lock().unwrap().is_some(),
            unlocked: self.unlocked.load(std::sync::atomic::Ordering::SeqCst),
        }
    }
    fn unlock(&self, password: &str, create: bool) -> Result<(), stoneql_desktop::UnlockError> {
        let mut stored = self.password.lock().unwrap();
        match (stored.as_deref(), create) {
            (None, true) if password.chars().count() >= 12 => *stored = Some(password.into()),
            (Some(expected), false) if expected == password => {}
            (Some(_), false) => return Err(stoneql_desktop::UnlockError::Rejected),
            _ => return Err(stoneql_desktop::UnlockError::Invalid),
        }
        self.unlocked
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn reset(&self) -> Result<(), stoneql_desktop::SecretError> {
        *self.password.lock().unwrap() = None;
        self.unlocked
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.items.0.lock().unwrap().clear();
        Ok(())
    }
}
impl VaultDouble {
    fn check(&self) -> Result<(), stoneql_desktop::SecretError> {
        if self.unlocked.load(std::sync::atomic::Ordering::SeqCst) {
            Ok(())
        } else {
            Err(stoneql_desktop::SecretError::Locked)
        }
    }
}
#[test]
fn the_master_password_vault_is_unlocked_over_ipc_without_echoing_the_password() {
    let store = std::sync::Arc::new(VaultDouble::default());
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            secrets: store.clone(),
            agent_socket: None,
            read_write_sql: false,
            outcome_journal: Some(journal.dir()),
            secret_deadline: std::time::Duration::from_secs(60),
            host_keys: None,
        },
    ));
    let webview = window(&app, "main");
    let session = bootstrap(&webview).payload.session_id;
    let call = |command: &str, payload: Value, session: &str| {
        let mut payload = payload;
        payload["sessionId"] = json!(session);
        let reply = invoke(
            &webview,
            command,
            json!({"version":1,"requestId":"vault_case","payload":payload}),
            "tauri://localhost",
        );
        assert!(!format!("{reply:?}").contains("synthetic-master"));
        reply
    };
    let code = |reply: Result<Value, Value>| reply.unwrap_err()["code"].clone();
    let status = call("secret_store_status", json!({}), &session).unwrap();
    assert_eq!(
        status["payload"],
        json!({"kind":"encrypted_file","vaultExists":false,"unlocked":false})
    );
    let target = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader"});
    let save = json!({"purpose":"database_password","target":target,"secret":"synthetic-secret"});
    assert_eq!(
        code(call("store_secret", save.clone(), &session)),
        "SECRET_STORE_LOCKED"
    );
    assert_eq!(
        code(call(
            "unlock_secret_store",
            json!({"password":"synthetic-master-password","create":true}),
            "stale_session"
        )),
        "FORBIDDEN"
    );
    assert_eq!(
        code(call(
            "unlock_secret_store",
            json!({"password":"synthetic-master","create":false}),
            &session
        )),
        "INVALID_REQUEST"
    );
    let unlocked = call(
        "unlock_secret_store",
        json!({"password":"synthetic-master-password","create":true}),
        &session,
    )
    .unwrap();
    assert_eq!(
        unlocked["payload"],
        json!({"kind":"encrypted_file","vaultExists":true,"unlocked":true})
    );
    let reference = call("store_secret", save, &session).unwrap()["payload"]["reference"].clone();
    // A connect that needs the locked vault is refused before any network work, and the
    // renderer's retry after unlocking may reuse the same attempt id.
    store
        .unlocked
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let connect = json!({"engine":"mysql","host":"192.0.2.1","port":3306,"username":"reader","password":"","database":"app","rootCertificatePem":null,"passwordRef":reference,"attemptId":"vault_attempt"});
    let started = std::time::Instant::now();
    assert_eq!(
        code(call("connect_direct", connect.clone(), &session)),
        "SECRET_STORE_LOCKED"
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    store
        .unlocked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut retry = connect;
    retry["passwordRef"] = json!(uuid::Uuid::new_v4().to_string());
    assert_eq!(
        code(call("connect_direct", retry, &session)),
        "SECRET_UNAVAILABLE"
    );
    // A later renderer session that sends the wrong password is refused, not locked out.
    store
        .unlocked
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        code(call(
            "unlock_secret_store",
            json!({"password":"synthetic-master-wrong","create":false}),
            &session
        )),
        "MASTER_PASSWORD_REJECTED"
    );
    let reset = call("reset_secret_store", json!({}), &session).unwrap();
    assert_eq!(
        reset["payload"],
        json!({"kind":"encrypted_file","vaultExists":false,"unlocked":false})
    );
    assert!(store.items.0.lock().unwrap().is_empty());
}
#[test]
fn ssh_and_secret_commands_fail_closed_before_network_and_never_echo_secrets() {
    let store = std::sync::Arc::new(MemoryStore::default());
    // The bastion is enrolled natively; `unenrolled.internal` is not.
    let host_keys = std::sync::Arc::new(stoneql_desktop::MemoryHostKeyStore::default());
    stoneql_desktop::HostKeyStore::trust(host_keys.as_ref(), "bastion.internal", 22, ED25519)
        .unwrap();
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            secrets: store.clone(),
            agent_socket: None,
            read_write_sql: false,
            outcome_journal: Some(journal.dir()),
            secret_deadline: std::time::Duration::from_secs(60),
            host_keys: Some(host_keys),
        },
    ));
    let webview = window(&app, "main");
    let session = bootstrap(&webview).payload.session_id;
    let call = |command: &str, payload: Value| {
        let mut payload = payload;
        payload["sessionId"] = json!(session);
        let reply = invoke(
            &webview,
            command,
            json!({"version":1,"requestId":"ssh_case","payload":payload}),
            "tauri://localhost",
        );
        let text = format!("{reply:?}");
        assert!(
            !text.contains("synthetic-secret"),
            "{command} echoed a secret"
        );
        reply
    };
    let code = |reply: Result<Value, Value>| reply.unwrap_err()["code"].clone();
    let target = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader"});
    let reference = call(
        "store_secret",
        json!({"purpose":"database_password","target":target,"secret":"synthetic-secret"}),
    )
    .unwrap()["payload"]["reference"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(uuid::Uuid::parse_str(&reference).is_ok());
    assert_eq!(store.0.lock().unwrap().len(), 1);
    // Purpose/target mismatches are refused without storing anything.
    for payload in [
        json!({"purpose":"ssh_password","target":target,"secret":"synthetic-secret"}),
        json!({"purpose":"database_password","target":{"engine":"sqlite","host":"h","port":1,"username":"u"},"secret":"synthetic-secret"}),
        json!({"purpose":"database_password","target":target,"secret":""}),
    ] {
        assert_eq!(code(call("store_secret", payload)), "INVALID_REQUEST");
    }
    assert_eq!(store.0.lock().unwrap().len(), 1);
    let connect = |extra: Value| {
        let mut payload = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader","password":"","database":"app","rootCertificatePem":null});
        for (key, value) in extra.as_object().unwrap() {
            payload[key] = value.clone();
        }
        call("connect_direct", payload)
    };
    let ssh =
        json!({"host":"bastion.internal","port":22,"username":"deploy","auth":{"method":"agent"}});
    // A saved password is bound to its target and route: via SSH it is a different target.
    assert_eq!(
        code(connect(json!({"passwordRef":reference,"ssh":ssh}))),
        "SECRET_UNAVAILABLE"
    );
    assert_eq!(
        code(connect(
            json!({"password":"synthetic-secret","passwordRef":reference})
        )),
        "INVALID_REQUEST"
    );
    // An unenrolled SSH endpoint is refused before any network access.
    let mut unenrolled = ssh.clone();
    unenrolled["host"] = json!("unenrolled.internal");
    assert_eq!(
        code(connect(
            json!({"password":"synthetic-secret","ssh":unenrolled})
        )),
        "SSH_HOST_KEY_UNKNOWN"
    );
    let mut keyed = ssh.clone();
    keyed["hostKey"] = json!("not an openssh key");
    assert_eq!(code(connect(json!({"ssh":keyed}))), "INVALID_REQUEST");
    let mut sqlite = json!({"engine":"sqlite","ssh":ssh});
    sqlite["password"] = json!("");
    assert_eq!(code(connect(sqlite)), "UNSUPPORTED_CAPABILITY");
    assert_eq!(
        code(connect(json!({"attemptId":"not opaque!","ssh":ssh}))),
        "INVALID_REQUEST"
    );
    assert_eq!(
        call("cancel_connect", json!({"attemptId":"attempt_1"})).unwrap()["payload"],
        json!({"cancelled":false})
    );
    assert_eq!(
        code(call("ssh_host_key", json!({"host":"two words","port":22}))),
        "INVALID_REQUEST"
    );
    assert_eq!(
        code(call("delete_secret", json!({"reference":"../escape"}))),
        "INVALID_REQUEST"
    );
    for _ in 0..2 {
        call("delete_secret", json!({"reference":reference})).unwrap();
    }
    assert!(store.0.lock().unwrap().is_empty());
    // A stale renderer session cannot store, resolve or scan.
    let stale = invoke(
        &webview,
        "store_secret",
        json!({"version":1,"requestId":"stale","payload":{"sessionId":"stale","purpose":"database_password","target":target,"secret":"synthetic-secret"}}),
        "tauri://localhost",
    );
    assert_eq!(stale.unwrap_err()["code"], "FORBIDDEN");
}
/// Requirement: the read-write gate is enforced natively. Production builds have it on (A15,
/// approved 2026-11-17); a host with the gate off reports it and refuses `access: read_write`
/// for every engine and route before credential lookup or network I/O; read-only connects are
/// unaffected. SSH read-write with the gate on is covered by `ssh_read_write.rs`.
#[test]
fn read_write_is_refused_natively_while_the_gate_is_off() {
    // Changing the shipped gate must be a deliberate, reviewed change to this test too.
    const { assert!(stoneql_desktop::READ_WRITE_SQL_ENABLED) };
    assert!(stoneql_desktop::HostServices::system().read_write_sql);
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            read_write_sql: false,
            ..journal.services()
        },
    ));
    let webview = window(&app, "main");
    let info = bootstrap(&webview);
    assert!(!info.payload.read_write_sql);
    let session = info.payload.session_id;
    let call = |command: &str, payload: Value| {
        let mut payload = payload;
        payload["sessionId"] = json!(session);
        invoke(
            &webview,
            command,
            json!({"version":1,"requestId":"gate","payload":payload}),
            "tauri://localhost",
        )
    };
    // TEST-NET-1 (unroutable): a refusal that reached the network would time out instead.
    let direct = json!({"engine":"mysql","host":"192.0.2.1","port":3306,"username":"writer","password":"","database":"app","rootCertificatePem":null,"access":"read_write"});
    // A password reference proves the refusal precedes the secret store (no SECRET_UNAVAILABLE).
    let mut referenced = direct.clone();
    referenced["passwordRef"] = json!(uuid::Uuid::new_v4().to_string());
    let mut ssh = direct.clone();
    ssh["ssh"] =
        json!({"host":"192.0.2.2","port":22,"username":"deploy","auth":{"method":"agent"}});
    let mut postgres = direct.clone();
    postgres["engine"] = json!("postgresql");
    for payload in [direct, referenced, ssh, postgres] {
        let started = std::time::Instant::now();
        let error = call("connect_direct", payload).unwrap_err();
        assert_eq!(error["code"], "UNSUPPORTED_CAPABILITY");
        assert_eq!(error["outcome"], "rejected");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("pending safety approval")
        );
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
    }
    // SQLite part only in builds that include it (excluded from release builds, A5).
    #[cfg(feature = "sqlite")]
    {
        let root = std::env::temp_dir().join(format!("stoneql-gate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = std::fs::canonicalize(&root).unwrap().join("gate.sqlite");
        std::fs::write(&file, b"").unwrap();
        let grant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
            .unwrap()
            .grant
            .unwrap();
        assert_eq!(
            call(
                "connect_sqlite",
                json!({"grant":grant,"access":"read_write"})
            )
            .unwrap_err()["code"],
            "UNSUPPORTED_CAPABILITY"
        );
        // Read-only (omitted or explicit) still connects.
        for extra in [json!({}), json!({"access":"read_only"})] {
            let mut payload = json!({"grant":grant});
            payload
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let connection = call("connect_sqlite", payload).unwrap()["payload"].clone();
            assert_eq!(connection["access"], "read_only");
            call(
                "disconnect_database",
                json!({"handle":connection["handle"]}),
            )
            .unwrap();
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// Every registered command, parsed from the handler list so a new command cannot skip this test.
fn registered_commands() -> Vec<String> {
    let source = include_str!("../src/lib.rs");
    let start = source.find("generate_handler![").unwrap() + "generate_handler![".len();
    let end = start + source[start..].find(']').unwrap();
    source[start..end]
        .split(',')
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}
/// M002 G1/G3 for the whole IPC surface: auxiliary windows and remote origins are refused,
/// unsupported envelope versions and unknown envelope fields are rejected, before any command work.
#[test]
fn every_command_refuses_foreign_windows_origins_versions_and_unknown_fields() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let other = window(&app, "auxiliary");
    let session = bootstrap(&main).payload.session_id;
    let commands = registered_commands();
    assert_eq!(commands.len(), 35, "{commands:?}");
    for command in &commands {
        let payload = json!({"version":1,"requestId":"acl","payload":{"sessionId":session}});
        assert!(
            invoke(&other, command, payload.clone(), "tauri://localhost").is_err(),
            "{command} accepted an auxiliary window"
        );
        assert!(
            invoke(&main, command, payload, "https://untrusted.invalid").is_err(),
            "{command} accepted a remote origin"
        );
        let newer = json!({"version":2,"requestId":"acl","payload":{"sessionId":session}});
        assert_eq!(
            invoke(&main, command, newer, "tauri://localhost").unwrap_err()["code"],
            "UNSUPPORTED_VERSION",
            "{command} version"
        );
        let extra =
            json!({"version":1,"requestId":"acl","payload":{"sessionId":session},"extra":1});
        assert_eq!(
            invoke(&main, command, extra, "tauri://localhost").unwrap_err()["code"],
            "INVALID_REQUEST",
            "{command} unknown envelope field"
        );
    }
}
/// The five M005 commands with otherwise valid payloads still refuse a stale or forged session
/// (security review "missing tests" 1); the key-file picker must not open for a forged session.
#[test]
fn ssh_and_secret_commands_refuse_forged_sessions() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    bootstrap(&main);
    let target = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader"});
    for (command, payload) in [
        ("ssh_host_key", json!({"host":"bastion.internal","port":22})),
        ("pick_ssh_key_file", json!({})),
        ("cancel_connect", json!({"attemptId":"attempt_1"})),
        (
            "store_secret",
            json!({"purpose":"database_password","target":target,"secret":"synthetic-secret"}),
        ),
        (
            "delete_secret",
            json!({"reference":"00000000-0000-4000-8000-000000000000"}),
        ),
    ] {
        let mut payload = payload;
        payload["sessionId"] = json!("forged_session");
        let reply = invoke(
            &main,
            command,
            json!({"version":1,"requestId":"forged","payload":payload}),
            "tauri://localhost",
        );
        let error = reply.unwrap_err();
        assert_eq!(error["code"], "FORBIDDEN", "{command}: {error}");
        assert!(!error.to_string().contains("synthetic-secret"));
    }
}
/// M005 T0: unknown credential fields are refused before any network I/O (unroutable target).
#[test]
fn ssh_requests_reject_unknown_credential_fields() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let session = bootstrap(&main).payload.session_id;
    for (auth, extra) in [
        (false, "privateKey"),
        (false, "path"),
        (false, "forwardAgent"),
        (true, "privateKey"),
        (true, "path"),
    ] {
        let mut ssh = json!({"host":"bastion.internal","port":22,"username":"deploy","auth":{"method":"agent"}});
        if auth {
            ssh["auth"][extra] = json!("synthetic");
        } else {
            ssh[extra] = json!("synthetic");
        }
        let payload = json!({"sessionId":session,"engine":"mysql","host":"192.0.2.1","port":9,"username":"u","password":"","database":"d","rootCertificatePem":null,"ssh":ssh});
        let started = std::time::Instant::now();
        let error = invoke(
            &main,
            "connect_direct",
            json!({"version":1,"requestId":"unknown","payload":payload}),
            "tauri://localhost",
        )
        .unwrap_err();
        assert_eq!(error["code"], "INVALID_REQUEST", "{extra}");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
/// Keychain whose deletions fail (locked or denied), for F-10.
#[derive(Default)]
struct UndeletableStore(MemoryStore);
impl stoneql_desktop::SecretStore for UndeletableStore {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), stoneql_desktop::SecretError> {
        self.0.set(account, value)
    }
    fn get(&self, account: &str) -> Result<Vec<u8>, stoneql_desktop::SecretError> {
        self.0.get(account)
    }
    fn delete(&self, _account: &str) -> Result<(), stoneql_desktop::SecretError> {
        Err(stoneql_desktop::SecretError::Failed)
    }
}
/// M005 F-10: a failed deletion is reported as `SECRET_UNAVAILABLE`, never as success.
#[test]
fn failed_secret_deletion_is_reported_and_the_item_remains() {
    let store = std::sync::Arc::new(UndeletableStore::default());
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            secrets: store.clone(),
            agent_socket: None,
            read_write_sql: false,
            outcome_journal: Some(journal.dir()),
            secret_deadline: std::time::Duration::from_secs(60),
            host_keys: Some(std::sync::Arc::new(
                stoneql_desktop::MemoryHostKeyStore::default(),
            )),
        },
    ));
    let webview = window(&app, "main");
    let session = bootstrap(&webview).payload.session_id;
    let target = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader"});
    let reference = invoke(
        &webview,
        "store_secret",
        json!({"version":1,"requestId":"store","payload":{"sessionId":session,"purpose":"database_password","target":target,"secret":"synthetic-secret"}}),
        "tauri://localhost",
    )
    .unwrap()["payload"]["reference"]
        .clone();
    let error = invoke(
        &webview,
        "delete_secret",
        json!({"version":1,"requestId":"delete","payload":{"sessionId":session,"reference":reference}}),
        "tauri://localhost",
    )
    .unwrap_err();
    assert_eq!(error["code"], "SECRET_UNAVAILABLE");
    assert_eq!(
        store.0.0.lock().unwrap().len(),
        1,
        "the item is still present"
    );
}
/// Keychain stand-in for F-02: `get` signals entry, then blocks until released (a prompt the
/// user has not answered) or fails at once (`locked`).
#[derive(Default)]
struct PromptStore {
    inner: MemoryStore,
    locked: bool,
    /// (prompts entered, released) behind one mutex for the condition variable.
    state: std::sync::Mutex<(usize, bool)>,
    changed: std::sync::Condvar,
}
impl PromptStore {
    fn wait_entered(&self, count: usize) {
        let guard = self.state.lock().unwrap();
        let (_guard, timeout) = self
            .changed
            .wait_timeout_while(guard, std::time::Duration::from_secs(5), |s| s.0 < count)
            .unwrap();
        assert!(!timeout.timed_out(), "resolution never started");
    }
    fn set_released(&self, released: bool) {
        self.state.lock().unwrap().1 = released;
        self.changed.notify_all();
    }
}
impl stoneql_desktop::SecretStore for PromptStore {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), stoneql_desktop::SecretError> {
        self.inner.set(account, value)
    }
    fn get(&self, account: &str) -> Result<Vec<u8>, stoneql_desktop::SecretError> {
        if self.locked {
            return Err(stoneql_desktop::SecretError::Failed);
        }
        let mut guard = self.state.lock().unwrap();
        guard.0 += 1;
        self.changed.notify_all();
        let _unused = self
            .changed
            .wait_timeout_while(guard, std::time::Duration::from_secs(10), |s| !s.1)
            .unwrap();
        self.inner.get(account)
    }
    fn delete(&self, account: &str) -> Result<(), stoneql_desktop::SecretError> {
        self.inner.delete(account)
    }
}
fn prompt_app(
    store: std::sync::Arc<PromptStore>,
    deadline: std::time::Duration,
) -> (tauri::App<MockRuntime>, common::TempJournal) {
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            secrets: store,
            agent_socket: None,
            read_write_sql: false,
            outcome_journal: Some(journal.dir()),
            secret_deadline: deadline,
            host_keys: Some(std::sync::Arc::new(
                stoneql_desktop::MemoryHostKeyStore::default(),
            )),
        },
    ));
    (app, journal)
}
/// Stores a database password reference for the unroutable test target.
fn saved_reference(webview: &WebviewWindow<MockRuntime>, session: &Value) -> Value {
    let target = json!({"engine":"mysql","host":"192.0.2.1","port":9,"username":"reader"});
    invoke(
        webview,
        "store_secret",
        json!({"version":1,"requestId":"store","payload":{"sessionId":session,"purpose":"database_password","target":target,"secret":"synthetic-secret"}}),
        "tauri://localhost",
    )
    .unwrap()["payload"]["reference"]
        .clone()
}
fn connect_with_reference(
    webview: &WebviewWindow<MockRuntime>,
    session: &Value,
    reference: &Value,
    attempt: &str,
) -> Result<Value, Value> {
    invoke(
        webview,
        "connect_direct",
        json!({"version":1,"requestId":"connect","payload":{"sessionId":session,"engine":"mysql","host":"192.0.2.1","port":9,"username":"reader","password":"","passwordRef":reference,"database":"d","rootCertificatePem":null,"attemptId":attempt}}),
        "tauri://localhost",
    )
}
/// M005 F-02 (T1): cancel reaches a pending Keychain prompt; the late result is dropped and the
/// attempt is unregistered. 20 cycles leave no pending setup behind (the same attempt name is
/// accepted again each time). The unroutable target is never dialled.
#[test]
fn cancel_reaches_secret_resolution_and_leaves_nothing_behind() {
    let store = std::sync::Arc::new(PromptStore::default());
    let (app, _journal) = prompt_app(store.clone(), std::time::Duration::from_secs(30));
    let webview = window(&app, "main");
    let session = bootstrap(&webview).payload.session_id;
    let session = json!(session);
    let reference = saved_reference(&webview, &session);
    for cycle in 1..=20 {
        store.set_released(false);
        std::thread::scope(|scope| {
            let connecting =
                scope.spawn(|| connect_with_reference(&webview, &session, &reference, "attempt_1"));
            store.wait_entered(cycle);
            let cancelled = invoke(
                &webview,
                "cancel_connect",
                json!({"version":1,"requestId":"cancel","payload":{"sessionId":session,"attemptId":"attempt_1"}}),
                "tauri://localhost",
            )
            .unwrap();
            assert_eq!(cancelled["payload"]["cancelled"], true, "cycle {cycle}");
            let started = std::time::Instant::now();
            let error = connecting.join().unwrap().unwrap_err();
            assert_eq!(error["code"], "CANCELLED", "cycle {cycle}");
            assert!(started.elapsed() < std::time::Duration::from_secs(1));
            assert!(!error.to_string().contains("synthetic-secret"));
            store.set_released(true);
        });
    }
    // Nothing is pending: a cancel for the finished attempt finds nothing.
    let none = invoke(
        &webview,
        "cancel_connect",
        json!({"version":1,"requestId":"cancel","payload":{"sessionId":session,"attemptId":"attempt_1"}}),
        "tauri://localhost",
    )
    .unwrap();
    assert_eq!(none["payload"]["cancelled"], false);
}
/// M005 F-02 (T1): an unanswered prompt ends at the secret deadline; a locked Keychain fails.
#[test]
fn secret_resolution_is_bounded_and_locked_keychains_fail_closed() {
    let store = std::sync::Arc::new(PromptStore::default());
    let (app, _journal) = prompt_app(store.clone(), std::time::Duration::from_millis(300));
    let webview = window(&app, "main");
    let session = json!(bootstrap(&webview).payload.session_id);
    let reference = saved_reference(&webview, &session);
    let started = std::time::Instant::now();
    let error =
        connect_with_reference(&webview, &session, &reference, "attempt_deadline").unwrap_err();
    assert_eq!(error["code"], "DEADLINE_EXCEEDED");
    assert!(started.elapsed() < std::time::Duration::from_millis(800));
    store.set_released(true);
    let locked = std::sync::Arc::new(PromptStore {
        locked: true,
        ..Default::default()
    });
    let (app, _journal) = prompt_app(locked, std::time::Duration::from_secs(30));
    let webview = window(&app, "main");
    let session = json!(bootstrap(&webview).payload.session_id);
    let reference = saved_reference(&webview, &session);
    let error =
        connect_with_reference(&webview, &session, &reference, "attempt_locked").unwrap_err();
    assert_eq!(error["code"], "SECRET_UNAVAILABLE");
}
/// M005 F-01 (T4) without network: status, forget, refused trust without a native scan,
/// SSH passwords need an enrolled key, and `hostKey` can never be supplied by the renderer.
#[test]
fn host_key_trust_is_native_and_bound_to_scans() {
    let host_keys = std::sync::Arc::new(stoneql_desktop::MemoryHostKeyStore::default());
    let journal = common::TempJournal::new();
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            outcome_journal: Some(journal.dir()),
            host_keys: Some(host_keys.clone()),
            secrets: std::sync::Arc::new(MemoryStore::default()),
            ..stoneql_desktop::HostServices::system()
        },
    ));
    let webview = window(&app, "main");
    let session = json!(bootstrap(&webview).payload.session_id);
    let call = |command: &str, payload: Value| {
        let mut payload = payload;
        payload["sessionId"] = session.clone();
        invoke(
            &webview,
            command,
            json!({"version":1,"requestId":"trust","payload":payload}),
            "tauri://localhost",
        )
    };
    let endpoint = json!({"host":"Bastion.Internal","port":22});
    assert_eq!(
        call("ssh_host_key_status", endpoint.clone()).unwrap()["payload"],
        json!({"trusted":null})
    );
    // No pending native scan: refused before any dialog; nothing enrolled.
    let mut trust = endpoint.clone();
    trust["fingerprint"] = json!("SHA256:9Xl0yQZzJ8m2kKpz1bq5N2f3S5gkT0r9u1g6vXy3K0M");
    assert_eq!(
        call("trust_ssh_host_key", trust).unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    assert_eq!(
        stoneql_desktop::commit_ssh_host_key(app.handle(), "bastion.internal", 22, "SHA256:x")
            .unwrap_err()
            .code,
        stoneql_contracts::v1::ErrorCodeDto::InvalidRequest
    );
    assert_eq!(
        stoneql_desktop::HostKeyStore::get(host_keys.as_ref(), "bastion.internal", 22).unwrap(),
        None
    );
    // SSH passwords cannot be saved for an unenrolled host (their scope names the fingerprint).
    let target = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader","ssh":{"host":"bastion.internal","port":22,"username":"deploy"}});
    assert_eq!(
        call(
            "store_secret",
            json!({"purpose":"ssh_password","target":target,"secret":"synthetic-secret"})
        )
        .unwrap_err()["code"],
        "SSH_HOST_KEY_UNKNOWN"
    );
    // Enrolled natively: status reports the derived fingerprint, case-insensitively.
    stoneql_desktop::HostKeyStore::trust(host_keys.as_ref(), "bastion.internal", 22, ED25519)
        .unwrap();
    let status = call("ssh_host_key_status", endpoint.clone()).unwrap()["payload"].clone();
    assert_eq!(status["trusted"]["algorithm"], "ssh-ed25519");
    assert!(
        status["trusted"]["fingerprint"]
            .as_str()
            .unwrap()
            .starts_with("SHA256:")
    );
    assert!(
        call(
            "store_secret",
            json!({"purpose":"ssh_password","target":target,"secret":"synthetic-secret"})
        )
        .is_ok()
    );
    // The renderer can never supply the key.
    let connect = json!({"engine":"mysql","host":"db.internal","port":3306,"username":"reader","password":"","database":"app","rootCertificatePem":null,"ssh":{"host":"bastion.internal","port":22,"username":"deploy","hostKey":ED25519,"auth":{"method":"agent"}}});
    assert_eq!(
        call("connect_direct", connect).unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    // Forget is idempotent and fails closed.
    for _ in 0..2 {
        assert_eq!(
            call("forget_ssh_host_key", endpoint.clone()).unwrap()["payload"],
            json!({"trusted":null})
        );
    }
    assert_eq!(
        stoneql_desktop::HostKeyStore::get(host_keys.as_ref(), "bastion.internal", 22).unwrap(),
        None
    );
    assert_eq!(
        call("ssh_host_key_status", json!({"host":"two words","port":22})).unwrap_err()["code"],
        "INVALID_REQUEST"
    );
}

/// Host-key and transfer commands refuse forged sessions with otherwise valid payloads.
#[test]
fn host_key_and_transfer_commands_refuse_forged_sessions() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    bootstrap(&main);
    for (command, payload) in [
        (
            "ssh_host_key_status",
            json!({"host":"bastion.internal","port":22}),
        ),
        (
            "trust_ssh_host_key",
            json!({"host":"bastion.internal","port":22,"fingerprint":"SHA256:x"}),
        ),
        (
            "forget_ssh_host_key",
            json!({"host":"bastion.internal","port":22}),
        ),
        (
            "pick_export_file",
            json!({"suggestedName":"t.sql","format":"sql"}),
        ),
        (
            "start_export",
            json!({"handle":"00000000-0000-4000-8000-000000000000","grant":"g","tables":["t"],"format":"sql","structure":false,"data":true}),
        ),
        ("export_status", json!({"transferId":"t"})),
        ("cancel_export", json!({"transferId":"t"})),
    ] {
        let mut payload = payload;
        payload["sessionId"] = json!("forged_session");
        let error = invoke(
            &main,
            command,
            json!({"version":1,"requestId":"forged","payload":payload}),
            "tauri://localhost",
        )
        .unwrap_err();
        assert_eq!(error["code"], "FORBIDDEN", "{command}: {error}");
    }
}
/// M007 d1: destination grants are native, one-shot and refuse symlinks, dot files and missing
/// directories; an unknown or used grant starts nothing and creates no file.
#[test]
fn export_grants_are_native_one_shot_and_refuse_unsafe_destinations() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let session = bootstrap(&main).payload.session_id;
    let dir = std::env::temp_dir().join(format!("stoneql-export-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    std::fs::write(dir.join("target.txt"), "x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("target.txt"), dir.join("link.sql")).unwrap();
    let authorize =
        |path: std::path::PathBuf| stoneql_desktop::authorize_export_file(app.handle(), &path);
    #[cfg(unix)]
    assert!(
        authorize(dir.join("link.sql")).is_err(),
        "symlink destination"
    );
    assert!(authorize(dir.join(".hidden.sql")).is_err(), "dot file");
    assert!(
        authorize(dir.join("missing").join("x.sql")).is_err(),
        "missing directory"
    );
    let grant = authorize(dir.join("out.sql")).unwrap();
    assert_eq!(grant.file_name.as_deref(), Some("out.sql"));
    let start = |grant: &str| {
        invoke(
            &main,
            "start_export",
            json!({"version":1,"requestId":"start","payload":{"sessionId":session,"handle":"00000000-0000-4000-8000-000000000000","grant":grant,"tables":["t"],"format":"sql","structure":false,"data":true}}),
            "tauri://localhost",
        )
    };
    assert_eq!(start("unknown_grant").unwrap_err()["code"], "FORBIDDEN");
    // A valid grant on an unknown handle: the export task fails; no destination or temp file.
    let accepted = start(grant.grant.as_deref().unwrap()).unwrap();
    let id = accepted["payload"]["transferId"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut status = Value::Null;
    for _ in 0..100 {
        status = invoke(
            &main,
            "export_status",
            json!({"version":1,"requestId":"s","payload":{"sessionId":session,"transferId":id}}),
            "tauri://localhost",
        )
        .unwrap()["payload"]
            .clone();
        if status["state"] != "running" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(status["state"], "failed", "{status}");
    assert_eq!(status["error"], "FORBIDDEN");
    assert_eq!(
        start(grant.grant.as_deref().unwrap()).unwrap_err()["code"],
        "FORBIDDEN",
        "one-shot"
    );
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(
        !names.iter().any(|n| n == "out.sql" || n.ends_with(".part")),
        "{names:?}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
/// A5: the host reports whether this build includes SQLite; without it, the SQLite commands
/// refuse with UNSUPPORTED_CAPABILITY before any dialog or file access.
#[test]
fn sqlite_capability_matches_the_build() {
    let (app, _journal) = app();
    let main = window(&app, "main");
    let info = bootstrap(&main).payload;
    assert_eq!(info.sqlite, cfg!(feature = "sqlite"));
    if !cfg!(feature = "sqlite") {
        let session = info.session_id;
        for (command, payload) in [
            ("pick_sqlite_file", json!({})),
            (
                "connect_sqlite",
                json!({"grant":"00000000-0000-4000-8000-000000000000"}),
            ),
        ] {
            let mut payload = payload;
            payload["sessionId"] = json!(session);
            let error = invoke(
                &main,
                command,
                json!({"version":1,"requestId":"sqlite","payload":payload}),
                "tauri://localhost",
            )
            .unwrap_err();
            assert_eq!(error["code"], "UNSUPPORTED_CAPABILITY", "{command}");
        }
        let file =
            std::env::temp_dir().join(format!("stoneql-no-sqlite-{}.db", uuid::Uuid::new_v4()));
        std::fs::write(&file, b"").unwrap();
        assert!(stoneql_desktop::authorize_sqlite_file(app.handle(), &file).is_err());
        std::fs::remove_file(&file).unwrap();
    }
}

/// M007 d5: SQL import files are granted only by native code (dialog/tests), never by a path
/// from the renderer; grants name canonical regular files; the commands refuse malformed ids,
/// foreign handles and engines other than MySQL/MariaDB before reading anything.
#[test]
fn sql_import_grants_are_native_and_commands_refuse_before_reading() {
    let (app, _journal) = app();
    let webview = window(&app, "main");
    let session = bootstrap(&webview).payload.session_id;
    let call = |command: &str, payload: Value| {
        let mut payload = payload;
        payload["sessionId"] = json!(session);
        invoke(
            &webview,
            command,
            json!({"version":1,"requestId":"import","payload":payload}),
            "tauri://localhost",
        )
    };
    let root = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("stoneql-import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let file = root.join("dump.sql");
    std::fs::write(&file, b"INSERT INTO t VALUES (1);").unwrap();
    let granted = stoneql_desktop::authorize_import_file(app.handle(), &file).unwrap();
    assert_eq!(granted.file_name.as_deref(), Some("dump.sql"));
    let grant = granted.grant.unwrap();
    for refused in [root.clone(), root.join("missing.sql")] {
        assert_eq!(
            stoneql_desktop::authorize_import_file(app.handle(), &refused)
                .unwrap_err()
                .code,
            stoneql_contracts::v1::ErrorCodeDto::Forbidden
        );
    }
    assert_eq!(
        call(
            "start_sql_import",
            json!({"handle":"h","grant":"../dump.sql"})
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    let foreign = call(
        "start_sql_import",
        json!({"handle":"00000000-0000-4000-8000-000000000000","grant":grant}),
    )
    .unwrap_err();
    assert_eq!(foreign["code"], "FORBIDDEN", "{foreign}");
    assert_eq!(
        call(
            "sql_import_status",
            json!({"importId":"00000000-0000-4000-8000-000000000000"})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    // A connected engine other than MySQL/MariaDB is refused before the grant is used.
    #[cfg(feature = "sqlite")]
    {
        let database = root.join("db.sqlite");
        std::fs::write(&database, b"").unwrap();
        let sqlite = stoneql_desktop::authorize_sqlite_file(app.handle(), &database)
            .unwrap()
            .grant
            .unwrap();
        let connection =
            call("connect_sqlite", json!({"grant":sqlite})).unwrap()["payload"].clone();
        let error = call(
            "start_sql_import",
            json!({"handle":connection["handle"],"grant":grant}),
        )
        .unwrap_err();
        assert_eq!(error["code"], "UNSUPPORTED_CAPABILITY");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("MySQL and MariaDB")
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// Use TLS: turning TLS off is accepted only for direct MySQL/MariaDB to this computer without a
/// CA. Every other request is refused before secrets, SSH or network work (no timeout).
#[test]
fn plaintext_is_refused_natively_except_for_direct_loopback_mysql() {
    let (app, _journal) = app();
    let webview = window(&app, "main");
    let session = bootstrap(&webview).payload.session_id;
    let base = json!({"sessionId":session,"engine":"mysql","host":"192.0.2.1","port":3306,"username":"u","password":"","database":"app","rootCertificatePem":null,"tls":false});
    let with = |changes: Value| {
        let mut payload = base.clone();
        payload
            .as_object_mut()
            .unwrap()
            .extend(changes.as_object().unwrap().clone());
        payload
    };
    for payload in [
        base.clone(),
        with(json!({"host":"localhost.example.com"})),
        with(json!({"engine":"postgresql","host":"127.0.0.1","port":5432})),
        with(
            json!({"host":"127.0.0.1","rootCertificatePem":"-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----"}),
        ),
        with(
            json!({"host":"127.0.0.1","ssh":{"host":"192.0.2.2","port":22,"username":"deploy","auth":{"method":"agent"}}}),
        ),
    ] {
        let started = std::time::Instant::now();
        let error = invoke(
            &webview,
            "connect_direct",
            json!({"version":1,"requestId":"tls","payload":payload}),
            "tauri://localhost",
        )
        .unwrap_err();
        assert_eq!(error["code"], "INVALID_REQUEST", "{payload}");
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
    }
}
