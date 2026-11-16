//! SSH transport commands: host-key scan for explicit enrollment, native key-file grants and
//! setup cancellation. Tunnels themselves are owned by the runtime registry per handle.
use super::{
    NativeSession,
    database::{admit, authorize, lifecycle},
    decode, forbidden,
    host_keys::{HostKeyStore, HostKeyStoreError, validated},
    invalid,
    secrets::{SecretStore, choose, ssh_passphrase_scope, ssh_password_scope},
};
use std::{
    path::Path,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use stoneql_contracts::v1::{
    self, CancelConnectRequest, CancelConnectResult, Envelope, ErrorDto, ProtocolVersion,
    SshAuthRequest, SshEndpoint, SshHostKey, SshHostKeyRequest, SshHostKeyStatus,
    SshHostKeyTrustRequest, SshKeyFileGrant, SshKeyFileRequest, SshTrustedHostKey,
    SshTunnelRequest,
};
use stoneql_core::{CoreError, ErrorCode};
use stoneql_runtime::{HostKey, LifecycleError, SshAuth, SshConfig};
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

pub(crate) fn endpoint(request: &SshTunnelRequest) -> SshEndpoint {
    SshEndpoint {
        host: request.host.clone(),
        port: request.port,
        username: request.username.clone(),
    }
}

/// Pending native scans expire after this long (Proposed, M005 plan T4).
const PENDING_SCAN_LIFETIME: Duration = Duration::from_secs(300);
const MAX_PENDING_SCANS: usize = 16;

fn internal() -> ErrorDto {
    CoreError::rejected(ErrorCode::Internal).into()
}
/// The native store, failing closed while it is unresolved (before the first bootstrap).
pub(crate) fn host_key_store(session: &NativeSession) -> Result<Arc<dyn HostKeyStore>, ErrorDto> {
    session.host_keys.get().cloned().ok_or_else(internal)
}
fn store_error(error: HostKeyStoreError) -> ErrorDto {
    match error {
        HostKeyStoreError::Invalid => invalid(),
        HostKeyStoreError::Unavailable => internal(),
    }
}
/// The enrolled key for an endpoint, or `SSH_HOST_KEY_UNKNOWN`. Reads only native storage.
pub(crate) fn enrolled_key(
    session: &NativeSession,
    host: &str,
    port: u16,
) -> Result<HostKey, ErrorDto> {
    let stored = host_key_store(session)?
        .get(host, port)
        .map_err(store_error)?
        .ok_or_else(|| lifecycle(LifecycleError::HostKeyUnknown))?;
    validated(&stored).map_err(|_| internal())
}

/// Resolves a tunnel request into runtime configuration with the natively enrolled `host_key`.
/// Secret references are resolved only for their bound SSH endpoint (and, for passwords, the
/// enrolled fingerprint); the agent socket comes from the trusted host, never IPC.
pub(crate) fn tunnel_config(
    secrets: &dyn SecretStore,
    request: SshTunnelRequest,
    host_key: HostKey,
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
                ssh_password_scope(&target, &host_key.fingerprint)
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
        host_key: Some(host_key.public_key),
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
    // Recorded so trust can only enroll a key native code actually saw for this endpoint.
    {
        let mut pending = session
            .pending_host_keys
            .lock()
            .expect("pending host keys lock");
        pending.retain(|_, (_, at)| at.elapsed() < PENDING_SCAN_LIFETIME);
        if pending.len() >= MAX_PENDING_SCANS
            && let Some(oldest) = pending
                .iter()
                .min_by_key(|(_, (_, at))| *at)
                .map(|(endpoint, _)| endpoint.clone())
        {
            pending.remove(&oldest);
        }
        pending.insert(
            (
                request.payload.host.to_ascii_lowercase(),
                request.payload.port,
            ),
            (key.clone(), Instant::now()),
        );
    }
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

fn status(key: Option<HostKey>) -> SshHostKeyStatus {
    SshHostKeyStatus {
        trusted: key.map(|key| SshTrustedHostKey {
            algorithm: key.algorithm,
            fingerprint: key.fingerprint,
        }),
    }
}
fn checked_endpoint(host: &str, port: u16) -> Result<(), ErrorDto> {
    if host.is_empty()
        || host.len() > 253
        || port == 0
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid());
    }
    Ok(())
}

