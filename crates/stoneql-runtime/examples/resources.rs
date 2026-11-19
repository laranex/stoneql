//! Owned disposable resource profile, not a production benchmark or arbitrary-target client.
use serde_json::json;
use sqlx::{
    ConnectOptions, Connection, MySqlConnection, PgConnection, Row,
    mysql::{MySqlConnectOptions, MySqlSslMode},
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{
    env,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use stoneql_core::Engine;
use stoneql_runtime::{Connections, DirectServer, OwnerId, QueryLimits, ReadError};
use tokio::{sync::Barrier, task::JoinSet};
fn rss_bytes() -> Result<u64, Box<dyn std::error::Error>> {
    let output = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()?;
    if !output.status.success() {
        return Err("connected-idle RSS unavailable".into());
    }
    let kib: u64 = std::str::from_utf8(&output.stdout)?.trim().parse()?;
    kib.checked_mul(1024)
        .filter(|value| *value > 0)
        .ok_or_else(|| "invalid connected-idle RSS".into())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::time::timeout(Duration::from_secs(180), profile())
        .await
        .map_err(|_| "resource profile deadline exceeded")?
}
/// The fixture's own verified-TLS observer: ownership marker, index plan and session cleanup.
enum Observer {
    MySql(MySqlConnection),
    Postgres(PgConnection),
}
impl Observer {
    async fn scalar_i64(&mut self, sql: &'static str) -> Result<i64, sqlx::Error> {
        match self {
            Self::MySql(c) => sqlx::query_scalar(sql).fetch_one(c).await,
            Self::Postgres(c) => sqlx::query_scalar(sql).fetch_one(c).await,
        }
    }
    async fn marker(&mut self) -> Result<String, sqlx::Error> {
        let sql = "SELECT id FROM stoneql_fixture_marker";
        match self {
            Self::MySql(c) => sqlx::query_scalar(sql).fetch_one(c).await,
            Self::Postgres(c) => sqlx::query_scalar(sql).fetch_one(c).await,
        }
    }
    /// The bounded page query must use the primary-key index.
    async fn primary_plan(&mut self) -> Result<bool, sqlx::Error> {
        let sql =
            "EXPLAIN SELECT id,payload FROM resource_rows WHERE id >= 1 AND id < 201 ORDER BY id";
        Ok(match self {
            Self::MySql(c) => {
                let key: Option<String> = sqlx::query(sql).fetch_one(c).await?.try_get("key")?;
                key.as_deref() == Some("PRIMARY")
            }
            Self::Postgres(c) => {
                let lines: Vec<String> = sqlx::query_scalar(sql).fetch_all(c).await?;
                lines
                    .iter()
                    .any(|line| line.contains("Index Scan using resource_rows_pkey"))
            }
        })
    }
    async fn reader_sessions(&mut self) -> Result<i64, sqlx::Error> {
        match self {
            Self::MySql(_) => {
                self.scalar_i64("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER='stoneql_reader'")
                    .await
            }
            Self::Postgres(_) => {
                self.scalar_i64("SELECT COUNT(*) FROM pg_stat_activity WHERE usename='stoneql_reader'")
                    .await
            }
        }
    }
    async fn close(self) -> Result<(), sqlx::Error> {
        match self {
            Self::MySql(c) => c.close().await,
            Self::Postgres(c) => c.close().await,
        }
    }
}
async fn profile() -> Result<(), Box<dyn std::error::Error>> {
    let engine = match env::var("STONEQL_LIFECYCLE_ENGINE")?.as_str() {
        "mysql" => Engine::MySql,
        "mariadb" => Engine::MariaDb,
        "postgresql" => Engine::PostgreSql,
        _ => return Err("owning fixture engine required".into()),
    };
    let port: u16 = env::var("STONEQL_LIFECYCLE_PORT")?.parse()?;
    let password = env::var("STONEQL_FIXTURE_PASSWORD")?;
    let marker = env::var("STONEQL_SPIKE_FIXTURE_ID")?;
    uuid::Uuid::parse_str(&marker).map_err(|_| "owning marker required")?;
    let ca = std::fs::read(env::var("STONEQL_LIFECYCLE_CA")?)?;
    let mut observer = if engine == Engine::PostgreSql {
        let options = PgConnectOptions::new_without_pgpass()
            .host("127.0.0.1")
            .port(port)
            .username("stoneql_reader")
            .password(&password)
            .database("stoneql_m001_fixture")
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert_from_pem(ca.clone())
            .disable_statement_logging();
        Observer::Postgres(
            PgConnection::connect_with(&options)
                .await
                .map_err(|_| "fixture TLS observer failed")?,
        )
    } else {
        let options = MySqlConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username("stoneql_reader")
            .password(&password)
            .database("stoneql_m001_fixture")
            .ssl_mode(MySqlSslMode::VerifyIdentity)
            .ssl_ca_from_pem(ca.clone())
            .disable_statement_logging();
        Observer::MySql(
            MySqlConnection::connect_with(&options)
                .await
                .map_err(|_| "fixture TLS observer failed")?,
        )
    };
    let actual = observer
        .marker()
        .await
        .map_err(|_| "fixture marker unavailable")?;
    if marker != actual {
        return Err("fixture ownership mismatch".into());
    }
    let rows = observer
        .scalar_i64("SELECT COUNT(*) FROM resource_rows")
        .await
        .map_err(|_| "resource fixture unavailable")?;
    assert_eq!(rows, 100000);
    let indexed = observer
        .primary_plan()
        .await
        .map_err(|_| "fixture index inspection failed")?;
    assert!(indexed, "bounded page query must use the primary-key index");

    let registry = Connections::new(8, Duration::from_secs(3), Duration::from_secs(120))?;
    let owner = OwnerId::new();
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(
            registry
                .connect(
                    owner,
                    DirectServer {
                        engine,
                        host: "127.0.0.1".into(),
                        port,
                        username: "stoneql_reader".into(),
                        password: password.clone(),
                        database: "stoneql_m001_fixture".into(),
                        root_certificate_pem: Some(ca.clone()),
                        tls: true,
                        rsa_key: stoneql_runtime::PasswordRsaKey::None,
                    },
                )
                .await?,
        );
    }
    let version = registry.engine_version(owner, handles[0])?;
    let idle_rss = rss_bytes()?;
    let limits = QueryLimits {
        rows: 1000,
        bytes: 1048576,
        deadline: Duration::from_secs(3),
    };
    let mut samples = Vec::new();
    for sample in 0..100 {
        let first = 1 + sample * 997;
        // Generated fixture bounds are integers; no input or secret is interpolated.
        let sql = format!(
            "SELECT id,payload FROM resource_rows WHERE id >= {first} AND id < {} ORDER BY id",
            first + 200
        );
        let start = Instant::now();
        let result = registry.read_only(owner, handles[0], &sql, limits).await?;
        let encoded = serde_json::to_vec(&result)?;
        assert_eq!(result.rows.len(), 200);
        assert!(!result.truncated);
        assert!(encoded.len() <= limits.bytes);
        samples.push(start.elapsed().as_micros() as u64);
    }
    let mut metadata = Vec::new();
    let widths: &[usize] = if engine == Engine::PostgreSql {
        &[16, 128, 512, 1600] // PostgreSQL target lists hold at most 1664 entries.
    } else {
        &[16, 128, 512, 2048]
    };
    for &count in widths {
        let projection = (0..count)
            .map(|n| format!("id AS c{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("SELECT {projection} FROM resource_rows WHERE id=0");
        assert!(sql.len() <= 65536);
        let result = registry.read_only(owner, handles[0], &sql, limits).await?;
        assert_eq!(result.columns.len(), count);
        assert!(result.rows.is_empty());
        let bytes = serde_json::to_vec(&result)?.len();
        assert!(bytes <= limits.bytes);
        metadata.push(json!({"columns":count,"serializedBytes":bytes}));
        assert_eq!(
            registry
                .read_only(
                    owner,
                    handles[0],
                    &sql,
                    QueryLimits {
                        bytes: 128,
                        ..limits
                    }
                )
                .await,
            Err(ReadError::ResultTooWide)
        );
        assert!(
            registry
                .read_only(
                    owner,
                    handles[0],
                    "SELECT id FROM resource_rows WHERE id=1",
                    limits
                )
                .await
                .is_ok()
        );
    }
    let barrier = Arc::new(Barrier::new(9));
    let mut jobs = JoinSet::new();
    for handle in &handles {
        let registry = registry.clone();
        let barrier = barrier.clone();
        let handle = *handle;
        jobs.spawn(async move {
            barrier.wait().await;
            registry
                .read_only(
                    owner,
                    handle,
                    "SELECT id,payload FROM resource_rows WHERE id >= 1 AND id <= 1000 ORDER BY id",
                    limits,
                )
                .await
        });
    }
    barrier.wait().await;
    let mut retained = Vec::new();
    let mut retained_serialized = 0;
    while let Some(result) = jobs.join_next().await {
        let result = result??;
        assert!(result.truncated);
        assert!(!result.rows.is_empty());
        let bytes = serde_json::to_vec(&result)?.len();
        assert!(bytes <= limits.bytes);
        retained_serialized += bytes;
        retained.push(result);
    }
    assert_eq!(retained.len(), 8);
    let retained_rss = rss_bytes()?;
    // Hold all eight results simultaneously through the sample; then explicitly release them.
    drop(retained);
    registry.shutdown().await;
    let start = Instant::now();
    loop {
        let count = observer
            .reader_sessions()
            .await
            .map_err(|_| "owned cleanup observation failed")?;
        if count == 1 {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err("owned query sessions did not close".into());
        }
    }
    observer
        .close()
        .await
        .map_err(|_| "fixture observer close failed")?;
    let mut sorted = samples.clone();
    sorted.sort_unstable();
    println!(
        "{}",
        json!({"verifiedTls":true,"engineVersion":version,"fixtureRows":rows,"payloadBytes":1024,"indexedSelects":100,"primaryIndexPlanVerified":true,"rowsPerSelect":200,"latencyUs":samples,"p50Us":sorted[49],"p95Us":sorted[94],"maxUs":sorted[99],"metadata":metadata,"metadataRejectionRecovery":true,"concurrentQueries":8,"retainedSerializedBytes":retained_serialized,"connectedIdleRssBytes":idle_rss,"retainedResultsRssBytes":retained_rss,"ownedSessionsCleaned":true,"scope":"debug runtime plus result serialization; no IPC or WebView"})
    );
    Ok(())
}
