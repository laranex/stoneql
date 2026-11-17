//! M006 job and transaction commands through the actual Tauri handlers (MockRuntime) against a
//! temporary SQLite file. Network engines are covered by `npm run test:execution`.
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
/// The app's own origin: Tauri serves the frontend from `http://tauri.localhost` on Windows.
const LOCAL: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn read_write_jobs_and_leases_cross_ipc_with_strict_ownership() {
    let root = std::env::temp_dir().join(format!("stoneql-desktop-exec-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let temp = TempDir(std::fs::canonicalize(&root).unwrap());
    let file = temp.0.join("fixture.sqlite");
    let count = |sql: &'static str| {
        tauri::async_runtime::block_on(async {
            let mut observer = SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(&file)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
            let value: i64 = sqlx::query_scalar(sql)
                .fetch_one(&mut observer)
                .await
                .unwrap();
            observer.close().await.unwrap();
            value
        })
    };
    tauri::async_runtime::block_on(async {
        let mut writer = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&file)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        writer
            .execute("CREATE TABLE t(id INTEGER PRIMARY KEY, big INTEGER, note TEXT)")
            .await
            .unwrap();
        writer.close().await.unwrap();
    });
    // Execution tests enable the read-write gate through the trusted host-services injection
    // path only; the production builder keeps it off (see `read_write_is_refused_natively…`).
    let journal = common::TempJournal::new();
    let app = stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            read_write_sql: true,
            ..journal.services()
        },
    )
    .build(tauri::generate_context!())
    .unwrap();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window, InvokeRequest { cmd: command.into(), callback: CallbackFn(0), error: CallbackFn(1), url: LOCAL.parse().unwrap(), body: InvokeBody::Json(json!({"message": json!({"version": 1, "requestId": "exec_request", "payload": payload}).to_string()})), headers: Default::default(), invoke_key: INVOKE_KEY.into() })
            .map(|body| body.deserialize().unwrap())
    };
    let session =
        invoke("host_info", json!({"nonce": "exec_nonce"})).unwrap()["payload"]["sessionId"]
            .clone();
    let grant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
        .unwrap()
        .grant
        .unwrap();
    // Read-only remains the default; jobs need an explicit read-write handle.
    let read_only = invoke(
        "connect_sqlite",
        json!({"sessionId": session, "grant": grant}),
    )
    .unwrap()["payload"]
        .clone();
    assert_eq!(read_only["access"], "read_only");
    assert_eq!(
        invoke(
            "submit_job",
            json!({"sessionId": session, "handle": read_only["handle"], "sql": "SELECT 1"})
        )
        .unwrap_err()["code"],
        "UNSUPPORTED_CAPABILITY"
    );
    let connection = invoke(
        "connect_sqlite",
        json!({"sessionId": session, "grant": grant, "access": "read_write"}),
    )
    .unwrap()["payload"]
        .clone();
    assert_eq!(connection["access"], "read_write");
    let handle = connection["handle"].clone();
    // Strict decoding: unknown fields, nulls for omitted options and foreign sessions refused.
    for (payload, code) in [
        (
            json!({"sessionId": session, "handle": handle, "sql": "SELECT 1", "extra": true}),
            "INVALID_REQUEST",
        ),
        (
            json!({"sessionId": session, "handle": handle, "sql": "SELECT 1", "leaseId": null}),
            "INVALID_REQUEST",
        ),
        (
            json!({"sessionId": "other", "handle": handle, "sql": "SELECT 1"}),
            "FORBIDDEN",
        ),
        (
            json!({"sessionId": session, "handle": handle, "sql": "SELECT 1", "deadlineSeconds": 301}),
            "INVALID_REQUEST",
        ),
        (
            json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (9, 0, ''); COMMIT"}),
            "UNSUPPORTED_SQL",
        ),
    ] {
        assert_eq!(invoke("submit_job", payload).unwrap_err()["code"], code);
    }
    assert_eq!(count("SELECT count(*) FROM t"), 0);
    let fetch_all = |job: &Value| {
        let mut batches = Vec::new();
        loop {
            let update = invoke(
                "fetch_job",
                json!({"sessionId": session, "jobId": job, "waitMs": 1000}),
            )
            .unwrap()["payload"]
                .clone();
            if !update["batch"].is_null() {
                batches.push(update["batch"].clone());
            }
            if update["done"] == true {
                return (batches, update);
            }
        }
    };
    let accepted = invoke("submit_job", json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (1, 9223372036854775807, NULL); SELECT big, note FROM t; INSERT INTO t VALUES (1, 0, '')"})).unwrap()["payload"].clone();
    assert_eq!(accepted["statementCount"], 3);
    let (batches, update) = fetch_all(&accepted["jobId"]);
    assert_eq!(update["outcome"], "failed");
    assert_eq!(update["cancellation"], "none");
    assert_eq!(
        update["statements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["succeeded", "succeeded", "failed"]
    );
    assert_eq!(update["statements"][2]["failure"]["constraint"], "unique");
    assert_eq!(
        batches[0]["rows"],
        json!([[{"kind": "integer", "text": "9223372036854775807"}, {"kind": "null"}]])
    );
    // A finished job is released after its final page.
    assert_eq!(
        invoke(
            "fetch_job",
            json!({"sessionId": session, "jobId": accepted["jobId"]})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    // Cancel: requested and confirmed, effects unknown; nothing after it is sent.
    let slow = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT max(x) FROM c; INSERT INTO t VALUES (2, 0, '')";
    let job = invoke(
        "submit_job",
        json!({"sessionId": session, "handle": handle, "sql": slow}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    assert_eq!(
        invoke(
            "read_only_query",
            json!({"sessionId": session, "handle": handle, "sql": "SELECT 1"})
        )
        .unwrap_err()["code"],
        "BUSY"
    );
    assert_eq!(
        invoke(
            "submit_job",
            json!({"sessionId": session, "handle": handle, "sql": "SELECT 1"})
        )
        .unwrap_err()["code"],
        "BUSY"
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(
        invoke("cancel_job", json!({"sessionId": session, "jobId": job})).unwrap()["payload"]["cancelRequested"],
        true
    );
    let (_, update) = fetch_all(&job);
    assert_eq!(
        (update["outcome"].as_str(), update["cancellation"].as_str()),
        (Some("unknown"), Some("confirmed"))
    );
    assert_eq!(update["statements"][1]["status"], "not_run");
    assert_eq!(count("SELECT count(*) FROM t WHERE id = 2"), 0);
    // Transactions: commit and rollback are reported only as the server confirmed them.
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": session, "handle": handle}),
    )
    .unwrap()["payload"]
        .clone();
    assert_eq!(lease["idleSeconds"], 60);
    let lease = lease["leaseId"].clone();
    let job = invoke("submit_job", json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (3, 0, '')", "leaseId": lease})).unwrap()["payload"]["jobId"].clone();
    let (_, update) = fetch_all(&job);
    assert_eq!(
        (update["outcome"].as_str(), update["lease"].as_str()),
        (Some("succeeded"), Some("active"))
    );
    assert_eq!(update["statements"][0]["inTransaction"], true);
    assert_eq!(
        invoke(
            "end_transaction",
            json!({"sessionId": session, "leaseId": lease, "action": "commit", "extra": 1})
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST"
    );
    let end = invoke(
        "end_transaction",
        json!({"sessionId": session, "leaseId": lease, "action": "commit"}),
    )
    .unwrap()["payload"]
        .clone();
    assert_eq!(
        end,
        json!({"outcome": "committed", "reason": "requested", "commitSent": true, "rollbackSent": false})
    );
    // B-2: asking again (for example after a lost reply) returns the recorded outcome; nothing
    // is sent again, whatever action is named.
    for action in ["commit", "rollback"] {
        assert_eq!(
            invoke(
                "end_transaction",
                json!({"sessionId": session, "leaseId": lease, "action": action}),
            )
            .unwrap()["payload"],
            end
        );
    }
    assert_eq!(count("SELECT count(*) FROM t WHERE id = 3"), 1);
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": session, "handle": handle}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    let job = invoke("submit_job", json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (4, 0, '')", "leaseId": lease})).unwrap()["payload"]["jobId"].clone();
    fetch_all(&job);
    let end = invoke(
        "end_transaction",
        json!({"sessionId": session, "leaseId": lease, "action": "rollback"}),
    )
    .unwrap()["payload"]
        .clone();
    assert_eq!(
        end,
        json!({"outcome": "rolled_back", "reason": "requested", "commitSent": false, "rollbackSent": true})
    );
    assert_eq!(count("SELECT count(*) FROM t WHERE id = 4"), 0);
    // V-1/B-3: disconnecting with an open transaction ends it; its recorded outcome stays
    // queryable by lease id (never FORBIDDEN).
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": session, "handle": handle}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    let job = invoke("submit_job", json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (6, 0, '')", "leaseId": lease})).unwrap()["payload"]["jobId"].clone();
    fetch_all(&job);
    invoke(
        "disconnect_database",
        json!({"sessionId": session, "handle": handle}),
    )
    .unwrap();
    assert_eq!(
        invoke(
            "end_transaction",
            json!({"sessionId": session, "leaseId": lease, "action": "commit"}),
        )
        .unwrap()["payload"],
        json!({"outcome": "rolled_back", "reason": "closed", "commitSent": false, "rollbackSent": true})
    );
    assert_eq!(count("SELECT count(*) FROM t WHERE id = 6"), 0);
    let handle = invoke(
        "connect_sqlite",
        json!({"sessionId": session, "grant": grant, "access": "read_write"}),
    )
    .unwrap()["payload"]["handle"]
        .clone();
    // A renderer reload ends open leases with a confirmed rollback and invalidates ids.
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": session, "handle": handle}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    let job = invoke("submit_job", json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (5, 0, '')", "leaseId": lease})).unwrap()["payload"]["jobId"].clone();
    fetch_all(&job);
    let next =
        invoke("host_info", json!({"nonce": "exec_nonce_2"})).unwrap()["payload"]["sessionId"]
            .clone();
    assert_ne!(next, session);
    assert_eq!(
        invoke(
            "submit_job",
            json!({"sessionId": next, "handle": handle, "sql": "SELECT 1"})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    assert_eq!(count("SELECT count(*) FROM t WHERE id = 5"), 0);
    // The reload's recorded outcome stays queryable by the (unguessable) lease id.
    assert_eq!(
        invoke(
            "end_transaction",
            json!({"sessionId": next, "leaseId": lease, "action": "commit"}),
        )
        .unwrap()["payload"],
        json!({"outcome": "rolled_back", "reason": "closed", "commitSent": false, "rollbackSent": true})
    );
}
