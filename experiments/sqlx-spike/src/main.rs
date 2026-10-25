//! Disposable-fixture evaluation only. No reusable production connection adapter.
use bigdecimal::BigDecimal;
use futures_util::TryStreamExt;
use serde::Serialize;
use sqlx::{
    Column, ConnectOptions, Connection, Either, MySqlConnection, PgConnection, Row,
    SqliteConnection, TypeInfo, ValueRef,
    mysql::{MySqlConnectOptions, MySqlSslMode},
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, sqlx::Error>;
const DECIMAL: &str = "123456789012345678901234567890.4500";
#[derive(Serialize)]
struct Probe {
    name: &'static str,
    status: &'static str,
    method: &'static str,
    elapsed_us: u64,
    error_code: Option<String>,
}
#[derive(Serialize)]
struct Report {
    engine: String,
    engine_version: String,
    sqlx_version: &'static str,
    probes: Vec<Probe>,
    query_us: Vec<u64>,
    connect_us: Vec<u64>,
}
impl Report {
    fn record(
        &mut self,
        name: &'static str,
        method: &'static str,
        start: Instant,
        result: Result<bool>,
    ) {
        let (status, error_code) = match result {
            Ok(true) => ("passed", None),
            Ok(false) => ("failed", Some("unexpected_result".into())),
            Err(error) => (
                "failed",
                Some(
                    error
                        .as_database_error()
                        .and_then(|e| e.code())
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|| "driver_error".into()),
                ),
            ),
        };
        self.probes.push(Probe {
            name,
            status,
            method,
            elapsed_us: start.elapsed().as_micros() as u64,
            error_code,
        });
    }
    fn limitation(&mut self, name: &'static str, status: &'static str, method: &'static str) {
        self.probes.push(Probe {
            name,
            status,
            method,
            elapsed_us: 0,
            error_code: None,
        });
    }
}
macro_rules! probe {
    ($report:expr,$name:expr,$method:expr,$body:expr) => {{
        let start = Instant::now();
        let result: Result<bool> = $body.await;
        $report.record($name, $method, start, result);
    }};
}
fn failure() -> sqlx::Error {
    sqlx::Error::Protocol("fixture boundary or observation failed".into())
}
fn validate_fixture_target(engine: &str, value: &str, marker: Option<&str>) -> Result<()> {
    let url = url::Url::parse(value).map_err(|_| failure())?;
    if url.host_str() != Some("127.0.0.1")
        || url.path() != "/stoneql_m001_fixture"
        || url.port().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || marker.is_none_or(|id| {
            id.len() != 36 || !id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
        })
        || url.scheme()
            != if engine == "postgresql" {
                "postgres"
            } else {
                "mysql"
            }
    {
        return Err(failure());
    }
    Ok(())
}
fn fixture_url(engine: &str) -> Result<String> {
    let value = env::var("STONEQL_SPIKE_URL").map_err(|_| failure())?;
    let marker = env::var("STONEQL_SPIKE_FIXTURE_ID").ok();
    validate_fixture_target(engine, &value, marker.as_deref())?;
    Ok(value)
}
// SQLx 0.9 propagates rustls handshake failures through std::io::Error.
// A generic connection/auth/network failure must not pass the TLS-negative probe.
fn tls_rejection(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Tls(_) => true,
        sqlx::Error::Io(error) => {
            error.kind() == std::io::ErrorKind::InvalidData
                && error
                    .get_ref()
                    .is_some_and(|cause| cause.to_string().starts_with("invalid peer certificate:"))
        }
        _ => false,
    }
}
fn mysql_options(url: &str) -> Result<MySqlConnectOptions> {
    Ok(url
        .parse::<MySqlConnectOptions>()?
        .disable_statement_logging())
}
fn pg_options(url: &str) -> Result<PgConnectOptions> {
    Ok(url.parse::<PgConnectOptions>()?.disable_statement_logging())
}

