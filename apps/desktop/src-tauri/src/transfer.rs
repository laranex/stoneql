//! M007 export commands: native save grants and streaming table export into a temporary file
//! beside the destination, renamed into place only after a complete, flushed write. The renderer
//! never sees a path. Failure, cancel, renderer reset and shutdown remove the temporary file, so
//! a destination is either fully written or untouched.
use super::{
    NativeSession,
    database::{admit, authorize, lifecycle},
    decode, forbidden, invalid,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::Ordering},
};
use stoneql_contracts::v1::{
    Envelope, ErrorCodeDto, ErrorDto, ExportFileGrant, ExportFileRequest, ProtocolVersion,
    StartExportRequest, TransferAccepted, TransferFormatDto, TransferRequest, TransferStateDto,
    TransferStatus,
};
use stoneql_core::{CoreError, ErrorCode};
use stoneql_runtime::{
    ConnectionId, ExportFormat, ExportProgress, ExportRequest, ReadError, TransferError,
};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Outstanding destination grants and transfer records per renderer session (Proposed limits).
const MAX_EXPORT_GRANTS: usize = 8;
const MAX_TRANSFERS: usize = 16;

pub(crate) struct Transfer {
    progress: Arc<ExportProgress>,
    file_name: String,
    state: Mutex<(TransferStateDto, Option<ErrorCodeDto>, Option<String>)>,
}
#[derive(Default)]
pub(crate) struct Transfers {
    grants: Mutex<HashMap<String, (PathBuf, String)>>,
    running: Mutex<HashMap<String, Arc<Transfer>>>,
}
impl Transfers {
    /// Renderer reset/shutdown: grants are revoked and running exports cancelled (their tasks
    /// remove the temporary files).
    pub(crate) fn reset(&self) {
        self.grants.lock().expect("export grant lock").clear();
        for transfer in self.running.lock().expect("transfer lock").values() {
            transfer.progress.cancel.store(true, Ordering::SeqCst);
        }
    }
}

fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

/// Trusted native callers only (the save dialog and host tests); never an IPC command. The
/// directory must exist; an existing destination must be a regular file (never a symlink).
pub fn authorize_export_file<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
) -> Result<ExportFileGrant, ErrorDto> {
    let session = app.state::<NativeSession>();
    if session.closing.load(Ordering::SeqCst) {
        return Err(forbidden());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| valid_file_name(name))
        .ok_or_else(invalid)?
        .to_owned();
    let directory =
        std::fs::canonicalize(path.parent().ok_or_else(invalid)?).map_err(|_| forbidden())?;
    if !directory.is_dir() {
        return Err(forbidden());
    }
    if let Ok(existing) = std::fs::symlink_metadata(directory.join(&name))
        && !existing.file_type().is_file()
    {
        return Err(forbidden());
    }
    let mut grants = session.transfers.grants.lock().expect("export grant lock");
    if grants.len() >= MAX_EXPORT_GRANTS {
        return Err(CoreError::rejected(ErrorCode::Capacity).into());
    }
    let grant = uuid::Uuid::new_v4().to_string();
    grants.insert(grant.clone(), (directory, name.clone()));
    Ok(ExportFileGrant {
        grant: Some(grant),
        file_name: Some(name),
    })
}

