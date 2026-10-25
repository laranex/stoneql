//! Host-independent connection lifecycle. No renderer/HTTP access or credential persistence.
mod read_only;
use dbadmin_core::Engine;
pub use read_only::{QueryLimits, ReadError};
use sqlx::{
    ConnectOptions, MySqlPool,
    mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode},
};
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};
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

/// Never derives Debug/Serialize: credentials and certificate material stay in memory.
pub struct DirectMySql {
    pub engine: Engine,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub root_certificate_pem: Option<Vec<u8>>,
}
impl DirectMySql {
    fn options(&self) -> Result<MySqlConnectOptions, LifecycleError> {
        if !matches!(self.engine, Engine::MySql | Engine::MariaDb)
            || self.host.is_empty()
            || self.host.len() > 253
            || self
                .host
                .chars()
                .any(|character| character.is_whitespace() || character == '\0')
            || self.port == 0
            || self.username.contains('\0')
            || self.database.contains('\0')
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
        let mut options = MySqlConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.username)
            .password(&self.password)
            .database(&self.database)
            .ssl_mode(MySqlSslMode::VerifyIdentity)
            .statement_cache_capacity(0)
            .max_receive_packet_size(
                std::num::NonZeroUsize::new(2 * 1024 * 1024).expect("nonzero packet limit"),
            )
            .timezone(None)
            .pipes_as_concat(false)
            .no_engine_substitution(false)
            .disable_statement_logging();
        if let Some(pem) = &self.root_certificate_pem {
            options = options.ssl_ca_from_pem(pem.clone());
        }
        Ok(options)
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
struct Entry {
    engine_version: String,
    owner: OwnerId,
    pool: MySqlPool,
    expires: Instant,
    _permit: OwnedSemaphorePermit,
    _expiry_cancel: oneshot::Sender<()>,
}
struct Inner {
    entries: Mutex<HashMap<ConnectionId, Entry>>,
    slots: Arc<Semaphore>,
    deadline: Duration,
    lifetime: Duration,
}
/// Clone shares one bounded registry; every handle is scoped to its trusted owner.
#[derive(Clone)]
pub struct Connections(Arc<Inner>);
impl Connections {
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
            entries: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(capacity)),
            deadline,
            lifetime,
        })))
    }
    pub async fn connect(
        &self,
        owner: OwnerId,
        config: DirectMySql,
    ) -> Result<ConnectionId, LifecycleError> {
        let options = config.options()?;
        let permit = self
            .0
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| LifecycleError::Capacity)?;
        let pool = tokio::time::timeout(
            self.0.deadline,
            MySqlPoolOptions::new()
                .max_connections(1)
                .min_connections(0)
                .acquire_timeout(self.0.deadline)
                .connect_with(options),
        )
        .await
        .map_err(|_| LifecycleError::Timeout)?
        .map_err(|error| {
            if matches!(error, sqlx::Error::PoolTimedOut) {
                LifecycleError::Timeout
            } else {
                LifecycleError::Unavailable
            }
        })?;
        let identity = tokio::time::timeout(
            self.0.deadline,
            sqlx::query_scalar::<_, String>("SELECT VERSION()").fetch_one(&pool),
        )
        .await;
        let version = match identity {
            Ok(Ok(version))
                if version.to_ascii_lowercase().contains("mariadb")
                    == (config.engine == Engine::MariaDb) =>
            {
                version
            }
            other => {
                pool.close().await;
                return Err(match other {
                    Err(_) => LifecycleError::Timeout,
                    Ok(Err(_)) => LifecycleError::Unavailable,
                    Ok(Ok(_)) => LifecycleError::EngineMismatch,
                });
            }
        };
        match self.register(owner, pool.clone(), permit, version) {
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
        pool: MySqlPool,
        permit: OwnedSemaphorePermit,
        engine_version: String,
    ) -> Result<ConnectionId, LifecycleError> {
        let id = ConnectionId(Uuid::new_v4());
        let (cancel, cancelled) = oneshot::channel();
        let expires = Instant::now() + self.0.lifetime;
        {
            let mut entries = self.0.entries.lock().expect("connection registry lock");
            if self.0.slots.is_closed() {
                return Err(LifecycleError::Capacity);
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
        self.0
            .entries
            .lock()
            .expect("connection registry lock")
            .get(&id)
            .is_some_and(|entry| {
                entry.owner == owner && entry.expires > Instant::now() && !entry.pool.is_closed()
            })
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
    /// Host shutdown closes every pool. Never retries user SQL or claims server cancellation.
    pub async fn shutdown(&self) {
        self.0.slots.close();
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
    fn config() -> DirectMySql {
        DirectMySql {
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
        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .min_connections(0)
            .connect_lazy_with(config().options().unwrap());
        connections
            .register(owner, pool, permit, "fixture".into())
            .unwrap()
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
        let pool = MySqlPoolOptions::new().connect_lazy_with(config().options().unwrap());
        connections.shutdown().await;
        assert_eq!(
            connections.register(OwnerId::new(), pool.clone(), permit, "fixture".into()),
            Err(LifecycleError::Capacity)
        );
        pool.close().await;
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
                input.options(),
                Err(LifecycleError::InvalidConfiguration)
            ));
        }
    }
    #[test]
    fn configuration_and_error_messages_do_not_expose_credentials() {
        let mut input = config();
        input.engine = Engine::Sqlite;
        assert!(matches!(
            input.options(),
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
