//! M007 d5 SQL import commands (A20, approved 2026-11-16 for MySQL and MariaDB). The renderer
//! never sees a path: it asks for the native open dialog and receives a one-shot grant. Starting
//! the import reads the granted file once; the runtime validates every statement in it and, only
//! if all pass, runs that same validated snapshot as a job (cancel with `cancel_job`). Read-write
//! handles exist only when the read-write gate is on, so the gate applies here too.
use super::{
    NativeSession,
    database::{admit, authorize, lifecycle},
    decode,
    execution::{execution_error, job_id},
    forbidden, invalid,
};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, atomic::Ordering},
};
use stoneql_contracts::v1::{
    Envelope, ErrorDto, ImportFileGrant, ImportFileRequest, ProtocolVersion, SqlImportAccepted,
    SqlImportRequest, SqlImportStatus, SqlImportStatusRequest,
};
use stoneql_core::{CoreError, Engine, ErrorCode};
use stoneql_runtime::{ConnectionId, ExecutionLimits, ImportError};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Outstanding source grants per renderer session (Proposed, like export grants).
const MAX_IMPORT_GRANTS: usize = 8;

#[derive(Default)]
pub(crate) struct ImportGrants(Mutex<HashMap<String, PathBuf>>);
impl ImportGrants {
    /// Renderer reset/shutdown revokes every grant.
    pub(crate) fn reset(&self) {
        self.0.lock().expect("import grant lock").clear();
    }
}

fn reply<T>(request_id: String, payload: T) -> Envelope<T> {
    Envelope {
        version: ProtocolVersion,
        request_id,
        payload,
    }
}
fn refused(message: String) -> ErrorDto {
    let mut error = ErrorDto::from(CoreError::rejected(ErrorCode::UnsupportedSql));
    // Static refusal text plus statement number and line; never SQL or file content.
    error.message = message;
    error
}

/// Trusted native callers only (the open dialog and host tests); never an IPC command. The
/// selection is resolved once to its canonical regular file.
pub fn authorize_import_file<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
) -> Result<ImportFileGrant, ErrorDto> {
    let session = app.state::<NativeSession>();
    if session.closing.load(Ordering::SeqCst) {
        return Err(forbidden());
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| forbidden())?;
    if !std::fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_file()) {
        return Err(forbidden());
    }
    let name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(invalid)?
        .to_owned();
    let mut grants = session.imports.0.lock().expect("import grant lock");
    if grants.len() >= MAX_IMPORT_GRANTS {
        return Err(CoreError::rejected(ErrorCode::Capacity).into());
    }
    let grant = uuid::Uuid::new_v4().to_string();
    grants.insert(grant.clone(), canonical);
    Ok(ImportFileGrant {
        grant: Some(grant),
        file_name: Some(name),
    })
}

/// Reads the granted file once. It must still be the same canonical regular file (a later
/// symlink or directory swap is refused) and within the import size limit.
fn read_snapshot(path: &Path) -> Result<Vec<u8>, ErrorDto> {
    if std::fs::canonicalize(path).ok().as_deref() != Some(path) {
        return Err(forbidden());
    }
    let file = std::fs::File::open(path).map_err(|_| forbidden())?;
    let metadata = file.metadata().map_err(|_| forbidden())?;
    if !metadata.is_file() {
        return Err(forbidden());
    }
    let limit = ExecutionLimits::MAX_IMPORT_BYTES;
    let too_large = || {
        refused(format!(
            "The file exceeds the {} MiB import limit. Nothing was run.",
            limit / (1024 * 1024)
        ))
    };
    if metadata.len() > limit as u64 {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| forbidden())?;
    if bytes.len() > limit {
        return Err(too_large());
    }
    Ok(bytes)
}

#[tauri::command]
pub async fn pick_import_file<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<ImportFileGrant>, ErrorDto> {
    let request: Envelope<ImportFileRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let picker = window
        .app_handle()
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Import SQL file")
        .add_filter("SQL", &["sql"])
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
        Some(path) => authorize_import_file(window.app_handle(), &path)?,
        None => ImportFileGrant {
            grant: None,
            file_name: None,
        },
    };
    Ok(reply(request.request_id, payload))
}

#[tauri::command]
pub async fn start_sql_import<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SqlImportAccepted>, ErrorDto> {
    let request: Envelope<SqlImportRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let input = request.payload;
    if uuid::Uuid::parse_str(&input.grant).is_err() {
        return Err(invalid());
    }
    let handle = ConnectionId::parse(&input.handle).map_err(lifecycle)?;
    let engine = session
        .connections
        .engine(session.owner, handle)
        .map_err(lifecycle)?;
    if !matches!(engine, Engine::MySql | Engine::MariaDb) {
        let mut error = ErrorDto::from(CoreError::rejected(ErrorCode::UnsupportedCapability));
        error.message = "SQL import is available for MySQL and MariaDB connections.".into();
        return Err(error);
    }
    // One-shot: the grant is consumed whether or not the import starts.
    let path = session
        .imports
        .0
        .lock()
        .expect("import grant lock")
        .remove(&input.grant)
        .ok_or_else(forbidden)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || read_snapshot(&path))
        .await
        .map_err(|_| forbidden())??;
    let size = bytes.len();
    let connections = session.connections.clone();
    let owner = session.owner;
    // Validation is CPU-bound for large files: it runs on a blocking worker.
    let started = tauri::async_runtime::spawn_blocking(move || {
        connections.start_sql_import(owner, handle, bytes)
    })
    .await
    .map_err(|_| forbidden())?;
    let (id, statement_count) = started.map_err(|error| match error {
        ImportError::Refused(refusal) => refused(refusal.to_string()),
        ImportError::Execution(error) => execution_error(error),
    })?;
    Ok(reply(
        request.request_id,
        SqlImportAccepted {
            import_id: id.opaque(),
            statement_count,
            bytes: size.to_string(),
        },
    ))
}

#[tauri::command]
pub fn sql_import_status<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SqlImportStatus>, ErrorDto> {
    let request: Envelope<SqlImportStatusRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let id = job_id(&request.payload.import_id)?;
    let status = session
        .connections
        .sql_import_status(session.owner, id)
        .map_err(execution_error)?;
    Ok(reply(request.request_id, status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_read_once_only_while_it_is_the_granted_regular_file_within_the_limit() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("stoneql-import-read-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("dump.sql");
        std::fs::write(&file, b"SELECT 1;").unwrap();
        assert_eq!(read_snapshot(&file).unwrap(), b"SELECT 1;");
        // A later symlink swap no longer resolves to the granted path.
        let target = root.join("other.sql");
        std::fs::write(&target, b"SELECT 2;").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&target, &file).unwrap();
        assert_eq!(
            read_snapshot(&file).unwrap_err().code,
            stoneql_contracts::v1::ErrorCodeDto::Forbidden
        );
        // Oversize is refused from the file's size, before reading (sparse file).
        let big = root.join("big.sql");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(ExecutionLimits::MAX_IMPORT_BYTES as u64 + 1)
            .unwrap();
        let error = read_snapshot(&big).unwrap_err();
        assert_eq!(
            error.code,
            stoneql_contracts::v1::ErrorCodeDto::UnsupportedSql
        );
        assert!(error.message.contains("1024 MiB"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
