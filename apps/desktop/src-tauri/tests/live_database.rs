//! Explicitly opted-in, owning-fixture IPC integration. Never reads saved profiles or arbitrary URLs.
use serde_json::{Value, json};
use sqlx::{
    ConnectOptions, Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use tauri::{
    WebviewUrl, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};
#[test]
#[ignore = "run through npm run test:native-database with disposable TLS fixtures"]
fn actual_database_commands_use_owned_tls_fixtures() {
    let marker = std::env::var("DBADMIN_SPIKE_FIXTURE_ID").expect("owning fixture required");
    assert_eq!(marker.len(), 36);
    let engine = std::env::var("DBADMIN_LIFECYCLE_ENGINE").unwrap();
    assert!(["mysql", "mariadb"].contains(&engine.as_str()));
    let port: u16 = std::env::var("DBADMIN_LIFECYCLE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let password = std::env::var("DBADMIN_FIXTURE_PASSWORD").unwrap();
    let ca = std::fs::read_to_string(std::env::var("DBADMIN_LIFECYCLE_CA").unwrap()).unwrap();
    tauri::async_runtime::block_on(async {
        let options = MySqlConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username("dbadmin_reader")
            .password(&password)
            .database("dbadmin_m001_fixture")
            .ssl_mode(MySqlSslMode::VerifyIdentity)
            .ssl_ca_from_pem(ca.as_bytes().to_vec())
            .disable_statement_logging();
        let mut connection = MySqlConnection::connect_with(&options)
            .await
            .map_err(|_| "fixture TLS failed")
            .unwrap();
        let actual: String = sqlx::query_scalar("SELECT id FROM dbadmin_fixture_marker")
            .fetch_one(&mut connection)
            .await
            .map_err(|_| "marker query failed")
            .unwrap();
        assert_eq!(actual, marker);
        connection
            .close()
            .await
            .map_err(|_| "observer close failed")
            .unwrap();
    });
    let app = dbadmin_desktop::app_builder(mock_builder(), false)
        .build(tauri::generate_context!())
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window,InvokeRequest{cmd:command.into(),callback:CallbackFn(0),error:CallbackFn(1),url:"tauri://localhost".parse().unwrap(),body:InvokeBody::Json(json!({"message":json!({"version":1,"requestId":"fixture_request","payload":payload}).to_string()})),headers:Default::default(),invoke_key:INVOKE_KEY.into()}).map(|body|body.deserialize().unwrap())
    };
    let session =
        invoke("host_info", json!({"nonce":"fixture_nonce"})).unwrap()["payload"]["sessionId"]
            .clone();
    let config = json!({"sessionId":session,"engine":engine,"host":"127.0.0.1","port":port,"username":"dbadmin_reader","password":password,"database":"dbadmin_m001_fixture","rootCertificatePem":ca});
    let first = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
    let second = invoke("connect_direct", config).unwrap()["payload"].clone();
    assert_ne!(first["handle"], second["handle"]);
    assert!(!first["engineVersion"].as_str().unwrap().is_empty());
    let read = |handle: &Value, sql: &str| {
        invoke(
            "read_only_query",
            json!({"sessionId":session,"handle":handle,"sql":sql,"maxRows":100,"maxBytes":1048576}),
        )
    };
    let result = read(
        &first["handle"],
        "SELECT u,d,zd,n FROM raw_values ORDER BY id",
    )
    .unwrap();
    assert_eq!(
        result["payload"]["rows"][0][0],
        json!({"kind":"integer","text":"18446744073709551615"})
    );
    assert_eq!(
        result["payload"]["rows"][0][1],
        json!({"kind":"decimal","text":"123456789012345678901234567890.4500"})
    );
    assert_eq!(
        result["payload"]["rows"][0][2],
        json!({"kind":"temporal","databaseType":"DATE","text":"0000-00-00"})
    );
    assert_eq!(result["payload"]["rows"][0][3], json!({"kind":"null"}));
    let duplicates = read(
        &first["handle"],
        "SELECT s AS value,u AS value FROM raw_values ORDER BY id",
    )
    .unwrap();
    assert_eq!(duplicates["payload"]["columns"][0]["name"], "value");
    assert_eq!(duplicates["payload"]["columns"][1]["name"], "value");
    assert_eq!(
        read(&first["handle"], "UPDATE raw_values SET id=9").unwrap_err()["code"],
        "UNSUPPORTED_SQL"
    );
    assert_eq!(
        read(&first["handle"], "SELECT payload FROM raw_large").unwrap_err()["code"],
        "PACKET_TOO_LARGE"
    );
    assert!(read(&first["handle"], "SELECT id FROM raw_values").is_ok());
    invoke(
        "disconnect_database",
        json!({"sessionId":session,"handle":first["handle"]}),
    )
    .unwrap();
    assert_eq!(
        read(&first["handle"], "SELECT 1").unwrap_err()["code"],
        "FORBIDDEN"
    );
    assert!(read(&second["handle"], "SELECT id FROM raw_values").is_ok());
    invoke(
        "disconnect_database",
        json!({"sessionId":session,"handle":second["handle"]}),
    )
    .unwrap();
    println!(
        "DBADMIN_NATIVE_RESULT {{\"realDatabase\":true,\"tauriMockRuntime\":true,\"strictIpc\":true,\"independentHandles\":true,\"exactValues\":true,\"readOnlyRefusal\":true,\"packetRecovery\":true,\"disconnect\":true}}"
    );
}
