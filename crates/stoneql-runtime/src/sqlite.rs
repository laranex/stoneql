//! SQLite authorized-file policy, read-only open and storage-class-preserving decoding.
//!
//! SQLx runs each SQLite connection on its own worker thread, so file I/O and busy waits never
//! block the async executor. Every pool holds at most one connection; each query discards it.
use super::{
    LifecycleError, ReadError,
    files::verify,
    read_only::{QueryLimits, driver_error, finish, float_text, hex},
};
use sqlx::{
    ConnectOptions, Row, SqlitePool, TypeInfo, ValueRef,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
};
use std::{path::Path, time::Duration};
use stoneql_contracts::v1::{CellValue, ResultSet};

/// Lock waits fail well inside the connect and query deadlines.
pub(crate) const BUSY_TIMEOUT: Duration = Duration::from_secs(1);

pub(crate) fn options(path: &Path) -> SqliteConnectOptions {
    // READONLY open is the engine boundary; query_only additionally refuses writes on this
    // handle. Hardening for files of unknown provenance: no schema-invoked functions with
    // side effects, corruption checks and no memory mapping.
    SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .immutable(false)
        .busy_timeout(BUSY_TIMEOUT)
        .statement_cache_capacity(0)
        .pragma("query_only", "ON")
        .pragma("trusted_schema", "OFF")
        .pragma("cell_size_check", "ON")
        .pragma("mmap_size", "0")
        .disable_statement_logging()
}

/// Explicit read-write profiles only: the same hardening without the read-only open.
/// SQLite may create `-journal`/`-wal`/`-shm` files beside the granted database.
pub(crate) fn write_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .read_only(false)
        .create_if_missing(false)
        .immutable(false)
        .busy_timeout(crate::ExecutionLimits::SQLITE_WRITE_BUSY_TIMEOUT)
        .statement_cache_capacity(0)
        .pragma("trusted_schema", "OFF")
        .pragma("cell_size_check", "ON")
        .pragma("mmap_size", "0")
        .disable_statement_logging()
}

pub(crate) async fn open(
    path: &Path,
    deadline: Duration,
) -> Result<(SqlitePool, String), LifecycleError> {
    verify(path)?;
    let pool = tokio::time::timeout(
        deadline,
        SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(0)
            .acquire_timeout(deadline)
            .connect_with(options(path)),
    )
    .await
    .map_err(|_| LifecycleError::Timeout)?
    .map_err(super::connect_error)?;
    // Reading the schema proves the file is an SQLite database, not merely openable.
    let identity = tokio::time::timeout(
        deadline,
        sqlx::query_as::<_, (String, i64)>(
            "SELECT sqlite_version(), (SELECT count(*) FROM sqlite_schema)",
        )
        .fetch_one(&pool),
    )
    .await;
    match identity {
        Ok(Ok((version, _))) => Ok((pool, format!("SQLite {version}"))),
        other => {
            pool.close().await;
            Err(match other {
                Err(_) => LifecycleError::Timeout,
                _ => LifecycleError::Unavailable,
            })
        }
    }
}

pub(crate) async fn read(
    pool: &SqlitePool,
    sql: &str,
    limits: QueryLimits,
) -> Result<ResultSet, ReadError> {
    // Each query opens a fresh connection, so re-check the grant before it does.
    verify(pool.connect_options().get_filename())
        .map_err(|_| ReadError::Connection(LifecycleError::Forbidden))?;
    let mut connection = pool.acquire().await.map_err(driver_error)?;
    // Interrupt the worker's statement at the deadline so an abandoned query cannot keep the
    // dedicated thread busy after the caller has timed out.
    let deadline = std::time::Instant::now() + limits.deadline;
    connection
        .lock_handle()
        .await
        .map_err(driver_error)?
        .set_progress_handler(1_000, move || std::time::Instant::now() < deadline);
    match finish(connection, &[], sql, limits, decode_cell).await {
        // The deadline interrupt surfaces as SQLITE_INTERRUPT before the caller's timer fires.
        Err(ReadError::Database) if std::time::Instant::now() >= deadline => {
            Err(ReadError::Timeout)
        }
        result => result,
    }
}

