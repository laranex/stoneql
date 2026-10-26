//! Desktop secret references (D005). Profiles hold only opaque references; secrets live in the
//! OS credential store, bound to the connection target they were saved for. There is no IPC
//! command that returns a secret: the host resolves references only while connecting.
//!
//! macOS uses the login Keychain (generic passwords). Other operating systems are explicitly
//! unsupported until their providers are implemented and verified.
use super::{NativeSession, database::authorize, decode, invalid};
use serde::{Deserialize, Serialize};
use stoneql_contracts::v1::{
    EngineDto, Envelope, ErrorDto, ProtocolVersion, SecretPurpose, SecretReference,
    SecretReferenceRequest, SshEndpoint, StoreSecretRequest,
};
use stoneql_core::{CoreError, ErrorCode};
use tauri::{Runtime, State, WebviewWindow};

/// Keychain service for production references. Tests pass their own per-run service.
pub const DEFAULT_SERVICE: &str = "app.dbadmin.desktop.credentials";
const MAX_SECRET_BYTES: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    /// No verified provider on this operating system.
    Unsupported,
    NotFound,
    /// Access denied, locked keychain or provider failure. Details are never surfaced.
    Failed,
}

/// Minimal OS credential-store binding. Implementations must not log values.
pub trait SecretStore: Send + Sync {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecretError>;
    fn get(&self, account: &str) -> Result<Vec<u8>, SecretError>;
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// The operating system's credential store under one service name.
pub struct OsSecretStore {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    service: String,
}
impl OsSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}
#[cfg(target_os = "macos")]
impl SecretStore for OsSecretStore {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecretError> {
        security_framework::passwords::set_generic_password(&self.service, account, value)
            .map_err(|_| SecretError::Failed)
    }
    fn get(&self, account: &str) -> Result<Vec<u8>, SecretError> {
        security_framework::passwords::get_generic_password(&self.service, account).map_err(
            |error| {
                if error.code() == ITEM_NOT_FOUND {
                    SecretError::NotFound
                } else {
                    SecretError::Failed
                }
            },
        )
    }
    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match security_framework::passwords::delete_generic_password(&self.service, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ITEM_NOT_FOUND => Err(SecretError::NotFound),
            Err(_) => Err(SecretError::Failed),
        }
    }
}
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = -25300;
#[cfg(not(target_os = "macos"))]
impl SecretStore for OsSecretStore {
    fn set(&self, _: &str, _: &[u8]) -> Result<(), SecretError> {
        Err(SecretError::Unsupported)
    }
    fn get(&self, _: &str) -> Result<Vec<u8>, SecretError> {
        Err(SecretError::Unsupported)
    }
    fn delete(&self, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unsupported)
    }
}

/// Stored item: the secret plus the exact target scope it may be used for.
#[derive(Serialize, Deserialize)]
struct Item {
    v: u8,
    scope: String,
    secret: String,
}

fn ssh_scope(purpose: &str, ssh: &SshEndpoint) -> String {
    format!(
        "v1|{purpose}|{}@{}:{}",
        ssh.username,
        ssh.host.to_ascii_lowercase(),
        ssh.port
    )
}
fn engine_name(engine: &EngineDto) -> &'static str {
    match engine {
        EngineDto::MySql => "mysql",
        EngineDto::MariaDb => "mariadb",
        EngineDto::PostgreSql => "postgresql",
        EngineDto::Sqlite => "sqlite",
    }
}
/// Database passwords are bound to engine, account, server and route (direct or which SSH
/// endpoint), so a renderer cannot replay a saved password against another server.
pub(crate) fn database_scope(
    engine: &EngineDto,
    host: &str,
    port: u16,
    username: &str,
    ssh: Option<&SshEndpoint>,
) -> String {
    format!(
        "v1|database_password|{}|{username}@{}:{port}|{}",
        engine_name(engine),
        host.to_ascii_lowercase(),
        ssh.map_or_else(|| "direct".to_owned(), |ssh| ssh_scope("via", ssh))
    )
}
pub(crate) fn ssh_password_scope(ssh: &SshEndpoint) -> String {
    ssh_scope("ssh_password", ssh)
}
pub(crate) fn ssh_passphrase_scope(ssh: &SshEndpoint) -> String {
    ssh_scope("ssh_passphrase", ssh)
}

