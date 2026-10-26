use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use stoneql_core::{CoreError, ErrorCode, Outcome};
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
}
/// SSH endpoint plus an enrolled OpenSSH public host key. Deliberately does not implement
/// Debug: authentication may carry a session-only secret.
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    // Absent means not enrolled; the host refuses before contacting the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub host_key: Option<String>,
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
