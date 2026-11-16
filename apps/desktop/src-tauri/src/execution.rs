//! Tauri adapter for M006 read-write jobs and transaction leases. It only translates strict v1
//! envelopes to the host-independent runtime service; ownership, limits and outcomes live there.
use super::{
    NativeSession,
    database::{admit, authorize, lifecycle},
    decode, invalid,
};
use std::time::Duration;
use stoneql_contracts::v1::{
    AcknowledgeOutcomeRequest, CancelJobResult, ConnectionHandleRequest, EndTransactionRequest,
    Envelope, ErrorDto, JobAccepted, JobRequest, JobUpdate, OutcomeDto, ProtocolVersion,
    SubmitJobRequest, TransactionActionDto, TransactionEnd, TransactionLease, UnresolvedOutcomes,
    UnresolvedOutcomesRequest, is_opaque_id,
};
use stoneql_core::{CoreError, ErrorCode};
use stoneql_runtime::{
    ConnectionId, ExecutionError, ExecutionLimits, JobId, JobOptions, LeaseId, OutcomeId,
};
use tauri::{Runtime, State, WebviewWindow};

/// Fixed, allowlisted text only; never SQL, values or driver messages.
pub(crate) fn execution_error(error: ExecutionError) -> ErrorDto {
    let code = match error {
        ExecutionError::Forbidden => ErrorCode::Forbidden,
        ExecutionError::ReadOnlyHandle => ErrorCode::UnsupportedCapability,
        ExecutionError::InvalidLimits => ErrorCode::InvalidRequest,
        ExecutionError::Capacity => ErrorCode::Capacity,
        ExecutionError::Busy => ErrorCode::Busy,
        ExecutionError::RollbackOnly => ErrorCode::Conflict,
        ExecutionError::Script(..) => ErrorCode::UnsupportedSql,
        ExecutionError::Connection(error) => return lifecycle(error),
        ExecutionError::Database => ErrorCode::QueryRejected,
        ExecutionError::UnresolvedOutcomes => ErrorCode::UnresolvedOutcomes,
        ExecutionError::JournalUnavailable => ErrorCode::OutcomeJournalUnavailable,
    };
    let mut dto = ErrorDto::from(CoreError::rejected(code));
    // The runtime's specific refusal text is static and safe to show.
    if matches!(
        error,
        ExecutionError::Script(..)
            | ExecutionError::RollbackOnly
            | ExecutionError::ReadOnlyHandle
            | ExecutionError::UnresolvedOutcomes
            | ExecutionError::JournalUnavailable
    ) {
        dto.message = error.to_string();
    }
    dto
}
fn reply<T>(request_id: String, payload: T) -> Envelope<T> {
    Envelope {
        version: ProtocolVersion,
        request_id,
        payload,
    }
}
pub(crate) fn job_id(value: &str) -> Result<JobId, ErrorDto> {
    if !is_opaque_id(value) {
        return Err(invalid());
    }
    JobId::parse(value).map_err(execution_error)
}

#[tauri::command]
pub async fn submit_job<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<JobAccepted>, ErrorDto> {
    let request: Envelope<SubmitJobRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let input = request.payload;
    let handle = ConnectionId::parse(&input.handle).map_err(lifecycle)?;
    let lease = match &input.lease_id {
        Some(id) if is_opaque_id(id) => Some(LeaseId::parse(id).map_err(execution_error)?),
        Some(_) => return Err(invalid()),
        None => None,
    };
    let mut options = JobOptions::new(input.sql);
    options.lease = lease;
    if let Some(rows) = input.max_rows_per_result {
        options.max_rows_per_result = rows as usize;
    }
    if let Some(seconds) = input.deadline_seconds {
        options.statement_deadline = Duration::from_secs(u64::from(seconds));
    }
    let (id, count) = session
        .connections
        .submit_job(session.owner, handle, options)
        .map_err(execution_error)?;
    Ok(reply(
        request.request_id,
        JobAccepted {
            job_id: id.opaque(),
            statement_count: count as u32,
        },
    ))
}

#[tauri::command]
pub async fn fetch_job<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<JobUpdate>, ErrorDto> {
    let request: Envelope<JobRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let id = job_id(&request.payload.job_id)?;
    let wait = Duration::from_millis(u64::from(request.payload.wait_ms.unwrap_or(0)))
        .min(ExecutionLimits::FETCH_WAIT_MAX);
    let update = session
        .connections
        .fetch_job(session.owner, id, wait)
        .await
        .map_err(execution_error)?;
    if !session.is_current(&request.payload.session_id) {
        return Err(stale());
    }
    Ok(reply(request.request_id, update))
}