fn unavailable() -> ErrorDto {
    CoreError::rejected(ErrorCode::SecretUnavailable).into()
}
fn provider_error(error: SecretError) -> ErrorDto {
    match error {
        SecretError::Unsupported => CoreError::rejected(ErrorCode::UnsupportedCapability).into(),
        SecretError::NotFound | SecretError::Failed => unavailable(),
    }
}
fn valid_secret(secret: &str) -> bool {
    !secret.is_empty() && secret.len() <= MAX_SECRET_BYTES && !secret.contains('\0')
}
fn reference_id(reference: &str) -> Result<String, ErrorDto> {
    uuid::Uuid::parse_str(reference)
        .map(|id| id.hyphenated().to_string())
        .map_err(|_| invalid())
}

/// Saves `secret` under a fresh opaque reference bound to `scope`.
pub(crate) fn store(
    secrets: &dyn SecretStore,
    scope: String,
    secret: String,
) -> Result<String, ErrorDto> {
    if !valid_secret(&secret) {
        return Err(invalid());
    }
    let reference = uuid::Uuid::new_v4().hyphenated().to_string();
    let item = serde_json::to_vec(&Item {
        v: 1,
        scope,
        secret,
    })
    .map_err(|_| invalid())?;
    secrets.set(&reference, &item).map_err(provider_error)?;
    Ok(reference)
}
/// Returns the secret only when the reference exists and was saved for `scope`.
pub(crate) fn resolve(
    secrets: &dyn SecretStore,
    reference: &str,
    scope: &str,
) -> Result<String, ErrorDto> {
    let reference = reference_id(reference)?;
    let bytes = secrets.get(&reference).map_err(provider_error)?;
    let item: Item = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if item.v != 1 || item.scope != scope || !valid_secret(&item.secret) {
        return Err(unavailable());
    }
    Ok(item.secret)
}

/// Picks a request's secret: a session-only value or a reference, never both.
pub(crate) fn choose(
    secrets: &dyn SecretStore,
    value: Option<String>,
    reference: Option<String>,
    scope: impl FnOnce() -> String,
) -> Result<Option<String>, ErrorDto> {
    match (value.filter(|value| !value.is_empty()), reference) {
        (Some(_), Some(_)) => Err(invalid()),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(reference)) => resolve(secrets, &reference, &scope()).map(Some),
        (None, None) => Ok(None),
    }
}

/// Keychain calls may block (unlock prompt); keep them off async workers.
pub(crate) async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, ErrorDto> + Send + 'static,
) -> Result<T, ErrorDto> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| ErrorDto::from(CoreError::rejected(ErrorCode::Internal)))?
}

#[tauri::command]
pub async fn store_secret<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SecretReference>, ErrorDto> {
    let request: Envelope<StoreSecretRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let input = request.payload;
    let target = input.target;
    let scope = match (input.purpose, target.ssh.as_ref()) {
        (SecretPurpose::DatabasePassword, ssh) if target.engine != EngineDto::Sqlite => {
            database_scope(
                &target.engine,
                &target.host,
                target.port,
                &target.username,
                ssh,
            )
        }
        (SecretPurpose::SshPassword, Some(ssh)) => ssh_password_scope(ssh),
        (SecretPurpose::SshPassphrase, Some(ssh)) => ssh_passphrase_scope(ssh),
        _ => return Err(invalid()),
    };
    let secrets = session.secrets.clone();
    let reference = blocking(move || store(secrets.as_ref(), scope, input.secret)).await?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: SecretReference { reference },
    })
}