async fn mysql(report: &mut Report, url: &str) -> Result<()> {
    let mut conn = MySqlConnection::connect_with(&mysql_options(url)?).await?;
    let marker: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
        .fetch_one(&mut conn)
        .await?;
    if marker != env::var("STONEQL_SPIKE_FIXTURE_ID").map_err(|_| failure())? {
        return Err(failure());
    }
    report.engine_version = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&mut conn)
        .await?;
    if report.engine_version.contains("MariaDB") != (report.engine == "mariadb") {
        return Err(failure());
    }
    probe!(
        report,
        "dynamic_null_bigint_decimal_binary",
        "runtime query; named metadata and lossless decoders",
        async {
            let row=sqlx::query("SELECT NULL AS n, '' AS empty_text, CAST('18446744073709551615' AS UNSIGNED) AS u, CAST('-9223372036854775808' AS SIGNED) AS i, CAST('123456789012345678901234567890.4500' AS DECIMAL(38,4)) AS d, 0x00ff0180 AS b").fetch_one(&mut conn).await?;
            Ok(row.len() == 6
                && row.column(2).name() == "u"
                && !row.column(2).type_info().name().is_empty()
                && row.try_get_raw("n")?.is_null()
                && row.try_get::<String, _>("empty_text")?.is_empty()
                && row.try_get::<u64, _>("u")?.to_string() == "18446744073709551615"
                && row.try_get::<i64, _>("i")?.to_string() == "-9223372036854775808"
                && row.try_get::<BigDecimal, _>("d")?.to_string() == DECIMAL
                && row.try_get::<Vec<u8>, _>("b")? == [0, 255, 1, 128])
        }
    );
    probe!(
        report,
        "native_enum",
        "schema-defined ENUM; text decoded without changing labels",
        async {
            sqlx::raw_sql("CREATE TABLE enum_probe (value ENUM('active','unknown label')); INSERT INTO enum_probe VALUES ('unknown label')").execute(&mut conn).await?;
            let row = sqlx::query("SELECT value FROM enum_probe")
                .fetch_one(&mut conn)
                .await?;
            Ok(row.try_get::<String, _>(0)? == "unknown label")
        }
    );
    probe!(
        report,
        "multiple_results",
        "raw_sql fetch_many preserves row/result boundaries",
        async {
            let mut stream = sqlx::raw_sql("SELECT 1 AS first_result; SELECT 2 AS second_result")
                .fetch_many(&mut conn);
            let mut rows = Vec::new();
            let mut boundaries = 0;
            while let Some(item) = stream.try_next().await? {
                match item {
                    Either::Left(_) => boundaries += 1,
                    Either::Right(row) => rows.push(row.try_get::<i32, _>(0)?),
                }
            }
            Ok(rows == [1, 2] && boundaries == 2)
        }
    );
    probe!(
        report,
        "stored_procedure_multiple_results",
        "CALL with two row sets plus protocol completion result",
        async {
            sqlx::raw_sql("CREATE PROCEDURE stoneql_m001_proc() BEGIN SELECT 7 AS first_result; SELECT NULL AS second_result; END").execute(&mut conn).await?;
            let mut stream = sqlx::raw_sql("CALL stoneql_m001_proc()").fetch_many(&mut conn);
            let mut rows = 0;
            let mut boundaries = 0;
            let mut valid = true;
            while let Some(item) = stream.try_next().await? {
                match item {
                    Either::Left(_) => boundaries += 1,
                    Either::Right(row) => {
                        valid &= if rows == 0 {
                            row.try_get::<i32, _>(0)? == 7
                        } else {
                            row.try_get_raw(0)?.is_null()
                        };
                        rows += 1;
                    }
                }
            }
            Ok(valid && rows == 2 && boundaries >= 2)
        }
    );
    probe!(
        report,
        "server_cancellation_and_reuse",
        "control connection KILL QUERY; verify interrupted result and reusable connection",
        mysql_cancel(url)
    );
    probe!(
        report,
        "untrusted_tls_rejected",
        "VerifyIdentity must reject fixture without trusted server identity",
        async {
            let options = mysql_options(url)?.ssl_mode(MySqlSslMode::VerifyIdentity);
            match MySqlConnection::connect_with(&options).await {
                Ok(c) => {
                    c.close().await?;
                    Ok(false)
                }
                Err(error) => Ok(tls_rejection(&error)),
            }
        }
    );
    report.limitation(
        "positive_tls_and_unknown_raw_types",
        "unverified",
        "no trusted TLS chain fixture or exhaustive opaque-type codec in M001",
    );
    for _ in 0..100 {
        let start = Instant::now();
        let value: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut conn).await?;
        if value != 1 {
            return Err(failure());
        }
        report.query_us.push(start.elapsed().as_micros() as u64);
        let start = Instant::now();
        let c = MySqlConnection::connect_with(&mysql_options(url)?).await?;
        c.close().await?;
        report.connect_us.push(start.elapsed().as_micros() as u64);
    }
    conn.close().await?;
    Ok(())
}
async fn mysql_cancel(url: &str) -> Result<bool> {
    let mut target = MySqlConnection::connect_with(&mysql_options(url)?).await?;
    let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut target)
        .await?;
    let mut control = MySqlConnection::connect_with(&mysql_options(url)?).await?;
    let task = tokio::spawn(async move {
        let result = sqlx::query("SELECT value FROM enum_probe WHERE SLEEP(10) = 0")
            .fetch_one(&mut target)
            .await;
        (target, result)
    });
    let wait = Instant::now();
    loop {
        let active:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID=? AND COMMAND IN ('Query','Execute') AND INFO LIKE '%SLEEP(10)%'").bind(id).fetch_one(&mut control).await?;
        if active == 1 {
            break;
        }
        if wait.elapsed() > Duration::from_secs(3) {
            return Err(failure());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Only a server-returned u64 is interpolated; no user SQL or identifiers enter this command.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("KILL QUERY {id}")))
        .execute(&mut control)
        .await?;
    let (mut target, result) = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .map_err(|_| failure())?
        .map_err(|_| failure())?;
    let interrupted = matches!(result,Err(ref error) if error.as_database_error().and_then(|e|e.code()).as_deref()==Some("70100"));
    let healthy: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut target)
        .await?;
    target.close().await?;
    control.close().await?;
    Ok(interrupted && healthy == 1)
}
async fn postgres(report: &mut Report, url: &str) -> Result<()> {
    let mut conn = PgConnection::connect_with(&pg_options(url)?).await?;
    let marker: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
        .fetch_one(&mut conn)
        .await?;
    if marker != env::var("STONEQL_SPIKE_FIXTURE_ID").map_err(|_| failure())? {
        return Err(failure());
    }
    report.engine_version = sqlx::query_scalar("SHOW server_version")
        .fetch_one(&mut conn)
        .await?;
    probe!(
        report,
        "dynamic_null_bigint_decimal_binary",
        "runtime metadata and lossless BIGINT/NUMERIC/BYTEA decoders",
        async {
            let row=sqlx::query("SELECT NULL::text AS n, ''::text AS empty_text, '-9223372036854775808'::bigint AS i, '123456789012345678901234567890.4500'::numeric(38,4) AS d, decode('00ff0180','hex') AS b").fetch_one(&mut conn).await?;
            Ok(row.len() == 5
                && row.column(2).name() == "i"
                && row.column(2).type_info().name() == "INT8"
                && row.try_get_raw("n")?.is_null()
                && row.try_get::<String, _>("empty_text")?.is_empty()
                && row.try_get::<i64, _>("i")?.to_string() == "-9223372036854775808"
                && row.try_get::<BigDecimal, _>("d")?.to_string() == DECIMAL
                && row.try_get::<Vec<u8>, _>("b")? == [0, 255, 1, 128])
        }
    );
    probe!(
        report,
        "unknown_native_enum_raw_bytes",
        "custom type metadata and opaque PostgreSQL wire bytes",
        async {
            sqlx::raw_sql("CREATE TYPE fixture_enum AS ENUM ('active', 'unknown label')")
                .execute(&mut conn)
                .await?;
            let row = sqlx::query("SELECT 'unknown label'::fixture_enum AS value")
                .fetch_one(&mut conn)
                .await?;
            Ok(row.column(0).type_info().name() == "fixture_enum"
                && row.try_get_raw(0)?.as_bytes().map_err(|_| failure())? == b"unknown label")
        }
    );
    probe!(
        report,
        "multiple_results",
        "raw_sql fetch_many records each statement boundary",
        async {
            let mut stream = sqlx::raw_sql("SELECT 1 AS first_result; SELECT 2 AS second_result")
                .fetch_many(&mut conn);
            let mut rows = Vec::new();
            let mut boundaries = 0;
            while let Some(item) = stream.try_next().await? {
                match item {
                    Either::Left(_) => boundaries += 1,
                    Either::Right(row) => rows.push(row.try_get::<i32, _>(0)?),
                }
            }
            Ok(rows == [1, 2] && boundaries == 2)
        }
    );
    probe!(
        report,
        "stored_procedure",
        "PostgreSQL INOUT procedure CALL returns its result",
        async {
            sqlx::raw_sql("CREATE PROCEDURE stoneql_m001_proc(INOUT value bigint) LANGUAGE plpgsql AS $$ BEGIN value := value + 1; END $$").execute(&mut conn).await?;
            let row = sqlx::query("CALL stoneql_m001_proc(7::bigint)")
                .fetch_one(&mut conn)
                .await?;
            Ok(row.try_get::<i64, _>(0)? == 8)
        }
    );
    probe!(
        report,
        "server_cancellation_and_reuse",
        "control connection pg_cancel_backend; verify SQLSTATE 57014 and reuse",
        pg_cancel(url)
    );
    probe!(
        report,
        "untrusted_tls_rejected",
        "VerifyFull must reject fixture without trusted TLS",
        async {
            match PgConnection::connect_with(&pg_options(url)?.ssl_mode(PgSslMode::VerifyFull))
                .await
            {
                Ok(c) => {
                    c.close().await?;
                    Ok(false)
                }
                Err(error) => Ok(tls_rejection(&error)),
            }
        }
    );
    report.limitation(
        "positive_tls_and_exhaustive_unknown_types",
        "unverified",
        "custom ENUM tested; no trusted TLS chain or exhaustive extension-type codecs",
    );
    for _ in 0..100 {
        let start = Instant::now();
        let value: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut conn).await?;
        if value != 1 {
            return Err(failure());
        }
        report.query_us.push(start.elapsed().as_micros() as u64);
        let start = Instant::now();
        let c = PgConnection::connect_with(&pg_options(url)?).await?;
        c.close().await?;
        report.connect_us.push(start.elapsed().as_micros() as u64);
    }
    conn.close().await?;
    Ok(())
}
async fn pg_cancel(url: &str) -> Result<bool> {
    let mut target = PgConnection::connect_with(&pg_options(url)?).await?;
    let id: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut target)
        .await?;
    let mut control = PgConnection::connect_with(&pg_options(url)?).await?;
    let task = tokio::spawn(async move {
        let result = sqlx::query("SELECT pg_sleep(10)")
            .fetch_one(&mut target)
            .await;
        (target, result)
    });
    let wait = Instant::now();
    loop {
        let active:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pg_stat_activity WHERE pid=$1 AND state='active' AND query LIKE 'SELECT pg_sleep%'").bind(id).fetch_one(&mut control).await?;
        if active == 1 {
            break;
        }
        if wait.elapsed() > Duration::from_secs(3) {
            return Err(failure());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let cancelled: bool = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
        .bind(id)
        .fetch_one(&mut control)
        .await?;
    let (mut target, result) = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .map_err(|_| failure())?
        .map_err(|_| failure())?;
    let interrupted = matches!(result,Err(ref error) if error.as_database_error().and_then(|e|e.code()).as_deref()==Some("57014"));
    let healthy: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut target)
        .await?;
    target.close().await?;
    control.close().await?;
    Ok(cancelled && interrupted && healthy == 1)
}
async fn sqlite(report: &mut Report) -> Result<()> {
    let mut conn = SqliteConnection::connect("sqlite::memory:").await?;
    report.engine_version = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut conn)
        .await?;
    probe!(
        report,
        "dynamic_null_bigint_decimal_binary",
        "SQLite runtime storage classes; exact decimal retained as TEXT",
        async {
            let row=sqlx::query("SELECT NULL AS n, '' AS empty_text, -9223372036854775808 AS i, '123456789012345678901234567890.4500' AS d, x'00ff0180' AS b").fetch_one(&mut conn).await?;
            Ok(row.len() == 5
                && row.try_get_raw("n")?.is_null()
                && row.try_get::<String, _>("empty_text")?.is_empty()
                && row.try_get::<i64, _>("i")?.to_string() == "-9223372036854775808"
                && row.try_get::<String, _>("d")? == DECIMAL
                && row.try_get::<Vec<u8>, _>("b")? == [0, 255, 1, 128])
        }
    );
    probe!(
        report,
        "numeric_affinity_precision_limit_observed",
        "NUMERIC affinity stores a large fractional value as REAL; not exact DECIMAL support",
        async {
            sqlx::raw_sql("CREATE TABLE affinity_probe (value DECIMAL(38,4)); INSERT INTO affinity_probe VALUES ('123456789012345678901234567890.4500')").execute(&mut conn).await?;
            let kind: String = sqlx::query_scalar("SELECT typeof(value) FROM affinity_probe")
                .fetch_one(&mut conn)
                .await?;
            Ok(kind == "real")
        }
    );
    probe!(
        report,
        "unsigned_bigint_limit_observed",
        "out-of-range integer CAST clamps; must never silently use as lossless input conversion",
        async {
            let value: i64 = sqlx::query_scalar("SELECT CAST('18446744073709551615' AS INTEGER)")
                .fetch_one(&mut conn)
                .await?;
            Ok(value == i64::MAX)
        }
    );
    probe!(
        report,
        "multiple_results",
        "raw_sql fetch_many returns both SQLite statement results",
        async {
            let mut stream = sqlx::raw_sql("SELECT 1; SELECT 2").fetch_many(&mut conn);
            let mut rows = Vec::new();
            let mut boundaries = 0;
            while let Some(item) = stream.try_next().await? {
                match item {
                    Either::Left(_) => boundaries += 1,
                    Either::Right(row) => rows.push(row.try_get::<i32, _>(0)?),
                }
            }
            Ok(rows == [1, 2] && boundaries == 2)
        }
    );
    probe!(
        report,
        "stored_procedure_rejected",
        "SQLite rejects CALL instead of emulating a stored procedure",
        async {
            Ok(matches!(
                sqlx::query("CALL stoneql_m001_proc()")
                    .execute(&mut conn)
                    .await,
                Err(sqlx::Error::Database(_))
            ))
        }
    );
    report.limitation(
        "stored_procedure",
        "unsupported",
        "SQLite has no stored procedure language; no CALL emulation",
    );
    probe!(
        report,
        "cancellation_and_reuse",
        "safe progress-handler interrupt; no unsafe sqlite FFI",
        sqlite_cancel()
    );
    for _ in 0..100 {
        let start = Instant::now();
        let value: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&mut conn).await?;
        if value != 1 {
            return Err(failure());
        }
        report.query_us.push(start.elapsed().as_micros() as u64);
        let start = Instant::now();
        let c = SqliteConnection::connect("sqlite::memory:").await?;
        c.close().await?;
        report.connect_us.push(start.elapsed().as_micros() as u64);
    }
    conn.close().await?;
    Ok(())
}
async fn sqlite_cancel() -> Result<bool> {
    let mut conn = SqliteConnection::connect("sqlite::memory:").await?;
    let started = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = started.clone();
    let flag = cancelled.clone();
    conn.lock_handle()
        .await?
        .set_progress_handler(1000, move || {
            signal.store(true, Ordering::SeqCst);
            !flag.load(Ordering::SeqCst)
        });
    let task = tokio::spawn(async move {
        let result=sqlx::query("WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT SUM(x) FROM n").fetch_one(&mut conn).await;
        (conn, result)
    });
    let wait = Instant::now();
    while !started.load(Ordering::SeqCst) {
        if wait.elapsed() > Duration::from_secs(3) {
            return Err(failure());
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    cancelled.store(true, Ordering::SeqCst);
    let (mut returned, result) = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .map_err(|_| failure())?
        .map_err(|_| failure())?;
    returned.lock_handle().await?.remove_progress_handler();
    let interrupted = matches!(result,Err(ref error) if error.as_database_error().and_then(|e|e.code()).as_deref()==Some("9"));
    let value: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut returned)
        .await?;
    returned.close().await?;
    Ok(interrupted && value == 1)
}
#[tokio::main]
async fn main() {
    let engine = env::args().nth(1).unwrap_or_default();
    let mut report = Report {
        engine: engine.clone(),
        engine_version: String::new(),
        sqlx_version: "0.9.0",
        probes: vec![],
        query_us: vec![],
        connect_us: vec![],
    };
    let start = Instant::now();
    let result = match engine.as_str() {
        "mysql" | "mariadb" => match fixture_url(&engine) {
            Ok(url) => mysql(&mut report, &url).await,
            Err(e) => Err(e),
        },
        "postgresql" => match fixture_url(&engine) {
            Ok(url) => postgres(&mut report, &url).await,
            Err(e) => Err(e),
        },
        "sqlite" => sqlite(&mut report).await,
        _ => Err(failure()),
    };
    report.record(
        "fixture_boundary_and_lifecycle",
        "unique DB marker, loopback-only URL and explicit closes; container cleanup checked by runner",
        start,
        result.map(|()| true),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .expect("report contains only serializable observations")
    );
    if report.probes.iter().any(|p| p.status == "failed") {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MARKER: Option<&str> = Some("00000000-0000-0000-0000-000000000001");
    #[test]
    fn refuses_non_fixture_targets_before_connecting() {
        for target in [
            "mysql://127.0.0.1:3306/production",
            "mysql://example.org:3306/stoneql_m001_fixture",
            "mysql://127.0.0.1/stoneql_m001_fixture",
            "mysql://127.0.0.1:3306/stoneql_m001_fixture?socket=other",
            "mysql://127.0.0.1:3306/stoneql_m001_fixture#fragment",
            "postgres://127.0.0.1:5432/stoneql_m001_fixture",
        ] {
            assert!(validate_fixture_target("mysql", target, MARKER).is_err());
        }
        let fixture = "mysql://127.0.0.1:3306/stoneql_m001_fixture";
        assert!(validate_fixture_target("mysql", fixture, None).is_err());
        assert!(validate_fixture_target("mysql", fixture, Some("not-owned")).is_err());
        assert!(validate_fixture_target("mysql", fixture, MARKER).is_ok());
    }
    #[test]
    fn tls_negative_probe_rejects_generic_connection_failures() {
        use std::io::{Error, ErrorKind};
        assert!(!tls_rejection(&sqlx::Error::Io(Error::from(
            ErrorKind::ConnectionRefused
        ))));
        assert!(!tls_rejection(&sqlx::Error::Io(Error::new(
            ErrorKind::InvalidData,
            "not a certificate failure"
        ))));
        assert!(tls_rejection(&sqlx::Error::Io(Error::new(
            ErrorKind::InvalidData,
            "invalid peer certificate: UnknownIssuer"
        ))));
    }
}