/// Enrolls the key a pending native scan of exactly `(host, port)` presented, if its
/// fingerprint matches. A different enrolled key is `SSH_HOST_KEY_CHANGED` (rotation needs an
/// explicit forget first); the same key is idempotent. Trusted native callers only (the
/// confirmed `trust_ssh_host_key` command and host tests); never an IPC command itself.
pub fn commit_ssh_host_key<R: Runtime>(
    app: &AppHandle<R>,
    host: &str,
    port: u16,
    fingerprint: &str,
) -> Result<SshHostKeyStatus, ErrorDto> {
    let session = app.state::<NativeSession>();
    if session.closing.load(Ordering::SeqCst) {
        return Err(forbidden());
    }
    checked_endpoint(host, port)?;
    let endpoint = (host.to_ascii_lowercase(), port);
    let scanned = {
        let pending = session
            .pending_host_keys
            .lock()
            .expect("pending host keys lock");
        match pending.get(&endpoint) {
            Some((key, at))
                if at.elapsed() < PENDING_SCAN_LIFETIME && key.fingerprint == fingerprint =>
            {
                key.clone()
            }
            _ => return Err(invalid()),
        }
    };
    let store = host_key_store(&session)?;
    if let Some(enrolled) = store.get(host, port).map_err(store_error)? {
        let enrolled = validated(&enrolled).map_err(|_| internal())?;
        if enrolled.fingerprint != scanned.fingerprint {
            return Err(lifecycle(LifecycleError::HostKeyChanged));
        }
    }
    store
        .trust(host, port, &scanned.public_key)
        .map_err(store_error)?;
    session
        .pending_host_keys
        .lock()
        .expect("pending host keys lock")
        .remove(&endpoint);
    Ok(status(Some(scanned)))
}

/// Native trust state of an endpoint. No network access.
#[tauri::command]
pub async fn ssh_host_key_status<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SshHostKeyStatus>, ErrorDto> {
    let request: Envelope<SshHostKeyRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    checked_endpoint(&request.payload.host, request.payload.port)?;
    let key = match enrolled_key(&session, &request.payload.host, request.payload.port) {
        Ok(key) => Some(key),
        Err(error) if error.code == v1::ErrorCodeDto::SshHostKeyUnknown => None,
        Err(error) => return Err(error),
    };
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: status(key),
    })
}

/// Enrolls a scanned host key after a native confirmation dialog the renderer cannot answer.
/// Declined → `CANCELLED`, store unchanged.
#[tauri::command]
pub async fn trust_ssh_host_key<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SshHostKeyStatus>, ErrorDto> {
    let request: Envelope<SshHostKeyTrustRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let input = request.payload;
    checked_endpoint(&input.host, input.port)?;
    let algorithm = {
        let pending = session
            .pending_host_keys
            .lock()
            .expect("pending host keys lock");
        match pending.get(&(input.host.to_ascii_lowercase(), input.port)) {
            Some((key, _)) if key.fingerprint == input.fingerprint => key.algorithm.clone(),
            _ => return Err(invalid()),
        }
    };
    let text = format!(
        "Trust the SSH host key for {}:{}?\n\n{} {}\n\nCompare this fingerprint with one obtained from the server administrator. Connections will be refused if the key later changes.",
        input.host, input.port, algorithm, input.fingerprint
    );
    let dialog = window
        .app_handle()
        .dialog()
        .message(text)
        .title("Trust SSH host key")
        .parent(&window)
        .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom(
            "Trust host key".into(),
            "Cancel".into(),
        ));
    let approved = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show())
        .await
        .map_err(|_| forbidden())?;
    if !session.is_current(&input.session_id) {
        return Err(forbidden());
    }
    if !approved {
        return Err(lifecycle(LifecycleError::Cancelled));
    }
    let payload = commit_ssh_host_key(
        window.app_handle(),
        &input.host,
        input.port,
        &input.fingerprint,
    )?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload,
    })
}

/// Forgets an endpoint's trust. Fail-closed (it can only cause refusals), so no native dialog;
/// the renderer confirms in-app. Idempotent.
#[tauri::command]
pub async fn forget_ssh_host_key<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SshHostKeyStatus>, ErrorDto> {
    let request: Envelope<SshHostKeyRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    checked_endpoint(&request.payload.host, request.payload.port)?;
    host_key_store(&session)?
        .forget(&request.payload.host, request.payload.port)
        .map_err(store_error)?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: status(None),
    })
}
