//! SSH transport commands: host-key scan for explicit enrollment, native key-file grants and
//! setup cancellation. Tunnels themselves are owned by the runtime registry per handle.
use super::{
    NativeSession,
    database::{admit, authorize, lifecycle},
    decode, forbidden, invalid,
    secrets::{SecretStore, choose, ssh_passphrase_scope, ssh_password_scope},
};
use std::{path::Path, path::PathBuf, sync::atomic::Ordering};
use stoneql_contracts::v1::{
    self, CancelConnectRequest, CancelConnectResult, Envelope, ErrorDto, ProtocolVersion,
    SshAuthRequest, SshEndpoint, SshHostKey, SshHostKeyRequest, SshKeyFileGrant, SshKeyFileRequest,
    SshTunnelRequest,
};
use stoneql_runtime::{SshAuth, SshConfig};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

pub(crate) fn endpoint(request: &SshTunnelRequest) -> SshEndpoint {
    SshEndpoint {
        host: request.host.clone(),
        port: request.port,
        username: request.username.clone(),
    }
}

/// Resolves a tunnel request into runtime configuration. Secret references are resolved only
/// for their bound SSH endpoint; the agent socket comes from the trusted host, never IPC.
pub(crate) fn tunnel_config(
    secrets: &dyn SecretStore,
    request: SshTunnelRequest,
    agent_socket: Option<PathBuf>,
) -> Result<SshConfig, ErrorDto> {
    let target = endpoint(&request);
    let auth = match request.auth {
        SshAuthRequest::Agent => SshAuth::Agent(agent_socket),
        SshAuthRequest::Password {
            password,
            password_ref,
        } => SshAuth::Password(
            choose(secrets, password, password_ref, || {
                ssh_password_scope(&target)
            })?
            .ok_or_else(invalid)?,
        ),
        SshAuthRequest::KeyFile {
            grant,
            passphrase,
            passphrase_ref,
        } => SshAuth::KeyFile {
            grant,
            passphrase: choose(secrets, passphrase, passphrase_ref, || {
                ssh_passphrase_scope(&target)
            })?,
        },
    };
    Ok(SshConfig {
        host: request.host,
        port: request.port,
        username: request.username,
        host_key: request.host_key,
        auth,
    })
}

/// Reads the server's host key for the user to verify. Never enrolls or trusts it.
#[tauri::command]
pub async fn ssh_host_key<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SshHostKey>, ErrorDto> {
    let request: Envelope<SshHostKeyRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let key = session
        .connections
        .scan_ssh_host_key(&request.payload.host, request.payload.port)
        .await
        .map_err(lifecycle)?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: SshHostKey {
            algorithm: key.algorithm,
            public_key: key.public_key,
            fingerprint: key.fingerprint,
        },
    })
}

/// Trusted native callers only (the key picker and host tests); never an IPC command.
pub fn authorize_ssh_key_file<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
) -> Result<SshKeyFileGrant, ErrorDto> {
    let session = app.state::<NativeSession>();
    if session.closing.load(Ordering::SeqCst) {
        return Err(forbidden());
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| forbidden())?;
    let grant = session
        .connections
        .grant_ssh_key_file(session.owner, &canonical)
        .map_err(lifecycle)?;
    let file_name = session
        .connections
        .ssh_key_file_name(session.owner, &grant)
        .map_err(lifecycle)?;
    Ok(SshKeyFileGrant {
        grant: Some(grant),
        file_name: Some(file_name),
    })
}

#[tauri::command]
pub async fn pick_ssh_key_file<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SshKeyFileGrant>, ErrorDto> {
    let request: Envelope<SshKeyFileRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let picker = window
        .app_handle()
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Choose SSH private key");
    let selected = tauri::async_runtime::spawn_blocking(move || {
        picker
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
    })
    .await
    .map_err(|_| forbidden())?;
    if !session.is_current(&request.payload.session_id) {
        return Err(forbidden());
    }
    let payload = match selected {
        Some(path) => authorize_ssh_key_file(window.app_handle(), &path)?,
        None => SshKeyFileGrant {
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
pub async fn cancel_connect<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<CancelConnectResult>, ErrorDto> {
    let request: Envelope<CancelConnectRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    if !v1::is_opaque_id(&request.payload.attempt_id) {
        return Err(invalid());
    }
    let cancelled = session
        .connections
        .cancel_connect(session.owner, &request.payload.attempt_id);
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: CancelConnectResult { cancelled },
    })
}