#[tauri::command]
pub fn cancel_job<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<CancelJobResult>, ErrorDto> {
    let request: Envelope<JobRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    if request.payload.wait_ms.is_some() {
        return Err(invalid());
    }
    let id = job_id(&request.payload.job_id)?;
    let cancel_requested = session
        .connections
        .cancel_job(session.owner, id)
        .map_err(execution_error)?;
    Ok(reply(
        request.request_id,
        CancelJobResult { cancel_requested },
    ))
}

#[tauri::command]
pub async fn begin_transaction<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<TransactionLease>, ErrorDto> {
    let request: Envelope<ConnectionHandleRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    let handle = ConnectionId::parse(&request.payload.handle).map_err(lifecycle)?;
    let lease = session
        .connections
        .begin_transaction(session.owner, handle)
        .await
        .map_err(execution_error)?;
    if !session.is_current(&request.payload.session_id) {
        // A stale renderer never learns the lease; reset already ends it.
        let _ = session
            .connections
            .end_transaction(session.owner, lease, false)
            .await;
        return Err(stale());
    }
    Ok(reply(
        request.request_id,
        TransactionLease {
            lease_id: lease.opaque(),
            idle_seconds: ExecutionLimits::LEASE_IDLE.as_secs() as u32,
        },
    ))
}

#[tauri::command]
pub async fn end_transaction<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<TransactionEnd>, ErrorDto> {
    let request: Envelope<EndTransactionRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let _slot = admit(&session)?;
    if !is_opaque_id(&request.payload.lease_id) {
        return Err(invalid());
    }
    let lease = LeaseId::parse(&request.payload.lease_id).map_err(execution_error)?;
    let end = session
        .connections
        .end_transaction(
            session.owner,
            lease,
            request.payload.action == TransactionActionDto::Commit,
        )
        .await
        .map_err(execution_error)?;
    Ok(reply(request.request_id, end.into()))
}

/// Unknown outcomes the user has not acknowledged: recorded by this app process (including
/// those of the renderer session before a reload; the owner outlives rotations) and restored
/// from the durable journal at startup (before a restart). SQL-free summaries only, plus the
/// journal's problem if unknown outcomes cannot be saved durably.
#[tauri::command]
pub fn list_unresolved_outcomes<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<UnresolvedOutcomes>, ErrorDto> {
    let request: Envelope<UnresolvedOutcomesRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    Ok(reply(
        request.request_id,
        session.connections.unresolved_outcomes(session.owner),
    ))
}

/// The user acknowledged one unknown outcome: it is hidden from the list (idempotent). The
/// recorded outcome never changes. Returns the outcomes still unresolved.
#[tauri::command]
pub fn acknowledge_outcome<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<UnresolvedOutcomes>, ErrorDto> {
    let request: Envelope<AcknowledgeOutcomeRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    if !is_opaque_id(&request.payload.outcome_id) {
        return Err(invalid());
    }
    let id = OutcomeId::parse(&request.payload.outcome_id).map_err(execution_error)?;
    let remaining = session
        .connections
        .acknowledge_outcome(session.owner, id)
        .map_err(execution_error)?;
    Ok(reply(request.request_id, remaining))
}

/// The renderer session changed while the request ran: the outcome is not reported to it.
fn stale() -> ErrorDto {
    let mut error = ErrorDto::from(CoreError::rejected(ErrorCode::Forbidden));
    error.outcome = OutcomeDto::Unknown;
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use stoneql_contracts::v1::ErrorCodeDto;
    use stoneql_runtime::ScriptError;
    #[test]
    fn errors_are_allowlisted_and_refusals_are_rejected() {
        let busy = execution_error(ExecutionError::Busy);
        assert_eq!(
            (busy.code, busy.outcome),
            (ErrorCodeDto::Busy, OutcomeDto::Rejected)
        );
        let script = execution_error(ExecutionError::Script(1, ScriptError::TransactionControl));
        assert_eq!(script.code, ErrorCodeDto::UnsupportedSql);
        assert!(script.message.contains("transaction controls"));
        assert_eq!(
            execution_error(ExecutionError::RollbackOnly).code,
            ErrorCodeDto::Conflict
        );
        assert!(!busy.retryable && !script.retryable);
        let unresolved = execution_error(ExecutionError::UnresolvedOutcomes);
        assert_eq!(
            (unresolved.code, unresolved.outcome),
            (ErrorCodeDto::UnresolvedOutcomes, OutcomeDto::Rejected)
        );
        assert!(unresolved.message.contains("acknowledge") && !unresolved.retryable);
        let journal = execution_error(ExecutionError::JournalUnavailable);
        assert_eq!(
            (journal.code, journal.outcome),
            (
                ErrorCodeDto::OutcomeJournalUnavailable,
                OutcomeDto::Rejected
            )
        );
        assert!(journal.message.starts_with("Nothing was sent") && !journal.retryable);
    }
}
