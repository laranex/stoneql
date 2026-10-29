//! Actual Tauri command handlers (MockRuntime) against temporary SQLite files. The native picker
//! itself cannot be driven here; the trusted `authorize_sqlite_file` step it calls is used instead.
mod common;
use serde_json::{Value, json};
use sqlx::{Connection, Executor, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::PathBuf;
use tauri::{
    WebviewUrl, WebviewWindowBuilder,
    ipc::{CallbackFn, InvokeBody},
    test::{INVOKE_KEY, get_ipc_response, mock_builder},
    webview::InvokeRequest,
};

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn sqlite_grants_open_read_only_files_through_actual_commands() {
    let root =
        std::env::temp_dir().join(format!("stoneql-desktop-sqlite-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let temp = TempDir(std::fs::canonicalize(&root).unwrap());
    let file = temp.0.join("fixture.sqlite");
    tauri::async_runtime::block_on(async {
        let mut writer = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&file)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        writer
            .execute(
                r#"CREATE TABLE "odd ""name"""(id INTEGER PRIMARY KEY, big INTEGER, amount TEXT, note TEXT DEFAULT 'd', raw BLOB) STRICT;
INSERT INTO "odd ""name"""(id,big,amount,raw) VALUES (1,-9223372036854775808,'123456789012345678901234567890.4500',x'00ff');
INSERT INTO "odd ""name"""(id,big,amount,note) VALUES (2,NULL,'',NULL);"#,
            )
            .await
            .unwrap();
        writer.close().await.unwrap();
    });
    let journal = common::TempJournal::new();
    let app = stoneql_desktop::app_builder_with(mock_builder(), false, journal.services())
        .build(tauri::generate_context!())
        .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window,InvokeRequest{cmd:command.into(),callback:CallbackFn(0),error:CallbackFn(1),url:"tauri://localhost".parse().unwrap(),body:InvokeBody::Json(json!({"message":json!({"version":1,"requestId":"sqlite_request","payload":payload}).to_string()})),headers:Default::default(),invoke_key:INVOKE_KEY.into()}).map(|body|body.deserialize().unwrap())
    };
    let session =
        invoke("host_info", json!({"nonce":"sqlite_nonce"})).unwrap()["payload"]["sessionId"]
            .clone();
    let granted = stoneql_desktop::authorize_sqlite_file(app.handle(), &file).unwrap();
    let grant = granted.grant.clone().unwrap();
    assert_eq!(granted.file_name.as_deref(), Some("fixture.sqlite"));
    // Renderer requests can name grants, never paths.
    assert_eq!(
        invoke(
            "connect_sqlite",
            json!({"sessionId":session,"grant":grant,"path":file})
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    for forged in [
        file.to_str().unwrap(),
        "../fixture.sqlite",
        "00000000-0000-0000-0000-000000000001",
    ] {
        assert_eq!(
            invoke(
                "connect_sqlite",
                json!({"sessionId":session,"grant":forged})
            )
            .unwrap_err()["code"],
            "FORBIDDEN"
        );
    }
    // Lifecycle profile: 100 open/identity/close cycles; every handle releases its slot.
    let mut samples: Vec<u128> = (0..100)
        .map(|_| {
            let start = std::time::Instant::now();
            let handle = invoke("connect_sqlite", json!({"sessionId":session,"grant":grant}))
                .unwrap()["payload"]["handle"]
                .clone();
            invoke(
                "disconnect_database",
                json!({"sessionId":session,"handle":handle}),
            )
            .unwrap();
            start.elapsed().as_micros()
        })
        .collect();
    samples.sort_unstable();
    println!(
        "STONEQL_SQLITE_LIFECYCLE {}",
        json!({"samples":samples.len(),"p50Micros":samples[49],"p95Micros":samples[94],"maxMicros":samples[99]})
    );
    let connection = invoke("connect_sqlite", json!({"sessionId":session,"grant":grant})).unwrap()
        ["payload"]
        .clone();
    assert_eq!(connection["database"], "fixture.sqlite");
    assert!(
        connection["engineVersion"]
            .as_str()
            .unwrap()
            .starts_with("SQLite 3.")
    );
    let read = |sql: &str| {
        invoke(
            "read_only_query",
            json!({"sessionId":session,"handle":connection["handle"],"sql":sql}),
        )
    };
    let result = read(r#"SELECT big, amount, note, raw FROM "odd ""name""" ORDER BY id"#).unwrap()
        ["payload"]
        .clone();
    assert_eq!(result["columns"][0]["databaseType"], "INTEGER");
    assert_eq!(
        result["rows"][0],
        json!([{"kind":"integer","text":"-9223372036854775808"},{"kind":"text","text":"123456789012345678901234567890.4500"},{"kind":"text","text":"d"},{"kind":"binary","hex":"00ff"}])
    );
    assert_eq!(
        result["rows"][1],
        json!([{"kind":"null"},{"kind":"text","text":""},{"kind":"null"},{"kind":"null"}])
    );
    for sql in [
        "DELETE FROM t",
        "CALL routine()",
        "PRAGMA query_only=OFF",
        "ATTACH DATABASE 'x' AS y",
    ] {
        assert_eq!(read(sql).unwrap_err()["code"], "UNSUPPORTED_SQL", "{sql}");
    }
    assert_eq!(
        read("SELECT missing FROM nowhere").unwrap_err()["outcome"],
        "unknown"
    );
    // A renderer bootstrap revokes the old handle and every outstanding file grant.
    let next =
        invoke("host_info", json!({"nonce":"sqlite_reload"})).unwrap()["payload"]["sessionId"]
            .clone();
    assert_ne!(next, session);
    assert_eq!(
        invoke("connect_sqlite", json!({"sessionId":next,"grant":grant})).unwrap_err()["code"],
        "FORBIDDEN"
    );
    assert_eq!(
        invoke(
            "database_status",
            json!({"sessionId":next,"handle":connection["handle"]})
        )
        .unwrap()["payload"]["connected"],
        false
    );
    let regrant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
        .unwrap()
        .grant
        .unwrap();
    let reopened = invoke("connect_sqlite", json!({"sessionId":next,"grant":regrant})).unwrap()["payload"]["handle"].clone();
    invoke(
        "disconnect_database",
        json!({"sessionId":next,"handle":reopened}),
    )
    .unwrap();
    assert!(stoneql_desktop::authorize_sqlite_file(app.handle(), &temp.0).is_err());
    // The source file was never modified by any of the above.
    tauri::async_runtime::block_on(async {
        let mut checker = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(&file).read_only(true),
        )
        .await
        .unwrap();
        let count: i64 = sqlx::query_scalar(r#"SELECT count(*) FROM "odd ""name""""#)
            .fetch_one(&mut checker)
            .await
            .unwrap();
        assert_eq!(count, 2);
        checker.close().await.unwrap();
    });
}
