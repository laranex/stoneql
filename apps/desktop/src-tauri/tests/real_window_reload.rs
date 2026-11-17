//! M006 blocker 2 in a real window (WKWebView on macOS, WebKitGTK on Linux): the native navigation guard refuses
//! `location.reload()`, a same-origin `location.assign` and the native `Webview::reload` while
//! the host holds an open lease and an unfetched job; the app's in-app dialog appears;
//! confirming it rotates the session (lease rolled back and recorded) and the reload proceeds.
//! Then (F-2) the web content process is killed while a lease and a running job are held: the
//! host ends the session itself and reloads, and the new renderer lists the interrupted job.
//!
//! Opens a real window, so it only runs when `STONEQL_REAL_WINDOW=1` (needs a desktop session):
//! `npm run build && STONEQL_REAL_WINDOW=1 cargo test --locked -p stoneql-desktop
//! --features custom-protocol,sqlite --test real_window_reload`. On a headless Linux host, run it
//! under `dbus-run-session -- xvfb-run -a`. Physical Cmd+R, the context-menu
//! Reload and the Web Inspector reload are manual checks (docs/development/verification.md).
use serde_json::{Value, json};
use sqlx::{Connection, Executor, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    sync::mpsc,
    time::{Duration, Instant},
};
use tauri::{Manager, WebviewWindow, Wry, webview::PageLoadEvent};

static STARTED: AtomicUsize = AtomicUsize::new(0);
static FINISHED: AtomicUsize = AtomicUsize::new(0);

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
fn eval(window: &WebviewWindow<Wry>, js: &str) -> Value {
    let (sender, receiver) = mpsc::channel();
    window
        .eval_with_callback(js, move |result| {
            let _ = sender.send(result);
        })
        .unwrap();
    let raw = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    serde_json::from_str(&raw).unwrap_or(Value::Null)
}
fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}
/// Runs an async IPC script in the page and returns its `window.__rt` result.
fn run(window: &WebviewWindow<Wry>, body: &str) -> Value {
    eval(window, "window.__rt = undefined");
    window
        .eval(format!(
            "(async () => {{ const inv = async (c, p) => {{ for (let i = 0; ; i++) {{ try {{ return (await window.__TAURI__.core.invoke(c, {{message: JSON.stringify({{version: 1, requestId: 'rw_' + c, payload: p}})}})).payload; }} catch (e) {{ if (i > 20 || e.code !== 'CAPACITY') throw e; await new Promise(r => setTimeout(r, 200)); }} }} }}; try {{ {body} }} catch (e) {{ window.__rt = {{error: JSON.stringify(e)}}; }} }})()"
        ))
        .unwrap();
    let mut result = Value::Null;
    wait_until("page script", || {
        result = eval(window, "window.__rt ?? null");
        !result.is_null()
    });
    assert!(
        result.get("error").is_none(),
        "page script failed: {result}"
    );
    result
}