#[tauri::command]
pub async fn pick_export_file<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<ExportFileGrant>, ErrorDto> {
    let request: Envelope<ExportFileRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    if !valid_file_name(&request.payload.suggested_name) {
        return Err(invalid());
    }
    let (label, extension) = match request.payload.format {
        TransferFormatDto::Csv => ("CSV", "csv"),
        TransferFormatDto::Sql => ("SQL", "sql"),
    };
    let dialog = window
        .app_handle()
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Export to file")
        .set_file_name(&request.payload.suggested_name)
        .add_filter(label, &[extension]);
    let selected = tauri::async_runtime::spawn_blocking(move || {
        dialog
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(|_| forbidden())?;
    if !session.is_current(&request.payload.session_id) {
        return Err(forbidden());
    }
    let payload = match selected {
        Some(path) => authorize_export_file(window.app_handle(), &path)?,
        None => ExportFileGrant {
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

fn transfer_error(error: &TransferError) -> (ErrorCodeDto, Option<String>) {
    match error {
        TransferError::Forbidden => (ErrorCodeDto::Forbidden, None),
        TransferError::InvalidRequest => (ErrorCodeDto::InvalidRequest, None),
        TransferError::Unsupported(detail) => {
            (ErrorCodeDto::UnsupportedCapability, Some(detail.clone()))
        }
        TransferError::Cancelled => (ErrorCodeDto::Cancelled, None),
        TransferError::Io => (
            ErrorCodeDto::Internal,
            Some("The destination could not be written (for example, the disk is full).".into()),
        ),
        TransferError::Read(ReadError::PacketTooLarge) => (
            ErrorCodeDto::PacketTooLarge,
            Some("A row exceeds the 2 MiB packet limit.".into()),
        ),
        TransferError::Read(read) => (crate::database::query_error(*read).code, None),
    }
}

#[tauri::command]
pub async fn start_export<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<TransferAccepted>, ErrorDto> {
    let request: Envelope<StartExportRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let input = request.payload;
    let handle = ConnectionId::parse(&input.handle).map_err(lifecycle)?;
    let format = match input.format {
        TransferFormatDto::Csv => ExportFormat::Csv,
        TransferFormatDto::Sql => ExportFormat::Sql,
    };
    let (directory, name) = session
        .transfers
        .grants
        .lock()
        .expect("export grant lock")
        .remove(&input.grant)
        .ok_or_else(forbidden)?;
    {
        let running = session.transfers.running.lock().expect("transfer lock");
        let active = running
            .values()
            .filter(|t| t.state.lock().expect("transfer state").0 == TransferStateDto::Running)
            .count();
        if running.len() >= MAX_TRANSFERS && active == running.len() {
            return Err(CoreError::rejected(ErrorCode::Capacity).into());
        }
    }
    let temporary = directory.join(format!(".{name}.stoneql-{}.part", uuid::Uuid::new_v4()));
    let file = create_private(&temporary).map_err(|_| forbidden())?;
    let id = uuid::Uuid::new_v4().to_string();
    let transfer = Arc::new(Transfer {
        progress: Arc::new(ExportProgress::default()),
        file_name: name.clone(),
        state: Mutex::new((TransferStateDto::Running, None, None)),
    });
    {
        let mut running = session.transfers.running.lock().expect("transfer lock");
        // Finished records make room for new ones (oldest arbitrary; bounded map).
        if running.len() >= MAX_TRANSFERS
            && let Some(done) = running
                .iter()
                .find(|(_, t)| {
                    t.state.lock().expect("transfer state").0 != TransferStateDto::Running
                })
                .map(|(key, _)| key.clone())
        {
            running.remove(&done);
        }
        running.insert(id.clone(), transfer.clone());
    }
    let connections = session.connections.clone();
    let owner = session.owner;
    let export = ExportRequest {
        table: input.table,
        format,
        structure: input.structure,
        data: input.data,
    };
    let destination = directory.join(&name);
    tauri::async_runtime::spawn(async move {
        let mut out = tokio::io::BufWriter::new(tokio::fs::File::from_std(file));
        let result = connections
            .export_table(owner, handle, &export, &mut out, &transfer.progress)
            .await;
        let finished = match result {
            Ok(_) => finish(out, &temporary, &destination)
                .await
                .map_err(|_| TransferError::Io),
            Err(error) => Err(error),
        };
        if finished.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        let mut state = transfer.state.lock().expect("transfer state");
        *state = match finished {
            Ok(()) => (TransferStateDto::Succeeded, None, None),
            Err(TransferError::Cancelled) => (
                TransferStateDto::Cancelled,
                Some(ErrorCodeDto::Cancelled),
                None,
            ),
            Err(error) => {
                let (code, detail) = transfer_error(&error);
                (TransferStateDto::Failed, Some(code), detail)
            }
        };
    });
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: TransferAccepted { transfer_id: id },
    })
}

#[cfg(unix)]
fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}
#[cfg(not(unix))]
fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}
/// Flushes and syncs the temporary file, then renames it over the destination, refusing to
/// replace anything but a regular file (a symlink placed after the grant is not followed).
async fn finish(
    mut out: tokio::io::BufWriter<tokio::fs::File>,
    temporary: &Path,
    destination: &Path,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    out.flush().await?;
    out.get_mut().sync_all().await?;
    drop(out);
    if let Ok(existing) = std::fs::symlink_metadata(destination)
        && !existing.file_type().is_file()
    {
        return Err(std::io::Error::other("destination is not a regular file"));
    }
    tokio::fs::rename(temporary, destination).await
}

fn status(id: &str, transfer: &Transfer) -> TransferStatus {
    let state = transfer.state.lock().expect("transfer state");
    TransferStatus {
        transfer_id: id.to_owned(),
        state: state.0,
        rows: transfer.progress.rows.load(Ordering::Relaxed).to_string(),
        bytes: transfer.progress.bytes.load(Ordering::Relaxed).to_string(),
        file_name: transfer.file_name.clone(),
        error: state.1.clone(),
        detail: state.2.clone(),
    }
}
fn lookup(session: &NativeSession, id: &str) -> Result<Arc<Transfer>, ErrorDto> {
    session
        .transfers
        .running
        .lock()
        .expect("transfer lock")
        .get(id)
        .cloned()
        .ok_or_else(forbidden)
}

#[tauri::command]
pub async fn export_status<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<TransferStatus>, ErrorDto> {
    let request: Envelope<TransferRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let transfer = lookup(&session, &request.payload.transfer_id)?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: status(&request.payload.transfer_id, &transfer),
    })
}

#[tauri::command]
pub async fn cancel_export<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<TransferStatus>, ErrorDto> {
    let request: Envelope<TransferRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let transfer = lookup(&session, &request.payload.transfer_id)?;
    transfer.progress.cancel.store(true, Ordering::SeqCst);
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: status(&request.payload.transfer_id, &transfer),
    })
}
