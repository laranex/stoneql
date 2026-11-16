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
//! - `fallback <root>`: freezes the renderer, clicks the window's close button, and expects the
//!   native "not responding" dialog: Cancel keeps the app running; a second close and
//!   "Close StoneQL" must exit with status 0. Writes `<root>/fallback.json` before the final click.
//! - `picker <root>`: opens the native SSH key-file picker through `pick_ssh_key_file`, cancels it
//!   (Escape → no grant), then chooses `<root>/id_picker_test` with Go to Folder (Cmd+Shift+G)
//!   and expects an opaque grant naming only that file. Writes `<root>/picker.json`.
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
    // WKWebView's web area is not reachable through System Events, so a real right-click is
    // posted with CoreGraphics on the status bar (JXA; the grid has its own app menu), then the menu item is chosen.
    let context = attempt("context menu Reload", {
        let bounds = osascript(&format!(
            "{}\ndelay 0.5\ntell application \"System Events\"\n set w to window 1 of (first process whose unix id is {pid})\n set {{x, y}} to position of w\n set {{wd, ht}} to size of w\n return (((x + wd / 2) as integer) as text) & \",\" & (((y + ht - 10) as integer) as text)\nend tell",
            front(pid)
        ));
        let point = bounds.1.replace(' ', "");
        let click = std::process::Command::new("/usr/bin/osascript")
            .args([
                "-l",
                "JavaScript",
                "-e",
                &format!(
                    "ObjC.import('CoreGraphics'); var p = $.CGPointMake({point}); [3, 4].forEach(function (t) {{ $.CGEventPost($.kCGHIDEventTap, $.CGEventCreateMouseEvent(null, t, p, $.kCGMouseButtonRight)); delay(0.1); }}); 'clicked'"
                ),
            ])
            .output()
            .unwrap();
        std::thread::sleep(Duration::from_millis(800));
        format!(
            "-- right-click at {point}: {}\ntell application \"System Events\"\n set p to first process whose unix id is {pid}\n set picked to false\n set names to {{}}\n try\n  set names to name of every menu item of menu 1 of p\n end try\n try\n  click (first menu item of menu 1 of p whose name is \"Reload\")\n  set picked to true\n end try\n -- WKWebView's native menu is not exposed to System Events: Down selects its first item (Reload).\n if not picked then\n  key code 125\n  delay 0.2\n  key code 36\n  set picked to \"keyboard\"\n end if\n return {{picked, names}}\nend tell",
            click.status.success()
        )
    });
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
            "[...(document.querySelector('[aria-label=\"Unknown outcomes\"]')?.querySelectorAll('button') ?? [])].filter(b => b.textContent.trim() === 'Acknowledge').length",
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

fn fallback(app: tauri::AppHandle<Wry>, root: PathBuf) {
    let window = app.get_webview_window("main").unwrap();
    let pid = std::process::id();
    // Fixed test script: a busy loop leaves the web content process unable to answer the
    // close request, which is exactly the case the native fallback exists for.
    window
        .eval("setTimeout(() => { for (;;) {} }, 50)")
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let close = || {
        osascript(&format!(
            "{}\ndelay 0.5\ntell application \"System Events\"\n set w to first window of (first process whose unix id is {pid}) whose name is \"StoneQL\"\n click (first button of w whose subrole is \"AXCloseButton\")\nend tell",
            front(pid)
        ))
    };
    // Without a parent window the dialog plugin shows a CFUserNotification, which macOS renders
    // in the UserNotificationCenter process (not this one). Its buttons need AXPress.
    const FIND: &str = "tell application \"System Events\"\n set found to {}\n repeat with w in (windows of process \"UserNotificationCenter\")\n  if exists (button \"Close StoneQL\" of w) then set end of found to w\n end repeat";
    let alerts = || {
        osascript(&format!("{FIND}\n return count of found\nend tell"))
            .1
            .parse::<u32>()
            .unwrap_or(0)
    };
    let press = |button: &str| {
        osascript(&format!(
            "{FIND}\n perform action \"AXPress\" of button \"{button}\" of (item 1 of found)\nend tell"
        ))
        .0
    };
    let first_close = close();
    std::thread::sleep(Duration::from_millis(3500));
    let first_dialog = alerts();
    let cancel = press("Cancel");
    std::thread::sleep(Duration::from_millis(1500));
    let after_cancel = alerts();
    let second_close = close();
    std::thread::sleep(Duration::from_millis(3500));
    let second_dialog = alerts();
    let report = json!({
        "firstClose": first_close, "alertsAfterClose": first_dialog,
        "cancelPressed": cancel, "alertsAfterCancel": after_cancel,
        "stillRunningAfterCancel": true,
        "secondClose": second_close, "alertsAfterSecondClose": second_dialog,
    });
    println!("{report}");
    write(&root, "fallback", &report);
    println!("CLOSING");
    let confirm = press("Close StoneQL");
    println!("{}", json!({"closeStoneQlPressed": confirm}));
    // The app must exit on its own (status 0) via shutdown; this deadline is a failure.
    std::thread::sleep(Duration::from_secs(10));
    eprintln!("fallback: app did not exit after Close StoneQL");
    press("Cancel");
    std::process::exit(3);
}

fn picker(app: tauri::AppHandle<Wry>, root: PathBuf) {
    let window = app.get_webview_window("main").unwrap();
    let pid = std::process::id();
    let key = root.join("id_picker_test");
    // Placeholder bytes only: the picker grant checks the file, not its contents.
    std::fs::write(&key, "not a key\n").unwrap();
    let s = session(&window, "jw_picker");
    let pick = |driver: String| -> Value {
        let shot = root.join("picker-open.png");
        let script = format!(
            "{}\ndelay 2\ndo shell script \"/usr/sbin/screencapture -x {}\"\n{driver}",
            front(pid),
            shot.display()
        );
        let drive = std::thread::spawn(move || osascript(&script));
        let result = run(
            &window,
            &format!("window.__rt = await inv('pick_ssh_key_file', {{sessionId: {s}}});"),
        );
        json!({"result": result, "driver": drive.join().unwrap()})
    };
    let cancelled = pick("tell application \"System Events\" to key code 53".into());
    let chosen = pick(format!(
        "tell application \"System Events\"\n keystroke \"g\" using {{command down, shift down}}\n delay 1\n keystroke \"{}\"\n delay 1\n key code 36\n delay 1\n key code 36\nend tell",
        key.display()
    ));
    let report = json!({
        "cancelled": cancelled, "chosen": chosen,
        "cancelGivesNoGrant": cancelled["result"].is_object() && cancelled["result"]["grant"].is_null() && cancelled["result"]["error"].is_null(),
        "chosenGrantOpaque": chosen["result"]["grant"].as_str().is_some_and(|g| !g.contains('/')),
        "chosenFileName": chosen["result"]["fileName"],
    });
    println!("{report}");
    write(&root, "picker", &report);
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
            "fallback" => fallback(handle, root),
            "picker" => picker(handle, root),
            _ => std::process::exit(64),
        }
    });
    app.run(|_, _| {});
}