fn drive(app: tauri::AppHandle<Wry>, file: PathBuf) {
    wait_until("first page load", || FINISHED.load(Ordering::SeqCst) >= 1);
    let window = app.get_webview_window("main").unwrap();
    // Let the app's own bootstrap finish (one bootstrap at a time; `inv` retries CAPACITY).
    std::thread::sleep(Duration::from_millis(1500));
    let session = run(
        &window,
        "window.__rt = (await inv('host_info', {nonce: 'real_window_1'})).sessionId;",
    );
    // After the rotation: a reset revokes earlier file grants.
    let grant = stoneql_desktop::authorize_sqlite_file(&app, &file)
        .unwrap()
        .grant
        .unwrap();
    let opened = run(
        &window,
        &format!(
            "window.__reloadEvents = 0; window.addEventListener('stoneql-reload-requested', () => window.__reloadEvents++); \
             const s = {session}; \
             const h = (await inv('connect_sqlite', {{sessionId: s, grant: {grant}, access: 'read_write'}})).handle; \
             const l = (await inv('begin_transaction', {{sessionId: s, handle: h}})).leaseId; \
             const j = (await inv('submit_job', {{sessionId: s, handle: h, sql: 'INSERT INTO t VALUES (1)', leaseId: l}})).jobId; \
             while (!(await inv('fetch_job', {{sessionId: s, jobId: j, waitMs: 1000}})).done) {{}} \
             const h2 = (await inv('connect_sqlite', {{sessionId: s, grant: {grant}, access: 'read_write'}})).handle; \
             await inv('submit_job', {{sessionId: s, handle: h2, sql: 'SELECT 1'}}); \
             window.__rt = {{lease: l}};",
            grant = json!(grant)
        ),
    );
    let lease = opened["lease"].clone();
    assert!(!stoneql_desktop::navigation_allowed(&app));

    // Every reload/navigation path the test can trigger is refused; the page (and its marker
    // variable) survives and the renderer is asked to confirm.
    let attempts = [
        "location.reload()",
        "same-origin location.assign",
        "native Webview::reload",
    ];
    for (index, name) in attempts.into_iter().enumerate() {
        match index {
            0 => window.eval("location.reload()").unwrap(),
            1 => window
                .eval("location.assign(location.origin + '/index.html')")
                .unwrap(),
            _ => window.reload().unwrap(),
        }
        wait_until(name, || {
            eval(&window, "window.__reloadEvents ?? -1") == json!(index + 1)
        });
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(STARTED.load(Ordering::SeqCst), 1, "{name} was not refused");
        assert_eq!(eval(&window, "window.__reloadEvents"), json!(index + 1));
    }
    assert_eq!(sqlite(&file, "SELECT count(*) FROM t"), 0);

    // The app's in-app dialog is shown; confirming it rotates the session, then reloads.
    wait_until("in-app reload dialog", || {
        eval(
            &window,
            "!!document.querySelector('[role=alertdialog]')?.textContent.includes('Reload StoneQL?')",
        ) == json!(true)
    });
    eval(
        &window,
        "[...document.querySelectorAll('[role=alertdialog] button')].find(b => b.textContent.trim() === 'Reload StoneQL').click()",
    );
    wait_until("confirmed reload", || {
        FINISHED.load(Ordering::SeqCst) >= 2 && STARTED.load(Ordering::SeqCst) >= 2
    });
    assert!(stoneql_desktop::navigation_allowed(&app));
    assert_eq!(eval(&window, "window.__reloadEvents ?? null"), Value::Null);
    std::thread::sleep(Duration::from_millis(1500));
    let ended = run(
        &window,
        &format!(
            "const s = (await inv('host_info', {{nonce: 'real_window_2'}})).sessionId; \
             window.__rt = await inv('end_transaction', {{sessionId: s, leaseId: {lease}, action: 'commit'}});"
        ),
    );
    assert_eq!(
        ended,
        json!({"outcome": "rolled_back", "reason": "closed", "commitSent": false, "rollbackSent": true})
    );
    assert_eq!(sqlite(&file, "SELECT count(*) FROM t"), 0);

    // F-2: the web content process dies while a lease and a running job are held. Nothing in
    // the page can confirm; the host ends the session itself, reloads, and the new renderer
    // lists the interrupted job without user action.
    std::thread::sleep(Duration::from_millis(1500));
    let session = run(
        &window,
        "window.__rt = (await inv('host_info', {nonce: 'real_window_3'})).sessionId;",
    );
    let grant = stoneql_desktop::authorize_sqlite_file(&app, &file)
        .unwrap()
        .grant
        .unwrap();
    run(
        &window,
        &format!(
            "window.__crashMarker = true; const s = {session}; \
             const h = (await inv('connect_sqlite', {{sessionId: s, grant: {grant}, access: 'read_write'}})).handle; \
             const l = (await inv('begin_transaction', {{sessionId: s, handle: h}})).leaseId; \
             const j = (await inv('submit_job', {{sessionId: s, handle: h, sql: 'INSERT INTO t VALUES (2)', leaseId: l}})).jobId; \
             while (!(await inv('fetch_job', {{sessionId: s, jobId: j, waitMs: 1000}})).done) {{}} \
             const h2 = (await inv('connect_sqlite', {{sessionId: s, grant: {grant}, access: 'read_write'}})).handle; \
             await inv('submit_job', {{sessionId: s, handle: h2, sql: 'SELECT 2; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1000000000) SELECT count(*) FROM c'}}); \
             window.__rt = true;",
            grant = json!(grant)
        ),
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(!stoneql_desktop::navigation_allowed(&app));
    let loads = STARTED.load(Ordering::SeqCst);
    let pid = web_content_pid();
    let killed = std::process::Command::new("/bin/kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    wait_until("reload after the content process terminated", || {
        STARTED.load(Ordering::SeqCst) > loads && FINISHED.load(Ordering::SeqCst) > loads
    });
    assert!(stoneql_desktop::navigation_allowed(&app));
    assert_eq!(eval(&window, "window.__crashMarker ?? null"), Value::Null);
    assert_eq!(sqlite(&file, "SELECT count(*) FROM t"), 0);
    wait_until("the interrupted job listed by the new renderer", || {
        eval(
            &window,
            "!!document.querySelector('[aria-label=\"Unknown outcomes\"]')?.textContent.includes('statement 2 of 2')",
        ) == json!(true)
    });
}

