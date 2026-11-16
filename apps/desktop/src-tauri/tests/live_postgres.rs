//! Explicitly opted-in, owning-fixture PostgreSQL IPC integration. Never reads saved profiles,
//! libpq environment or arbitrary URLs: every value comes from `npm run test:native-postgres`.
mod common;
use serde_json::{Value, json};
use sqlx::{
    ConnectOptions, Connection, PgConnection,
    postgres::{PgConnectOptions, PgSslMode},
};
use std::time::{Duration, Instant};
use tauri::{
    WebviewUrl, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};
const TRUST_VIEWS: [&str; 3] = [
    "SELECT effect FROM trust_view",
    "SELECT effect FROM trust_nested_view",
    "SELECT value FROM trust_sequence_view",
];
struct Fixture {
    port: u16,
    password: String,
    ca: String,
    bad_ca: String,
}
impl Fixture {
    fn options(&self, user: &str) -> PgConnectOptions {
        PgConnectOptions::new_without_pgpass()
            .host("127.0.0.1")
            .port(self.port)
            .username(user)
            .password(&self.password)
            .database("stoneql_m001_fixture")
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert_from_pem(self.ca.as_bytes().to_vec())
            .disable_statement_logging()
    }
    async fn connect(&self, user: &str) -> PgConnection {
        PgConnection::connect_with(&self.options(user))
            .await
            .map_err(|_| "fixture TLS connection failed")
            .unwrap()
    }
}
async fn scalar_i64(connection: &mut PgConnection, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(connection)
        .await
        .expect("fixture inspection")
}
/// Waits for owned reader backends to settle; never inspects unrelated sessions.
async fn settle_readers(observer: &mut PgConnection, expected: i64) {
    let start = Instant::now();
    loop {
        let count = scalar_i64(
            observer,
            "SELECT count(*) FROM pg_stat_activity WHERE usename='stoneql_reader'",
        )
        .await;
        if count == expected {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "reader sessions did not settle"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
/// rustls certificate rejections surface either as TLS or as invalid-data I/O errors.
fn certificate_failure(error: &sqlx::Error) -> String {
    let reason = match error {
        sqlx::Error::Tls(cause) => cause.to_string(),
        sqlx::Error::Io(cause) if cause.kind() == std::io::ErrorKind::InvalidData => {
            cause.get_ref().map(ToString::to_string).unwrap_or_default()
        }
        _ => String::new(),
    }
    .to_ascii_lowercase();
    assert!(
        reason.starts_with("invalid peer certificate:"),
        "not a certificate rejection"
    );
    reason
}
fn percentile(samples: &mut [u128], percentile: usize) -> u128 {
    samples.sort_unstable();
    samples[(samples.len() * percentile).div_ceil(100) - 1]
}

#[test]
#[ignore = "run through npm run test:native-postgres with a disposable TLS fixture"]
fn postgres_commands_use_owned_tls_fixture() {
    let marker = std::env::var("STONEQL_SPIKE_FIXTURE_ID").expect("owning fixture required");
    assert_eq!(marker.len(), 36);
    let fixture = Fixture {
        port: std::env::var("STONEQL_LIFECYCLE_PORT")
            .unwrap()
            .parse()
            .unwrap(),
        password: std::env::var("STONEQL_FIXTURE_PASSWORD").unwrap(),
        ca: std::fs::read_to_string(std::env::var("STONEQL_LIFECYCLE_CA").unwrap()).unwrap(),
        bad_ca: std::fs::read_to_string(std::env::var("STONEQL_LIFECYCLE_BAD_CA").unwrap())
            .unwrap(),
    };
    let mut report = serde_json::Map::new();
    tauri::async_runtime::block_on(async {
        let mut reader = fixture.connect("stoneql_reader").await;
        let actual: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
            .fetch_one(&mut reader)
            .await
            .unwrap();
        assert_eq!(actual, marker);
        // Trusted classification: certificate failures are TLS errors, not generic failures.
        let untrusted = PgConnection::connect_with(
            &fixture
                .options("stoneql_reader")
                .ssl_root_cert_from_pem(fixture.bad_ca.as_bytes().to_vec()),
        )
        .await
        .expect_err("untrusted CA must fail");
        assert!(certificate_failure(&untrusted).contains("unknownissuer"));
        let mismatch =
            PgConnection::connect_with(&fixture.options("stoneql_reader").host("localhost"))
                .await
                .expect_err("hostname mismatch must fail");
        assert!(certificate_failure(&mismatch).contains("not valid for name"));
        let wrong = PgConnection::connect_with(
            &fixture
                .options("stoneql_reader")
                .password("wrong-fixture-password"),
        )
        .await
        .expect_err("wrong password must fail");
        assert_eq!(
            wrong
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("28P01")
        );
        // The server refuses plaintext TCP regardless of client settings.
        let plaintext = PgConnection::connect_with(
            &fixture
                .options("stoneql_reader")
                .ssl_mode(PgSslMode::Disable),
        )
        .await
        .expect_err("hostnossl must be rejected");
        assert_eq!(
            plaintext
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("28000")
        );
        // SELECT-only privilege boundary.
        let denied = sqlx::query("DELETE FROM trust_counter")
            .execute(&mut reader)
            .await
            .expect_err("reader has no DELETE grant");
        assert_eq!(
            denied
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref(),
            Some("42501")
        );
        // Positive controls: outside a read-only transaction the SELECT-only reader really
        // writes through definer functions and sequences reached from views.
        for sql in TRUST_VIEWS {
            let value: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .fetch_one(&mut reader)
                .await
                .expect("definer positive control");
            assert!(value >= 1);
        }
        assert_eq!(
            scalar_i64(&mut reader, "SELECT touches::bigint FROM trust_counter").await,
            2
        );
        let mut owner = fixture.connect("stoneql_fixture").await;
        sqlx::raw_sql("UPDATE trust_counter SET touches=0; ALTER SEQUENCE trust_seq RESTART")
            .execute(&mut owner)
            .await
            .expect("positive control reset");
        sqlx::raw_sql("START TRANSACTION READ ONLY")
            .execute(&mut reader)
            .await
            .unwrap();
        for sql in TRUST_VIEWS {
            let error = sqlx::query(sqlx::AssertSqlSafe(sql))
                .fetch_all(&mut reader)
                .await
                .expect_err("engine must reject indirect writes");
            assert_eq!(
                error
                    .as_database_error()
                    .and_then(|error| error.code())
                    .as_deref(),
                Some("25006")
            );
            sqlx::raw_sql("ROLLBACK; START TRANSACTION READ ONLY")
                .execute(&mut reader)
                .await
                .unwrap();
        }
        reader.close().await.unwrap();
        owner.close().await.unwrap();
        // Runtime lease expiry closes the server backend and releases capacity.
        let connections =
            stoneql_runtime::Connections::new(1, Duration::from_secs(3), Duration::from_secs(1))
                .unwrap();
        let mut observer = fixture.connect("stoneql_fixture").await;
        let owner_id = stoneql_runtime::OwnerId::new();
        let config = || stoneql_runtime::DirectServer {
            engine: stoneql_core::Engine::PostgreSql,
            host: "127.0.0.1".into(),
            port: fixture.port,
            username: "stoneql_reader".into(),
            password: fixture.password.clone(),
            database: "stoneql_m001_fixture".into(),
            root_certificate_pem: Some(fixture.ca.as_bytes().to_vec()),
        };
        let leased = connections.connect(owner_id, config()).await.unwrap();
        settle_readers(&mut observer, 1).await;
        std::thread::sleep(Duration::from_millis(1200));
        assert!(!connections.is_connected(owner_id, leased));
        settle_readers(&mut observer, 0).await;
        let again = connections.connect(owner_id, config()).await.unwrap();
        connections.disconnect(owner_id, again).await.unwrap();
        settle_readers(&mut observer, 0).await;
        let mut mismatched = config();
        mismatched.engine = stoneql_core::Engine::MySql;
        assert!(connections.connect(owner_id, mismatched).await.is_err());
        connections.shutdown().await;
        observer.close().await.unwrap();
    });

    let journal = common::TempJournal::new();
    let app = stoneql_desktop::app_builder_with(mock_builder(), false, journal.services())
        .build(tauri::generate_context!())
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window,InvokeRequest{cmd:command.into(),callback:CallbackFn(0),error:CallbackFn(1),url:"tauri://localhost".parse().unwrap(),body:InvokeBody::Json(json!({"message":json!({"version":1,"requestId":"fixture_request","payload":payload}).to_string()})),headers:Default::default(),invoke_key:INVOKE_KEY.into()}).map(|body|body.deserialize().unwrap())
    };
    let session =
        invoke("host_info", json!({"nonce":"pg_fixture_nonce"})).unwrap()["payload"]["sessionId"]
            .clone();
    let config = json!({"sessionId":session,"engine":"postgresql","host":"127.0.0.1","port":fixture.port,"username":"stoneql_reader","password":fixture.password,"database":"stoneql_m001_fixture","rootCertificatePem":fixture.ca});
    for (field, value) in [
        ("rootCertificatePem", json!(fixture.bad_ca)),
        ("host", json!("localhost")),
        ("password", json!("wrong-fixture-password")),
    ] {
        let mut bad = config.clone();
        bad[field] = value;
        let error = invoke("connect_direct", bad).unwrap_err();
        assert_eq!(error["code"], "CONNECTION_UNAVAILABLE", "{field}");
        assert!(!error.to_string().contains(&fixture.password));
    }
    let mut mysql_profile = config.clone();
    mysql_profile["engine"] = json!("mysql");
    assert!(invoke("connect_direct", mysql_profile).is_err());

    // Lifecycle profile: 100 verified-TLS connect/identity/disconnect cycles via actual IPC.
    let mut samples = Vec::with_capacity(100);
    for _ in 0..100 {
        let start = Instant::now();
        let handle = invoke("connect_direct", config.clone()).unwrap()["payload"]["handle"].clone();
        invoke(
            "disconnect_database",
            json!({"sessionId":session,"handle":handle}),
        )
        .unwrap();
        samples.push(start.elapsed().as_micros());
    }
    report.insert("lifecycleSamples".into(), json!(samples.len()));
    report.insert(
        "lifecycleP50Micros".into(),
        json!(percentile(&mut samples, 50)),
    );
    report.insert(
        "lifecycleP95Micros".into(),
        json!(percentile(&mut samples, 95)),
    );

    let first = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
    let second = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
    assert_ne!(first["handle"], second["handle"]);
    let version = first["engineVersion"].as_str().unwrap().to_owned();
    assert!(version.starts_with("PostgreSQL 17.6"), "{version}");
    report.insert("engineVersion".into(), json!(version));
    let read = |handle: &Value, sql: &str| {
        invoke(
            "read_only_query",
            json!({"sessionId":session,"handle":handle,"sql":sql}),
        )
    };
    // A8 backend-message limit (vendored driver): a 3 MiB DataRow is refused from its header
    // (2 MiB limit) and the next query on a fresh connection works.
    assert_eq!(
        read(&first["handle"], "SELECT payload FROM raw_large").unwrap_err()["code"],
        "PACKET_TOO_LARGE"
    );
    assert_eq!(
        read(&first["handle"], "SELECT id FROM raw_large").unwrap()["payload"]["rows"][0][0]["text"],
        "1"
    );
    report.insert("messageLimitRefused".into(), json!(true));
    let values = read(
        &first["handle"],
        r#"SELECT i8, n, nn, f8, b, t, d, e, ts, tz, dt, tm, ttz, j, u, ip, flag, "Quoted ""Column""" FROM "Mixed ""Case"" values" ORDER BY id"#,
    )
    .unwrap()["payload"]
        .clone();
    let types: Vec<_> = values["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|column| column["databaseType"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        [
            "INT8",
            "NUMERIC",
            "NUMERIC",
            "FLOAT8",
            "BYTEA",
            "TEXT",
            "TEXT",
            "fixture_mood",
            "TIMESTAMP",
            "TIMESTAMPTZ",
            "DATE",
            "TIME",
            "TIMETZ",
            "JSONB",
            "UUID",
            "INET",
            "BOOL",
            "TEXT"
        ]
    );
    assert_eq!(values["columns"][17]["name"], "Quoted \"Column\"");
    assert_eq!(
        values["rows"][0],
        json!([
            {"kind":"integer","text":"-9223372036854775808"},
            {"kind":"decimal","text":"123456789012345678901234567890.4500"},
            {"kind":"decimal","text":"NaN"},
            {"kind":"float","text":"0.1"},
            {"kind":"binary","hex":"00ff0180"},
            {"kind":"text","text":""},
            {"kind":"text","text":"fallback"},
            {"kind":"text","text":"unknown label"},
            {"kind":"temporal","databaseType":"TIMESTAMP","text":"2024-02-29 12:34:56.123456"},
            {"kind":"temporal","databaseType":"TIMESTAMPTZ","text":"2024-02-29 07:04:56.123456+00"},
            {"kind":"temporal","databaseType":"DATE","text":"0044-03-15 BC"},
            {"kind":"temporal","databaseType":"TIME","text":"24:00:00"},
            {"kind":"temporal","databaseType":"TIMETZ","text":"12:34:56.5-07"},
            {"kind":"json","text":"{\"a\": [1, 2], \"b\": 1}"},
            {"kind":"text","text":"a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"},
            {"kind":"unknown","databaseType":"INET","hex":"02180004c0a80001"},
            {"kind":"boolean","value":true},
            {"kind":"text","text":"quoted identifier"}
        ])
    );
    assert_eq!(
        values["rows"][1],
        json!([
            {"kind":"integer","text":"9223372036854775807"},
            {"kind":"decimal","text":"-0.0001"},
            {"kind":"decimal","text":"0.00000000000000000001"},
            {"kind":"float","text":"Infinity"},
            {"kind":"binary","hex":""},
            {"kind":"null"},
            {"kind":"null"},
            {"kind":"text","text":"active"},
            {"kind":"temporal","databaseType":"TIMESTAMP","text":"infinity"},
            {"kind":"temporal","databaseType":"TIMESTAMPTZ","text":"-infinity"},
            {"kind":"temporal","databaseType":"DATE","text":"infinity"},
            {"kind":"temporal","databaseType":"TIME","text":"00:00:00"},
            {"kind":"temporal","databaseType":"TIMETZ","text":"00:00:00+00"},
            {"kind":"json","text":"null"},
            {"kind":"null"},
            {"kind":"null"},
            {"kind":"boolean","value":false},
            {"kind":"null"}
        ])
    );
    // M006: the renderer's PostgreSQL catalog and browse statements (liveSchema.ts) over verified
    // TLS through the real handler, as the SELECT-only reader.
    let text = |cell: &Value| -> String {
        match cell["kind"].as_str().unwrap() {
            "null" => String::new(),
            "text" | "integer" => cell["text"].as_str().unwrap().to_owned(),
            other => panic!("unexpected catalog cell {other}: {cell}"),
        }
    };
    let tables = read(&first["handle"], r#"SELECT table_name AS name, table_type AS kind FROM information_schema.tables WHERE table_schema = current_schema ORDER BY table_name;"#).unwrap();
    let listed: Vec<(String, String)> = tables["payload"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (text(&row[0]), text(&row[1])))
        .collect();
    assert!(
        listed.contains(&("Mixed \"Case\" values".into(), "BASE TABLE".into())),
        "{listed:?}"
    );
    assert!(
        listed.contains(&("trust_view".into(), "VIEW".into())),
        "{listed:?}"
    );
    let columns = read(&first["handle"], r#"SELECT c.column_name AS name, c.data_type AS type, c.udt_name AS udt, c.character_maximum_length AS len, c.numeric_precision AS prec, c.numeric_scale AS scale, c.datetime_precision AS dprec, c.is_nullable AS nullable, c.column_default AS dflt, c.is_identity AS ident, (SELECT COUNT(*) FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class r ON r.oid = i.indrelid JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace JOIN pg_catalog.pg_attribute a ON a.attrelid = r.oid WHERE i.indisprimary AND n.nspname = c.table_schema AND r.relname = c.table_name AND a.attname = c.column_name AND CONCAT(' ', i.indkey, ' ') LIKE CONCAT('% ', a.attnum, ' %')) AS pk FROM information_schema.columns c WHERE c.table_schema = current_schema AND c.table_name = 'Mixed "Case" values' ORDER BY c.ordinal_position;"#).unwrap();
    let rows = columns["payload"]["rows"].as_array().unwrap();
    let by_name = |name: &str| {
        rows.iter()
            .find(|row| text(&row[0]) == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(
        text(&by_name("id")[10]),
        "1",
        "primary key visible to the SELECT-only reader"
    );
    assert_eq!(text(&by_name("i8")[10]), "0");
    assert_eq!(
        (
            text(&by_name("n")[1]),
            text(&by_name("n")[4]),
            text(&by_name("n")[5])
        ),
        ("numeric".into(), "38".into(), "4".into())
    );
    assert_eq!(
        (text(&by_name("e")[1]), text(&by_name("e")[2])),
        ("USER-DEFINED".into(), "fixture_mood".into())
    );
    assert_eq!(text(&by_name("d")[8]), "'fallback'::text");
    assert_eq!(text(&by_name("Quoted \"Column\"")[0]), "Quoted \"Column\"");
    let page = read(
        &first["handle"],
        r#"SELECT * FROM "Mixed ""Case"" values"
WHERE LOWER(CONCAT("id")) LIKE '%5!_%' ESCAPE '!' OR LOWER(CONCAT("nn")) LIKE '%5!_%' ESCAPE '!'
ORDER BY "nn" DESC NULLS LAST
LIMIT 25 OFFSET 50;"#,
    )
    .unwrap();
    assert!(page["payload"]["columns"].as_array().unwrap().len() >= 10);
    let count = read(
        &first["handle"],
        r#"SELECT COUNT(*) AS total FROM "Mixed ""Case"" values"
WHERE LOWER(CONCAT("id")) LIKE '%5!_%' ESCAPE '!' OR LOWER(CONCAT("nn")) LIKE '%5!_%' ESCAPE '!';"#,
    )
    .unwrap();
    assert_eq!(count["payload"]["rows"][0][0]["kind"], "integer");
    let all = read(
        &first["handle"],
        "SELECT * FROM \"Mixed \"\"Case\"\" values\"\nORDER BY \"id\" ASC\nLIMIT 25 OFFSET 0;",
    )
    .unwrap();
    assert_eq!(all["payload"]["rows"].as_array().unwrap().len(), 2);
    // Indexes (server definitions) and a composite foreign key (fixture seed `fk_child`).
    let indexes = read(&first["handle"], r#"SELECT indexname AS name, indexdef AS definition FROM pg_catalog.pg_indexes WHERE schemaname = current_schema AND tablename = 'fk_child' ORDER BY indexname;"#).unwrap();
    let definitions: Vec<String> = indexes["payload"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| text(&row[1]))
        .collect();
    assert!(
        definitions.iter().any(|d| d
            .starts_with("CREATE UNIQUE INDEX fk_child_xy ON public.fk_child USING btree (x, y)")),
        "{definitions:?}"
    );
    let keys = read(&first["handle"], r#"SELECT c.conname AS name, a.attname AS col, fr.relname AS ref_table, fa.attname AS ref_col, c.confdeltype AS on_delete, c.confupdtype AS on_update, a.attnum AS pos FROM pg_catalog.pg_constraint c JOIN pg_catalog.pg_class r ON r.oid = c.conrelid JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace JOIN pg_catalog.pg_class fr ON fr.oid = c.confrelid JOIN pg_catalog.pg_attribute a ON a.attrelid = c.conrelid JOIN pg_catalog.pg_attribute fa ON fa.attrelid = c.confrelid WHERE c.contype = 'f' AND n.nspname = current_schema AND r.relname = 'fk_child' AND CONCAT(',', SUBSTRING(CONCAT(c.conkey), 2, CHAR_LENGTH(CONCAT(c.conkey)) - 2), ',') LIKE CONCAT('%,', a.attnum, ',%') AND CONCAT(',', SUBSTRING(CONCAT(c.confkey), 2, CHAR_LENGTH(CONCAT(c.confkey)) - 2), ',') LIKE CONCAT('%,', fa.attnum, ',%') ORDER BY c.conname, a.attnum, fa.attnum;"#).unwrap();
    let rows = keys["payload"]["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        4,
        "column x referenced-column product of one composite key"
    );
    for row in rows {
        assert_eq!(
            (text(&row[0]), text(&row[2]), text(&row[4])),
            ("fk_child_pair".into(), "fk_parent".into(), "n".into())
        );
    }
    // M007: streaming exports into native grants; the runner restores them with psql.
    let exports = std::path::PathBuf::from(std::env::var("STONEQL_EXPORT_DIR").unwrap());
    let export = |table: &str, name: &str, format: &str, structure: bool| -> Value {
        let grant = stoneql_desktop::authorize_export_file(app.handle(), &exports.join(name))
            .unwrap()
            .grant
            .unwrap();
        let accepted = invoke(
            "start_export",
            json!({"sessionId":session,"handle":first["handle"],"grant":grant,"table":table,"format":format,"structure":structure,"data":true}),
        )
        .unwrap();
        let id = accepted["payload"]["transferId"].clone();
        for _ in 0..600 {
            let status = invoke(
                "export_status",
                json!({"sessionId":session,"transferId":id}),
            )
            .unwrap()["payload"]
                .clone();
            if status["state"] != "running" {
                return status;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("export did not finish");
    };
    for (name, format) in [("export_values.sql", "sql"), ("export_values.csv", "csv")] {
        let status = export("export_values", name, format, false);
        assert_eq!(
            (status["state"].as_str(), status["rows"].as_str()),
            (Some("succeeded"), Some("5")),
            "{status}"
        );
    }
    // Explicit refusals, naming the reason; nothing is left behind.
    let structure = export("export_values", "structure.sql", "sql", true);
    assert_eq!(structure["error"], "UNSUPPORTED_CAPABILITY", "{structure}");
    let binary = export("Mixed \"Case\" values", "mixed.csv", "csv", false);
    assert!(
        binary["detail"].as_str().unwrap().contains("column b"),
        "{binary}"
    );
    let unknown = export("Mixed \"Case\" values", "mixed.sql", "sql", false);
    assert!(
        unknown["detail"].as_str().unwrap().contains("column ip"),
        "{unknown}"
    );
    let leftovers: Vec<String> = std::fs::read_dir(&exports)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| !name.starts_with("export_values."))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let duplicates = read(&first["handle"], "SELECT 1 AS value, 2 AS value").unwrap();
    assert_eq!(duplicates["payload"]["columns"][1]["name"], "value");
    let many = read(&first["handle"], "SELECT a.n FROM big a CROSS JOIN big b").unwrap()["payload"]
        .clone();
    assert_eq!(many["rows"].as_array().unwrap().len(), 1000);
    assert_eq!(many["truncated"], true);
    for sql in [
        "UPDATE big SET n=0",
        "SELECT pg_sleep(1)",
        "SELECT nextval('trust_seq')",
        "SELECT n::text FROM big",
        "SELECT * FROM big FOR UPDATE",
        "CALL fixture_procedure()",
    ] {
        assert_eq!(
            read(&first["handle"], sql).unwrap_err()["code"],
            "UNSUPPORTED_SQL",
            "{sql}"
        );
    }
    for sql in TRUST_VIEWS {
        let error = read(&first["handle"], sql).unwrap_err();
        assert_eq!(error["code"], "QUERY_REJECTED");
        assert_eq!(error["outcome"], "unknown");
        assert!(read(&first["handle"], "SELECT n FROM big LIMIT 1").is_ok());
    }
    // An indirect advisory lock is possible, and the discarded backend releases it.
    let lock = read(&first["handle"], "SELECT acquired FROM trust_lock_view").unwrap();
    assert_eq!(
        lock["payload"]["rows"][0][0],
        json!({"kind":"boolean","value":true})
    );
    // The server-side statement timeout stops a slow view even though the client gave up.
    let started = Instant::now();
    let slow = read(&first["handle"], "SELECT slept FROM slow_view").unwrap_err();
    assert!(["DEADLINE_EXCEEDED", "QUERY_REJECTED"].contains(&slow["code"].as_str().unwrap()));
    assert_eq!(slow["outcome"], "unknown");
    assert!(started.elapsed() < Duration::from_secs(5));
    report.insert("slowViewCode".into(), slow["code"].clone());
    tauri::async_runtime::block_on(async {
        let mut observer = fixture.connect("stoneql_fixture").await;
        assert_eq!(
            scalar_i64(&mut observer, "SELECT touches::bigint FROM trust_counter").await,
            0
        );
        assert_eq!(
            scalar_i64(
                &mut observer,
                "SELECT is_called::int::bigint FROM trust_seq"
            )
            .await,
            0
        );
        let start = Instant::now();
        loop {
            let active = scalar_i64(&mut observer, "SELECT count(*) FROM pg_stat_activity WHERE usename='stoneql_reader' AND state='active'").await;
            let locks = scalar_i64(
                &mut observer,
                "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND objid=4242",
            )
            .await;
            if active == 0 && locks == 0 {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "server statement or advisory lock outlived the discarded connection"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        observer.close().await.unwrap();
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
    assert!(read(&second["handle"], "SELECT n FROM big LIMIT 1").is_ok());
    // Reload bootstrap closes idle physical sessions and revokes old handles.
    let idle = invoke("connect_direct", config.clone()).unwrap()["payload"]["handle"].clone();
    let mut observer =
        tauri::async_runtime::block_on(async { fixture.connect("stoneql_fixture").await });
    tauri::async_runtime::block_on(settle_readers(&mut observer, 1));
    let reloaded = invoke("host_info", json!({"nonce":"pg_fixture_reload"})).unwrap();
    let next_session = reloaded["payload"]["sessionId"].clone();
    assert_ne!(next_session, session);
    tauri::async_runtime::block_on(settle_readers(&mut observer, 0));
    assert_eq!(read(&idle, "SELECT 1").unwrap_err()["code"], "FORBIDDEN");
    let mut next_config = config.clone();
    next_config["sessionId"] = next_session.clone();
    let restored: Vec<_> = (0..8)
        .map(|_| {
            invoke("connect_direct", next_config.clone()).unwrap()["payload"]["handle"].clone()
        })
        .collect();
    assert_eq!(
        invoke("connect_direct", next_config.clone()).unwrap_err()["code"],
        "CAPACITY"
    );
    for handle in restored {
        invoke(
            "disconnect_database",
            json!({"sessionId":next_session,"handle":handle}),
        )
        .unwrap();
    }
    tauri::async_runtime::block_on(async {
        settle_readers(&mut observer, 0).await;
        observer.close().await.unwrap();
    });
    for key in [
        "realDatabase",
        "tauriMockRuntime",
        "verifiedTls",
        "tlsFailuresClassified",
        "plaintextRejectedByServer",
        "selectOnlyPrivilege",
        "definerPositiveControls",
        "readOnlyTransactionRejects25006",
        "indirectWritesRejected",
        "exactValues",
        "enumAndUnknownTypes",
        "quotedIdentifiers",
        "defaultQueryLimits",
        "advisoryLockReleased",
        "serverStatementStopped",
        "leaseExpiryCleanup",
        "rendererReloadCleanup",
        "capacityReused",
        "disconnect",
    ] {
        report.insert(key.into(), json!(true));
    }
    println!("STONEQL_NATIVE_RESULT {}", Value::Object(report));
}
