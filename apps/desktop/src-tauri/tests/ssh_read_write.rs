//! R-9 / D025-9: read-write over SSH is refused natively with `UNSUPPORTED_CAPABILITY` before
//! any Keychain lookup or network access, even in a host whose read-write gate is enabled.
//! No Docker: the SSH endpoint is an unroutable documentation address and the secret store
//! only records lookups.
mod common;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use stoneql_desktop::{HostServices, SecretError, SecretStore};
use tauri::{
    WebviewUrl, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};

#[derive(Default)]
struct RecordingStore(AtomicUsize);
impl SecretStore for RecordingStore {
    fn set(&self, _: &str, _: &[u8]) -> Result<(), SecretError> {
        Err(SecretError::Unsupported)
    }
    fn get(&self, _: &str) -> Result<Vec<u8>, SecretError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(SecretError::NotFound)
    }
    fn delete(&self, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported)
    }
}

#[test]
fn ssh_read_write_is_refused_before_keychain_or_network_with_the_gate_enabled() {
    let store = Arc::new(RecordingStore::default());
    let journal = common::TempJournal::new();
    let app = stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        HostServices {
            secrets: store.clone(),
            agent_socket: None,
            // Test-only host: the production gate stays off (asserted in `ipc.rs`).
            read_write_sql: true,
            outcome_journal: Some(journal.dir()),
            secret_deadline: std::time::Duration::from_secs(60),
        },
    )
    .build(tauri::generate_context!())
    .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let raw = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window, InvokeRequest { cmd: command.into(), callback: CallbackFn(0), error: CallbackFn(1), url: "tauri://localhost".parse().unwrap(), body: InvokeBody::Json(json!({"message": json!({"version": 1, "requestId": "rw_request", "payload": payload}).to_string()})), headers: Default::default(), invoke_key: INVOKE_KEY.into() })
            .map(|body| body.deserialize().unwrap())
    };
    let info = raw("host_info", json!({"nonce": "rw_nonce"})).unwrap()["payload"].clone();
    assert_eq!(info["readWriteSql"], json!(true));
    let session = info["sessionId"].clone();
    // Unused references: resolving either would reach the store and fail SECRET_UNAVAILABLE.
    let reference = "6f1c2b9e-3d4a-4f5b-8c7d-9e0f1a2b3c4d";
    let request = |engine: &str, access: &str, ssh: bool| {
        let mut payload = json!({"sessionId": session, "engine": engine, "host": "db.example", "port": 3306, "username": "app", "password": "", "passwordRef": reference, "database": "app", "rootCertificatePem": null, "access": access});
        if ssh {
            // 192.0.2.1:9 (TEST-NET-1) never answers: any network attempt would stall.
            payload["ssh"] = json!({"host": "192.0.2.1", "port": 9, "username": "deploy", "hostKey": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl", "auth": {"method": "password", "passwordRef": reference}});
        }
        payload
    };
    let code = |reply: Result<Value, Value>| reply.unwrap_err()["code"].clone();

    for engine in ["mysql", "mariadb", "postgresql"] {
        let started = Instant::now();
        assert_eq!(
            code(raw("connect_direct", request(engine, "read_write", true))),
            json!("UNSUPPORTED_CAPABILITY"),
            "{engine} SSH read-write"
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{engine}: refused without network"
        );
        assert_eq!(
            store.0.load(Ordering::SeqCst),
            0,
            "{engine}: no Keychain lookup"
        );
    }

    // Controls: the same requests reach credential resolution when they are allowed past the
    // access check, so the refusal above happened before it.
    assert_eq!(
        code(raw("connect_direct", request("mysql", "read_write", false))),
        json!("SECRET_UNAVAILABLE"),
        "Direct read-write passes the enabled gate"
    );
    assert_eq!(store.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        code(raw("connect_direct", request("mysql", "read_only", true))),
        json!("SECRET_UNAVAILABLE"),
        "SSH read-only passes the access check"
    );
    assert_eq!(store.0.load(Ordering::SeqCst), 2);
}
