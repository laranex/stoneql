//! Legitimate-query metadata measurements against the owned disposable fixture created by
//! experiments/sqlx-metadata-limit/run-live.mjs. Prints one JSON line; no credentials or rows.
//!
//! Usage: live_metadata setup
//!        live_metadata <baseline|limited> <expr|alias64|table> <columns>
use futures_util::TryStreamExt;
use sqlx::{
    ConnectOptions, Connection, Executor, MySqlConnection, SqlSafeStr, Statement,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{env, num::NonZeroUsize, time::Instant};

// Proposed adoption values under evaluation.
const LIMIT_COLUMNS: usize = 4096;
const LIMIT_BYTES: usize = 4 * 1024 * 1024;
const LIMIT_MESSAGE: &str = "result metadata exceeds configured limit";

fn options(limited: bool) -> Result<MySqlConnectOptions, Box<dyn std::error::Error>> {
    let port: u16 = env::var("STONEQL_METADATA_PORT")?.parse()?;
    let password = env::var("STONEQL_FIXTURE_PASSWORD")?;
    // Mirrors crates/stoneql-runtime options that affect metadata handling. The fixture is a
    // disposable loopback container; TLS identity verification is out of scope here.
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("root")
        .password(&password)
        .database("stoneql_m001_fixture")
        .ssl_mode(MySqlSslMode::Preferred)
        .statement_cache_capacity(0)
        .max_receive_packet_size(NonZeroUsize::new(2 * 1024 * 1024).unwrap())
        .disable_statement_logging();
    Ok(if limited {
        options.max_result_metadata(
            NonZeroUsize::new(LIMIT_COLUMNS).unwrap(),
            NonZeroUsize::new(LIMIT_BYTES).unwrap(),
        )
    } else {
        options
    })
}

fn classify(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Protocol(message) if message == LIMIT_MESSAGE => "metadata_limit".into(),
        sqlx::Error::Protocol(message)
            if message == "receive packet exceeds configured byte limit" =>
        {
            "packet_limit".into()
        }
        sqlx::Error::Database(error) => {
            format!("database_{}", error.code().unwrap_or_default())
        }
        sqlx::Error::Protocol(_) => "protocol_other".into(),
        sqlx::Error::Io(_) => "io".into(),
        sqlx::Error::ColumnIndexOutOfBounds { .. } => "column_index".into(),
        _ => "other".into(),
    }
}

fn wide_name(index: usize) -> String {
    format!("{:0<64}", format!("c{index}_"))
}

fn sql(kind: &str, columns: usize) -> String {
    match kind {
        "table" => "SELECT * FROM stoneql_wide".into(),
        "alias64" => format!(
            "SELECT {}",
            (1..=columns)
                .map(|index| format!("1 AS `{}`", wide_name(index)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => format!(
            "SELECT {}",
            (1..=columns)
                .map(|index| format!("1 AS c{index}"))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

// Same order as Connections::read_only: read-only transaction, prepare, then
// non-persistent execution (which prepares again and reads result metadata).
async fn query(connection: &mut MySqlConnection, sql: &str) -> Result<(usize, usize), sqlx::Error> {
    sqlx::raw_sql("START TRANSACTION READ ONLY")
        .execute(&mut *connection)
        .await?;
    let statement = connection
        .prepare(sqlx::AssertSqlSafe(sql.to_owned()).into_sql_str())
        .await?;
    let columns = statement.columns().len();
    let mut rows = statement.query().persistent(false).fetch(&mut *connection);
    let mut count = 0;
    while let Some(row) = rows.try_next().await? {
        let _ = sqlx::Row::len(&row);
        count += 1;
    }
    Ok((columns, count))
}

async fn setup() -> Result<(), Box<dyn std::error::Error>> {
    let mut connection = MySqlConnection::connect_with(&options(false)?).await?;
    let marker: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
        .fetch_one(&mut connection)
        .await?;
    if marker != env::var("STONEQL_SPIKE_FIXTURE_ID")? {
        return Err("fixture ownership mismatch".into());
    }
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&mut connection)
        .await?;
    // Largest table-backed shape: 64-character identifiers in a MyISAM table.
    let mut table = None;
    for count in [4096usize, 4095, 4000, 2048] {
        let ddl = format!(
            "CREATE TABLE stoneql_wide ({}) ENGINE=MyISAM",
            (1..=count)
                .map(|index| format!("`{}` TINYINT", wide_name(index)))
                .collect::<Vec<_>>()
                .join(",")
        );
        match sqlx::raw_sql(sqlx::AssertSqlSafe(ddl))
            .execute(&mut connection)
            .await
        {
            Ok(_) => {
                table = Some((count, None));
                break;
            }
            Err(error) => table = Some((0, Some(classify(&error)))),
        }
    }
    let (table_columns, table_error) = table.unwrap();
    println!(
        "{{\"serverVersion\":\"{version}\",\"wideTableColumns\":{table_columns},\"wideTableLastError\":{},\"sizeOfMySqlColumn\":{},\"limitColumns\":{LIMIT_COLUMNS},\"limitBytes\":{LIMIT_BYTES}}}",
        table_error.map_or("null".into(), |error| format!("\"{error}\"")),
        std::mem::size_of::<sqlx::mysql::MySqlColumn>(),
    );
    connection.close().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("setup") {
        return setup().await;
    }
    let [mode, kind, columns] = args.as_slice() else {
        return Err(
            "usage: live_metadata <baseline|limited> <expr|alias64|table> <columns>".into(),
        );
    };
    let limited = match mode.as_str() {
        "baseline" => false,
        "limited" => true,
        _ => return Err("unknown mode".into()),
    };
    let columns: usize = columns.parse()?;
    let sql = sql(kind, columns);
    let options = options(limited)?;
    let mut connection = MySqlConnection::connect_with(&options).await?;
    let started = Instant::now();
    let result = query(&mut connection, &sql).await;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (outcome, received, rows) = match &result {
        Ok((received, rows)) => ("ok".to_string(), *received, *rows),
        Err(error) => (classify(error), 0, 0),
    };
    // Like the runtime, never reuse a connection after an error: drop it and prove that a
    // fresh connection with the same options serves a small query.
    let discarded_after_error = result.is_err();
    if discarded_after_error {
        drop(connection);
    } else {
        connection.close().await?;
    }
    let mut fresh = MySqlConnection::connect_with(&options).await?;
    let fresh_ok = query(&mut fresh, "SELECT 1").await.is_ok();
    fresh.close().await?;
    println!(
        "{{\"mode\":\"{mode}\",\"kind\":\"{kind}\",\"requestedColumns\":{columns},\"sqlBytes\":{},\"outcome\":\"{outcome}\",\"preparedColumns\":{received},\"rows\":{rows},\"elapsedMs\":{elapsed_ms:.3},\"discardedAfterError\":{discarded_after_error},\"freshConnectionOk\":{fresh_ok}}}",
        sql.len()
    );
    Ok(())
}
