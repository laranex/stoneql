use super::{
    NativeSession, decode, forbidden, guard, invalid,
    secrets::{blocking, choose, database_scope},
    ssh_transport::{endpoint, tunnel_config},
};
use std::{sync::atomic::Ordering, time::Duration};
use stoneql_contracts::v1::AccessModeDto;
use stoneql_contracts::v1::{
    ConnectionHandleRequest, DirectConnectionRequest, EngineDto, Envelope, ErrorDto,
    LiveConnection, LiveConnectionStatus, LiveQueryRequest, ProtocolVersion, ResultSet,
};
use stoneql_core::{CoreError, Engine, ErrorCode, Outcome};
use stoneql_runtime::{Access, ConnectionId, DirectServer, LifecycleError, QueryLimits, ReadError};
use tauri::{Runtime, State, WebviewWindow};
pub(crate) fn authorize<R: Runtime>(
    window: &WebviewWindow<R>,
    session: &NativeSession,
    id: &str,
) -> Result<(), ErrorDto> {
    guard(window)?;
    if !session.is_current(id) {
        return Err(forbidden());
    }
    Ok(())
}
pub(crate) struct RequestSlot<'a>(&'a std::sync::atomic::AtomicUsize);
impl Drop for RequestSlot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
pub(crate) fn admit(session: &NativeSession) -> Result<RequestSlot<'_>, ErrorDto> {
    session
        .active_requests
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            (count < 16).then_some(count + 1)
        })
        .map_err(|_| ErrorDto::from(CoreError::rejected(ErrorCode::Capacity)))?;
    Ok(RequestSlot(&session.active_requests))
}
/// Omitted means read-only; read-write is an explicit per-connection opt-in, refused here
/// (before credential lookup or network I/O) while the host's read-write gate is off, and over
/// SSH until the user enables SSH read-write separately (D025-9).
pub(crate) fn access(
    session: &NativeSession,
    mode: Option<AccessModeDto>,
    ssh: bool,
) -> Result<Access, ErrorDto> {
    match mode {
        Some(AccessModeDto::ReadWrite) if !session.read_write_sql => {
            let mut error = lifecycle(LifecycleError::Unsupported);
            error.message =
                "Read-write SQL is disabled in this build pending safety approval.".into();
            Err(error)
        }
        Some(AccessModeDto::ReadWrite) if ssh => Err(lifecycle(LifecycleError::Unsupported)),
        Some(AccessModeDto::ReadWrite) => Ok(Access::ReadWrite),
        _ => Ok(Access::ReadOnly),
    }
}
pub(crate) fn lifecycle(error: LifecycleError) -> ErrorDto {
    CoreError::rejected(match error {
        LifecycleError::InvalidConfiguration | LifecycleError::EngineMismatch => {
            ErrorCode::InvalidRequest
        }
        LifecycleError::Capacity => ErrorCode::Capacity,
        LifecycleError::Forbidden => ErrorCode::Forbidden,
        LifecycleError::Unavailable => ErrorCode::ConnectionUnavailable,
        LifecycleError::Timeout => ErrorCode::DeadlineExceeded,
        LifecycleError::HostKeyUnknown => ErrorCode::SshHostKeyUnknown,
        LifecycleError::HostKeyChanged => ErrorCode::SshHostKeyChanged,
        LifecycleError::SshAuthentication => ErrorCode::SshAuthenticationFailed,
        LifecycleError::AgentUnavailable => ErrorCode::SshAgentUnavailable,
        LifecycleError::Unsupported => ErrorCode::UnsupportedCapability,
        LifecycleError::Cancelled => ErrorCode::Cancelled,
    })
    .into()
}
fn query_error(error: ReadError) -> ErrorDto {
    if let ReadError::Connection(error) = error {
        return lifecycle(error);
    }
    let code = match error {
        ReadError::InvalidLimits => ErrorCode::InvalidRequest,
        ReadError::UnsupportedSql => ErrorCode::UnsupportedSql,
        ReadError::PacketTooLarge | ReadError::ResultTooWide => ErrorCode::PacketTooLarge,
        ReadError::Timeout => ErrorCode::DeadlineExceeded,
        ReadError::Busy => ErrorCode::Busy,
        _ => ErrorCode::QueryRejected,
    };
    CoreError {
        code,
        outcome: if matches!(
            error,
            ReadError::InvalidLimits | ReadError::UnsupportedSql | ReadError::Busy
        ) {
            Outcome::Rejected
        } else {
            Outcome::Unknown
        },
    }
    .into()
}
#[tauri::command]
pub async fn connect_direct<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<LiveConnection>, ErrorDto> {
    let request: Envelope<DirectConnectionRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let input = request.payload;
    let engine = match input.engine {
        EngineDto::MySql => Engine::MySql,
        EngineDto::MariaDb => Engine::MariaDb,
        EngineDto::PostgreSql => Engine::PostgreSql,
        // SQLite has no network endpoint; it is opened only through a native file grant.
        EngineDto::Sqlite => {
            return Err(CoreError::rejected(ErrorCode::UnsupportedCapability).into());
        }
    };
    if input.root_certificate_pem.as_ref().is_some_and(|pem| {
        pem.len() > 16_384
            || !pem.contains("-----BEGIN CERTIFICATE-----")
            || pem.contains("PRIVATE KEY")
    }) || input
        .attempt_id
        .as_ref()
        .is_some_and(|attempt| !stoneql_contracts::v1::is_opaque_id(attempt))
    {
        return Err(invalid());
    }
    let mode = access(&session, input.access, input.ssh.is_some())?;
    // References are resolved here, for this exact target and route, off the async workers.
    let scope = database_scope(
        &input.engine,
        &input.host,
        input.port,
        &input.username,
        input.ssh.as_ref().map(endpoint).as_ref(),
    );
    let secrets = session.secrets.clone();
    let agent_socket = session.agent_socket.clone();
    let (password, password_ref, tunnel) = (input.password, input.password_ref, input.ssh);
    let (password, tunnel) = blocking(move || {
        let password = choose(secrets.as_ref(), Some(password), password_ref, || scope)?;
        let tunnel = tunnel
            .map(|tunnel| tunnel_config(secrets.as_ref(), tunnel, agent_socket))
            .transpose()?;
        Ok((password.unwrap_or_default(), tunnel))
    })
    .await?;
    let database = input.database.clone();
    let handle = session
        .connections
        .connect_with_access(
            session.owner,
            DirectServer {
                engine,
                host: input.host,
                port: input.port,
                username: input.username,
                password,
                database: input.database,
                root_certificate_pem: input.root_certificate_pem.map(String::into_bytes),
            },
            tunnel,
            input.attempt_id.as_deref(),
            mode,
        )
        .await;
    connected(
        &session,
        &input.session_id,
        request.request_id,
        handle,
        database,
    )
    .await
}
/// Shared post-connect step: a handle that completed after a renderer bootstrap is closed
/// before any reply, so a stale renderer never learns a live handle.
pub(crate) async fn connected(
    session: &NativeSession,
    session_id: &str,
    request_id: String,
    handle: Result<ConnectionId, LifecycleError>,
    database: String,
) -> Result<Envelope<LiveConnection>, ErrorDto> {
    let handle = handle.map_err(lifecycle)?;
    if !session.is_current(session_id) {
        let _ = session.connections.disconnect(session.owner, handle).await;
        return Err(forbidden());
    }
    let engine_version = session
        .connections
        .engine_version(session.owner, handle)
        .map_err(lifecycle)?;
    let access = match session
        .connections
        .access(session.owner, handle)
        .map_err(lifecycle)?
    {
        Access::ReadOnly => AccessModeDto::ReadOnly,
        Access::ReadWrite => AccessModeDto::ReadWrite,
    };
    Ok(Envelope {
        version: ProtocolVersion,
        request_id,
        payload: LiveConnection {
            handle: handle.opaque(),
            engine_version,
            database,
            expires_after_seconds: 3600,
            access,
        },
    })
}
/// Omitted limits default to the runtime maximum; `read_only` remains the bounds authority.
fn query_limits(max_rows: Option<u32>, max_bytes: Option<u32>) -> QueryLimits {
    QueryLimits {
        rows: max_rows.map_or(QueryLimits::MAX_ROWS, |rows| rows as usize),
        bytes: max_bytes.map_or(QueryLimits::MAX_BYTES, |bytes| bytes as usize),
        deadline: Duration::from_secs(3),
    }
}
#[tauri::command]
pub async fn read_only_query<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<ResultSet>, ErrorDto> {
    let request: Envelope<LiveQueryRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let input = request.payload;
    let handle = ConnectionId::parse(&input.handle).map_err(lifecycle)?;
    let payload = session
        .connections
        .read_only(
            session.owner,
            handle,
            &input.sql,
            query_limits(input.max_rows, input.max_bytes),
        )
        .await
        .map_err(query_error)?;
    if !session.is_current(&input.session_id) {
        let mut error = forbidden();
        error.outcome = stoneql_contracts::v1::OutcomeDto::Unknown;
        return Err(error);
    }
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload,
    })
}
#[tauri::command]
pub async fn disconnect_database<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<LiveConnectionStatus>, ErrorDto> {
    let request: Envelope<ConnectionHandleRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let handle = ConnectionId::parse(&request.payload.handle).map_err(lifecycle)?;
    session
        .connections
        .disconnect(session.owner, handle)
        .await
        .map_err(lifecycle)?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: LiveConnectionStatus { connected: false },
    })
}
#[tauri::command]
pub fn database_status<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<LiveConnectionStatus>, ErrorDto> {
    let request: Envelope<ConnectionHandleRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let handle = ConnectionId::parse(&request.payload.handle).map_err(lifecycle)?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: LiveConnectionStatus {
            connected: session.connections.is_connected(session.owner, handle),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_slots_are_bounded_and_released_on_drop() {
        let session = NativeSession::new(
            false,
            crate::HostServices {
                secrets: std::sync::Arc::new(crate::secrets::tests::MemoryStore::default()),
                agent_socket: None,
                read_write_sql: false,
                outcome_journal: None,
            },
        );
        let mut slots = Vec::new();
        for _ in 0..16 {
            slots.push(admit(&session).unwrap());
        }
        assert!(
            matches!(admit(&session),Err(error) if error.code==stoneql_contracts::v1::ErrorCodeDto::Capacity)
        );
        drop(slots.pop());
        assert!(admit(&session).is_ok());
    }
    /// Fail closed from construction: until the first bootstrap opens the journal, the
    /// runtime reports it unopened (and refuses new read-write work).
    #[test]
    fn a_new_session_requires_the_outcome_journal_before_any_read_write_work() {
        let session = NativeSession::new(
            false,
            crate::HostServices {
                secrets: std::sync::Arc::new(crate::secrets::tests::MemoryStore::default()),
                agent_socket: None,
                read_write_sql: true,
                outcome_journal: None,
            },
        );
        assert_eq!(
            session
                .connections
                .unresolved_outcomes(session.owner)
                .journal,
            Some(stoneql_contracts::v1::JournalProblemDto::NotOpened)
        );
    }
    #[test]
    fn omitted_query_limits_use_runtime_maximums() {
        let limits = query_limits(None, None);
        assert_eq!(
            (limits.rows, limits.bytes, limits.deadline),
            (1000, 1_048_576, Duration::from_secs(3))
        );
        let explicit = query_limits(Some(10), Some(1024));
        assert_eq!((explicit.rows, explicit.bytes), (10, 1024));
    }
}
