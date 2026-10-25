mod database;
use database::{connect_direct, database_status, disconnect_database, read_only_query};
use dbadmin_contracts::v1::{
    self, CloseAction, CloseDecision, CloseResult, ContractProbe, Envelope, ErrorDto, HostInfo,
    HostInfoRequest, ProbeAction, ProbeResult, ProtocolVersion,
};
use dbadmin_core::{CoreError, ErrorCode};
use std::{
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};
use tauri::{Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

pub struct NativeSession {
    closing: AtomicBool,
    active_requests: AtomicUsize,
    owner: dbadmin_runtime::OwnerId,
    connections: dbadmin_runtime::Connections,
    id: String,
    echoed: AtomicBool,
    allow_exit: AtomicBool,
    close_pending: AtomicBool,
    close_prompted: AtomicBool,
    fallback_open: AtomicBool,
    close_generation: AtomicU64,
    smoke: bool,
    started: Instant,
}
impl NativeSession {
    fn new(smoke: bool) -> Self {
        Self {
            closing: AtomicBool::new(false),
            active_requests: AtomicUsize::new(0),
            owner: dbadmin_runtime::OwnerId::new(),
            connections: dbadmin_runtime::Connections::new(
                8,
                std::time::Duration::from_secs(3),
                std::time::Duration::from_secs(3600),
            )
            .expect("static native limits"),
            id: uuid::Uuid::new_v4().to_string(),
            echoed: AtomicBool::new(false),
            allow_exit: AtomicBool::new(false),
            close_pending: AtomicBool::new(false),
            close_prompted: AtomicBool::new(false),
            fallback_open: AtomicBool::new(false),
            close_generation: AtomicU64::new(0),
            smoke,
            started: Instant::now(),
        }
    }
}
fn forbidden() -> ErrorDto {
    CoreError::rejected(ErrorCode::Forbidden).into()
}
fn invalid() -> ErrorDto {
    CoreError::rejected(ErrorCode::InvalidRequest).into()
}
fn local_origin(url: &tauri::Url) -> bool {
    (url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none())
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none())
}
fn guard<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), ErrorDto> {
    if window.label() != "main" || !window.url().map(|url| local_origin(&url)).unwrap_or(false) {
        return Err(forbidden());
    }
    Ok(())
}
fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(
    message: &str,
) -> Result<Envelope<T>, ErrorDto> {
    if message.len() > 65_536 {
        return Err(invalid());
    }
    v1::decode_envelope(message).map_err(Into::into)
}
#[tauri::command]
fn host_info<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<HostInfo>, ErrorDto> {
    guard(&window)?;
    let request: Envelope<HostInfoRequest> = decode(&message)?;
    if !v1::is_opaque_id(&request.payload.nonce) {
        return Err(invalid());
    }
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: HostInfo {
            runtime: "desktop".into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            session_id: session.id.clone(),
            nonce: request.payload.nonce,
            simulated: true,
        },
    })
}
#[tauri::command]
fn contract_probe<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<ProbeResult>, ErrorDto> {
    guard(&window)?;
    let request: Envelope<ContractProbe> = decode(&message)?;
    if request.payload.session_id != session.id || request.payload.values.len() > 256 {
        return Err(forbidden());
    }
    match request.payload.action {
        ProbeAction::Echo => session.echoed.store(true, Ordering::SeqCst),
        ProbeAction::Ready => {
            if !session.echoed.load(Ordering::SeqCst) || !request.payload.values.is_empty() {
                return Err(invalid());
            }
            if session.smoke && cfg!(debug_assertions) {
                println!(
                    "{{\"nativeSmokePassed\":true,\"startupMs\":{}}}",
                    session.started.elapsed().as_millis()
                );
                session.allow_exit.store(true, Ordering::SeqCst);
                window.app_handle().exit(0);
            }
        }
    }
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: ProbeResult {
            values: request.payload.values,
            simulated: true,
        },
    })
}
fn shutdown_and_exit<R: Runtime>(app: &tauri::AppHandle<R>) {
    let state = app.state::<NativeSession>();
    if state.closing.swap(true, Ordering::SeqCst) {
        return;
    }
    let connections = state.connections.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        connections.shutdown().await;
        handle
            .state::<NativeSession>()
            .allow_exit
            .store(true, Ordering::SeqCst);
        handle.exit(0);
    });
}
/// Trusted native close request. Renderer confirmation is required before application exit.
pub fn request_close<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), ErrorDto> {
    let state = app.state::<NativeSession>();
    if state.closing.load(Ordering::SeqCst) {
        return Ok(());
    }
    let Some(window) = app.get_webview_window("main") else {
        shutdown_and_exit(app);
        return Ok(());
    };
    if state.fallback_open.load(Ordering::SeqCst) {
        return Ok(());
    }
    let generation = state.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
    state.close_pending.store(true, Ordering::SeqCst);
    state.close_prompted.store(false, Ordering::SeqCst);
    // This script is fixed application code; no SQL, path, identifier or user data is evaluated.
    let _ = window.eval("window.dispatchEvent(new Event('dbadmin-close-requested'))");
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let state = handle.state::<NativeSession>();
        if state.close_generation.load(Ordering::SeqCst) != generation
            || !state.close_pending.load(Ordering::SeqCst)
            || state.close_prompted.load(Ordering::SeqCst)
            || state.fallback_open.swap(true, Ordering::SeqCst)
        {
            return;
        }
        let callback_handle = handle.clone();
        let dialog = handle
            .dialog()
            .message("The workspace is not responding. Close dbAdmin? Unsaved changes may be lost.")
            .title("Close dbAdmin")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Close dbAdmin".into(),
                "Cancel".into(),
            ));
        dialog.show(move |approved| {
            let state = callback_handle.state::<NativeSession>();
            state.fallback_open.store(false, Ordering::SeqCst);
            if state.close_generation.load(Ordering::SeqCst) != generation
                || !state.close_pending.load(Ordering::SeqCst)
            {
                return;
            }
            state.close_pending.store(false, Ordering::SeqCst);
            if approved {
                shutdown_and_exit(&callback_handle);
            }
        });
    });
    Ok(())
}
#[tauri::command]
async fn resolve_close<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<CloseResult>, ErrorDto> {
    guard(&window)?;
    let request: Envelope<CloseDecision> = decode(&message)?;
    if request.payload.session_id != session.id || !session.close_pending.load(Ordering::SeqCst) {
        return Err(forbidden());
    }
    let closed = match request.payload.action {
        CloseAction::Prompted => {
            session.close_prompted.store(true, Ordering::SeqCst);
            false
        }
        CloseAction::Cancel => {
            session.close_pending.store(false, Ordering::SeqCst);
            false
        }
        CloseAction::Confirm => {
            session.close_pending.store(false, Ordering::SeqCst);
            shutdown_and_exit(window.app_handle());
            true
        }
    };
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: CloseResult { closed },
    })
}
pub fn app_builder<R: Runtime>(builder: tauri::Builder<R>, smoke: bool) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
                && !window
                    .state::<NativeSession>()
                    .allow_exit
                    .load(Ordering::SeqCst)
            {
                api.prevent_close();
                let _ = request_close(window.app_handle());
            }
        })
        .setup(move |app| {
            if smoke {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(15));
                    eprintln!("native smoke did not acknowledge rendered UI before timeout");
                    handle
                        .state::<NativeSession>()
                        .allow_exit
                        .store(true, Ordering::SeqCst);
                    handle.exit(1);
                });
            }
            Ok(())
        })
        .manage(NativeSession::new(smoke))
        .invoke_handler(tauri::generate_handler![
            host_info,
            contract_probe,
            resolve_close,
            connect_direct,
            read_only_query,
            disconnect_database,
            database_status
        ])
}
pub fn run() {
    let smoke = cfg!(debug_assertions) && std::env::args().any(|arg| arg == "--smoke-test");
    app_builder(tauri::Builder::default(), smoke)
        .build(tauri::generate_context!())
        .expect("build dbAdmin desktop host")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && !app
                    .state::<NativeSession>()
                    .allow_exit
                    .load(Ordering::SeqCst)
            {
                api.prevent_exit();
                let _ = request_close(app);
            }
        });
}