/// Decodes by each value's runtime storage class, so dynamic-affinity columns keep exactly
/// what was stored (an INTEGER column may hold TEXT) and nothing is coerced.
pub(crate) fn decode_cell(row: &SqliteRow, index: usize) -> Result<CellValue, ReadError> {
    let raw = row.try_get_raw(index).map_err(|_| ReadError::Database)?;
    if raw.is_null() {
        return Ok(CellValue::Null);
    }
    let class = raw.type_info().name().to_owned();
    let bytes = || row.try_get_unchecked::<Vec<u8>, _>(index);
    Ok(match class.as_str() {
        "INTEGER" => CellValue::Integer {
            text: row
                .try_get_unchecked::<i64, _>(index)
                .map_err(|_| ReadError::Database)?
                .to_string(),
        },
        "REAL" => CellValue::Float {
            text: float_text(
                row.try_get_unchecked::<f64, _>(index)
                    .map_err(|_| ReadError::Database)?,
            ),
        },
        "TEXT" => {
            let bytes = bytes().map_err(|_| ReadError::Database)?;
            match String::from_utf8(bytes) {
                Ok(text) => CellValue::Text { text },
                Err(error) => CellValue::Unknown {
                    database_type: class,
                    hex: hex(error.as_bytes()),
                },
            }
        }
        "BLOB" => CellValue::Binary {
            hex: hex(&bytes().map_err(|_| ReadError::Database)?),
        },
        _ => CellValue::Unknown {
            database_type: class,
            hex: hex(&bytes().map_err(|_| ReadError::Database)?),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Connections, OwnerId, ReadError};
    use sqlx::{Connection, Executor, SqliteConnection};
    use std::path::PathBuf;

    /// Private temporary directory, removed on drop even when an assertion fails.
    struct Fixture(PathBuf);
    impl Fixture {
        async fn new(sql: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("stoneql-sqlite-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            let root = std::fs::canonicalize(root).unwrap();
            let fixture = Self(root);
            let mut writer = fixture.writer().await;
            writer
                .execute(sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned())))
                .await
                .unwrap();
            writer.close().await.unwrap();
            fixture
        }
        fn file(&self) -> PathBuf {
            self.0.join("fixture.sqlite")
        }
        async fn writer(&self) -> SqliteConnection {
            SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(self.file())
                    .create_if_missing(true),
            )
            .await
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn registry() -> Connections {
        Connections::new(2, Duration::from_secs(3), Duration::from_secs(60)).unwrap()
    }
    fn limits(deadline: Duration) -> QueryLimits {
        QueryLimits {
            rows: 100,
            bytes: 65_536,
            deadline,
        }
    }
    const VALUES: &str = r#"
CREATE TABLE "odd ""quoted"" table"(id INTEGER PRIMARY KEY, "select" TEXT, amount DECIMAL(38,4), flag BOOLEAN, created DATETIME, raw BLOB, d TEXT DEFAULT 'fallback');
INSERT INTO "odd ""quoted"" table"(id,"select",amount,flag,created,raw) VALUES
 (1,'',  '123456789012345678901234567890.4500', 1, '2024-02-29 12:34:56.123456', x'00ff0180'),
 (2,NULL,'0.1', 'yes', 20240229, NULL);
INSERT INTO "odd ""quoted"" table"(id,"select",d) VALUES (3,'x',NULL);
CREATE TABLE strict_values(id INTEGER PRIMARY KEY, i INTEGER, r REAL, t TEXT, b BLOB, a ANY) STRICT;
INSERT INTO strict_values VALUES (1,-9223372036854775808,1.5,'-0',x'',9223372036854775807),(2,0,-0.0,'',x'00','42');
"#;

    #[tokio::test]
    async fn grants_accept_only_canonical_regular_files_owned_by_the_session() {
        let fixture = Fixture::new("CREATE TABLE t(x)").await;
        let connections = registry();
        let owner = OwnerId::new();
        let file = fixture.file();
        let directory = fixture.0.join("directory");
        std::fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        let link = fixture.0.join("link.sqlite");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&file, &link).unwrap();
        for (path, error) in [
            (
                PathBuf::from("fixture.sqlite"),
                LifecycleError::InvalidConfiguration,
            ),
            (
                // Built as text: on Windows, `join` resolves `..` in verbatim paths.
                PathBuf::from(format!(
                    "{}{sep}..{sep}fixture.sqlite",
                    directory.display(),
                    sep = std::path::MAIN_SEPARATOR
                )),
                LifecycleError::Forbidden,
            ),
            (directory.clone(), LifecycleError::Forbidden),
            (fixture.0.join("missing.sqlite"), LifecycleError::Forbidden),
            #[cfg(unix)]
            (link.clone(), LifecycleError::Forbidden),
        ] {
            assert_eq!(
                connections.grant_sqlite_file(owner, &path),
                Err(error),
                "{path:?}"
            );
        }
        let grant = connections.grant_sqlite_file(owner, &file).unwrap();
        assert_eq!(connections.grant_sqlite_file(owner, &file).unwrap(), grant);
        assert_eq!(
            connections.sqlite_file_name(owner, &grant).unwrap(),
            "fixture.sqlite"
        );
        let other = OwnerId::new();
        assert_eq!(
            connections.connect_sqlite(other, &grant).await,
            Err(LifecycleError::Forbidden)
        );
        assert_eq!(
            connections
                .connect_sqlite(owner, &uuid::Uuid::new_v4().to_string())
                .await,
            Err(LifecycleError::Forbidden)
        );
        assert_eq!(
            connections.connect_sqlite(owner, "../fixture.sqlite").await,
            Err(LifecycleError::Forbidden)
        );
        let handle = connections.connect_sqlite(owner, &grant).await.unwrap();
        assert!(
            connections
                .engine_version(owner, handle)
                .unwrap()
                .starts_with("SQLite 3.")
        );
        connections.reset().await;
        assert_eq!(
            connections.connect_sqlite(owner, &grant).await,
            Err(LifecycleError::Forbidden)
        );
        for _ in 0..Connections::MAX_FILE_GRANTS {
            let copy = fixture.0.join(format!("{}.sqlite", uuid::Uuid::new_v4()));
            std::fs::copy(&file, &copy).unwrap();
            connections.grant_sqlite_file(owner, &copy).unwrap();
        }
        assert_eq!(
            connections.grant_sqlite_file(owner, &file),
            Err(LifecycleError::Capacity)
        );
        connections.shutdown().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_granted_path_replaced_by_a_symlink_is_refused_at_open_and_query() {
        let fixture = Fixture::new("CREATE TABLE t(x); INSERT INTO t VALUES (1)").await;
        let elsewhere =
            Fixture::new("CREATE TABLE secret(x); INSERT INTO secret VALUES ('other file')").await;
        let connections = registry();
        let owner = OwnerId::new();
        let grant = connections
            .grant_sqlite_file(owner, &fixture.file())
            .unwrap();
        let handle = connections.connect_sqlite(owner, &grant).await.unwrap();
        std::fs::remove_file(fixture.file()).unwrap();
        std::os::unix::fs::symlink(elsewhere.file(), fixture.file()).unwrap();
        assert_eq!(
            connections
                .read_only(
                    owner,
                    handle,
                    "SELECT x FROM secret",
                    limits(Duration::from_secs(3))
                )
                .await,
            Err(ReadError::Connection(LifecycleError::Forbidden))
        );
        assert_eq!(
            connections.connect_sqlite(owner, &grant).await,
            Err(LifecycleError::Forbidden)
        );
        connections.shutdown().await;
    }

    #[tokio::test]
    async fn dynamic_and_strict_values_survive_without_coercion() {
        let fixture = Fixture::new(VALUES).await;
        let connections = registry();
        let owner = OwnerId::new();
        let grant = connections
            .grant_sqlite_file(owner, &fixture.file())
            .unwrap();
        let handle = connections.connect_sqlite(owner, &grant).await.unwrap();
        let read = |sql: &'static str| {
            connections.read_only(owner, handle, sql, limits(Duration::from_secs(3)))
        };
        let result = read(r#"SELECT "select", amount, typeof(amount), flag, created, raw, d FROM "odd ""quoted"" table" ORDER BY id"#)
            .await
            .unwrap();
        let json = serde_json::to_value(&result.rows).unwrap();
        assert_eq!(result.columns[0].name, "select");
        // Empty text, NULL and an explicitly inserted NULL over a DEFAULT stay distinct.
        assert_eq!(json[0][0], serde_json::json!({"kind":"text","text":""}));
        assert_eq!(json[1][0], serde_json::json!({"kind":"null"}));
        assert_eq!(
            json[0][6],
            serde_json::json!({"kind":"text","text":"fallback"})
        );
        assert_eq!(json[2][6], serde_json::json!({"kind":"null"}));
        // NUMERIC affinity converts a lossy literal to REAL; it is reported as stored, never
        // presented as an exact decimal.
        assert_eq!(json[0][2], serde_json::json!({"kind":"text","text":"real"}));
        assert_eq!(json[0][1]["kind"], "float");
        assert_eq!(json[1][1], serde_json::json!({"kind":"float","text":"0.1"}));
        assert_eq!(json[1][3], serde_json::json!({"kind":"text","text":"yes"}));
        assert_eq!(
            json[0][4],
            serde_json::json!({"kind":"text","text":"2024-02-29 12:34:56.123456"})
        );
        assert_eq!(
            json[1][4],
            serde_json::json!({"kind":"integer","text":"20240229"})
        );
        assert_eq!(
            json[0][5],
            serde_json::json!({"kind":"binary","hex":"00ff0180"})
        );
        let strict = read("SELECT i, r, t, b, a, typeof(a) FROM strict_values ORDER BY id")
            .await
            .unwrap();
        let json = serde_json::to_value(&strict.rows).unwrap();
        assert_eq!(
            json[0][0],
            serde_json::json!({"kind":"integer","text":"-9223372036854775808"})
        );
        assert_eq!(
            json[0][4],
            serde_json::json!({"kind":"integer","text":"9223372036854775807"})
        );
        assert_eq!(json[0][2], serde_json::json!({"kind":"text","text":"-0"}));
        assert_eq!(json[0][3], serde_json::json!({"kind":"binary","hex":""}));
        assert_eq!(json[1][4], serde_json::json!({"kind":"text","text":"42"}));
        assert_eq!(json[1][5], serde_json::json!({"kind":"text","text":"text"}));
        assert_eq!(strict.columns[0].database_type, "INTEGER");
        connections.shutdown().await;
    }

    #[tokio::test]
    async fn writes_routines_and_attachments_are_refused_by_policy_and_engine() {
        let fixture = Fixture::new("CREATE TABLE t(x); INSERT INTO t VALUES (1)").await;
        let connections = registry();
        let owner = OwnerId::new();
        let grant = connections
            .grant_sqlite_file(owner, &fixture.file())
            .unwrap();
        let handle = connections.connect_sqlite(owner, &grant).await.unwrap();
        for sql in [
            "DELETE FROM t",
            "INSERT INTO t VALUES (2)",
            "CALL p()",
            "PRAGMA writable_schema=ON",
            "ATTACH DATABASE 'other.sqlite' AS other",
            "SELECT load_extension('x')",
            "SELECT readfile('/etc/passwd')",
            "SELECT 1; DELETE FROM t",
            "WITH c AS (SELECT 1) SELECT * FROM c",
        ] {
            assert_eq!(
                connections
                    .read_only(owner, handle, sql, limits(Duration::from_secs(3)))
                    .await,
                Err(ReadError::UnsupportedSql),
                "{sql}"
            );
        }
        // Engine boundary independently of the lexer: the same options refuse writes.
        let mut direct = SqliteConnection::connect_with(&options(&fixture.file()))
            .await
            .unwrap();
        for sql in ["INSERT INTO t VALUES (3)", "CREATE TABLE created(x)"] {
            let error = direct.execute(sql).await.unwrap_err();
            let code = error
                .as_database_error()
                .and_then(|error| error.code())
                .unwrap();
            assert!(["8", "1"].contains(&code.as_ref()), "{sql}: {code}");
        }
        direct.close().await.unwrap();
        let mut checker = fixture.writer().await;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM t")
            .fetch_one(&mut checker)
            .await
            .unwrap();
        assert_eq!(count, 1);
        checker.close().await.unwrap();
        connections.shutdown().await;
    }

    #[tokio::test]
    async fn busy_locks_fail_within_the_timeout_and_never_report_success() {
        let fixture = Fixture::new("CREATE TABLE t(x); INSERT INTO t VALUES (1)").await;
        let connections = registry();
        let owner = OwnerId::new();
        let grant = connections
            .grant_sqlite_file(owner, &fixture.file())
            .unwrap();
        let handle = connections.connect_sqlite(owner, &grant).await.unwrap();
        let mut writer = fixture.writer().await;
        writer.execute("BEGIN EXCLUSIVE").await.unwrap();
        let started = std::time::Instant::now();
        let locked = connections
            .read_only(
                owner,
                handle,
                "SELECT x FROM t",
                limits(Duration::from_secs(3)),
            )
            .await;
        assert_eq!(locked, Err(ReadError::Database));
        assert!(started.elapsed() >= BUSY_TIMEOUT && started.elapsed() < Duration::from_secs(3));
        writer.execute("COMMIT").await.unwrap();
        writer.close().await.unwrap();
        let result = connections
            .read_only(
                owner,
                handle,
                "SELECT x FROM t",
                limits(Duration::from_secs(3)),
            )
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 1);
        connections.shutdown().await;
    }

    #[tokio::test]
    async fn deadline_interrupts_the_worker_and_releases_the_connection() {
        let fixture = Fixture::new(
            "CREATE TABLE n(x); WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM s WHERE x<1000) INSERT INTO n SELECT x FROM s;",
        )
        .await;
        let connections = registry();
        let owner = OwnerId::new();
        let grant = connections
            .grant_sqlite_file(owner, &fixture.file())
            .unwrap();
        let handle = connections.connect_sqlite(owner, &grant).await.unwrap();
        let started = std::time::Instant::now();
        assert_eq!(
            connections
                .read_only(
                    owner,
                    handle,
                    "SELECT count(*) FROM n a, n b, n c",
                    limits(Duration::from_millis(200))
                )
                .await,
            Err(ReadError::Timeout)
        );
        // The single-connection pool only admits the next query after the interrupted worker
        // has stopped and its connection was discarded.
        let next = connections
            .read_only(
                owner,
                handle,
                "SELECT count(*) FROM n",
                limits(Duration::from_secs(3)),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&next.rows).unwrap()[0][0]["text"],
            "1000"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        connections.shutdown().await;
    }
}
