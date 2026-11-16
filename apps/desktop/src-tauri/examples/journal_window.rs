//! Validation harness (uncommitted, test-only): the real desktop host in a real window, with
//! the read-write gate enabled only through the test injection (`HostServices`) and the journal
//! in `<root>/outcome-journal` (never the app data directory). Build after `npm run build`:
//! `cargo build --locked -p stoneql-desktop --features custom-protocol --example journal_window`.
//!
//! - `work <root>`: an unknown job (cancelled in-flight statement), an open transaction with a
//!   write and a running statement; writes `<root>/work.json`, prints `READY`, then waits to
//!   be killed (SIGKILL by the driver).
//! - `list <root> <tag> [ack <id>]`: lists (and optionally acknowledges), tries to open and
//!   roll back a transaction, writes `<root>/<tag>.json` and exits.
//! - `reload <root>`: with an open transaction and a running job, tries Cmd+R, the context-menu
//!   Reload and the Web Inspector reload through System Events, records whether each reloaded,
//!   then confirms the in-app dialog and checks the unknown outcomes after the reload.
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
const ENDLESS: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";

fn create(file: &Path) {
    tauri::async_runtime::block_on(async {
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(file)
                .create_if_missing(true),
        )
        .await
        .unwrap();
        connection
            .execute("CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY)")
            .await
            .unwrap();
        connection.close().await.unwrap();
    });
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
fn wait_until(what: &str, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !condition() {
        if Instant::now() > deadline {
            eprintln!("timed out waiting for {what}");
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}
/// Runs an async IPC script in the page and returns its `window.__rt` (or `{error}`).
fn run(window: &WebviewWindow<Wry>, body: &str) -> Value {
    eval(window, "window.__rt = undefined");
    window
        .eval(format!(
            "(async () => {{ const inv = async (c, p) => {{ for (let i = 0; ; i++) {{ try {{ return (await window.__TAURI__.core.invoke(c, {{message: JSON.stringify({{version: 1, requestId: 'jw_' + c, payload: p}})}})).payload; }} catch (e) {{ if (i > 20 || e.code !== 'CAPACITY') throw e; await new Promise(r => setTimeout(r, 200)); }} }} }}; try {{ {body} }} catch (e) {{ window.__rt = {{error: e}}; }} }})()"
        ))
        .unwrap();
    let mut result = Value::Null;
    wait_until("page script", || {
        result = eval(window, "window.__rt ?? null");
        !result.is_null()
    });
    result
}
fn session(window: &WebviewWindow<Wry>, nonce: &str) -> Value {
    run(
        window,
        &format!("window.__rt = (await inv('host_info', {{nonce: '{nonce}'}})).sessionId;"),
    )
}
fn grant(app: &tauri::AppHandle<Wry>, file: &Path) -> Value {
    json!(
        stoneql_desktop::authorize_sqlite_file(app, file)
            .unwrap()
            .grant
            .unwrap()
    )
}
/// An unknown job, an open lease with a write, and a running statement on another handle.
fn active_work(window: &WebviewWindow<Wry>, session: &Value, grant: &Value) -> Value {
    run(
        window,
        &format!(
            "const s = {session}; const g = {grant}; \
             const open = async () => (await inv('connect_sqlite', {{sessionId: s, grant: g, access: 'read_write'}})).handle; \
             const [h1, h2, h3] = [await open(), await open(), await open()]; \
             const j1 = (await inv('submit_job', {{sessionId: s, handle: h1, sql: {endless}}})).jobId; \
             await new Promise(r => setTimeout(r, 80)); \
             await inv('cancel_job', {{sessionId: s, jobId: j1}}); \
             let u; do {{ u = await inv('fetch_job', {{sessionId: s, jobId: j1, waitMs: 500}}); }} while (!u.done); \
             const l = (await inv('begin_transaction', {{sessionId: s, handle: h2}})).leaseId; \
             const jl = (await inv('submit_job', {{sessionId: s, handle: h2, sql: 'INSERT INTO t VALUES (7)', leaseId: l}})).jobId; \
             do {{ u = await inv('fetch_job', {{sessionId: s, jobId: jl, waitMs: 500}}); }} while (!u.done); \
             const j3 = (await inv('submit_job', {{sessionId: s, handle: h3, sql: {endless}}})).jobId; \
             await new Promise(r => setTimeout(r, 200)); \
             const list = await inv('list_unresolved_outcomes', {{sessionId: s}}); \
             window.__rt = {{cancelled: j1, cancelledOutcome: u.outcome, lease: l, running: j3, list}};",
            endless = json!(ENDLESS)
        ),
    )
}
fn write(root: &Path, name: &str, value: &Value) {
    std::fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

fn work(app: tauri::AppHandle<Wry>, root: PathBuf, file: PathBuf) {
    let window = app.get_webview_window("main").unwrap();
    let s = session(&window, "jw_work");
    let result = active_work(&window, &s, &grant(&app, &file));
    write(
        &root,
        "work",
        &json!({"pid": std::process::id(), "result": result}),
    );
    println!("READY");
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn list(
    app: tauri::AppHandle<Wry>,
    root: PathBuf,
    file: PathBuf,
    tag: String,
    ack: Option<String>,
) {
    let window = app.get_webview_window("main").unwrap();
    let s = session(&window, &format!("jw_{tag}"));
    let before = run(
        &window,
        &format!("window.__rt = await inv('list_unresolved_outcomes', {{sessionId: {s}}});"),
    );
    let acked = ack.map(|id| {
        run(
            &window,
            &format!(
                "window.__rt = await inv('acknowledge_outcome', {{sessionId: {s}, outcomeId: {}}});",
                json!(id)
            ),
        )
    });
    let g = grant(&app, &file);
    let begin = run(
        &window,
        &format!(
            "const h = (await inv('connect_sqlite', {{sessionId: {s}, grant: {g}, access: 'read_write'}})).handle; \
             let r; try {{ const l = (await inv('begin_transaction', {{sessionId: {s}, handle: h}})).leaseId; \
             r = {{begun: true, end: await inv('end_transaction', {{sessionId: {s}, leaseId: l, action: 'rollback'}})}}; }} \
             catch (e) {{ r = {{begun: false, error: e}}; }} window.__rt = r;"
        ),
    );
    write(
        &root,
        &tag,
        &json!({"pid": std::process::id(), "listing": before, "ack": acked, "begin": begin}),
    );
    println!("DONE");
    std::process::exit(0);
}

/// System Events script against this process; returns (success, output).
fn osascript(script: &str) -> (bool, String) {
    let out = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .trim()
        .to_string(),
    )
}
fn front(pid: u32) -> String {
    format!(
        "tell application \"System Events\" to set frontmost of (first process whose unix id is {pid}) to true"
    )
}

fn reload(app: tauri::AppHandle<Wry>, root: PathBuf, file: PathBuf) {
    let window = app.get_webview_window("main").unwrap();
    let pid = std::process::id();
    let s = session(&window, "jw_reload");
    let work = active_work(&window, &s, &grant(&app, &file));
    eval(
        &window,
        "window.__marker = true; window.__reloadEvents = 0; window.addEventListener('stoneql-reload-requested', () => window.__reloadEvents++);",
    );
    let mut report =
        json!({"work": work, "activeBefore": !stoneql_desktop::navigation_allowed(&app)});
    let attempt = |name: &str, script: String| -> Value {
        let loads = STARTED.load(Ordering::SeqCst);
        let events = eval(&window, "window.__reloadEvents ?? null");
        let (ok, output) = osascript(&script);
        std::thread::sleep(Duration::from_millis(2500));
        let marker = eval(&window, "window.__marker ?? null");
        let after = eval(&window, "window.__reloadEvents ?? null");
        let value = json!({
            "path": name, "osascriptOk": ok, "osascript": output,
            "pageLoadsStarted": STARTED.load(Ordering::SeqCst) - loads,
            "markerSurvived": marker == json!(true),
            "reloadRequestedEvents": [events, after],
            "workStillActive": !stoneql_desktop::navigation_allowed(&app),
        });
        println!("{value}");
        value
    };
    let cmd_r = attempt(
        "Cmd+R",
        format!(
            "{}\ndelay 0.5\ntell application \"System Events\" to keystroke \"r\" using command down",
            front(pid)
        ),
    );
    let context = attempt(
        "context menu Reload",
        format!(
            "{}\ndelay 0.5\ntell application \"System Events\"\n set p to first process whose unix id is {pid}\n set wa to missing value\n repeat with e in (entire contents of window 1 of p)\n  try\n   if role of e is \"AXWebArea\" then\n    set wa to e\n    exit repeat\n   end if\n  end try\n end repeat\n perform action \"AXShowMenu\" of wa\n delay 0.8\n set names to {{}}\n try\n  set names to name of every menu item of menu 1 of wa\n end try\n set picked to false\n try\n  click (first menu item of menu 1 of wa whose name is \"Reload\")\n  set picked to true\n end try\n if not picked then key code 53\n return {{picked, names}}\nend tell",
            front(pid)
        ),
    );
    let inspector = {
        window.open_devtools();
        std::thread::sleep(Duration::from_millis(2000));
        attempt(
            "Web Inspector reload (Cmd+R in the inspector)",
            format!(
                "{}\ndelay 0.3\ntell application \"System Events\"\n set p to first process whose unix id is {pid}\n set titles to name of every window of p\n repeat with w in (every window of p)\n  if name of w contains \"Inspector\" then perform action \"AXRaise\" of w\n end repeat\n delay 0.5\n keystroke \"r\" using command down\n return titles\nend tell",
                front(pid)
            ),
        )
    };
    window.close_devtools();
    report["attempts"] = json!([cmd_r, context, inspector]);
    // Recovery: the in-app dialog (if a refused reload asked for it) or a native reload request.
    let loads = STARTED.load(Ordering::SeqCst);
    let dialog = eval(
        &window,
        "!!document.querySelector('[role=alertdialog]')?.textContent.includes('Reload StoneQL?')",
    ) == json!(true);
    if !dialog {
        let _ = window.reload();
        wait_until("in-app reload dialog", || {
            eval(
                &window,
                "!!document.querySelector('[role=alertdialog]')?.textContent.includes('Reload StoneQL?')",
            ) == json!(true)
        });
    }
    eval(
        &window,
        "[...document.querySelectorAll('[role=alertdialog] button')].find(b => b.textContent.trim() === 'Reload StoneQL').click()",
    );
    let reloaded = wait_until("confirmed reload", || {
        STARTED.load(Ordering::SeqCst) > loads && FINISHED.load(Ordering::SeqCst) > loads
    });
    std::thread::sleep(Duration::from_millis(2000));
    let ui = wait_until("unknown outcomes shown after reload", || {
        eval(
            &window,
            "document.querySelector('[aria-label=\"Unknown outcomes\"]')?.querySelectorAll('li').length ?? 0",
        )
        .as_u64()
            >= Some(2)
    });
    let ui_text = eval(
        &window,
        "document.querySelector('[aria-label=\"Unknown outcomes\"]')?.textContent ?? null",
    );
    let s = session(&window, "jw_after");
    let listed = run(
        &window,
        &format!(
            "window.__rt = {{list: await inv('list_unresolved_outcomes', {{sessionId: {s}}}), lease: await inv('end_transaction', {{sessionId: {s}, leaseId: {}, action: 'commit'}})}};",
            work["lease"]
        ),
    );
    report["dialogShownByRefusal"] = json!(dialog);
    report["reloadedAfterConfirm"] = json!(reloaded);
    report["uiListsUnknownOutcomes"] = json!(ui);
    report["uiText"] = ui_text;
    report["after"] = listed;
    write(&root, "reload", &report);
    println!("DONE");
    std::process::exit(0);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args[0].clone();
    let root = std::fs::canonicalize(&args[1]).unwrap();
    let file = root.join("fixture.sqlite");
    create(&file);
    let app = stoneql_desktop::app_builder_with(
        tauri::Builder::default(),
        false,
        stoneql_desktop::HostServices {
            read_write_sql: true,
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
        std::thread::sleep(Duration::from_secs(180));
        eprintln!("journal_window: deadline exceeded");
        std::process::exit(2);
    });
    std::thread::spawn(move || {
        wait_until("first page load", || FINISHED.load(Ordering::SeqCst) >= 1);
        // Let the app's own bootstrap finish (one bootstrap at a time; `inv` retries CAPACITY).
        std::thread::sleep(Duration::from_millis(1500));
        match mode.as_str() {
            "work" => work(handle, root, file),
            "list" => {
                let ack = (args.get(3).map(String::as_str) == Some("ack")).then(|| args[4].clone());
                list(handle, root, file, args[2].clone(), ack)
            }
            "reload" => reload(handle, root, file),
            _ => std::process::exit(64),
        }
    });
    app.run(|_, _| {});
}