/// WebKit content processes (XPC services, children of launchd) of every app of this user.
#[cfg(target_os = "macos")]
fn web_content_pids() -> std::collections::BTreeSet<i32> {
    let listing = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,comm="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|line| line.ends_with("/com.apple.WebKit.WebContent"))
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect()
}
/// WebKitGTK web processes descending from this test process (directly or through its
/// bubblewrap sandbox). `comm` is truncated to 15 bytes, so `argv[0]` is matched instead; the
/// network process (`WebKitNetworkProcess`) is never selected.
#[cfg(target_os = "linux")]
fn web_content_pids() -> std::collections::BTreeSet<i32> {
    let me = std::process::id() as i32;
    let parent = |pid: i32| -> Option<i32> {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))?
            .trim()
            .parse()
            .ok()
    };
    let descends = |mut pid: i32| {
        while pid > 1 {
            match parent(pid) {
                Some(next) if next == me => return true,
                Some(next) => pid = next,
                None => return false,
            }
        }
        false
    };
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            std::fs::read(format!("/proc/{pid}/cmdline")).is_ok_and(|cmdline| {
                cmdline
                    .split(|byte| *byte == 0)
                    .next()
                    .is_some_and(|argv0| argv0.ends_with(b"/WebKitWebProcess"))
            })
        })
        .filter(|pid| descends(*pid))
        .collect()
}
/// This window's content process: the only one that appeared since the app started. The
/// workspace forbids `unsafe`, so WebKit's `_webProcessIdentifier` SPI is not used. When
/// another app started one meanwhile the choice is ambiguous and nothing is killed.
fn web_content_pid() -> i32 {
    let before = BEFORE.get().unwrap();
    let new: Vec<_> = web_content_pids().difference(before).copied().collect();
    assert_eq!(
        new.len(),
        1,
        "cannot identify this window's web content process ({new:?}); nothing killed, rerun"
    );
    new[0]
}
static BEFORE: std::sync::OnceLock<std::collections::BTreeSet<i32>> = std::sync::OnceLock::new();

fn main() {
    if std::env::var_os("STONEQL_REAL_WINDOW").is_none() {
        println!("real_window_reload skipped: set STONEQL_REAL_WINDOW=1 to open a real window");
        return;
    }
    let root = std::env::temp_dir().join(format!("stoneql-real-reload-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(&root).unwrap();
    let file = root.join("fixture.sqlite");
    sqlite(&file, "CREATE TABLE t(id INTEGER PRIMARY KEY)");
    std::thread::spawn({
        let root = root.clone();
        move || {
            std::thread::sleep(Duration::from_secs(120));
            eprintln!("real_window_reload: deadline exceeded");
            let _ = std::fs::remove_dir_all(&root);
            std::process::exit(2);
        }
    });
    BEFORE.set(web_content_pids()).unwrap();
    let app = stoneql_desktop::app_builder_with(
        tauri::Builder::default(),
        false,
        stoneql_desktop::HostServices {
            read_write_sql: true,
            // Never the user's app data directory; removed with `root`.
            outcome_journal: Some(root.join("outcome-journal")),
            ..stoneql_desktop::HostServices::system()
        },
    )
    .on_page_load(|webview, payload| {
        if webview.label() == "main" {
            match payload.event() {
                PageLoadEvent::Started => STARTED.fetch_add(1, Ordering::SeqCst),
                PageLoadEvent::Finished => FINISHED.fetch_add(1, Ordering::SeqCst),
            };
        }
    })
    .build(tauri::generate_context!())
    .unwrap();
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drive(handle, file);
        }));
        let _ = std::fs::remove_dir_all(&root);
        if outcome.is_ok() {
            println!("real_window_reload: passed");
        }
        std::process::exit(if outcome.is_ok() { 0 } else { 1 });
    });
    app.run(|_, _| {});
}
