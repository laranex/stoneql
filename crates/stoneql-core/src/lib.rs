//! Host- and driver-independent domain vocabulary. No connection implementation.
use std::{collections::BTreeMap, error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    MySql,
    MariaDb,
    PostgreSql,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Direct,
    Ssh,
    Teleport,
    AwsSsm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    ReadOnlyQueries,
    Mutations,
    Transactions,
    Cancellation,
    MultipleResults,
    Routines,
    Events,
    MaterializedViews,
    SchemaMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    Supported,
    Unsupported,
    Unverified,
}

#[derive(Debug, Clone, Default)]
pub struct Capabilities(BTreeMap<Capability, Support>);
impl Capabilities {
    pub fn record(&mut self, capability: Capability, support: Support) {
        self.0.insert(capability, support);
    }
    pub fn support(&self, capability: Capability) -> Support {
        self.0
            .get(&capability)
            .copied()
            .unwrap_or(Support::Unverified)
    }
    pub fn require(&self, capability: Capability) -> Result<(), CoreError> {
        match self.support(capability) {
            Support::Supported => Ok(()),
            Support::Unsupported => Err(CoreError::rejected(ErrorCode::UnsupportedCapability)),
            Support::Unverified => Err(CoreError::rejected(ErrorCode::UnverifiedCapability)),
        }
    }
}

pub trait CapabilityProvider {
    fn engine(&self) -> Engine;
    fn capabilities(&self) -> &Capabilities;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
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
    /// The handle is running a job or holds an open transaction. Work is never queued.
    Busy,
    /// Too many unknown outcomes await the user's acknowledgement; new read-write work is
    /// refused until some are acknowledged. Nothing is dropped to make room.
    UnresolvedOutcomes,
    /// The durable journal of unknown outcomes cannot be opened, validated or written, or a
    /// damaged journal awaits acknowledgement: new read-write work is refused before anything
    /// is sent.
    OutcomeJournalUnavailable,
}
impl ErrorCode {
    pub const fn message(self) -> &'static str {
        match self {
            Self::Forbidden => "This action is not permitted.",
            Self::InvalidRequest => "Invalid request.",
            Self::UnsupportedVersion => "Unsupported protocol version.",
            Self::UnsupportedCapability => "This capability is not supported.",
            Self::UnverifiedCapability => "This capability has not been verified.",
            Self::Conflict => "The operation conflicts with current state.",
            Self::ConnectionLost => "The database connection was lost.",
            Self::Cancelled => "The operation was cancelled.",
            Self::DeadlineExceeded => {
                "The operation deadline was exceeded; server cancellation is unconfirmed."
            }
            Self::PacketTooLarge => {
                "The response exceeded the incoming packet limit; its connection was discarded."
            }
            Self::QueryRejected => {
                "The database rejected or could not complete the read-only query."
            }
            Self::ConnectionUnavailable => {
                "Connection failed. Check authentication, TLS and availability."
            }
            Self::Capacity => "Connection capacity reached.",
            Self::UnsupportedSql => "This read-only SQL form is not supported.",
            Self::SshHostKeyUnknown => {
                "The SSH host key is not enrolled. Fetch it, verify the fingerprint and trust it explicitly."
            }
            Self::SshHostKeyChanged => {
                "The SSH host key changed; connection refused. Verify the new key with the server administrator before re-enrolling."
            }
            Self::SshAuthenticationFailed => "SSH authentication failed.",
            Self::SshAgentUnavailable => "The SSH agent is unavailable.",
            Self::SecretUnavailable => {
                "The saved credential is unavailable or belongs to a different connection target."
            }
            Self::Busy => "The connection is busy with another statement or an open transaction.",
            Self::UnresolvedOutcomes => {
                "Unknown outcomes are waiting for acknowledgement; no new read-write work was started."
            }
            Self::OutcomeJournalUnavailable => {
                "Unknown outcomes cannot be saved durably right now; no new read-write work was started."
            }
            Self::Internal => "The operation could not be completed.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Rejected,
    RolledBack,
    Committed,
    Unknown,
}

/// Terminal result of a job or statement; exactly one applies. `Succeeded`/`Failed` need the
/// server's reply. `Cancelled` means stopped before the work was sent, so nothing ran.
/// An interrupted write is `Unknown` even when its end was observed: partial or committed
/// effects cannot be excluded without a server-confirmed ROLLBACK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

/// Carries only allowlisted codes/outcomes: raw driver errors and SQL are not diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    pub code: ErrorCode,
    pub outcome: Outcome,
}
impl CoreError {
    pub const fn rejected(code: ErrorCode) -> Self {
        Self {
            code,
            outcome: Outcome::Rejected,
        }
    }
    pub const fn connection_lost() -> Self {
        Self {
            code: ErrorCode::ConnectionLost,
            outcome: Outcome::Unknown,
        }
    }
}
impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code.message())
    }
}
impl Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_fail_closed_until_evidenced() {
        let mut caps = Capabilities::default();
        assert_eq!(
            caps.require(Capability::Cancellation).unwrap_err().code,
            ErrorCode::UnverifiedCapability
        );
        caps.record(Capability::Cancellation, Support::Unsupported);
        assert_eq!(
            caps.require(Capability::Cancellation).unwrap_err().code,
            ErrorCode::UnsupportedCapability
        );
        caps.record(Capability::ReadOnlyQueries, Support::Supported);
        assert!(caps.require(Capability::ReadOnlyQueries).is_ok());
    }
    #[test]
    fn connection_loss_never_claims_rollback() {
        assert_eq!(CoreError::connection_lost().outcome, Outcome::Unknown);
        assert_eq!(
            CoreError::connection_lost().to_string(),
            "The database connection was lost."
        );
    }
}
