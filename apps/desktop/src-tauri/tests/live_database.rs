//! Explicitly opted-in, owning-fixture IPC integration. Never reads saved profiles or arbitrary URLs.
mod common;
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
/// The app's own origin: Tauri serves the frontend from `http://tauri.localhost` on Windows.
const LOCAL: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
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
    let journal = common::TempJournal::new();
    let app = stoneql_desktop::app_builder_with(mock_builder(), false, journal.services())
        .build(tauri::generate_context!())
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window,InvokeRequest{cmd:command.into(),callback:CallbackFn(0),error:CallbackFn(1),url:LOCAL.parse().unwrap(),body:InvokeBody::Json(json!({"message":json!({"version":1,"requestId":"fixture_request","payload":payload}).to_string()})),headers:Default::default(),invoke_key:INVOKE_KEY.into()}).map(|body|body.deserialize().unwrap())
    };
    let session =
        invoke("host_info", json!({"nonce":"fixture_nonce"})).unwrap()["payload"]["sessionId"]
            .clone();
    let config = json!({"sessionId":session,"engine":engine,"host":"127.0.0.1","port":port,"username":"stoneql_reader","password":password,"database":"stoneql_m001_fixture","rootCertificatePem":ca});
    let first = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
    // Use TLS, both loopback modes and the remote requirement. The fixture listens on
    // 127.0.0.1; `stoneql_plain` has no REQUIRE SSL, `stoneql_reader` does.
    let tls_mode = |user: &str, host: &str, tls: bool, ca: Option<&str>| json!({"sessionId":session,"engine":engine,"host":host,"port":port,"username":user,"password":password,"database":"stoneql_m001_fixture","rootCertificatePem":ca,"tls":tls});
    let close = |handle: &Value| {
        invoke(
            "disconnect_database",
            json!({"sessionId":session,"handle":handle}),
        )
        .unwrap();
    };
    // Switching databases: reads and jobs follow the switch on the same handle; an unknown
    // database or one the account may not use is refused with a clear code and the session
    // stays where it was; an open transaction blocks the switch.
    {
        let cell = |handle: &Value, sql: &str| {
            invoke(
                "read_only_query",
                json!({"sessionId":session,"handle":handle,"sql":sql}),
            )
            .unwrap()["payload"]["rows"][0][0]["text"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let switch = |handle: &Value, database: &str| {
            invoke(
                "switch_database",
                json!({"sessionId":session,"handle":handle,"database":database}),
            )
        };
        let mut writer_config = config.clone();
        writer_config["username"] = json!("stoneql_writer");
        writer_config["access"] = json!("read_write");
        let writer = invoke("connect_direct", writer_config).unwrap()["payload"]["handle"].clone();
        assert_eq!(
            cell(&writer, "SELECT label FROM switch_marker WHERE id = 1"),
            "default"
        );
        let switched = switch(&writer, "stoneql_switch_b").unwrap();
        assert_eq!(switched["payload"]["database"], "stoneql_switch_b");
        assert_eq!(cell(&writer, "SELECT DATABASE()"), "stoneql_switch_b");
        assert_eq!(
            cell(&writer, "SELECT label FROM switch_marker WHERE id = 1"),
            "other"
        );
        let job = |sql: &str| {
            let accepted = invoke(
                "submit_job",
                json!({"sessionId":session,"handle":writer,"sql":sql}),
            )
            .unwrap();
            let id = accepted["payload"]["jobId"].clone();
            loop {
                let update = invoke(
                    "fetch_job",
                    json!({"sessionId":session,"jobId":id,"waitMs":1000}),
                )
                .unwrap()["payload"]
                    .clone();
                if update["done"] == json!(true) {
                    return update;
                }
            }
        };
        assert_eq!(
            job("INSERT INTO switch_marker VALUES (2, 'job')")["outcome"],
            "succeeded"
        );
        assert_eq!(
            cell(
                &writer,
                "SELECT COUNT(*) FROM stoneql_switch_b.switch_marker"
            ),
            "2"
        );
        assert_eq!(
            job("DELETE FROM switch_marker WHERE id = 2")["outcome"],
            "succeeded"
        );
        // Without global privileges the server answers 1044 for a missing database too, so
        // the account cannot probe which databases exist.
        assert_eq!(
            switch(&writer, "stoneql_missing_db").unwrap_err()["code"],
            "PERMISSION_DENIED"
        );
        assert_eq!(cell(&writer, "SELECT DATABASE()"), "stoneql_switch_b");
        // The reader may not use stoneql_switch_b: denied, and it stays on its database.
        assert_eq!(
            switch(&first["handle"], "stoneql_switch_b").unwrap_err()["code"],
            "PERMISSION_DENIED"
        );
        assert_eq!(
            cell(&first["handle"], "SELECT DATABASE()"),
            "stoneql_m001_fixture"
        );
        // An open transaction blocks the switch; after it ends the switch works.
        let lease = invoke(
            "begin_transaction",
            json!({"sessionId":session,"handle":writer}),
        )
        .unwrap()["payload"]["leaseId"]
            .clone();
        assert_eq!(
            switch(&writer, "stoneql_m001_fixture").unwrap_err()["code"],
            "BUSY"
        );
        invoke(
            "end_transaction",
            json!({"sessionId":session,"leaseId":lease,"action":"rollback"}),
        )
        .unwrap();
        assert_eq!(
            switch(&writer, "stoneql_m001_fixture").unwrap()["payload"]["database"],
            "stoneql_m001_fixture"
        );

        // Schema edits as the renderer builds them. The restricted writer (no ALTER/CREATE) is
        // refused with `denied`; a privileged account creates a database and adds, renames,
        // redefines and drops columns; restating a column's own SHOW CREATE TABLE definition
        // changes nothing.
        let run = |handle: &Value, sql: &str| {
            let accepted = invoke(
                "submit_job",
                json!({"sessionId":session,"handle":handle,"sql":sql}),
            )
            .unwrap();
            let id = accepted["payload"]["jobId"].clone();
            let mut rows = Vec::new();
            loop {
                let update = invoke(
                    "fetch_job",
                    json!({"sessionId":session,"jobId":id,"waitMs":1000}),
                )
                .unwrap()["payload"]
                    .clone();
                if let Some(batch) = update["batch"].as_object() {
                    rows.extend(batch["rows"].as_array().unwrap().iter().cloned());
                }
                if update["done"] == json!(true) {
                    return (update, rows);
                }
            }
        };
        let text = |cell: &Value| match cell["kind"].as_str() {
            Some("binary") => {
                let hex = cell["hex"].as_str().unwrap();
                String::from_utf8(
                    (0..hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                        .collect(),
                )
                .unwrap()
            }
            _ => cell["text"].as_str().unwrap().to_owned(),
        };
        let mut admin_config = config.clone();
        admin_config["username"] = json!("stoneql_admin");
        admin_config["access"] = json!("read_write");
        admin_config["database"] = json!("stoneql_switch_b");
        let admin = invoke("connect_direct", admin_config).unwrap()["payload"]["handle"].clone();
        let show = |handle: &Value| {
            let (update, rows) = run(handle, "SHOW CREATE TABLE `ddl_probe`;");
            assert_eq!(update["outcome"], "succeeded", "{update}");
            text(&rows[0][1])
        };
        assert_eq!(run(&admin, "CREATE TABLE `ddl_probe` (`id` int NOT NULL AUTO_INCREMENT PRIMARY KEY, `label` varchar(40) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL DEFAULT 'x' COMMENT 'c;d', `changed` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP);").0["outcome"], "succeeded");
        let before = show(&admin);
        // Restricted: ALTER and CREATE DATABASE are refused as denied; nothing changes.
        switch(&writer, "stoneql_switch_b").unwrap();
        for sql in [
            "ALTER TABLE `ddl_probe` ADD COLUMN `blocked` INT NULL;",
            "CREATE DATABASE `stoneql_writer_db`;",
        ] {
            let (update, _) = run(&writer, sql);
            assert_eq!(update["outcome"], "failed", "{sql}");
            assert_eq!(
                update["statements"][0]["failure"]["denied"], true,
                "{sql}: {update}"
            );
        }
        assert_eq!(show(&admin), before);
        // Restating each column's exact definition (from SHOW CREATE TABLE) is a no-op.
        let definition = |create: &str, column: &str| {
            let prefix = format!("  `{column}` ");
            create
                .lines()
                .find_map(|line| line.strip_prefix(&prefix))
                .unwrap()
                .trim_end_matches(',')
                .to_owned()
        };
        for column in ["id", "label", "changed"] {
            let sql = format!(
                "ALTER TABLE `ddl_probe` CHANGE COLUMN `{column}` `{column}` {};",
                definition(&before, column)
            );
            assert_eq!(run(&admin, &sql).0["outcome"], "succeeded", "{sql}");
        }
        assert_eq!(
            show(&admin),
            before,
            "restated definitions changed the table"
        );
        // Add, rename (definition kept), redefine and drop.
        assert_eq!(
            run(
                &admin,
                "ALTER TABLE `ddl_probe` ADD COLUMN `note` VARCHAR(20) NULL AFTER `id`;"
            )
            .0["outcome"],
            "succeeded"
        );
        assert_eq!(
            run(
                &admin,
                "ALTER TABLE `ddl_probe` RENAME COLUMN `label` TO `title`;"
            )
            .0["outcome"],
            "succeeded"
        );
        let renamed = show(&admin);
        assert_eq!(definition(&renamed, "title"), definition(&before, "label"));
        assert_eq!(
            run(
                &admin,
                "ALTER TABLE `ddl_probe` CHANGE COLUMN `note` `note` VARCHAR(80) NULL;"
            )
            .0["outcome"],
            "succeeded"
        );
        assert!(show(&admin).contains("`note` varchar(80)"));
        assert_eq!(
            run(&admin, "ALTER TABLE `ddl_probe` DROP COLUMN `note`;").0["outcome"],
            "succeeded"
        );
        assert!(!show(&admin).contains("`note`"));
        // Indexes: the restricted writer is denied; the privileged account adds a unique
        // index with columns in the chosen order and drops it.
        let (update, _) = run(
            &writer,
            "ALTER TABLE `ddl_probe` ADD INDEX `blocked_ix` (`title`);",
        );
        assert_eq!(
            update["statements"][0]["failure"]["denied"], true,
            "{update}"
        );
        assert_eq!(
            run(
                &admin,
                "ALTER TABLE `ddl_probe` ADD UNIQUE INDEX `title_ix` (`title`, `id`);"
            )
            .0["outcome"],
            "succeeded"
        );
        assert!(show(&admin).contains("UNIQUE KEY `title_ix` (`title`,`id`)"));
        assert_eq!(
            run(&admin, "ALTER TABLE `ddl_probe` DROP INDEX `title_ix`;").0["outcome"],
            "succeeded"
        );
        assert!(!show(&admin).contains("title_ix"));
        // Create database, then switch to it as the account that made it.
        assert_eq!(
            run(
                &admin,
                "CREATE DATABASE `stoneql_created_db` CHARACTER SET utf8mb4;"
            )
            .0["outcome"],
            "succeeded"
        );
        assert_eq!(
            switch(&admin, "stoneql_created_db").unwrap()["payload"]["database"],
            "stoneql_created_db"
        );
        close(&admin);
        close(&writer);
    }
    // Remote with TLS off: refused before any network or secret work.
    let started = std::time::Instant::now();
    assert_eq!(
        invoke(
            "connect_direct",
            tls_mode("stoneql_plain", "192.0.2.1", false, None)
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    // A CA with TLS off is refused rather than ignored.
    assert_eq!(
        invoke(
            "connect_direct",
            tls_mode("stoneql_plain", "127.0.0.1", false, Some(&ca))
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    // The fixture server starts with require_secure_transport=ON: plaintext is refused by the
    // server and reported as ENCRYPTED_LOGIN_REQUIRED, never retried with or without TLS.
    assert_eq!(
        invoke(
            "connect_direct",
            tls_mode("stoneql_plain", "127.0.0.1", false, None)
        )
        .unwrap_err()["code"],
        "ENCRYPTED_LOGIN_REQUIRED"
    );
    // The fixture administrator (verified TLS) relaxes it for the plaintext cases only.
    let secure_transport = |on: bool| {
        let admin = options.clone().username("root");
        tauri::async_runtime::block_on(async {
            let mut connection = MySqlConnection::connect_with(&admin)
                .await
                .map_err(|_| "fixture admin TLS failed")
                .unwrap();
            sqlx::raw_sql(if on {
                "SET GLOBAL require_secure_transport = ON"
            } else {
                "SET GLOBAL require_secure_transport = OFF"
            })
            .execute(&mut connection)
            .await
            .map_err(|_| "fixture admin setting failed")
            .unwrap();
            connection.close().await.unwrap();
        });
    };
    secure_transport(false);
    // Loopback, TLS off, first sign-in: MariaDB's default mysql_native_password works over
    // plaintext; MySQL's caching_sha2_password needs an encrypted first sign-in in this build.
    let cold = invoke(
        "connect_direct",
        tls_mode("stoneql_plain", "127.0.0.1", false, None),
    );
    if engine == "mysql" {
        assert_eq!(cold.unwrap_err()["code"], "ENCRYPTED_LOGIN_REQUIRED");
    } else {
        close(&cold.unwrap()["payload"]["handle"]);
    }
    // Loopback, TLS on: the certificate and host name are verified (the fixture CA).
    let verified = invoke(
        "connect_direct",
        tls_mode("stoneql_plain", "127.0.0.1", true, Some(&ca)),
    )
    .unwrap()["payload"]
        .clone();
    close(&verified["handle"]);
    // Loopback, TLS off, after that sign-in: works on both engines and returns data.
    let plain = invoke(
        "connect_direct",
        tls_mode("stoneql_plain", "127.0.0.1", false, None),
    )
    .unwrap()["payload"]
        .clone();
    let rows = invoke(
        "read_only_query",
        json!({"sessionId":session,"handle":plain["handle"],"sql":"SELECT id FROM raw_values ORDER BY id"}),
    )
    .unwrap();
    assert!(!rows["payload"]["rows"].as_array().unwrap().is_empty());
    close(&plain["handle"]);
    // Plaintext is really plaintext: an account that requires SSL is refused.
    assert_eq!(
        invoke(
            "connect_direct",
            tls_mode("stoneql_reader", "127.0.0.1", false, None)
        )
        .unwrap_err()["code"],
        "CONNECTION_UNAVAILABLE"
    );
    // TLS on never falls back: system trust roots do not trust the fixture CA, so this fails
    // although the same account would sign in over plaintext.
    assert_eq!(
        invoke(
            "connect_direct",
            tls_mode("stoneql_plain", "127.0.0.1", true, None)
        )
        .unwrap_err()["code"],
        "CONNECTION_UNAVAILABLE"
    );
    // TLS on with the right CA but a host name the certificate does not name: refused.
    assert_eq!(
        invoke(
            "connect_direct",
            tls_mode("stoneql_plain", "localhost", true, Some(&ca))
        )
        .unwrap_err()["code"],
        "CONNECTION_UNAVAILABLE"
    );
    secure_transport(true);
    let second = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
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
    // M003/M006: the renderer's live catalog and browse statements (apps/frontend/src/services/
    // liveSchema.ts) over verified TLS through the real handler. MySQL 8 returns some catalog
    // columns as binary cells; the renderer decodes those as UTF-8, mirrored here.
    let catalog_text = |cell: &Value| -> String {
        match cell["kind"].as_str().unwrap() {
            "text" => cell["text"].as_str().unwrap().to_owned(),
            "binary" => {
                let hex = cell["hex"].as_str().unwrap();
                let bytes: Vec<u8> = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect();
                String::from_utf8(bytes).unwrap()
            }
            "null" => String::new(),
            other => panic!("unexpected catalog cell {other}"),
        }
    };
    let tables = read(&first["handle"], "SELECT TABLE_NAME AS name, TABLE_TYPE AS kind FROM information_schema.TABLES WHERE TABLE_SCHEMA = 'stoneql_m001_fixture' ORDER BY TABLE_NAME;").unwrap();
    let listed: Vec<(String, String)> = tables["payload"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (catalog_text(&row[0]), catalog_text(&row[1])))
        .collect();
    assert!(
        listed.contains(&("raw_values".into(), "BASE TABLE".into())),
        "{listed:?}"
    );
    let columns = read(&first["handle"], "SELECT COLUMN_NAME AS name, COLUMN_TYPE AS type, IS_NULLABLE AS nullable, COLUMN_KEY AS ckey, COLUMN_DEFAULT AS dflt, EXTRA AS extra FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = 'stoneql_m001_fixture' AND TABLE_NAME = 'raw_values' ORDER BY ORDINAL_POSITION;").unwrap();
    let id = &columns["payload"]["rows"][0];
    assert_eq!(catalog_text(&id[0]), "id");
    assert_eq!(catalog_text(&id[3]), "PRI");
    let page = read(&first["handle"], "SELECT * FROM `raw_values`\nWHERE LOWER(COALESCE(`id`, '')) LIKE '%1%' ESCAPE '!'\nORDER BY `id` DESC\nLIMIT 0, 25;").unwrap();
    assert!(!page["payload"]["rows"].as_array().unwrap().is_empty());
    let count = read(&first["handle"], "SELECT COUNT(*) AS total FROM `raw_values`\nWHERE LOWER(COALESCE(`id`, '')) LIKE '%1%' ESCAPE '!';").unwrap();
    assert_eq!(count["payload"]["rows"][0][0]["kind"], "integer");
    // Indexes and foreign keys of a child table (fixture seed `fk_child`).
    let indexes = read(&first["handle"], r#"SELECT INDEX_NAME AS name, NON_UNIQUE AS nonunique, COLUMN_NAME AS col, INDEX_TYPE AS type FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = 'stoneql_m001_fixture' AND TABLE_NAME = 'fk_child' ORDER BY INDEX_NAME, SEQ_IN_INDEX;"#).unwrap();
    let names: Vec<String> = indexes["payload"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| catalog_text(&row[0]))
        .collect();
    assert!(
        names.contains(&"PRIMARY".into()) && names.contains(&"fk_parent_idx".into()),
        "{names:?}"
    );
    let keys = read(&first["handle"], r#"SELECT k.CONSTRAINT_NAME AS name, k.COLUMN_NAME AS col, k.REFERENCED_TABLE_NAME AS ref_table, k.REFERENCED_COLUMN_NAME AS ref_col, r.DELETE_RULE AS on_delete, r.UPDATE_RULE AS on_update FROM information_schema.KEY_COLUMN_USAGE k LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS r ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME WHERE k.TABLE_SCHEMA = 'stoneql_m001_fixture' AND k.TABLE_NAME = 'fk_child' AND k.REFERENCED_TABLE_NAME IS NOT NULL ORDER BY k.CONSTRAINT_NAME, k.ORDINAL_POSITION;"#).unwrap();
    let key = &keys["payload"]["rows"][0];
    assert_eq!(
        [&key[0], &key[1], &key[2], &key[3]].map(catalog_text),
        ["fk_child_parent", "parent", "raw_values", "id"]
    );
    // MySQL shows the rule to the SELECT-only reader; MariaDB hides REFERENTIAL_CONSTRAINTS
    // (the LEFT JOIN keeps the key and the renderer says the rule is not visible).
    let rule = catalog_text(&key[4]);
    assert!(
        rule == "CASCADE" || (engine == "mariadb" && rule.is_empty()),
        "{engine}: {rule}"
    );
    // M007: streaming exports into native grants; the runner restores them independently.
    let exports = std::path::PathBuf::from(std::env::var("STONEQL_EXPORT_DIR").unwrap());
    let export = |table: &str, name: &str, format: &str, structure: bool| -> Value {
        let grant = stoneql_desktop::authorize_export_file(app.handle(), &exports.join(name))
            .unwrap()
            .grant
            .unwrap();
        let accepted = invoke(
            "start_export",
            json!({"sessionId":session,"handle":first["handle"],"grant":grant,"tables":[table],"format":format,"structure":structure,"data":true}),
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
    let sql = export("export_values", "export_values.sql", "sql", true);
    assert_eq!(
        (sql["state"].as_str(), sql["rows"].as_str()),
        (Some("succeeded"), Some("5")),
        "{sql}"
    );
    let csv = export("export_values", "export_values.csv", "csv", false);
    assert_eq!(
        (csv["state"].as_str(), csv["rows"].as_str()),
        (Some("succeeded"), Some("5")),
        "{csv}"
    );
    // Binary data has no exact CSV form: refused, naming the column, and nothing is left behind.
    let binary = export("raw_values", "raw_values.csv", "csv", false);
    assert_eq!(binary["state"], "failed", "{binary}");
    assert_eq!(binary["error"], "UNSUPPORTED_CAPABILITY");
    assert!(
        binary["detail"].as_str().unwrap().contains("column b"),
        "{binary}"
    );
    let leftovers: Vec<String> = std::fs::read_dir(&exports)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("raw_values") || name.ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let duplicates = read(
        &first["handle"],
        "SELECT s AS value,u AS value FROM raw_values ORDER BY id",
    )
    .unwrap();
    assert_eq!(duplicates["payload"]["columns"][0]["name"], "value");
    assert_eq!(duplicates["payload"]["columns"][1]["name"], "value");
    // Omitted limits must apply the runtime maximums exactly like explicit 1000 rows / 1 MiB.
    let read_default = |sql: &str| {
        invoke(
            "read_only_query",
            json!({"sessionId":session,"handle":first["handle"],"sql":sql}),
        )
        .unwrap()["payload"]
            .clone()
    };
    let read_maximum = |sql: &str| {
        invoke(
            "read_only_query",
            json!({"sessionId":session,"handle":first["handle"],"sql":sql,"maxRows":1000,"maxBytes":1048576}),
        )
        .unwrap()["payload"]
            .clone()
    };
    let product = (1..=11)
        .map(|alias| format!("raw_values t{alias}"))
        .collect::<Vec<_>>()
        .join(" CROSS JOIN ");
    let many_rows = format!("SELECT t1.id FROM {product}");
    let rows = read_default(&many_rows);
    assert_eq!(rows["rows"].as_array().unwrap().len(), 1000);
    assert_eq!(rows["truncated"], true);
    assert_eq!(rows, read_maximum(&many_rows));
    let wide = "w".repeat(1500);
    let many_bytes = format!("SELECT '{wide}' AS payload FROM {product}");
    let bytes = read_default(&many_bytes);
    let serialized = serde_json::to_vec(&bytes).unwrap().len();
    assert_eq!(bytes["truncated"], true);
    assert!(bytes["rows"].as_array().unwrap().len() < 1000);
    assert!(serialized <= 1_048_576 && serialized + 1600 > 1_048_576);
    assert_eq!(bytes, read_maximum(&many_bytes));
    assert_eq!(
        read(&first["handle"], "UPDATE raw_values SET id=9").unwrap_err()["code"],
        "UNSUPPORTED_SQL"
    );
    assert_eq!(
        read(&first["handle"], "SELECT payload FROM raw_large").unwrap_err()["code"],
        "PACKET_TOO_LARGE"
    );
    assert!(read(&first["handle"], "SELECT id FROM raw_values").is_ok());
    // A6 result-metadata limit (vendored driver): 4,096 columns pass; 4,097 are refused by the
    // driver before it reserves descriptor storage (short names keep the runtime's 1 MiB JSON
    // check far away), and the next query on a fresh connection works.
    let columns = |count: usize| {
        let list: Vec<String> = (1..=count).map(|n| format!("{n} AS c{n}")).collect();
        format!("SELECT {}", list.join(","))
    };
    let widest = read(&first["handle"], &columns(4096)).unwrap();
    assert_eq!(widest["payload"]["columns"].as_array().unwrap().len(), 4096);
    assert_eq!(widest["payload"]["rows"][0][4095]["text"], "4096");
    assert_eq!(
        read(&first["handle"], &columns(4097)).unwrap_err()["code"],
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
    // Reload bootstrap must close idle physical sessions and invalidate old tokens/handles.
    let idle = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
    let mut observer = tauri::async_runtime::block_on(async {
        MySqlConnection::connect_with(&options)
            .await
            .map_err(|_| "reload observer TLS failed")
            .unwrap()
    });
    let count_readers = |connection: &mut MySqlConnection, expected: i64| {
        tauri::async_runtime::block_on(async {
            let start = std::time::Instant::now();
            loop {
                let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER='stoneql_reader'")
                    .fetch_one(&mut *connection).await.expect("owned session inspection");
                if count == expected {
                    break;
                }
                assert!(
                    start.elapsed() < std::time::Duration::from_secs(3),
                    "owned session count did not settle"
                );
            }
        });
    };
    count_readers(&mut observer, 2);
    let retry = invoke("host_info", json!({"nonce":"fixture_nonce"})).unwrap();
    assert_eq!(retry["payload"]["sessionId"], session);
    assert!(read(&idle["handle"], "SELECT id FROM raw_values").is_ok());
    // Restore an idle physical connection after the successful query discarded its socket.
    let idle_again = invoke("connect_direct", config.clone()).unwrap()["payload"].clone();
    count_readers(&mut observer, 2);
    let reloaded = invoke("host_info", json!({"nonce":"fixture_reload_nonce"})).unwrap();
    let next_session = reloaded["payload"]["sessionId"].clone();
    assert_ne!(next_session, session);
    count_readers(&mut observer, 1);
    assert_eq!(
        read(&idle_again["handle"], "SELECT 1").unwrap_err()["code"],
        "FORBIDDEN"
    );
    let status = invoke(
        "database_status",
        json!({"sessionId":next_session,"handle":idle_again["handle"]}),
    )
    .unwrap();
    assert_eq!(status["payload"]["connected"], false);
    let mut next_config = config.clone();
    next_config["sessionId"] = next_session.clone();
    let mut restored = Vec::new();
    for _ in 0..8 {
        restored.push(
            invoke("connect_direct", next_config.clone()).unwrap()["payload"]["handle"].clone(),
        );
    }
    for handle in restored {
        invoke(
            "disconnect_database",
            json!({"sessionId":next_session,"handle":handle}),
        )
        .unwrap();
    }
    count_readers(&mut observer, 1);
    tauri::async_runtime::block_on(observer.close())
        .map_err(|_| "reload observer close failed")
        .unwrap();
    println!(
        "STONEQL_NATIVE_RESULT {{\"realDatabase\":true,\"tauriMockRuntime\":true,\"strictIpc\":true,\"independentHandles\":true,\"exactValues\":true,\"readOnlyRefusal\":true,\"packetRecovery\":true,\"indirectWritesRejected\":true,\"definerPositiveControls\":true,\"nonTransactionalWritesRejected\":true,\"indirectLockReleased\":true,\"rendererReloadCleanup\":true,\"bootstrapRetryIdempotent\":true,\"reloadCapacityReused\":true,\"disconnect\":true,\"defaultQueryLimits\":true,\"tlsModes\":true}}"
    );
}
