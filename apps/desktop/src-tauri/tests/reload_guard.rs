//! M006 blocker 2: the native reload/navigation guard decides from the runtime registry only.
//! MockRuntime never runs navigation handlers, so these tests drive the decision function the
//! guard calls against real host state (actual IPC handlers, temporary SQLite file). The hook
//! itself is exercised in a real WKWebView by `real_window_reload`.
use serde_json::{Value, json};
use sqlx::{Connection, Executor, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::{Path, PathBuf};
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
fn sqlite(file: &Path, sql: &'static str) -> i64 {
    tauri::async_runtime::block_on(async {
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(file)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        let value = if sql.starts_with("SELECT") {
            sqlx::query_scalar(sql)
                .fetch_one(&mut connection)
                .await
                .unwrap()
        } else {
            connection.execute(sql).await.unwrap();
            0
        };
        connection.close().await.unwrap();
        value
    })
}

#[test]
fn reload_is_refused_natively_while_a_lease_or_job_is_held_and_allowed_after_rotation() {
    let root = std::env::temp_dir().join(format!("stoneql-reload-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let temp = TempDir(std::fs::canonicalize(&root).unwrap());
    let file = temp.0.join("fixture.sqlite");
    sqlite(&file, "CREATE TABLE t(id INTEGER PRIMARY KEY)");
    let app = rw_app();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = |command: &str, payload: Value| -> Result<Value, Value> {
        get_ipc_response(&window, InvokeRequest { cmd: command.into(), callback: CallbackFn(0), error: CallbackFn(1), url: "tauri://localhost".parse().unwrap(), body: InvokeBody::Json(json!({"message": json!({"version": 1, "requestId": "reload_request", "payload": payload}).to_string()})), headers: Default::default(), invoke_key: INVOKE_KEY.into() })
            .map(|body| body.deserialize().unwrap())
    };
    let allowed = || stoneql_desktop::navigation_allowed(app.handle());
    let session =
        invoke("host_info", json!({"nonce": "reload_nonce_1"})).unwrap()["payload"]["sessionId"]
            .clone();
    let grant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
        .unwrap()
        .grant
        .unwrap();
    let handle = invoke(
        "connect_sqlite",
        json!({"sessionId": session, "grant": grant, "access": "read_write"}),
    )
    .unwrap()["payload"]["handle"]
        .clone();
    // An idle read-write handle alone does not hold reload.
    assert!(allowed());

    // A job record (submitted, outcome not yet read) holds reload.
    let job = invoke(
        "submit_job",
        json!({"sessionId": session, "handle": handle, "sql": "SELECT 1"}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    assert!(!allowed());
    loop {
        let update = invoke(
            "fetch_job",
            json!({"sessionId": session, "jobId": job, "waitMs": 1000}),
        )
        .unwrap()["payload"]
            .clone();
        if update["done"] == true {
            break;
        }
    }
    assert!(allowed());

    // An open lease holds reload, with or without a job.
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": session, "handle": handle}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    assert!(!allowed());
    let job = invoke(
        "submit_job",
        json!({"sessionId": session, "handle": handle, "sql": "INSERT INTO t VALUES (1)", "leaseId": lease}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    loop {
        let update = invoke(
            "fetch_job",
            json!({"sessionId": session, "jobId": job, "waitMs": 1000}),
        )
        .unwrap()["payload"]
            .clone();
        if update["done"] == true {
            break;
        }
    }
    assert!(!allowed(), "the open lease still holds reload");

    // A cancelled reload changes nothing: the renderer session and lease stay usable.
    assert!(
        invoke(
            "database_status",
            json!({"sessionId": session, "handle": handle})
        )
        .is_ok()
    );

    // Confirmed reload: the renderer rotates the session first. The host resets the registry
    // (lease rolled back and recorded), the stale session is refused and reload is allowed.
    let next =
        invoke("host_info", json!({"nonce": "reload_nonce_2"})).unwrap()["payload"]["sessionId"]
            .clone();
    assert_ne!(next, session);
    assert!(allowed());
    assert_eq!(
        invoke(
            "submit_job",
            json!({"sessionId": session, "handle": handle, "sql": "SELECT 1"})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );
    assert_eq!(sqlite(&file, "SELECT count(*) FROM t"), 0);
    assert_eq!(
        invoke(
            "end_transaction",
            json!({"sessionId": next, "leaseId": lease, "action": "commit"}),
        )
        .unwrap()["payload"],
        json!({"outcome": "rolled_back", "reason": "closed", "commitSent": false, "rollbackSent": true})
    );

    // A confirmed reload is not refused again by job records the rotation already released:
    // one never fetched and one still running when the session rotates.
    let mut session = next;
    for sql in [
        "SELECT 1",
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000000000) SELECT count(*) FROM c",
    ] {
        let grant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
            .unwrap()
            .grant
            .unwrap();
        let handle = invoke(
            "connect_sqlite",
            json!({"sessionId": session, "grant": grant, "access": "read_write"}),
        )
        .unwrap()["payload"]["handle"]
            .clone();
        invoke(
            "submit_job",
            json!({"sessionId": session, "handle": handle, "sql": sql}),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!allowed());
        session = invoke(
            "host_info",
            json!({"nonce": format!("reload_{}", sql.len())}),
        )
        .unwrap()["payload"]["sessionId"]
            .clone();
        assert!(allowed(), "rotation released `{sql}`");
    }
    let next = session;

    // A lease ended by the user releases reload too.
    let handle = invoke(
        "connect_sqlite",
        json!({"sessionId": next, "grant": stoneql_desktop::authorize_sqlite_file(app.handle(), &file).unwrap().grant.unwrap(), "access": "read_write"}),
    )
    .unwrap()["payload"]["handle"]
        .clone();
    // Records left by the previous session do not hold reload once this session's own job
    // has delivered its final update.
    let job = invoke(
        "submit_job",
        json!({"sessionId": next, "handle": handle, "sql": "SELECT 2"}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    assert!(!allowed());
    while invoke(
        "fetch_job",
        json!({"sessionId": next, "jobId": job, "waitMs": 1000}),
    )
    .unwrap()["payload"]["done"]
        != true
    {}
    assert!(allowed());
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": next, "handle": handle}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    assert!(!allowed());
    invoke(
        "end_transaction",
        json!({"sessionId": next, "leaseId": lease, "action": "rollback"}),
    )
    .unwrap();
    assert!(allowed());
}

type Invoke<'a> = Box<dyn Fn(&str, Value) -> Result<Value, Value> + 'a>;
fn rw_app() -> tauri::App<tauri::test::MockRuntime> {
    stoneql_desktop::app_builder_with(
        mock_builder(),
        false,
        stoneql_desktop::HostServices {
            read_write_sql: true,
            ..stoneql_desktop::HostServices::system()
        },
    )
    .build(tauri::generate_context!())
    .unwrap()
}
fn invoker(window: &tauri::WebviewWindow<tauri::test::MockRuntime>) -> Invoke<'_> {
    Box::new(move |command: &str, payload: Value| {
        get_ipc_response(window, InvokeRequest { cmd: command.into(), callback: CallbackFn(0), error: CallbackFn(1), url: "tauri://localhost".parse().unwrap(), body: InvokeBody::Json(json!({"message": json!({"version": 1, "requestId": "outcome_request", "payload": payload}).to_string()})), headers: Default::default(), invoke_key: INVOKE_KEY.into() })
            .map(|body| body.deserialize().unwrap())
    })
}

/// M006 blocker 3 (Contract 4.5): an unknown outcome recorded under one renderer session stays
/// listable by the next session of the same app process after the reload rotation, until the
/// user acknowledges it. Confirmed outcomes are not listed. Another app instance (another owner)
/// never sees or acknowledges it. Acknowledgement hides it without changing it.
#[test]
fn unknown_outcomes_survive_the_reload_rotation_until_acknowledged_and_stay_owner_scoped() {
    let root = std::env::temp_dir().join(format!("stoneql-outcomes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let temp = TempDir(std::fs::canonicalize(&root).unwrap());
    let file = temp.0.join("outcomes.sqlite");
    sqlite(&file, "CREATE TABLE t(id INTEGER PRIMARY KEY)");
    let app = rw_app();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = invoker(&window);
    let first =
        invoke("host_info", json!({"nonce": "outcomes_nonce_1"})).unwrap()["payload"]["sessionId"]
            .clone();
    let connect = |session: &Value| {
        let grant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
            .unwrap()
            .grant
            .unwrap();
        invoke(
            "connect_sqlite",
            json!({"sessionId": session, "grant": grant, "access": "read_write"}),
        )
        .unwrap()["payload"]["handle"]
            .clone()
    };
    let (leased, running) = (connect(&first), connect(&first));
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": first, "handle": leased}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    let insert = invoke(
        "submit_job",
        json!({"sessionId": first, "handle": leased, "sql": "INSERT INTO t VALUES (1)", "leaseId": lease}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    while invoke(
        "fetch_job",
        json!({"sessionId": first, "jobId": insert, "waitMs": 1000}),
    )
    .unwrap()["payload"]["done"]
        != true
    {}
    let job = invoke(
        "submit_job",
        json!({"sessionId": first, "handle": running, "sql": "SELECT 2; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000000000) SELECT count(*) FROM c"}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(
        invoke("list_unresolved_outcomes", json!({"sessionId": first})).unwrap()["payload"],
        json!({"outcomes": [], "limit": 16, "pending": 0})
    );
    assert!(!stoneql_desktop::navigation_allowed(app.handle()));

    // Confirmed reload: the renderer rotates the session; the running job is interrupted.
    let second =
        invoke("host_info", json!({"nonce": "outcomes_nonce_2"})).unwrap()["payload"]["sessionId"]
            .clone();
    assert!(stoneql_desktop::navigation_allowed(app.handle()));
    assert_eq!(
        invoke("list_unresolved_outcomes", json!({"sessionId": first})).unwrap_err()["code"],
        "FORBIDDEN",
        "the previous renderer session is stale"
    );
    let listed =
        invoke("list_unresolved_outcomes", json!({"sessionId": second})).unwrap()["payload"]
            .clone();
    let outcomes = listed["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 1, "{listed}");
    assert_eq!(outcomes[0]["outcomeId"], job);
    assert_eq!(outcomes[0]["database"], "outcomes.sqlite");
    assert_eq!(outcomes[0]["transaction"], Value::Null);
    assert_eq!(
        outcomes[0]["job"],
        json!({"statementCount": 2, "unknownStatement": 1, "inTransaction": false, "cancellation": "confirmed", "serverCancelSent": true, "deadlineExceeded": false})
    );
    assert!(!listed.to_string().contains("SELECT") && !listed.to_string().contains("RECURSIVE"));
    // The lease's ROLLBACK was confirmed by the server: resolved, so not listed.
    assert_eq!(
        invoke(
            "end_transaction",
            json!({"sessionId": second, "leaseId": lease, "action": "rollback"})
        )
        .unwrap()["payload"]["outcome"],
        "rolled_back"
    );

    // Another app instance has its own owner: it neither sees nor acknowledges the outcome.
    let other_app = rw_app();
    let other_window =
        WebviewWindowBuilder::new(&other_app, "main", WebviewUrl::App("index.html".into()))
            .build()
            .unwrap();
    let other = invoker(&other_window);
    let other_session = other("host_info", json!({"nonce": "outcomes_other"})).unwrap()["payload"]
        ["sessionId"]
        .clone();
    assert_eq!(
        other(
            "list_unresolved_outcomes",
            json!({"sessionId": other_session})
        )
        .unwrap()["payload"]["outcomes"],
        json!([])
    );
    assert_eq!(
        other(
            "acknowledge_outcome",
            json!({"sessionId": other_session, "outcomeId": job})
        )
        .unwrap_err()["code"],
        "FORBIDDEN"
    );

    // A third rotation still lists it; only an explicit acknowledgement hides it (idempotent).
    let third =
        invoke("host_info", json!({"nonce": "outcomes_nonce_3"})).unwrap()["payload"]["sessionId"]
            .clone();
    let mut again =
        invoke("list_unresolved_outcomes", json!({"sessionId": third})).unwrap()["payload"].clone();
    again["outcomes"][0]["secondsAgo"] = listed["outcomes"][0]["secondsAgo"].clone();
    assert_eq!(again, listed);
    for _ in 0..2 {
        assert_eq!(
            invoke(
                "acknowledge_outcome",
                json!({"sessionId": third, "outcomeId": job})
            )
            .unwrap()["payload"]["outcomes"],
            json!([])
        );
    }
    for (outcome_id, code) in [
        (json!(uuid::Uuid::new_v4().to_string()), "FORBIDDEN"),
        (json!("../job"), "INVALID_REQUEST"),
    ] {
        assert_eq!(
            invoke(
                "acknowledge_outcome",
                json!({"sessionId": third, "outcomeId": outcome_id})
            )
            .unwrap_err()["code"],
            code
        );
    }
    assert_eq!(
        invoke(
            "acknowledge_outcome",
            json!({"sessionId": third, "outcomeId": job, "outcome": "rolled_back"})
        )
        .unwrap_err()["code"],
        "INVALID_REQUEST",
        "an acknowledgement cannot carry an outcome"
    );
    assert_eq!(
        invoke("list_unresolved_outcomes", json!({"sessionId": third})).unwrap()["payload"]["outcomes"],
        json!([])
    );
    assert_eq!(sqlite(&file, "SELECT count(*) FROM t"), 0);
}

/// F-2: when the web content process dies, the host itself treats the renderer as gone. The
/// termination handler's body (`renderer_lost`) rotates the session exactly as a confirmed
/// reload does — lease rolled back and recorded, running job interrupted and recorded — so the
/// reload it then issues is not refused by the navigation guard, and the next renderer lists
/// the unknown outcome. MockRuntime cannot crash WebKit; the hook itself is exercised by
/// `real_window_reload` (STONEQL_REAL_WINDOW=1).
#[test]
fn a_terminated_web_content_process_resets_the_session_and_releases_reload() {
    let root = std::env::temp_dir().join(format!("stoneql-crash-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let temp = TempDir(std::fs::canonicalize(&root).unwrap());
    let file = temp.0.join("crash.sqlite");
    sqlite(&file, "CREATE TABLE t(id INTEGER PRIMARY KEY)");
    let app = rw_app();
    let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .unwrap();
    let invoke = invoker(&window);
    let session =
        invoke("host_info", json!({"nonce": "crash_nonce_1"})).unwrap()["payload"]["sessionId"]
            .clone();
    let connect = || {
        let grant = stoneql_desktop::authorize_sqlite_file(app.handle(), &file)
            .unwrap()
            .grant
            .unwrap();
        invoke(
            "connect_sqlite",
            json!({"sessionId": session, "grant": grant, "access": "read_write"}),
        )
        .unwrap()["payload"]["handle"]
            .clone()
    };
    let (leased, running) = (connect(), connect());
    let lease = invoke(
        "begin_transaction",
        json!({"sessionId": session, "handle": leased}),
    )
    .unwrap()["payload"]["leaseId"]
        .clone();
    let insert = invoke(
        "submit_job",
        json!({"sessionId": session, "handle": leased, "sql": "INSERT INTO t VALUES (1)", "leaseId": lease}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    while invoke(
        "fetch_job",
        json!({"sessionId": session, "jobId": insert, "waitMs": 1000}),
    )
    .unwrap()["payload"]["done"]
        != true
    {}
    let job = invoke(
        "submit_job",
        json!({"sessionId": session, "handle": running, "sql": "SELECT 2; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000000000) SELECT count(*) FROM c"}),
    )
    .unwrap()["payload"]["jobId"]
        .clone();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!stoneql_desktop::navigation_allowed(app.handle()));

    // The content process terminates: no renderer confirms anything.
    tauri::async_runtime::block_on(stoneql_desktop::renderer_lost(app.handle()));
    assert!(
        stoneql_desktop::navigation_allowed(app.handle()),
        "the host's own reload must not be refused"
    );
    assert_eq!(
        invoke("list_unresolved_outcomes", json!({"sessionId": session})).unwrap_err()["code"],
        "FORBIDDEN",
        "the dead renderer's session is revoked"
    );
    assert_eq!(sqlite(&file, "SELECT count(*) FROM t"), 0);

    // The reloaded renderer bootstraps a new session and lists the interrupted job.
    let next =
        invoke("host_info", json!({"nonce": "crash_nonce_2"})).unwrap()["payload"]["sessionId"]
            .clone();
    let listed =
        invoke("list_unresolved_outcomes", json!({"sessionId": next})).unwrap()["payload"].clone();
    let outcomes = listed["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 1, "{listed}");
    assert_eq!(outcomes[0]["outcomeId"], job);
    assert_eq!(
        invoke(
            "end_transaction",
            json!({"sessionId": next, "leaseId": lease, "action": "commit"}),
        )
        .unwrap()["payload"],
        json!({"outcome": "rolled_back", "reason": "closed", "commitSent": false, "rollbackSent": true})
    );
}
