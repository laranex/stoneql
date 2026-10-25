use super::{NativeSession, decode, forbidden, guard, invalid};
use dbadmin_contracts::v1::{
    ConnectionHandleRequest, DirectConnectionRequest, EngineDto, Envelope, ErrorDto,
    LiveConnection, LiveConnectionStatus, LiveQueryRequest, ProtocolVersion, ResultSet,
};
use dbadmin_core::{CoreError, Engine, ErrorCode, Outcome};
use dbadmin_runtime::{ConnectionId, DirectMySql, LifecycleError, QueryLimits, ReadError};
use std::{sync::atomic::Ordering, time::Duration};
use tauri::{Runtime, State, WebviewWindow};
fn authorize<R: Runtime>(
    window: &WebviewWindow<R>,
    session: &NativeSession,
    id: &str,
) -> Result<(), ErrorDto> {
    guard(window)?;
    if session.id != id
        || session.closing.load(Ordering::SeqCst)
        || session.allow_exit.load(Ordering::SeqCst)
    {
        return Err(forbidden());
    }
    Ok(())
}
struct RequestSlot<'a>(&'a std::sync::atomic::AtomicUsize);
impl Drop for RequestSlot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
fn admit(session: &NativeSession) -> Result<RequestSlot<'_>, ErrorDto> {
    session
        .active_requests
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            (count < 16).then_some(count + 1)
        })
        .map_err(|_| ErrorDto::from(CoreError::rejected(ErrorCode::Capacity)))?;
    Ok(RequestSlot(&session.active_requests))
}
fn lifecycle(error: LifecycleError) -> ErrorDto {
    CoreError::rejected(match error {
        LifecycleError::InvalidConfiguration | LifecycleError::EngineMismatch => {
            ErrorCode::InvalidRequest
        }
        LifecycleError::Capacity => ErrorCode::Capacity,
        LifecycleError::Forbidden => ErrorCode::Forbidden,
        LifecycleError::Unavailable => ErrorCode::ConnectionUnavailable,
        LifecycleError::Timeout => ErrorCode::DeadlineExceeded,
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
        _ => ErrorCode::QueryRejected,
    };
    CoreError {
        code,
        outcome: if matches!(error, ReadError::InvalidLimits | ReadError::UnsupportedSql) {
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
        _ => return Err(CoreError::rejected(ErrorCode::UnsupportedCapability).into()),
    };
    if input.root_certificate_pem.as_ref().is_some_and(|pem| {
        pem.len() > 16_384
            || !pem.contains("-----BEGIN CERTIFICATE-----")
            || pem.contains("PRIVATE KEY")
    }) {
        return Err(invalid());
    }
    let database = input.database.clone();
    let handle = session
        .connections
        .connect(
            session.owner,
            DirectMySql {
                engine,
                host: input.host,
                port: input.port,
                username: input.username,
                password: input.password,
                database: input.database,
                root_certificate_pem: input.root_certificate_pem.map(String::into_bytes),
            },
        )
        .await
        .map_err(lifecycle)?;
    let engine_version = session
        .connections
        .engine_version(session.owner, handle)
        .map_err(lifecycle)?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: LiveConnection {
            handle: handle.opaque(),
            engine_version,
            database,
            expires_after_seconds: 3600,
        },
    })
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
            QueryLimits {
                rows: input.max_rows as usize,
                bytes: input.max_bytes as usize,
                deadline: Duration::from_secs(3),
            },
        )
        .await
        .map_err(query_error)?;
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
        let session = NativeSession::new(false);
        let mut slots = Vec::new();
        for _ in 0..16 {
            slots.push(admit(&session).unwrap());
        }
        assert!(
            matches!(admit(&session),Err(error) if error.code==dbadmin_contracts::v1::ErrorCodeDto::Capacity)
        );
        drop(slots.pop());
        assert!(admit(&session).is_ok());
    }
}