/// Forgets a reference. Missing items are treated as already deleted.
#[tauri::command]
pub async fn delete_secret<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SecretReference>, ErrorDto> {
    let request: Envelope<SecretReferenceRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let reference = reference_id(&request.payload.reference)?;
    let secrets = session.secrets.clone();
    let account = reference.clone();
    blocking(move || match secrets.delete(&account) {
        Ok(()) | Err(SecretError::NotFound) => Ok(()),
        Err(error) => Err(provider_error(error)),
    })
    .await?;
    Ok(Envelope {
        version: ProtocolVersion,
        request_id: request.request_id,
        payload: SecretReference { reference },
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};
    #[derive(Default)]
    pub(crate) struct MemoryStore(pub(crate) Mutex<HashMap<String, Vec<u8>>>);
    impl SecretStore for MemoryStore {
        fn set(&self, account: &str, value: &[u8]) -> Result<(), SecretError> {
            self.0.lock().unwrap().insert(account.into(), value.into());
            Ok(())
        }
        fn get(&self, account: &str) -> Result<Vec<u8>, SecretError> {
            self.0
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .ok_or(SecretError::NotFound)
        }
        fn delete(&self, account: &str) -> Result<(), SecretError> {
            self.0
                .lock()
                .unwrap()
                .remove(account)
                .map(|_| ())
                .ok_or(SecretError::NotFound)
        }
    }
    fn bastion() -> SshEndpoint {
        SshEndpoint {
            host: "Bastion.Example".into(),
            port: 22,
            username: "deploy".into(),
        }
    }
    #[test]
    fn references_resolve_only_for_their_bound_target() {
        let store_impl = MemoryStore::default();
        let direct = database_scope(&EngineDto::MySql, "db.example", 3306, "app", None);
        let reference = store(&store_impl, direct.clone(), "s3cret".into()).unwrap();
        assert!(uuid::Uuid::parse_str(&reference).is_ok());
        assert_eq!(resolve(&store_impl, &reference, &direct).unwrap(), "s3cret");
        for other in [
            database_scope(&EngineDto::MySql, "evil.example", 3306, "app", None),
            database_scope(&EngineDto::MariaDb, "db.example", 3306, "app", None),
            database_scope(&EngineDto::MySql, "db.example", 3307, "app", None),
            database_scope(&EngineDto::MySql, "db.example", 3306, "root", None),
            database_scope(
                &EngineDto::MySql,
                "db.example",
                3306,
                "app",
                Some(&bastion()),
            ),
            ssh_password_scope(&bastion()),
        ] {
            assert_eq!(
                resolve(&store_impl, &reference, &other).unwrap_err().code,
                stoneql_contracts::v1::ErrorCodeDto::SecretUnavailable
            );
        }
        // Host names are case-insensitive; the SSH route is part of the identity.
        assert_eq!(
            database_scope(&EngineDto::MySql, "DB.example", 3306, "app", None),
            direct
        );
        assert_ne!(
            ssh_password_scope(&bastion()),
            ssh_passphrase_scope(&bastion())
        );
    }
    #[test]
    fn stored_items_never_contain_more_than_scope_and_secret() {
        let store_impl = MemoryStore::default();
        let reference = store(&store_impl, "scope".into(), "value".into()).unwrap();
        let raw = store_impl.get(&reference).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&raw).unwrap(),
            serde_json::json!({"v":1,"scope":"scope","secret":"value"})
        );
        assert!(store(&store_impl, "scope".into(), String::new()).is_err());
        assert!(store(&store_impl, "scope".into(), "nul\0".into()).is_err());
        assert!(resolve(&store_impl, "not-a-uuid", "scope").is_err());
        assert!(resolve(&store_impl, &uuid::Uuid::new_v4().to_string(), "scope").is_err());
    }
    #[test]
    fn a_request_names_a_value_or_a_reference_never_both() {
        let store_impl = MemoryStore::default();
        let reference = store(&store_impl, "scope".into(), "saved".into()).unwrap();
        let scope = || "scope".to_owned();
        assert_eq!(
            choose(&store_impl, Some("typed".into()), None, scope).unwrap(),
            Some("typed".into())
        );
        assert_eq!(
            choose(
                &store_impl,
                Some(String::new()),
                Some(reference.clone()),
                scope
            )
            .unwrap(),
            Some("saved".into())
        );
        assert!(choose(&store_impl, Some("typed".into()), Some(reference), scope).is_err());
        assert_eq!(choose(&store_impl, None, None, scope).unwrap(), None);
    }
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn other_operating_systems_are_explicitly_unsupported() {
        let store_impl = OsSecretStore::new("unused");
        assert_eq!(store_impl.set("a", b"b"), Err(SecretError::Unsupported));
        assert_eq!(
            store(&store_impl, "scope".into(), "value".into())
                .unwrap_err()
                .code,
            stoneql_contracts::v1::ErrorCodeDto::UnsupportedCapability
        );
    }
}
