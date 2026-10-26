//! Native SQLite file grants. The renderer never supplies a path: it asks the trusted host to
//! show the OS file picker and receives an opaque, session-scoped grant for the chosen file.
use super::{
    NativeSession,
    database::{admit, authorize, connected, lifecycle},
    decode, forbidden,
};
use std::{path::Path, sync::atomic::Ordering};
use stoneql_contracts::v1::{
    Envelope, ErrorDto, LiveConnection, ProtocolVersion, SqliteConnectionRequest, SqliteFileGrant,
    SqliteFileRequest,
};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Trusted native callers only (the file picker and host tests); never an IPC command.
/// The selection is resolved once to its canonical target. The runtime then refuses the grant
/// if that path later stops being the same canonical regular file.
pub fn authorize_sqlite_file<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
) -> Result<SqliteFileGrant, ErrorDto> {
    let session = app.state::<NativeSession>();
    if session.closing.load(Ordering::SeqCst) {
        return Err(forbidden());
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| forbidden())?;
    let grant = session
        .connections
        .grant_sqlite_file(session.owner, &canonical)
        .map_err(lifecycle)?;
    let file_name = session
        .connections
        .sqlite_file_name(session.owner, &grant)
        .map_err(lifecycle)?;
    Ok(SqliteFileGrant {
        grant: Some(grant),
        file_name: Some(file_name),
    })
}

#[tauri::command]
pub async fn pick_sqlite_file<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SqliteFileGrant>, ErrorDto> {
    let request: Envelope<SqliteFileRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let picker = window
        .app_handle()
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Open SQLite database (read-only)")
        .add_filter("SQLite database", &["sqlite", "sqlite3", "db", "db3"])
        .add_filter("All files", &["*"]);
    // The blocking variant waits on a worker thread, never on the event loop.
    let selected = tauri::async_runtime::spawn_blocking(move || {
        picker
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(|_| forbidden())?;
    // A bootstrap while the picker was open invalidates this renderer: issue no grant.
    if !session.is_current(&request.payload.session_id) {
        return Err(forbidden());
    }
    let payload = match selected {
        Some(path) => authorize_sqlite_file(window.app_handle(), &path)?,
        None => SqliteFileGrant {
            grant: None,
            file_name: None,
        },
    };
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload,
    })
}

#[tauri::command]
pub async fn connect_sqlite<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<LiveConnection>, ErrorDto> {
    let request: Envelope<SqliteConnectionRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let input = request.payload;
    let file_name = session
        .connections
        .sqlite_file_name(session.owner, &input.grant)
        .map_err(lifecycle)?;
    let handle = session
        .connections
        .connect_sqlite(session.owner, &input.grant)
        .await;
    connected(
        &session,
        &input.session_id,
        request.request_id,
        handle,
        file_name,
    )
    .await
}
