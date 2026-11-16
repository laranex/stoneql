//! SQL import benchmark for the A20 limit sign-off (MySQL/MariaDB). Run only by
//! `npm run bench:sql-import`, which owns the fixture containers and credentials and runs one
//! case per process under `/usr/bin/time -l` (peak RSS). Prints one JSON line; no SQL text or
//! credentials are reported. A case generates a StoneQL-export-style SQL file (multi-row INSERTs
//! with `_utf8mb4 X'…'` literals) in the runner's temporary directory, reads it once as the host
//! does, validates and runs it through `Connections::start_sql_import` with the durable outcome
//! journal attached (as in the app), verifies the row count and removes the file.
use serde_json::json;
use sqlx::{
    Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{
    env,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};
use stoneql_contracts::v1::ExecutionOutcomeDto;
use stoneql_core::Engine;
use stoneql_runtime::{Access, Connections, DirectServer, ExecutionLimits, JobOptions, OwnerId};

type Failure = Box<dyn std::error::Error>;
const MIB: usize = 1024 * 1024;

struct Case {
    file: usize,
    statement: usize,
    execute: bool,
    /// Diagnostic only: run without the durable journal to separate its per-statement cost
    /// from the server's.
    journal: bool,
}
fn case(name: &str) -> Option<Case> {
    let (file, statement, execute) = match name {
        "exec-1k-4m" | "exec-1k-4m-nojournal" => (4 * MIB, 1024, true),
        "exec-64k-64m" => (64 * MIB, 64 * 1024, true),
        "exec-1m-256m" => (256 * MIB, MIB, true),
        "stmt-4m" => (4 * MIB, 4 * MIB, true),
        "stmt-8m" => (8 * MIB, 8 * MIB, true),
        "stmt-15m" => (15 * MIB, 15 * MIB, true),
        "stmt-16m" => (16 * MIB, 16 * MIB, true),
        "validate-1k-1g" => (1024 * MIB - 4096, 1450, false),
        "exec-4m-1g" => (1024 * MIB - 4096, 4 * MIB, true),
        _ => return None,
    };
    Some(Case {
        file,
        statement,
        execute,
        journal: !name.ends_with("-nojournal"),
    })
}

fn hex(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02X}")).collect()
}
/// Writes multi-row INSERTs of at most `statement` bytes until the file reaches `file` bytes.
/// Returns (rows, statements).
fn generate(path: &Path, file: usize, statement: usize) -> Result<(u64, u64), Failure> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    let header = "-- StoneQL import benchmark (synthetic).\nSET NAMES utf8mb4;\n";
    out.write_all(header.as_bytes())?;
    let prefix = "INSERT INTO `import_bench` (`id`, `label`, `amount`, `payload`) VALUES\n";
    let payload = hex(
        "synthetic payload é ✓ with a comma, a quote ' and a backslash \\ — padded to look like real text",
    );
    let (mut written, mut rows, mut statements) = (header.len(), 0u64, 0u64);
    let mut current = String::with_capacity(statement.min(32 * MIB));
    while written < file {
        current.clear();
        current.push_str(prefix);
        let mut first = true;
        loop {
            let id = rows + 1;
            let row = format!(
                "(_utf8mb4 X'{}', _utf8mb4 X'{}', _utf8mb4 X'{}', _utf8mb4 X'{payload}')",
                hex(&id.to_string()),
                hex(&format!("row-{id}")),
                hex(&format!("{}.{:04}", id % 100_000, id % 10_000))
            );
            // Statement end ";\n" is 2 bytes; a separator ",\n" precedes all but the first row.
            let extra = row.len() + if first { 0 } else { 2 };
            if !first && current.len() + extra + 2 > statement {
                break;
            }
            if written + current.len() + extra + 2 > file && !first {
                break;
            }
            if !first {
                current.push_str(",\n");
            }
            current.push_str(&row);
            first = false;
            rows += 1;
            if current.len() + 2 >= statement || written + current.len() + 2 >= file {
                break;
            }
        }
        current.push_str(";\n");
        out.write_all(current.as_bytes())?;
        written += current.len();
        statements += 1;
    }
    out.flush()?;
    Ok((rows, statements))
}

