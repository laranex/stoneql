//! Desktop secret references (D005). Profiles hold only opaque references; secrets live in the
//! OS credential store, bound to the connection target they were saved for. There is no IPC
//! command that returns a secret: the host resolves references only while connecting.
//!
//! macOS uses the login Keychain (generic passwords). Linux uses the desktop's Secret Service
//! (`secret_service`); only when none is on the session bus does it use StoneQL's encrypted file
//! vault (`vault`, unlocked with a master password once per app session). The provider is part
//! of each reference: version-4 UUIDs name OS-store items, version-8 UUIDs name vault items, so
//! a reference is never looked up in the other store. Other operating systems are explicitly
//! unsupported until their providers are implemented and verified.
use super::{NativeSession, database::authorize, decode, invalid};
use serde::{Deserialize, Serialize};
use stoneql_contracts::v1::{
    EngineDto, Envelope, ErrorDto, ProtocolVersion, SecretPurpose, SecretReference,
    SecretReferenceRequest, SecretStoreKindDto, SecretStoreRequest, SecretStoreStatus, SshEndpoint,
    StoreSecretRequest, UnlockSecretStoreRequest,
};
use stoneql_core::{CoreError, ErrorCode};
use tauri::{Runtime, State, WebviewWindow};

#[cfg(target_os = "linux")]
mod secret_service;
#[cfg(target_os = "linux")]
pub mod vault;

/// Keychain service for production references. Tests pass their own per-run service.
pub const DEFAULT_SERVICE: &str = "me.thixpin.stoneql.credentials";
const MAX_SECRET_BYTES: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    /// No verified provider on this operating system.
    Unsupported,
    NotFound,
    /// Access denied, locked keychain or provider failure. Details are never surfaced.
    Failed,
    /// The encrypted file vault needs the master password first (Linux fallback).
    Locked,
}
/// Result of unlocking or creating the master-password vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnlockError {
    /// Wrong master password.
    Rejected,
    /// Too short, or create/unlock does not match whether a vault exists.
    Invalid,
    /// No vault on this system, or it cannot be used.
    Failed,
}

/// Minimal OS credential-store binding. Implementations must not log values.
pub trait SecretStore: Send + Sync {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecretError>;
    fn get(&self, account: &str) -> Result<Vec<u8>, SecretError>;
    fn delete(&self, account: &str) -> Result<(), SecretError>;
    /// A fresh reference naming the provider that will hold it (see the module comment).
    fn new_reference(&self) -> Result<uuid::Uuid, SecretError> {
        Ok(uuid::Uuid::new_v4())
    }
    /// Where new saves go, and the master-password vault's state.
    fn status(&self) -> SecretStoreStatus {
        SecretStoreStatus {
            kind: if cfg!(target_os = "macos") {
                SecretStoreKindDto::Keychain
            } else {
                SecretStoreKindDto::Unavailable
            },
            vault_exists: false,
            unlocked: false,
        }
    }
    /// Creates (`create`) or unlocks the master-password vault for this app session.
    fn unlock(&self, _password: &str, _create: bool) -> Result<(), UnlockError> {
        Err(UnlockError::Failed)
    }
    /// Deletes the vault and every password saved in it.
    fn reset(&self) -> Result<(), SecretError> {
        Err(SecretError::Unsupported)
    }
}

/// Identifier of the app (`tauri.conf.json`), naming its data directory.
#[cfg(target_os = "linux")]
const APP_IDENTIFIER: &str = "me.thixpin.stoneql";

