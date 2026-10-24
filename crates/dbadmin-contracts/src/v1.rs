use dbadmin_core::{CoreError, ErrorCode, Outcome};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
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

/// Validates the envelope/limits, never SQL syntax, record values or query execution.
pub fn decode_query_request(json: &str) -> Result<Envelope<QueryRequest>, CoreError> {
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
    let payload = fields["payload"].as_object().ok_or_else(bad)?;
    if payload.len() != 5
        || !["connectionId", "database", "sql", "maxRows", "maxBytes"]
            .iter()
            .all(|key| payload.contains_key(*key))
    {
        return Err(bad());
    }
    let envelope: Envelope<QueryRequest> = serde_json::from_str(json).map_err(|_| bad())?;
    let valid_id = |id: &str| {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    };
    if !valid_id(&envelope.request_id)
        || !valid_id(&envelope.payload.connection_id)
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
    InvalidRequest,
    UnsupportedVersion,
    UnsupportedCapability,
    UnverifiedCapability,
    Conflict,
    ConnectionLost,
    Cancelled,
    Internal,
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
            ErrorCode::InvalidRequest => ErrorCodeDto::InvalidRequest,
            ErrorCode::UnsupportedVersion => ErrorCodeDto::UnsupportedVersion,
            ErrorCode::UnsupportedCapability => ErrorCodeDto::UnsupportedCapability,
            ErrorCode::UnverifiedCapability => ErrorCodeDto::UnverifiedCapability,
            ErrorCode::Conflict => ErrorCodeDto::Conflict,
            ErrorCode::ConnectionLost => ErrorCodeDto::ConnectionLost,
            ErrorCode::Cancelled => ErrorCodeDto::Cancelled,
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

pub fn typescript() -> String {
    let config = ts_rs::Config::default();
    let declarations = [
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
        String::from("// Generated by dbadmin-contracts from Rust v1 DTOs. Do not edit.\n");
    for declaration in declarations {
        output.push_str("export ");
        output.push_str(&declaration);
        output.push('\n');
    }
    output
}