fn rss_now() -> Option<u64> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kib| kib * 1024)
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let name = env::args().nth(1).ok_or("case name required")?;
    let case = case(&name).ok_or("unknown case")?;
    let engine = match env::var("STONEQL_LIFECYCLE_ENGINE")?.as_str() {
        "mysql" => Engine::MySql,
        "mariadb" => Engine::MariaDb,
        _ => return Err("MySQL or MariaDB fixture required".into()),
    };
    let port: u16 = env::var("STONEQL_LIFECYCLE_PORT")?.parse()?;
    let password = env::var("STONEQL_FIXTURE_PASSWORD")?;
    let marker = env::var("STONEQL_SPIKE_FIXTURE_ID")?;
    let ca = std::fs::read(env::var("STONEQL_LIFECYCLE_CA")?)?;
    let dir = std::path::PathBuf::from(env::var("STONEQL_BENCH_DIR")?);
    let database = "stoneql_m001_fixture";
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("stoneql_writer")
        .password(&password)
        .database(database)
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .ssl_ca_from_pem(ca.clone());
    let mut observer = MySqlConnection::connect_with(&options).await?;
    let owned: String = sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
        .fetch_one(&mut observer)
        .await?;
    if owned != marker {
        return Err("fixture ownership mismatch".into());
    }
    sqlx::raw_sql(
        "DROP TABLE IF EXISTS import_bench; CREATE TABLE import_bench(id BIGINT UNSIGNED PRIMARY KEY, label VARCHAR(64) NOT NULL, amount DECIMAL(20,4) NOT NULL, payload VARCHAR(255) NOT NULL) ENGINE=InnoDB CHARACTER SET utf8mb4",
    )
    .execute(&mut observer)
    .await?;
    let max_allowed_packet: u64 = sqlx::query_scalar("SELECT @@max_allowed_packet")
        .fetch_one(&mut observer)
        .await?;
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&mut observer)
        .await?;

    let connections = Connections::new(4, Duration::from_secs(3), Duration::from_secs(3600))?;
    let owner = OwnerId::new();
    // The durable unknown-outcome journal, as in the app: every statement is journaled first.
    let journal = dir.join(format!("journal-{name}"));
    if case.journal {
        connections.open_outcome_journal(&journal, owner);
    }
    let handle = connections
        .connect_with_access(
            owner,
            DirectServer {
                engine,
                host: "127.0.0.1".into(),
                port,
                username: "stoneql_writer".into(),
                password: password.clone(),
                database: database.into(),
                root_certificate_pem: Some(ca.clone()),
                tls: true,
            },
            None,
            None,
            Access::ReadWrite,
        )
        .await?;
    // A job proves the handle works before timing (and warms the TLS path).
    let (probe, _) = connections.submit_job(owner, handle, JobOptions::new("SELECT 1"))?;
    loop {
        let update = connections
            .fetch_job(owner, probe, Duration::from_secs(1))
            .await?;
        if update.done {
            break;
        }
    }

    let path = dir.join(format!("{name}.sql"));
    let started = Instant::now();
    let (rows, statements) = generate(&path, case.file, case.statement)?;
    let generate_ms = started.elapsed().as_secs_f64() * 1000.0;
    let file_bytes = std::fs::metadata(&path)?.len();
    let baseline_rss = rss_now();
    let started = Instant::now();
    let bytes = std::fs::read(&path)?;
    let read_ms = started.elapsed().as_secs_f64() * 1000.0;
    std::fs::remove_file(&path)?;
    let mut report = json!({
        "case": name, "engine": format!("{engine:?}"), "serverVersion": version,
        "maxAllowedPacket": max_allowed_packet,
        "limits": {"fileBytes": ExecutionLimits::MAX_IMPORT_BYTES, "statementBytes": ExecutionLimits::MAX_IMPORT_STATEMENT_BYTES, "statements": ExecutionLimits::MAX_IMPORT_STATEMENTS},
        "fileBytes": file_bytes, "generatedRows": rows, "generatedStatements": statements,
        "targetStatementBytes": case.statement, "journal": case.journal,
        "generateMs": generate_ms, "readMs": read_ms, "rssBeforeReadBytes": baseline_rss,
    });
    if !case.execute {
        // Validation only: a read-only handle would refuse before validating, so validation is
        // timed through the same call on the read-write handle and the import is cancelled
        // before its connection opens (nothing is sent: `Cancelled`, not unknown).
        let started = Instant::now();
        let accepted = connections.start_sql_import(owner, handle, bytes);
        let validate_ms = started.elapsed().as_secs_f64() * 1000.0;
        let rss_after_validate = rss_now();
        let (id, count) = accepted.map_err(|error| format!("{error:?}"))?;
        connections.cancel_job(owner, id)?;
        let status = loop {
            let status = connections.sql_import_status(owner, id)?;
            if status.done {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        report["validateMs"] = json!(validate_ms);
        report["validatedStatements"] = json!(count);
        report["rssAfterValidateBytes"] = json!(rss_after_validate);
        report["outcome"] = json!(status.outcome);
        report["completedStatements"] = json!(status.completed);
        println!("{report}");
        return Ok(());
    }
    let started = Instant::now();
    let accepted = connections.start_sql_import(owner, handle, bytes);
    let validate_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rss_after_validate = rss_now();
    report["validateMs"] = json!(validate_ms);
    report["rssAfterValidateBytes"] = json!(rss_after_validate);
    let (id, count) = match accepted {
        Ok(started) => started,
        Err(error) => {
            report["refused"] = json!(format!("{error:?}"));
            println!("{report}");
            return Ok(());
        }
    };
    let started = Instant::now();
    let status = loop {
        let status = connections.sql_import_status(owner, id)?;
        if status.done {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let execute_ms = started.elapsed().as_secs_f64() * 1000.0;
    let imported: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM import_bench")
        .fetch_one(&mut observer)
        .await?;
    report["validatedStatements"] = json!(count);
    report["executeMs"] = json!(execute_ms);
    report["outcome"] = json!(status.outcome);
    report["completedStatements"] = json!(status.completed);
    report["affectedRows"] = json!(status.affected_rows);
    report["importedRows"] = json!(imported);
    report["failedSqlState"] = json!(
        status
            .failed
            .as_ref()
            .and_then(|failed| failed.failure.as_ref())
            .and_then(|failure| failure.sql_state.clone())
    );
    report["unknownStatement"] = json!(status.unknown.as_ref().map(|unknown| unknown.index));
    report["mibPerSecond"] = json!(file_bytes as f64 / MIB as f64 / (execute_ms / 1000.0));
    report["rowsPerSecond"] = json!(imported as f64 / (execute_ms / 1000.0));
    report["statementsPerSecond"] = json!(f64::from(status.completed) / (execute_ms / 1000.0));
    if status.outcome == Some(ExecutionOutcomeDto::Succeeded) && imported as u64 != rows {
        return Err("imported row count differs from the generated file".into());
    }
    // Unknown outcomes from probes stay out of later cases: each case has its own journal dir.
    sqlx::raw_sql("DROP TABLE import_bench")
        .execute(&mut observer)
        .await?;
    println!("{report}");
    Ok(())
}
