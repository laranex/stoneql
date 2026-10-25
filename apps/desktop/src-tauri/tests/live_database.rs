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
const TRUST_VIEW_QUERIES: [&str; 3] = [
    "SELECT effect FROM trust_view_innodb",
    "SELECT effect FROM trust_view_myisam",
    "SELECT effect FROM trust_nested_view",
];
#[test]
#[ignore = "run through npm run test:native-database with disposable TLS fixtures"]
fn actual_database_commands_use_owned_tls_fixtures() {
    let marker = std::env::var("STONEQL_SPIKE_FIXTURE_ID").expect("owning fixture required");
    assert_eq!(marker.len(), 36);
    let engine = std::env::var("STONEQL_LIFECYCLE_ENGINE").unwrap();
    assert!(["mysql", "mariadb"].contains(&engine.as_str()));
    let port: u16 = std::env::var("STONEQL_LIFECYCLE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let password = std::env::var("STONEQL_FIXTURE_PASSWORD").unwrap();
    let ca = std::fs::read_to_string(std::env::var("STONEQL_LIFECYCLE_CA").unwrap()).unwrap();
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("stoneql_reader")
        .password(&password)
        .database("stoneql_m001_fixture")
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .ssl_ca_from_pem(ca.as_bytes().to_vec())
        .disable_statement_logging();
    tauri::async_runtime::block_on(async {
        let mut connection = MySqlConnection::connect_with(&options)
            .await
            .map_err(|_| "fixture TLS failed")
            .unwrap();
        let actual: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
            .fetch_one(&mut connection)
            .await
            .map_err(|_| "marker query failed")
            .unwrap();
        assert_eq!(actual, marker);
        // Positive controls prove the views really can write under their definer,
        // even though the reader itself has SELECT-only grants.
        for sql in TRUST_VIEW_QUERIES {
            let value: i32 = sqlx::query_scalar(sql)
                .fetch_one(&mut connection)
                .await
                .expect("definer positive control");
            assert_eq!(value, 1);
        }
        let counts: (i32, i32) = sqlx::query_as(
            "SELECT (SELECT touches FROM trust_innodb),(SELECT touches FROM trust_myisam)",
        )
        .fetch_one(&mut connection)
        .await
        .expect("positive effect inspection");
        assert_eq!(counts, (2, 1));
        let mut writer = MySqlConnection::connect_with(&options.clone().username("stoneql_writer"))
            .await
            .map_err(|_| "fixture reset connection failed")
            .unwrap();
        for sql in [
            "UPDATE trust_innodb SET touches=0",
            "UPDATE trust_myisam SET touches=0",
        ] {
            sqlx::raw_sql(sql)
                .execute(&mut writer)
                .await
                .expect("positive control reset");
        }
        writer
            .close()
            .await
            .map_err(|_| "reset connection close failed")
            .unwrap();
        sqlx::raw_sql("START TRANSACTION READ ONLY")
            .execute(&mut connection)
            .await
            .expect("read only control");
        for sql in TRUST_VIEW_QUERIES {
            let error = sqlx::query(sql)
                .fetch_all(&mut connection)
                .await
                .expect_err("engine must reject indirect writes");
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|error| error.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
                    .map(|error| error.number()),
                Some(1792)
            );
        }
        connection
            .close()
            .await
            .map_err(|_| "observer close failed")
            .unwrap();
    });
    let app = stoneql_desktop::app_builder(mock_builder(), false)
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
    let config = json!({"sessionId":session,"engine":engine,"host":"127.0.0.1","port":port,"username":"stoneql_reader","password":password,"database":"stoneql_m001_fixture","rootCertificatePem":ca});
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
    for sql in TRUST_VIEW_QUERIES {
        let error = read(&first["handle"], sql).unwrap_err();
        assert_eq!(error["code"], "QUERY_REJECTED");
        assert_eq!(error["outcome"], "unknown");
        assert!(read(&first["handle"], "SELECT id FROM trust_safe_view").is_ok());
    }
    // An allowlisted SELECT can acquire a lock indirectly through a view. Closing
    // the physical query connection must release that session-scoped side effect.
    let lock = read(&first["handle"], "SELECT acquired FROM trust_lock_view").unwrap();
    assert_eq!(lock["payload"]["rows"][0][0]["text"], "1");
    tauri::async_runtime::block_on(async {
        let mut observer = MySqlConnection::connect_with(&options)
            .await
            .map_err(|_| "trust observer TLS failed")
            .unwrap();
        let counts: (i32, i32) = sqlx::query_as(
            "SELECT (SELECT touches FROM trust_innodb),(SELECT touches FROM trust_myisam)",
        )
        .fetch_one(&mut observer)
        .await
        .expect("counter inspection");
        assert_eq!(counts, (0, 0));
        let free: i32 = sqlx::query_scalar("SELECT IS_FREE_LOCK(?)")
            .bind(format!("stoneql_{marker}"))
            .fetch_one(&mut observer)
            .await
            .expect("lock cleanup inspection");
        assert_eq!(free, 1);
        observer
            .close()
            .await
            .map_err(|_| "trust observer close failed")
            .unwrap();
    });
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
        "STONEQL_NATIVE_RESULT {{\"realDatabase\":true,\"tauriMockRuntime\":true,\"strictIpc\":true,\"independentHandles\":true,\"exactValues\":true,\"readOnlyRefusal\":true,\"packetRecovery\":true,\"indirectWritesRejected\":true,\"definerPositiveControls\":true,\"nonTransactionalWritesRejected\":true,\"indirectLockReleased\":true,\"disconnect\":true}}"
    );
}
