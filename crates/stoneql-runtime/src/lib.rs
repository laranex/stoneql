//! Host-independent connection lifecycle. No renderer/HTTP access or credential persistence.
mod mysql;
mod postgres;
mod read_only;
mod sqlite;
pub use read_only::{QueryLimits, ReadError};
use sqlx::{MySqlPool, PgPool, SqlitePool};
use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use stoneql_core::Engine;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, oneshot},
    time::Instant,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OwnerId(Uuid);
impl OwnerId {
    /// Created by a trusted host session, never supplied by an untrusted request.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for OwnerId {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionId(Uuid);
impl ConnectionId {
    pub fn parse(value: &str) -> Result<Self, LifecycleError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| LifecycleError::Forbidden)
    }
    pub fn opaque(self) -> String {
        self.0.to_string()
    }
}

/// Network engine profile (MySQL, MariaDB or PostgreSQL). Never derives Debug/Serialize:
/// credentials and certificate material stay in memory.
pub struct DirectServer {
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub root_certificate_pem: Option<Vec<u8>>,
}
impl DirectServer {
    fn validate(&self) -> Result<(), LifecycleError> {
        if !matches!(
            self.engine,
            Engine::MySql | Engine::MariaDb | Engine::PostgreSql
        ) || self.host.is_empty()
            || self.host.len() > 253
            || self
                .host
                .chars()
                .any(|character| character.is_whitespace() || character == '\0')
            || self.port == 0
            || self.username.contains('\0')
            || self.database.contains('\0')
            || self.password.contains('\0')
            || self.username.is_empty()
            || self.username.len() > 256
            || self.database.is_empty()
            || self.database.len() > 256
            || self.password.len() > 16_384
            || self
                .root_certificate_pem
                .as_ref()
                .is_some_and(|pem| pem.is_empty() || pem.len() > 1_048_576)
        {
            return Err(LifecycleError::InvalidConfiguration);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidConfiguration,
    Capacity,
    Forbidden,
    Unavailable,
    Timeout,
    EngineMismatch,
}
impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidConfiguration => "Invalid connection configuration.",
            Self::Capacity => "Connection capacity reached.",
            Self::Forbidden => "Connection is not available to this session.",
            Self::Unavailable => "Connection failed. Check authentication, TLS and availability.",
            Self::Timeout => "Connection deadline exceeded.",
            Self::EngineMismatch => "The connected database engine does not match the profile.",
        })
    }
}
impl std::error::Error for LifecycleError {}
fn connect_error(error: sqlx::Error) -> LifecycleError {
    if matches!(error, sqlx::Error::PoolTimedOut) {
        LifecycleError::Timeout
    } else {
        LifecycleError::Unavailable
    }
}
/// One bounded single-connection pool per logical handle. Engine-specific work stays in the
/// engine modules; the registry only owns lifetime, ownership and cleanup.
#[derive(Clone)]
enum EnginePool {
    MySql(MySqlPool),
    Postgres(PgPool),
    Sqlite(SqlitePool),
}
impl EnginePool {
    async fn close(&self) {
        match self {
            Self::MySql(pool) => pool.close().await,
            Self::Postgres(pool) => pool.close().await,
            Self::Sqlite(pool) => pool.close().await,
        }
    }
    fn is_closed(&self) -> bool {
        match self {
            Self::MySql(pool) => pool.is_closed(),
            Self::Postgres(pool) => pool.is_closed(),
            Self::Sqlite(pool) => pool.is_closed(),
        }
    }
}
struct Entry {
    engine_version: String,
    owner: OwnerId,
    pool: EnginePool,
    expires: Instant,
    _permit: OwnedSemaphorePermit,
    _expiry_cancel: oneshot::Sender<()>,
}
struct Inner {
    generation: AtomicU64,
    entries: Mutex<HashMap<ConnectionId, Entry>>,
    grants: Mutex<HashMap<Uuid, (OwnerId, PathBuf)>>,
    slots: Arc<Semaphore>,
    deadline: Duration,
    lifetime: Duration,
}
/// Clone shares one bounded registry; every handle is scoped to its trusted owner.
#[derive(Clone)]
pub struct Connections(Arc<Inner>);
impl Connections {
    /// Outstanding SQLite file grants per registry; reset/shutdown revoke them all.
    pub const MAX_FILE_GRANTS: usize = 32;
    pub fn new(
        capacity: usize,
        deadline: Duration,
        lifetime: Duration,
    ) -> Result<Self, LifecycleError> {
        if !(1..=64).contains(&capacity)
            || deadline.is_zero()
            || deadline > Duration::from_secs(30)
            || lifetime.is_zero()
            || lifetime > Duration::from_secs(86_400)
        {
            return Err(LifecycleError::InvalidConfiguration);
        }
        Ok(Self(Arc::new(Inner {
            generation: AtomicU64::new(0),
            entries: Mutex::new(HashMap::new()),
            grants: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(capacity)),
            deadline,
            lifetime,
        })))
    }
    /// Opens a verified-TLS MySQL, MariaDB or PostgreSQL handle.
    pub async fn connect(
        &self,
        owner: OwnerId,
        config: DirectServer,
    ) -> Result<ConnectionId, LifecycleError> {
        config.validate()?;
        let generation = self.0.generation.load(Ordering::SeqCst);
        let permit = self.permit()?;
        let deadline = self.0.deadline;
        let (pool, version) = match config.engine {
            Engine::PostgreSql => postgres::open(&config, deadline)
                .await
                .map(|(pool, version)| (EnginePool::Postgres(pool), version))?,
            _ => mysql::open(&config, deadline)
                .await
                .map(|(pool, version)| (EnginePool::MySql(pool), version))?,
        };
        self.register_or_close(owner, pool, permit, version, generation)
            .await
    }
    /// Trusted host file-picker result → owner-scoped opaque grant. The renderer never supplies
    /// a path; it can only name a grant previously issued to its own session.
    pub fn grant_sqlite_file(&self, owner: OwnerId, path: &Path) -> Result<String, LifecycleError> {
        let canonical = sqlite::authorize(path)?;
        let mut grants = self.0.grants.lock().expect("file grant lock");
        if let Some((id, _)) = grants
            .iter()
            .find(|(_, (grant_owner, granted))| *grant_owner == owner && *granted == canonical)
        {
            return Ok(id.to_string());
        }
        if grants.len() >= Self::MAX_FILE_GRANTS {
            return Err(LifecycleError::Capacity);
        }
        let id = Uuid::new_v4();
        grants.insert(id, (owner, canonical));
        Ok(id.to_string())
    }
    /// File name of an owned grant, for display only.
    pub fn sqlite_file_name(&self, owner: OwnerId, grant: &str) -> Result<String, LifecycleError> {
        let path = self.granted_path(owner, grant)?;
        Ok(path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned())
    }
    fn granted_path(&self, owner: OwnerId, grant: &str) -> Result<PathBuf, LifecycleError> {
        let id = Uuid::parse_str(grant).map_err(|_| LifecycleError::Forbidden)?;
        self.0
            .grants
            .lock()
            .expect("file grant lock")
            .get(&id)
            .filter(|(grant_owner, _)| *grant_owner == owner)
            .map(|(_, path)| path.clone())
            .ok_or(LifecycleError::Forbidden)
    }
    /// Opens an owned SQLite grant read-only on SQLx's dedicated worker thread.
    pub async fn connect_sqlite(
        &self,
        owner: OwnerId,
        grant: &str,
    ) -> Result<ConnectionId, LifecycleError> {
        let path = self.granted_path(owner, grant)?;
        let generation = self.0.generation.load(Ordering::SeqCst);
        let permit = self.permit()?;
        let (pool, version) = sqlite::open(&path, self.0.deadline).await?;
        self.register_or_close(owner, EnginePool::Sqlite(pool), permit, version, generation)
            .await
    }
    fn permit(&self) -> Result<OwnedSemaphorePermit, LifecycleError> {
        self.0
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| LifecycleError::Capacity)
    }
    async fn register_or_close(
        &self,
        owner: OwnerId,
        pool: EnginePool,
        permit: OwnedSemaphorePermit,
        version: String,
        generation: u64,
    ) -> Result<ConnectionId, LifecycleError> {
        match self.register(owner, pool.clone(), permit, version, generation) {
            Ok(id) => Ok(id),
            Err(error) => {
                pool.close().await;
                Err(error)
            }
        }
    }
    fn register(
        &self,
        owner: OwnerId,
        pool: EnginePool,
        permit: OwnedSemaphorePermit,
        engine_version: String,
        generation: u64,
    ) -> Result<ConnectionId, LifecycleError> {
        let id = ConnectionId(Uuid::new_v4());
        let (cancel, cancelled) = oneshot::channel();
        let expires = Instant::now() + self.0.lifetime;
        {
            let mut entries = self.0.entries.lock().expect("connection registry lock");
            if self.0.slots.is_closed() {
                return Err(LifecycleError::Capacity);
            }
            if self.0.generation.load(Ordering::SeqCst) != generation {
                return Err(LifecycleError::Forbidden);
            }
            entries.insert(
                id,
                Entry {
                    engine_version,
                    owner,
                    pool,
                    expires,
                    _permit: permit,
                    _expiry_cancel: cancel,
                },
            );
        }
        let registry = Arc::downgrade(&self.0);
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep_until(expires) => {},
                _ = cancelled => return,
            }
            let entry = registry.upgrade().and_then(|inner| {
                inner
                    .entries
                    .lock()
                    .expect("connection registry lock")
                    .remove(&id)
            });
            if let Some(entry) = entry {
                entry.pool.close().await;
            }
        });
        Ok(id)
    }
    fn owned_pool(&self, owner: OwnerId, id: ConnectionId) -> Option<EnginePool> {
        self.0
            .entries
            .lock()
            .expect("connection registry lock")
            .get(&id)
            .filter(|entry| {
                entry.owner == owner && entry.expires > Instant::now() && !entry.pool.is_closed()
            })
            .map(|entry| entry.pool.clone())
    }
    pub fn engine_version(
        &self,
        owner: OwnerId,
        id: ConnectionId,
    ) -> Result<String, LifecycleError> {
        self.0
            .entries
            .lock()
            .expect("connection registry lock")
            .get(&id)
            .filter(|entry| {
                entry.owner == owner && entry.expires > Instant::now() && !entry.pool.is_closed()
            })
            .map(|entry| entry.engine_version.clone())
            .ok_or(LifecycleError::Forbidden)
    }
    pub fn is_connected(&self, owner: OwnerId, id: ConnectionId) -> bool {
        self.owned_pool(owner, id).is_some()
    }
    pub async fn disconnect(&self, owner: OwnerId, id: ConnectionId) -> Result<(), LifecycleError> {
        let entry = {
            let mut entries = self.0.entries.lock().expect("connection registry lock");
            if !entries.get(&id).is_some_and(|entry| entry.owner == owner) {
                return Err(LifecycleError::Forbidden);
            }
            entries.remove(&id).expect("checked entry")
        };
        entry.pool.close().await;
        Ok(())
    }
    /// Trusted host reset revokes all handles, file grants and pending registrations, retaining
    /// capacity. Closing pools waits for bounded active queries; it does not confirm server
    /// cancellation.
    pub async fn reset(&self) {
        let entries: Vec<_> = {
            let mut entries = self.0.entries.lock().expect("connection registry lock");
            self.0.generation.fetch_add(1, Ordering::SeqCst);
            self.0.grants.lock().expect("file grant lock").clear();
            entries.drain().map(|(_, entry)| entry).collect()
        };
        for entry in entries {
            entry.pool.close().await;
        }
    }

    /// Host shutdown closes every pool. Never retries user SQL or claims server cancellation.
    pub async fn shutdown(&self) {
        self.0.slots.close();
        self.0.grants.lock().expect("file grant lock").clear();
        let entries: Vec<_> = self
            .0
            .entries
            .lock()
            .expect("connection registry lock")
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        for entry in entries {
            entry.pool.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::mysql::MySqlPoolOptions;
    fn config() -> DirectServer {
        DirectServer {
            engine: Engine::MySql,
            host: "127.0.0.1".into(),
            port: 3306,
            username: "fixture".into(),
            password: "do_not_log_secret".into(),
            database: "fixture".into(),
            root_certificate_pem: None,
        }
    }
    fn register(connections: &Connections, owner: OwnerId) -> ConnectionId {
        let permit = connections.0.slots.clone().try_acquire_owned().unwrap();
        let pool = lazy_pool();
        connections
            .register(
                owner,
                pool,
                permit,
                "fixture".into(),
                connections.0.generation.load(Ordering::SeqCst),
            )
            .unwrap()
    }
    fn lazy_pool() -> EnginePool {
        EnginePool::MySql(
            MySqlPoolOptions::new()
                .max_connections(1)
                .min_connections(0)
                .connect_lazy_with(mysql::options(&config()).unwrap()),
        )
    }
    #[tokio::test]
    async fn handles_are_owner_scoped_and_disconnect_releases_capacity() {
        let connections =
            Connections::new(1, Duration::from_secs(1), Duration::from_secs(60)).unwrap();
        let owner = OwnerId::new();
        let other = OwnerId::new();
        let id = register(&connections, owner);
        assert!(connections.is_connected(owner, id));
        assert!(!connections.is_connected(other, id));
        assert_eq!(
            connections.disconnect(other, id).await,
            Err(LifecycleError::Forbidden)
        );
        assert!(connections.is_connected(owner, id));
        assert!(connections.0.slots.clone().try_acquire_owned().is_err());
        connections.disconnect(owner, id).await.unwrap();
        assert!(!connections.is_connected(owner, id));
        assert!(connections.0.slots.clone().try_acquire_owned().is_ok());
    }
    #[tokio::test(start_paused = true)]
    async fn expiry_closes_pool_and_releases_capacity_without_polling() {
        let connections =
            Connections::new(1, Duration::from_secs(1), Duration::from_secs(60)).unwrap();
        let owner = OwnerId::new();
        let id = register(&connections, owner);
        let pool = connections.0.entries.lock().unwrap()[&id].pool.clone();
        tokio::time::advance(Duration::from_secs(61)).await;
        tokio::task::yield_now().await;
        assert!(!connections.is_connected(owner, id));
        assert!(pool.is_closed());
        assert!(connections.0.slots.clone().try_acquire_owned().is_ok());
    }
    #[tokio::test]
    async fn shutdown_closes_all_pools_and_rejects_new_work() {
        let connections =
            Connections::new(2, Duration::from_secs(1), Duration::from_secs(60)).unwrap();
        let owner = OwnerId::new();
        let id = register(&connections, owner);
        connections.shutdown().await;
        assert!(!connections.is_connected(owner, id));
        assert_eq!(
            connections.connect(owner, config()).await,
            Err(LifecycleError::Capacity)
        );
    }
    #[tokio::test]
    async fn shutdown_rejects_an_inflight_connection_before_registration() {
        let connections =
            Connections::new(1, Duration::from_secs(1), Duration::from_secs(60)).unwrap();
        let permit = connections.0.slots.clone().try_acquire_owned().unwrap();
        let pool = lazy_pool();
        connections.shutdown().await;
        assert_eq!(
            connections.register(OwnerId::new(), pool.clone(), permit, "fixture".into(), 0),
            Err(LifecycleError::Capacity)
        );
        pool.close().await;
        assert!(connections.0.entries.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn reset_revokes_handles_closes_pools_and_preserves_new_capacity() {
        let connections =
            Connections::new(2, Duration::from_secs(1), Duration::from_secs(60)).unwrap();
        let owner = OwnerId::new();
        let first = register(&connections, owner);
        let second = register(&connections, OwnerId::new());
        let pools: Vec<_> = connections
            .0
            .entries
            .lock()
            .unwrap()
            .values()
            .map(|entry| entry.pool.clone())
            .collect();
        connections.reset().await;
        assert!(!connections.is_connected(owner, first));
        assert!(pools.iter().all(EnginePool::is_closed));
        assert!(connections.0.entries.lock().unwrap().get(&second).is_none());
        let next = register(&connections, owner);
        assert!(connections.is_connected(owner, next));
        connections.shutdown().await;
    }
    #[tokio::test]
    async fn reset_rejects_late_registration_from_the_previous_generation() {
        let connections =
            Connections::new(1, Duration::from_secs(1), Duration::from_secs(60)).unwrap();
        let generation = connections.0.generation.load(Ordering::SeqCst);
        let permit = connections.0.slots.clone().try_acquire_owned().unwrap();
        let pool = lazy_pool();
        connections.reset().await;
        assert_eq!(
            connections.register(
                OwnerId::new(),
                pool.clone(),
                permit,
                "fixture".into(),
                generation
            ),
            Err(LifecycleError::Forbidden)
        );
        pool.close().await;
        assert!(connections.0.slots.clone().try_acquire_owned().is_ok());
        assert!(connections.0.entries.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn stalled_handshake_obeys_deadline_without_leaking_capacity() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let connections =
            Connections::new(1, Duration::from_millis(100), Duration::from_secs(60)).unwrap();
        let mut input = config();
        input.port = port;
        assert_eq!(
            connections.connect(OwnerId::new(), input).await,
            Err(LifecycleError::Timeout)
        );
        assert!(connections.0.slots.clone().try_acquire_owned().is_ok());
        assert!(connections.0.entries.lock().unwrap().is_empty());
        server.abort();
    }
    #[test]
    fn protocol_identifiers_reject_embedded_nuls_instead_of_truncating() {
        for field in ["host", "username", "database"] {
            let mut input = config();
            match field {
                "host" => input.host.push('\0'),
                "username" => input.username.push('\0'),
                _ => input.database.push('\0'),
            }
            assert!(matches!(
                mysql::options(&input),
                Err(LifecycleError::InvalidConfiguration)
            ));
            input.engine = Engine::PostgreSql;
            assert!(matches!(
                postgres::options(&input),
                Err(LifecycleError::InvalidConfiguration)
            ));
        }
    }
    #[test]
    fn postgres_refuses_unix_sockets_and_engine_mismatch() {
        let mut input = config();
        input.engine = Engine::PostgreSql;
        input.port = 5432;
        assert!(postgres::options(&input).is_ok());
        input.host = "/var/run/postgresql".into();
        assert!(matches!(
            postgres::options(&input),
            Err(LifecycleError::InvalidConfiguration)
        ));
        assert!(matches!(
            mysql::options(&DirectServer {
                engine: Engine::PostgreSql,
                ..config()
            }),
            Err(LifecycleError::InvalidConfiguration)
        ));
    }
    #[test]
    fn configuration_and_error_messages_do_not_expose_credentials() {
        let mut input = config();
        input.engine = Engine::Sqlite;
        assert!(matches!(
            input.validate(),
            Err(LifecycleError::InvalidConfiguration)
        ));
        for error in [
            LifecycleError::Unavailable,
            LifecycleError::Timeout,
            LifecycleError::EngineMismatch,
        ] {
            assert!(!error.to_string().contains(&input.password));
        }
        assert!(Connections::new(65, Duration::from_secs(1), Duration::from_secs(60)).is_err());
    }
}