/// The operating system's credential store under one service name.
pub struct OsSecretStore {
    #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
    service: String,
    /// Linux fallback; `None` when no data directory can be named.
    #[cfg(target_os = "linux")]
    vault: Option<vault::Vault>,
    /// D-Bus address of the Secret Service; `None` is the session bus (tests use private ones).
    #[cfg(target_os = "linux")]
    bus: Option<String>,
}
impl OsSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            #[cfg(target_os = "linux")]
            vault: vault::Vault::default_dir(APP_IDENTIFIER).map(vault::Vault::new),
            #[cfg(target_os = "linux")]
            bus: None,
        }
    }
    /// Tests: the vault in `dir` instead of the user's data directory.
    #[cfg(target_os = "linux")]
    pub fn with_vault_dir(service: impl Into<String>, dir: std::path::PathBuf) -> Self {
        Self {
            service: service.into(),
            vault: Some(vault::Vault::new(dir)),
            bus: None,
        }
    }
    #[cfg(all(test, target_os = "linux"))]
    fn on_bus(mut self, bus: &str) -> Self {
        self.bus = Some(bus.to_owned());
        self
    }
    #[cfg(target_os = "linux")]
    fn vault(&self) -> Result<&vault::Vault, SecretError> {
        self.vault.as_ref().ok_or(SecretError::Unsupported)
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
/// The provider a Linux reference names: version 4 = Secret Service, version 8 = vault.
#[cfg(target_os = "linux")]
fn in_vault(account: &str) -> Result<bool, SecretError> {
    match uuid::Uuid::parse_str(account).map(|id| id.get_version_num()) {
        Ok(4) => Ok(false),
        Ok(8) => Ok(true),
        _ => Err(SecretError::Failed),
    }
}
#[cfg(target_os = "linux")]
impl SecretStore for OsSecretStore {
    fn set(&self, account: &str, value: &[u8]) -> Result<(), SecretError> {
        if in_vault(account)? {
            self.vault()?.set(&self.service, account, value)
        } else {
            secret_service::set(self.bus.as_deref(), &self.service, account, value)
        }
    }
    fn get(&self, account: &str) -> Result<Vec<u8>, SecretError> {
        if in_vault(account)? {
            self.vault()?.get(&self.service, account)
        } else {
            secret_service::get(self.bus.as_deref(), &self.service, account)
        }
    }
    fn delete(&self, account: &str) -> Result<(), SecretError> {
        if in_vault(account)? {
            self.vault()?.delete(account)
        } else {
            secret_service::delete(self.bus.as_deref(), &self.service, account)
        }
    }
    /// The vault only when no Secret Service is on the bus; a present one that fails never
    /// falls back.
    fn new_reference(&self) -> Result<uuid::Uuid, SecretError> {
        match secret_service::probe(self.bus.as_deref()) {
            Ok(()) => Ok(uuid::Uuid::new_v4()),
            Err(SecretError::Unsupported) => {
                self.vault()?;
                let mut bytes = [0u8; 16];
                getrandom::fill(&mut bytes).map_err(|_| SecretError::Failed)?;
                Ok(uuid::Builder::from_custom_bytes(bytes).into_uuid())
            }
            Err(error) => Err(error),
        }
    }
    fn status(&self) -> SecretStoreStatus {
        let kind = match (secret_service::probe(self.bus.as_deref()), &self.vault) {
            (Err(SecretError::Unsupported), Some(_)) => SecretStoreKindDto::EncryptedFile,
            (Err(SecretError::Unsupported), None) => SecretStoreKindDto::Unavailable,
            _ => SecretStoreKindDto::SecretService,
        };
        let vault = self.vault.as_ref();
        SecretStoreStatus {
            kind,
            vault_exists: vault.is_some_and(|vault| vault.exists().unwrap_or(false)),
            unlocked: vault.is_some_and(vault::Vault::unlocked),
        }
    }
    fn unlock(&self, password: &str, create: bool) -> Result<(), UnlockError> {
        let vault = self.vault().map_err(|_| UnlockError::Failed)?;
        vault.unlock(password, create)
    }
    fn reset(&self) -> Result<(), SecretError> {
        self.vault()?.reset()
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
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
/// SSH passwords are sent to the server, so they are also bound to the enrolled host-key
/// fingerprint (F-01, scope v2): after a key swap or rotation a saved reference no longer
/// resolves and is never sent; the user enters it again.
pub(crate) fn ssh_password_scope(ssh: &SshEndpoint, fingerprint: &str) -> String {
    format!(
        "v2|ssh_password|{}@{}:{}|{fingerprint}",
        ssh.username,
        ssh.host.to_ascii_lowercase(),
        ssh.port
    )
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
        SecretError::Locked => CoreError::rejected(ErrorCode::SecretStoreLocked).into(),
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
    let reference = secrets
        .new_reference()
        .map_err(provider_error)?
        .hyphenated()
        .to_string();
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
        (SecretPurpose::SshPassword, Some(ssh)) => {
            // Saving requires an enrolled host key: the scope names its fingerprint.
            let key = super::ssh_transport::enrolled_key(&session, &ssh.host, ssh.port)?;
            ssh_password_scope(ssh, &key.fingerprint)
        }
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

fn store_status<T>(
    request_id: String,
    status: SecretStoreStatus,
) -> Result<Envelope<SecretStoreStatus>, T> {
    Ok(Envelope {
        version: ProtocolVersion,
        request_id,
        payload: status,
    })
}
/// Where new saves go and whether the master-password vault exists and is unlocked.
#[tauri::command]
pub async fn secret_store_status<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SecretStoreStatus>, ErrorDto> {
    let request: Envelope<SecretStoreRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let secrets = session.secrets.clone();
    let status = blocking(move || Ok(secrets.status())).await?;
    store_status(request.request_id, status)
}
/// Creates or unlocks the vault. The password (and the message carrying it) are zeroized as
/// soon as the key is derived; a wrong password answers `MASTER_PASSWORD_REJECTED` after a
/// fixed delay, with no lockout.
#[tauri::command]
pub async fn unlock_secret_store<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SecretStoreStatus>, ErrorDto> {
    let message = zeroize::Zeroizing::new(message);
    let request: Envelope<UnlockSecretStoreRequest> = decode(&message)?;
    drop(message);
    authorize(&window, &session, &request.payload.session_id)?;
    let request_id = request.request_id;
    let create = request.payload.create;
    let password = zeroize::Zeroizing::new(request.payload.password);
    let secrets = session.secrets.clone();
    let status = blocking(move || {
        secrets
            .unlock(&password, create)
            .map_err(|error| match error {
                UnlockError::Rejected => {
                    CoreError::rejected(ErrorCode::MasterPasswordRejected).into()
                }
                UnlockError::Invalid => invalid(),
                UnlockError::Failed => unavailable(),
            })?;
        Ok(secrets.status())
    })
    .await?;
    store_status(request_id, status)
}
/// Forgotten master password: deletes the vault and every password saved in it (the renderer
/// confirms first). OS-store items are untouched.
#[tauri::command]
pub async fn reset_secret_store<R: Runtime>(
    window: WebviewWindow<R>,
    session: State<'_, NativeSession>,
    message: String,
) -> Result<Envelope<SecretStoreStatus>, ErrorDto> {
    let request: Envelope<SecretStoreRequest> = decode(&message)?;
    authorize(&window, &session, &request.payload.session_id)?;
    let secrets = session.secrets.clone();
    let status = blocking(move || {
        secrets.reset().map_err(provider_error)?;
        Ok(secrets.status())
    })
    .await?;
    store_status(request.request_id, status)
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
            ssh_password_scope(&bastion(), "SHA256:a"),
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
            ssh_password_scope(&bastion(), "SHA256:a"),
            ssh_passphrase_scope(&bastion())
        );
    }
    /// F-01: a saved SSH password resolves only under the fingerprint it was saved for, so a
    /// swapped or rotated host key cannot unlock it.
    #[test]
    fn ssh_password_references_are_bound_to_the_enrolled_fingerprint() {
        let store_impl = MemoryStore::default();
        let saved = ssh_password_scope(&bastion(), "SHA256:A");
        let reference = store(&store_impl, saved.clone(), "ssh-secret".into()).unwrap();
        assert_eq!(
            resolve(&store_impl, &reference, &saved).unwrap(),
            "ssh-secret"
        );
        assert_eq!(
            resolve(
                &store_impl,
                &reference,
                &ssh_password_scope(&bastion(), "SHA256:B")
            )
            .unwrap_err()
            .code,
            stoneql_contracts::v1::ErrorCodeDto::SecretUnavailable
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
    /// Without a Secret Service on the bus, new references are vault (v8) references that need
    /// the master password; they never go to the OS store.
    #[cfg(target_os = "linux")]
    #[test]
    fn without_a_secret_service_new_saves_use_the_master_password_vault() {
        let bus = secret_service::tests::EmptyBus::start();
        let dir = bus.dir.join("data").join("credentials");
        let store_impl = OsSecretStore::with_vault_dir("me.thixpin.stoneql.test", dir.clone())
            .on_bus(&bus.address);
        let status = store_impl.status();
        assert_eq!(status.kind, SecretStoreKindDto::EncryptedFile);
        assert!(!status.vault_exists && !status.unlocked);
        assert_eq!(
            store(&store_impl, "scope".into(), "value".into())
                .unwrap_err()
                .code,
            stoneql_contracts::v1::ErrorCodeDto::SecretStoreLocked
        );
        store_impl
            .unlock("synthetic master password", true)
            .unwrap();
        let reference = store(&store_impl, "scope".into(), "value".into()).unwrap();
        assert_eq!(
            uuid::Uuid::parse_str(&reference).unwrap().get_version_num(),
            8
        );
        assert!(dir.join(format!("{reference}.item")).exists());
        assert_eq!(resolve(&store_impl, &reference, "scope").unwrap(), "value");
        // An OS-store (v4) reference is never looked up in the vault.
        let foreign = uuid::Uuid::new_v4().to_string();
        assert_eq!(store_impl.get(&foreign), Err(SecretError::Unsupported));
        store_impl.reset().unwrap();
        assert!(!store_impl.status().vault_exists);
        // After a reset the reference is simply gone; nothing asks to create a new vault.
        assert_eq!(
            resolve(&store_impl, &reference, "scope").unwrap_err().code,
            stoneql_contracts::v1::ErrorCodeDto::SecretUnavailable
        );
    }
    /// With a Secret Service present, new saves go to it (v4) and the vault directory is never
    /// created. Driven by `npm run test:secret-service`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a disposable Secret Service: npm run test:secret-service"]
    fn a_present_secret_service_never_selects_the_vault() {
        let service = std::env::var("STONEQL_KEYCHAIN_SERVICE").unwrap();
        let dir = std::env::temp_dir()
            .join(format!("stoneql-no-vault-{}", uuid::Uuid::new_v4()))
            .join("credentials");
        let store_impl = OsSecretStore::with_vault_dir(service, dir.clone());
        assert_eq!(store_impl.status().kind, SecretStoreKindDto::SecretService);
        let reference = store(&store_impl, "scope".into(), "value".into()).unwrap();
        assert_eq!(
            uuid::Uuid::parse_str(&reference).unwrap().get_version_num(),
            4
        );
        assert_eq!(resolve(&store_impl, &reference, "scope").unwrap(), "value");
        store_impl.delete(&reference).unwrap();
        assert!(!dir.parent().unwrap().exists());
    }
    /// A Secret Service that is on the bus but cannot be used (here: no default collection)
    /// fails closed: no vault reference is issued and the vault directory is never created.
    /// Driven by `npm run test:secret-service` against a keyring started without `--unlock`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a disposable Secret Service: npm run test:secret-service"]
    fn an_unusable_secret_service_fails_closed_without_the_vault() {
        let dir = std::env::temp_dir()
            .join(format!("stoneql-no-vault-{}", uuid::Uuid::new_v4()))
            .join("credentials");
        let store_impl =
            OsSecretStore::with_vault_dir("me.thixpin.stoneql.test-unusable", dir.clone());
        assert_eq!(store_impl.status().kind, SecretStoreKindDto::SecretService);
        assert_eq!(
            store(&store_impl, "scope".into(), "value".into())
                .unwrap_err()
                .code,
            stoneql_contracts::v1::ErrorCodeDto::SecretUnavailable
        );
        assert!(!dir.parent().unwrap().exists());
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
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
