use serde_json::{Value, json};
use stoneql_contracts::v1::{CellValue, Envelope, HostInfo, ProbeResult};
use tauri::{
    WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};
/// The only `generate_context!` expansion in this test binary (it embeds Info.plist once).
fn build(builder: tauri::Builder<MockRuntime>) -> tauri::App<MockRuntime> {
    builder.build(tauri::generate_context!()).unwrap()
}
fn app() -> tauri::App<MockRuntime> {
    build(stoneql_desktop::app_builder(mock_builder(), false))
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
    let app = app();
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
    let app = app();
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
    let app = app();
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
    let app = app();
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
    let app = app();
    let main = window(&app, "main");
    let other = window(&app, "auxiliary");
    let session = bootstrap(&main).payload.session_id;
    let config = json!({"sessionId":session,"engine":"mysql","host":"127.0.0.1","port":3306,"username":"fixture","password":"synthetic-secret","database":"fixture","rootCertificatePem":null});
    for command in [
        "connect_direct",
        "read_only_query",
        "disconnect_database",
        "database_status",
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
    let app = app();
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
#[test]
fn ssh_and_secret_commands_fail_closed_before_network_and_never_echo_secrets() {
    let store = std::sync::Arc::new(MemoryStore::default());
    let app = build(stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            secrets: store.clone(),
            agent_socket: None,
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
    // An unenrolled SSH host key is refused before any network access.
    assert_eq!(
        code(connect(json!({"password":"synthetic-secret","ssh":ssh}))),
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
