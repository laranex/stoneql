use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use stoneql_core::{CoreError, ErrorCode, ExecutionOutcome, Outcome};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion;
impl Serialize for ProtocolVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}
impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if u8::deserialize(deserializer)? == 1 {
            Ok(Self)
        } else {
            Err(de::Error::custom("unsupported protocol version"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<T> {
    #[ts(type = "1")]
    pub version: ProtocolVersion,
    pub request_id: String,
    pub payload: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellValue {
    Null,
    Text {
        text: String,
    },
    Boolean {
        value: bool,
    },
    Integer {
        text: String,
    },
    Decimal {
        text: String,
    },
    Float {
        text: String,
    },
    Temporal {
        #[serde(rename = "databaseType")]
        database_type: String,
        text: String,
    },
    Binary {
        hex: String,
    },
    Json {
        text: String,
    },
    Unknown {
        #[serde(rename = "databaseType")]
        database_type: String,
        hex: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputValue {
    Default,
    Null,
    Value { value: CellValue },
    Expression { sql: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub connection_id: String,
    pub database: Option<String>,
    pub sql: String,
    pub max_rows: u32,
    pub max_bytes: u32,
}

/// Validates an opaque protocol identifier without interpreting database names or SQL.
pub fn is_opaque_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

pub fn decode_envelope<T: serde::de::DeserializeOwned + Serialize>(
    json: &str,
) -> Result<Envelope<T>, CoreError> {
    let bad = || CoreError::rejected(ErrorCode::InvalidRequest);
    if json.len() > 131_072 {
        return Err(bad());
    }
    let object: serde_json::Value = serde_json::from_str(json).map_err(|_| bad())?;
    let fields = object.as_object().ok_or_else(bad)?;
    if fields.len() != 3
        || !["version", "requestId", "payload"]
            .iter()
            .all(|key| fields.contains_key(*key))
    {
        return Err(bad());
    }
    if fields["version"] != 1 {
        return Err(CoreError::rejected(ErrorCode::UnsupportedVersion));
    }
    let envelope: Envelope<T> = serde_json::from_str(json).map_err(|_| bad())?;
    if serde_json::to_value(&envelope).map_err(|_| bad())? != object {
        return Err(bad());
    }
    if !is_opaque_id(&envelope.request_id) {
        return Err(bad());
    }
    Ok(envelope)
}

pub fn decode_query_request(json: &str) -> Result<Envelope<QueryRequest>, CoreError> {
    let bad = || CoreError::rejected(ErrorCode::InvalidRequest);
    let envelope: Envelope<QueryRequest> = decode_envelope(json)?;
    if !is_opaque_id(&envelope.payload.connection_id)
        || envelope.payload.sql.len() > 65_536
        || envelope.payload.sql.is_empty()
        || !(1..=1000).contains(&envelope.payload.max_rows)
        || !(1..=1_048_576).contains(&envelope.payload.max_bytes)
        || envelope
            .payload
            .database
            .as_ref()
            .is_some_and(|name| name.len() > 1024)
    {
        return Err(bad());
    }
    Ok(envelope)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub database_type: String,
    pub nullable: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    pub index: u32,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<CellValue>>,
    pub affected_rows: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub result_sets: Vec<ResultSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCodeDto {
    Forbidden,
    InvalidRequest,
    UnsupportedVersion,
    UnsupportedCapability,
    UnverifiedCapability,
    Conflict,
    ConnectionLost,
    Cancelled,
    Internal,
    DeadlineExceeded,
    PacketTooLarge,
    QueryRejected,
    ConnectionUnavailable,
    Capacity,
    UnsupportedSql,
    SshHostKeyUnknown,
    SshHostKeyChanged,
    SshAuthenticationFailed,
    SshAgentUnavailable,
    SecretUnavailable,
    Busy,
    UnresolvedOutcomes,
    OutcomeJournalUnavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeDto {
    Rejected,
    RolledBack,
    Committed,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ErrorDto {
    pub code: ErrorCodeDto,
    pub message: String,
    pub retryable: bool,
    pub outcome: OutcomeDto,
}
impl From<CoreError> for ErrorDto {
    fn from(error: CoreError) -> Self {
        let code = match error.code {
            ErrorCode::Forbidden => ErrorCodeDto::Forbidden,
            ErrorCode::InvalidRequest => ErrorCodeDto::InvalidRequest,
            ErrorCode::UnsupportedVersion => ErrorCodeDto::UnsupportedVersion,
            ErrorCode::UnsupportedCapability => ErrorCodeDto::UnsupportedCapability,
            ErrorCode::UnverifiedCapability => ErrorCodeDto::UnverifiedCapability,
            ErrorCode::Conflict => ErrorCodeDto::Conflict,
            ErrorCode::ConnectionLost => ErrorCodeDto::ConnectionLost,
            ErrorCode::Cancelled => ErrorCodeDto::Cancelled,
            ErrorCode::DeadlineExceeded => ErrorCodeDto::DeadlineExceeded,
            ErrorCode::PacketTooLarge => ErrorCodeDto::PacketTooLarge,
            ErrorCode::QueryRejected => ErrorCodeDto::QueryRejected,
            ErrorCode::ConnectionUnavailable => ErrorCodeDto::ConnectionUnavailable,
            ErrorCode::Capacity => ErrorCodeDto::Capacity,
            ErrorCode::UnsupportedSql => ErrorCodeDto::UnsupportedSql,
            ErrorCode::SshHostKeyUnknown => ErrorCodeDto::SshHostKeyUnknown,
            ErrorCode::SshHostKeyChanged => ErrorCodeDto::SshHostKeyChanged,
            ErrorCode::SshAuthenticationFailed => ErrorCodeDto::SshAuthenticationFailed,
            ErrorCode::SshAgentUnavailable => ErrorCodeDto::SshAgentUnavailable,
            ErrorCode::SecretUnavailable => ErrorCodeDto::SecretUnavailable,
            ErrorCode::Busy => ErrorCodeDto::Busy,
            ErrorCode::UnresolvedOutcomes => ErrorCodeDto::UnresolvedOutcomes,
            ErrorCode::OutcomeJournalUnavailable => ErrorCodeDto::OutcomeJournalUnavailable,
            ErrorCode::Internal => ErrorCodeDto::Internal,
        };
        let outcome = match error.outcome {
            Outcome::Rejected => OutcomeDto::Rejected,
            Outcome::RolledBack => OutcomeDto::RolledBack,
            Outcome::Committed => OutcomeDto::Committed,
            Outcome::Unknown => OutcomeDto::Unknown,
        };
        Self {
            code,
            message: error.code.message().into(),
            retryable: false,
            outcome,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum EngineDto {
    MySql,
    MariaDb,
    PostgreSql,
    Sqlite,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SupportDto {
    Supported,
    Unsupported,
    Unverified,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStatus {
    pub name: String,
    pub support: SupportDto,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    pub engine: EngineDto,
    pub engine_version: String,
    pub capabilities: Vec<CapabilityStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostInfoRequest {
    pub nonce: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub runtime: String,
    pub app_version: String,
    pub session_id: String,
    pub nonce: String,
    pub simulated: bool,
    // Whether this host accepts `access: read_write` connections. A native build-time gate
    // (off until the M006 safety tests pass and the user approves); the renderer only mirrors it.
    pub read_write_sql: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProbeAction {
    Echo,
    Ready,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContractProbe {
    pub session_id: String,
    pub action: ProbeAction,
    pub values: Vec<CellValue>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub values: Vec<CellValue>,
    pub simulated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CloseAction {
    Prompted,
    Cancel,
    Confirm,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloseDecision {
    pub session_id: String,
    pub action: CloseAction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CloseResult {
    pub closed: bool,
}

/// Session-only credential transfer. Deliberately does not implement Debug.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DirectConnectionRequest {
    pub session_id: String,
    pub engine: EngineDto,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub root_certificate_pem: Option<String>,
    // Opaque OS secret reference used when `password` is empty. Resolved only by the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub password_ref: Option<String>,
    // Optional SSH tunnel. The database host/port stay the TLS identity and are resolved by
    // the SSH server; SQLite never accepts a tunnel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ssh: Option<SshTunnelRequest>,
    // Renderer-chosen opaque id that `cancel_connect` can name while setup is in flight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub attempt_id: Option<String>,
    // Omitted means read-only. Read-write is an explicit per-session opt-in for the handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub access: Option<AccessModeDto>,
}
/// SSH endpoint plus an enrolled OpenSSH public host key. Deliberately does not implement
/// Debug: authentication may carry a session-only secret.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    // The trusted host key never crosses IPC (F-01): the host reads it from its native store.
    pub auth: SshAuthRequest,
}
/// Agent auth uses the host's own agent socket; agent forwarding is never requested.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum SshAuthRequest {
    Agent,
    Password {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        password: Option<String>,
        #[serde(
            rename = "passwordRef",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        #[ts(optional)]
        password_ref: Option<String>,
    },
    /// `grant` names a key file chosen in the native picker; key bytes never cross IPC.
    KeyFile {
        grant: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        passphrase: Option<String>,
        #[serde(
            rename = "passphraseRef",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        #[ts(optional)]
        passphrase_ref: Option<String>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshEndpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SecretPurpose {
    DatabasePassword,
    SshPassword,
    SshPassphrase,
}
/// The connection target a stored secret is bound to. The host refuses to use a reference
/// for any other target, so a stored password cannot be redirected to another server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecretTarget {
    pub engine: EngineDto,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ssh: Option<SshEndpoint>,
}
/// Moves one secret into the OS store. Deliberately does not implement Debug; the reply
/// carries only the new opaque reference, never the secret.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StoreSecretRequest {
    pub session_id: String,
    pub purpose: SecretPurpose,
    pub target: SecretTarget,
    pub secret: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub reference: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecretReferenceRequest {
    pub session_id: String,
    pub reference: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyRequest {
    pub session_id: String,
    pub host: String,
    pub port: u16,
}
/// Presented, not trusted: the renderer shows the fingerprint and enrolls only on explicit
/// user confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKey {
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint: String,
}
/// Enrolls the key a native scan of exactly this endpoint presented, after native confirmation.
/// The fingerprint names which scan; the key itself never comes from the renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyTrustRequest {
    pub session_id: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
}
/// An enrolled host key, derived natively from the stored key on every read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshTrustedHostKey {
    pub algorithm: String,
    pub fingerprint: String,
}
/// Native trust state of one SSH endpoint; `trusted: null` means not enrolled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshHostKeyStatus {
    pub trusted: Option<SshTrustedHostKey>,
}
/// M007 transfer formats (alpha slice). GZIP, JSON and XLSX are not offered (D008 unapproved).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TransferFormatDto {
    Csv,
    Sql,
}
/// Asks the host to show its native save dialog. The renderer suggests a file name only; the
/// chosen path never crosses IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileRequest {
    pub session_id: String,
    pub suggested_name: String,
    pub format: TransferFormatDto,
}
/// One-shot, session-scoped destination grant; `fileName` is display-only. `None` = cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileGrant {
    pub grant: Option<String>,
    pub file_name: Option<String>,
}
/// Streams one table of a live handle into a granted destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartExportRequest {
    pub session_id: String,
    pub handle: String,
    pub grant: String,
    pub table: String,
    pub format: TransferFormatDto,
    pub structure: bool,
    pub data: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransferAccepted {
    pub transfer_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub session_id: String,
    pub transfer_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TransferStateDto {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
/// Rows/bytes are decimal strings (no JS number precision limit). On success the destination
/// holds the complete file; otherwise no file was created or replaced and the temporary file is
/// removed. `detail` is StoneQL's own text (for example the column that cannot be exported).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransferStatus {
    pub transfer_id: String,
    pub state: TransferStateDto,
    pub rows: String,
    pub bytes: String,
    pub file_name: String,
    pub error: Option<ErrorCodeDto>,
    pub detail: Option<String>,
}
/// Asks the trusted host to show its native SSH private-key picker. Carries no path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyFileRequest {
    pub session_id: String,
}
/// Opaque session-scoped key-file grant; `fileName` is display-only. `None` = cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyFileGrant {
    pub grant: Option<String>,
    pub file_name: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelConnectRequest {
    pub session_id: String,
    pub attempt_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelConnectResult {
    pub cancelled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LiveConnection {
    pub handle: String,
    pub engine_version: String,
    pub database: String,
    pub expires_after_seconds: u32,
    pub access: AccessModeDto,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHandleRequest {
    pub session_id: String,
    pub handle: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LiveQueryRequest {
    pub session_id: String,
    pub handle: String,
    pub sql: String,
    // Omitted limits use the native runtime maximums; explicit values are still bounds-checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_rows: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_bytes: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LiveConnectionStatus {
    pub connected: bool,
}
/// Asks the trusted host to show its native SQLite file picker. Carries no path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SqliteFileRequest {
    pub session_id: String,
}
/// Opaque, session-scoped grant for one user-selected file; `fileName` is display-only.
/// `None` means the picker was cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SqliteFileGrant {
    pub grant: Option<String>,
    pub file_name: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SqliteConnectionRequest {
    pub session_id: String,
    pub grant: String,
    // Omitted means read-only (SQLITE_OPEN_READONLY for every connection of the handle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub access: Option<AccessModeDto>,
}

/// Handle-level execution permission chosen at connect time. Database privileges remain the
/// authority; read-only additionally keeps M003's engine read-only enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccessModeDto {
    ReadOnly,
    ReadWrite,
}
/// Starts a bounded SQL job on a read-write handle. The script is split into statements and
/// executed one at a time; it is never retried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SubmitJobRequest {
    pub session_id: String,
    pub handle: String,
    pub sql: String,
    // Runs inside this owned transaction lease; omitted means each statement autocommits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lease_id: Option<String>,
    // Omitted limits use the runtime defaults; explicit values are bounds-checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_rows_per_result: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deadline_seconds: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobAccepted {
    pub job_id: String,
    pub statement_count: u32,
}
/// Fetches the next bounded page of a job, or requests its cancellation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobRequest {
    pub session_id: String,
    pub job_id: String,
    // Long-poll wait for new data, capped by the runtime; omitted means do not wait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wait_ms: Option<u32>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcomeDto {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}
impl From<ExecutionOutcome> for ExecutionOutcomeDto {
    fn from(outcome: ExecutionOutcome) -> Self {
        match outcome {
            ExecutionOutcome::Succeeded => Self::Succeeded,
            ExecutionOutcome::Failed => Self::Failed,
            ExecutionOutcome::Cancelled => Self::Cancelled,
            ExecutionOutcome::Unknown => Self::Unknown,
        }
    }
}
/// `not_run` statements were never sent to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StatementStatusDto {
    NotRun,
    Running,
    Succeeded,
    Failed,
    // Sent, but no definitive reply (interrupted, lost or abandoned): effects may exist.
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKindDto {
    Unique,
    ForeignKey,
    NotNull,
    Check,
}
/// Authoritative database refusal without its message text, which can contain row values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatementFailure {
    pub constraint: Option<ConstraintKindDto>,
    pub sql_state: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatementReport {
    pub index: u32,
    pub status: StatementStatusDto,
    // True inside a transaction lease: success is durable only after a confirmed commit.
    pub in_transaction: bool,
    pub affected_rows: String,
    pub result_sets: u32,
    pub failure: Option<StatementFailure>,
    // Result data of this statement (rows, pages or whole result sets) was read but not
    // delivered because the job was interrupted (cancel, deadline, close): the delivered
    // result sets of this statement are incomplete, even when not marked `truncated`.
    pub results_withheld: bool,
}
/// One bounded page of one result set. `columns` is present on the first page of each set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResultBatch {
    pub statement: u32,
    pub result: u32,
    pub columns: Option<Vec<Column>>,
    pub rows: Vec<Vec<CellValue>>,
    pub last: bool,
    pub truncated: bool,
}
// Jobs are never queued: a busy handle refuses submission with BUSY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobStateDto {
    Running,
    Finished,
}
// `requested`: a cancel or deadline interrupt was raised; `confirmed`: the work was observed to
// stop (nothing in flight, or the running statement ended within the grace period). Neither
// says anything about effects: see the statement status and job outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CancellationDto {
    None,
    Requested,
    Confirmed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LeaseStateDto {
    Active,
    // A statement in the transaction failed or was cancelled: only rollback is accepted.
    RollbackOnly,
    Ended,
}
/// `done` means the job finished and no further pages exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdate {
    pub job_id: String,
    pub state: JobStateDto,
    pub outcome: Option<ExecutionOutcomeDto>,
    pub cancellation: CancellationDto,
    // A best-effort server interrupt (KILL QUERY, pg_cancel_backend, SQLite interrupt) was
    // delivered. It is not proof that anything stopped.
    pub server_cancel_sent: bool,
    pub deadline_exceeded: bool,
    pub statements: Vec<StatementReport>,
    pub batch: Option<ResultBatch>,
    pub lease: Option<LeaseStateDto>,
    pub done: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CancelJobResult {
    pub cancel_requested: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionLease {
    pub lease_id: String,
    pub idle_seconds: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TransactionActionDto {
    Commit,
    Rollback,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EndTransactionRequest {
    pub session_id: String,
    pub lease_id: String,
    pub action: TransactionActionDto,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TransactionEndReasonDto {
    Requested,
    Expired,
    ConnectionLost,
    Closed,
}
/// `committed`/`rolled_back` only when the server confirmed COMMIT/ROLLBACK; otherwise unknown.
/// `commitSent`/`rollbackSent` say whether COMMIT/ROLLBACK was attempted on the lease's session
/// (set just before sending, so a connection lost before the bytes left still reports it); an
/// unknown outcome with either set means the server may have applied it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransactionEnd {
    pub outcome: OutcomeDto,
    pub reason: TransactionEndReasonDto,
    // COMMIT was attempted: set just before it is written, so it is also set when the
    // connection failed before any byte left. It never says the server received COMMIT.
    pub commit_sent: bool,
    // ROLLBACK was attempted, with the same meaning as `commitSent`.
    pub rollback_sent: bool,
}
/// Lists the unresolved outcomes of this app process's owner, including those recorded under
/// an earlier renderer session (before a reload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedOutcomesRequest {
    pub session_id: String,
}
/// Hides one unresolved outcome after the user explicitly acknowledged it. Idempotent; it never
/// changes the recorded outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AcknowledgeOutcomeRequest {
    pub session_id: String,
    pub outcome_id: String,
}
/// SQL-free facts of a job whose outcome is unknown. `unknownStatement` is the zero-based index
/// of the statement sent without a definitive reply (absent when the session's transaction
/// state after the job did not match).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedJob {
    pub statement_count: u32,
    pub unknown_statement: Option<u32>,
    pub in_transaction: bool,
    pub cancellation: CancellationDto,
    pub server_cancel_sent: bool,
    pub deadline_exceeded: bool,
}
/// An unknown-outcome journal that could not be read intact at startup. The damaged file was
/// renamed, never deleted or overwritten (`preservedAs`, a file name in the journal directory;
/// absent when it could not be renamed). `unreadableRecords` counts records that could not be
/// recovered (absent when the file was too damaged to count them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JournalDamage {
    pub unreadable_records: Option<u32>,
    pub preserved_as: Option<String>,
}
/// Why the durable unknown-outcome journal cannot be used; new read-write work is refused
/// (`OUTCOME_JOURNAL_UNAVAILABLE`) while one is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JournalProblemDto {
    /// Not opened yet (or the app data directory could not be resolved).
    NotOpened,
    /// The directory or file could not be created, read or locked.
    Unavailable,
    /// A symlink, not owned by the user, or accessible by group or others.
    Insecure,
    /// Another StoneQL process holds the journal.
    InUse,
    /// Written by a newer StoneQL; left untouched.
    UnsupportedVersion,
    /// Damaged and could not be renamed aside; left untouched.
    DamagedNotPreserved,
    /// The last durable write failed (for example a full disk).
    WriteFailed,
}
/// An `unknown` outcome (a transaction end or a job) the user has not acknowledged, or a
/// damaged journal (`journalDamage`). The server may or may not have applied the work; only the
/// server can tell. No SQL text, values, driver messages or credentials: `database` is the
/// database or SQLite file name of the connection. Exactly one of `transaction`, `job` and
/// `journalDamage` is present. Kept in a durable per-user journal across restarts.
/// `restored`: recorded by an earlier run of StoneQL (before a restart). `appExited`: StoneQL
/// quit or crashed while the work was in flight; the facts are the last ones journaled before
/// anything was sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedOutcome {
    pub outcome_id: String,
    pub engine: Option<EngineDto>,
    pub database: String,
    pub seconds_ago: u32,
    pub restored: bool,
    pub app_exited: bool,
    pub transaction: Option<TransactionEnd>,
    pub job: Option<UnresolvedJob>,
    pub journal_damage: Option<JournalDamage>,
}
/// Oldest first. At `limit` unresolved outcomes new jobs and transactions are refused
/// (`UNRESOLVED_OUTCOMES`) until some are acknowledged; none is ever dropped to make room.
/// `journal` reports why unknown outcomes cannot be saved durably (null when they can).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedOutcomes {
    pub outcomes: Vec<UnresolvedOutcome>,
    pub limit: u32,
    // Jobs a renderer reset detached while still running. Each may still add an unknown
    // outcome; list again until this is 0, then `outcomes` includes all of them.
    pub pending: u32,
    pub journal: Option<JournalProblemDto>,
}

pub fn typescript() -> String {
    let config = ts_rs::Config::default();
    let declarations = [
        DirectConnectionRequest::decl(&config),
        SshTunnelRequest::decl(&config),
        SshAuthRequest::decl(&config),
        SshEndpoint::decl(&config),
        SecretPurpose::decl(&config),
        SecretTarget::decl(&config),
        StoreSecretRequest::decl(&config),
        SecretReference::decl(&config),
        SecretReferenceRequest::decl(&config),
        SshHostKeyRequest::decl(&config),
        SshHostKey::decl(&config),
        SshHostKeyTrustRequest::decl(&config),
        SshTrustedHostKey::decl(&config),
        SshHostKeyStatus::decl(&config),
        TransferFormatDto::decl(&config),
        ExportFileRequest::decl(&config),
        ExportFileGrant::decl(&config),
        StartExportRequest::decl(&config),
        TransferAccepted::decl(&config),
        TransferRequest::decl(&config),
        TransferStateDto::decl(&config),
        TransferStatus::decl(&config),
        SshKeyFileRequest::decl(&config),
        SshKeyFileGrant::decl(&config),
        CancelConnectRequest::decl(&config),
        CancelConnectResult::decl(&config),
        LiveConnection::decl(&config),
        ConnectionHandleRequest::decl(&config),
        LiveQueryRequest::decl(&config),
        LiveConnectionStatus::decl(&config),
        SqliteFileRequest::decl(&config),
        SqliteFileGrant::decl(&config),
        SqliteConnectionRequest::decl(&config),
        AccessModeDto::decl(&config),
        SubmitJobRequest::decl(&config),
        JobAccepted::decl(&config),
        JobRequest::decl(&config),
        ExecutionOutcomeDto::decl(&config),
        StatementStatusDto::decl(&config),
        ConstraintKindDto::decl(&config),
        StatementFailure::decl(&config),
        StatementReport::decl(&config),
        ResultBatch::decl(&config),
        JobStateDto::decl(&config),
        CancellationDto::decl(&config),
        LeaseStateDto::decl(&config),
        JobUpdate::decl(&config),
        CancelJobResult::decl(&config),
        TransactionLease::decl(&config),
        TransactionActionDto::decl(&config),
        EndTransactionRequest::decl(&config),
        TransactionEndReasonDto::decl(&config),
        TransactionEnd::decl(&config),
        UnresolvedOutcomesRequest::decl(&config),
        AcknowledgeOutcomeRequest::decl(&config),
        UnresolvedJob::decl(&config),
        JournalDamage::decl(&config),
        JournalProblemDto::decl(&config),
        UnresolvedOutcome::decl(&config),
        UnresolvedOutcomes::decl(&config),
        CloseAction::decl(&config),
        CloseDecision::decl(&config),
        CloseResult::decl(&config),
        HostInfoRequest::decl(&config),
        HostInfo::decl(&config),
        ProbeAction::decl(&config),
        ContractProbe::decl(&config),
        ProbeResult::decl(&config),
        CellValue::decl(&config),
        InputValue::decl(&config),
        QueryRequest::decl(&config),
        Column::decl(&config),
        ResultSet::decl(&config),
        QueryResult::decl(&config),
        ErrorCodeDto::decl(&config),
        OutcomeDto::decl(&config),
        ErrorDto::decl(&config),
        EngineDto::decl(&config),
        SupportDto::decl(&config),
        CapabilityStatus::decl(&config),
        CapabilitySnapshot::decl(&config),
        Envelope::<QueryRequest>::decl(&config),
    ];
    let mut output =
        String::from("// Generated by stoneql-contracts from Rust v1 DTOs. Do not edit.\n");
    for declaration in declarations {
        output.push_str("export ");
        output.push_str(&declaration);
        output.push('\n');
    }
    output
}
