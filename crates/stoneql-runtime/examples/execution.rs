//! Disposable fixture harness for M006 read-write execution (MySQL, MariaDB, PostgreSQL).
//! Run only by `npm run test:execution`, which owns the fixture containers and credentials.
//! Prints one JSON report; assertions fail the run. No SQL text or credentials are reported.
//! A failure names the harness step and, for a fixture-owner query, that query's fixed harness
//! text (tags and server ids only) on stderr; the runner also redacts the password.
use serde_json::json;
use sqlx::{
    Connection, MySqlConnection, PgConnection, SqlSafeStr,
    mysql::{MySqlConnectOptions, MySqlSslMode},
    postgres::{PgConnectOptions, PgSslMode},
};
use std::{
    env,
    time::{Duration, Instant},
};
use stoneql_contracts::v1::{
    CancellationDto, CellValue, ConstraintKindDto, ExecutionOutcomeDto, JobUpdate, LeaseStateDto,
    ResultBatch, SqlImportStatus, StatementStatusDto,
};
use stoneql_core::{Engine, Outcome};
use stoneql_runtime::{
    Access, ConnectionId, Connections, DirectServer, EndReason, ExecutionError, ExecutionLimits,
    ExportFormat, ExportProgress, ExportRequest, ImportError, JobId, JobOptions, LeaseId,
    OutcomeId, OwnerId, QueryLimits, ReadError, ScriptError, TransactionEnd, mysql_state_matches,
    postgres_commit, postgres_state_matches,
};

type Failure = Box<dyn std::error::Error>;

/// The harness step now running, prefixed to any error `main` returns so a failure (including a
/// bare driver error from an observer query) is locatable. Static labels only.
static STEP: std::sync::Mutex<&'static str> = std::sync::Mutex::new("startup");
fn step(name: &'static str) {
    *STEP.lock().expect("step lock") = name;
}
/// Context for an observer/locker query error: the harness's own fixed SQL (tags and server ids
/// only, never credentials or user data); long text keeps its head and its tag-bearing tail.
fn observer_error(sql: &str, error: sqlx::Error) -> Failure {
    let chars: Vec<char> = sql.chars().collect();
    let sql: String = if chars.len() > 240 {
        let (head, tail) = (&chars[..120], &chars[chars.len() - 120..]);
        format!("{}…{}", String::from_iter(head), String::from_iter(tail))
    } else {
        sql.to_owned()
    };
    format!("fixture-owner query `{sql}`: {error} ({error:?})").into()
}

/// Fixture-owner connection used only to observe and inject faults.
enum Observer {
    MySql(MySqlConnection),
    Postgres(PgConnection),
}
impl Observer {
    async fn scalar_i64(&mut self, sql: &'static str) -> Result<i64, Failure> {
        match self {
            Self::MySql(connection) => sqlx::query_scalar(sql).fetch_one(connection).await,
            Self::Postgres(connection) => sqlx::query_scalar(sql).fetch_one(connection).await,
        }
        .map_err(|error| observer_error(sql, error))
    }
    async fn scalar(&mut self, sql: String) -> Result<i64, Failure> {
        let text = sqlx::AssertSqlSafe(sql.clone());
        match self {
            Self::MySql(connection) => sqlx::query_scalar(text).fetch_one(connection).await,
            Self::Postgres(connection) => sqlx::query_scalar(text).fetch_one(connection).await,
        }
        .map_err(|error| observer_error(&sql, error))
    }
    async fn execute(&mut self, sql: String) -> Result<(), Failure> {
        let text = sqlx::AssertSqlSafe(sql.clone());
        match self {
            Self::MySql(connection) => sqlx::raw_sql(text).execute(connection).await.map(drop),
            Self::Postgres(connection) => sqlx::raw_sql(text).execute(connection).await.map(drop),
        }
        .map_err(|error| observer_error(&sql, error))
    }
}

struct Run {
    engine: Engine,
    connections: Connections,
    owner: OwnerId,
    handle: ConnectionId,
}
impl Run {
    fn mysql(&self) -> bool {
        self.engine != Engine::PostgreSql
    }
    async fn drain(&self, id: JobId) -> Result<(Vec<ResultBatch>, JobUpdate), Failure> {
        let mut batches = Vec::new();
        for _ in 0..900 {
            let update = self
                .connections
                .fetch_job(self.owner, id, Duration::from_secs(1))
                .await?;
            if let Some(batch) = update.batch.clone() {
                batches.push(batch);
            }
            if update.done {
                self.settle(id, &update)?;
                return Ok((batches, update));
            }
        }
        Err("job did not finish".into())
    }
    /// P3: an unknown job outcome stays listed until acknowledged. The harness checks it is
    /// listed, then acknowledges it (after asserting the outcome itself), so the many unknown
    /// samples below are not refused by `MAX_UNRESOLVED_OUTCOMES_PER_OWNER`.
    fn settle(&self, id: JobId, update: &JobUpdate) -> Result<(), Failure> {
        if update.outcome != Some(ExecutionOutcomeDto::Unknown) {
            return Ok(());
        }
        self.acknowledge(
            &id.opaque(),
            "unknown job outcome listed until acknowledged",
        )
    }
    fn acknowledge(&self, id: &str, listed: &'static str) -> Result<(), Failure> {
        let unresolved = self.connections.unresolved_outcomes(self.owner).outcomes;
        ensure(
            unresolved.iter().any(|outcome| outcome.outcome_id == id),
            listed,
        )?;
        let remaining = self
            .connections
            .acknowledge_outcome(self.owner, OutcomeId::parse(id)?)?;
        ensure(
            remaining
                .outcomes
                .iter()
                .all(|outcome| outcome.outcome_id != id),
            "acknowledged outcome hidden",
        )
    }
    async fn run(
        &self,
        sql: &str,
        lease: Option<LeaseId>,
    ) -> Result<(Vec<ResultBatch>, JobUpdate), Failure> {
        self.run_on(self.handle, sql, lease).await
    }
    async fn run_on(
        &self,
        handle: ConnectionId,
        sql: &str,
        lease: Option<LeaseId>,
    ) -> Result<(Vec<ResultBatch>, JobUpdate), Failure> {
        let mut options = JobOptions::new(sql);
        options.lease = lease;
        let (id, _) = self.connections.submit_job(self.owner, handle, options)?;
        self.drain(id).await
    }
    /// A statement that runs until interrupted, preceded by any session setup it needs.
    fn slow(&self) -> &'static str {
        match self.engine {
            Engine::PostgreSql => "SELECT pg_sleep(30)",
            Engine::MySql => {
                "SET SESSION cte_max_recursion_depth = 4294967295; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 4000000000) SELECT MAX(x) FROM c"
            }
            // MariaDB's recursion bound is a session variable with a small default.
            _ => {
                "SET SESSION max_recursive_iterations = 4294967295; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 4000000000) SELECT MAX(x) FROM c"
            }
        }
    }
}
fn statuses(update: &JobUpdate) -> Vec<StatementStatusDto> {
    update
        .statements
        .iter()
        .map(|report| report.status)
        .collect()
}
fn text(value: &CellValue) -> Option<&str> {
    match value {
        CellValue::Integer { text }
        | CellValue::Decimal { text }
        | CellValue::Text { text }
        | CellValue::Temporal { text, .. } => Some(text),
        _ => None,
    }
}
fn percentile(samples: &mut [f64], p: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[((samples.len() as f64 * p).ceil() as usize).saturating_sub(1)]
}
fn ensure(condition: bool, message: &'static str) -> Result<(), Failure> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

/// The fixture's administrative MySQL/MariaDB account over verified TLS (observer/lock holder).
fn admin_options(port: u16, password: &str, ca: &[u8]) -> MySqlConnectOptions {
    MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("root")
        .password(password)
        .database("stoneql_m001_fixture")
        .ssl_mode(MySqlSslMode::VerifyIdentity)
        .ssl_ca_from_pem(ca.to_vec())
}

/// One command to the test-owned loopback fault relay (`tools/tcp-fault-relay.mjs`).
async fn relay(command: &str) -> Result<String, Failure> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let port: u16 = env::var("STONEQL_RELAY_CONTROL")?.parse()?;
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    stream.write_all(format!("{command}\n").as_bytes()).await?;
    let mut line = String::new();
    tokio::io::BufReader::new(stream)
        .read_line(&mut line)
        .await?;
    ensure(line.trim() != "error" && !line.is_empty(), "relay command")?;
    Ok(line.trim().to_owned())
}

/// Polls `sql` (via the observer) until it returns `expected`, for at most `limit`.
async fn wait_for(
    observer: &mut Observer,
    sql: &str,
    expected: impl Fn(i64) -> bool,
    limit: Duration,
) -> Result<bool, Failure> {
    let until = Instant::now() + limit;
    loop {
        if expected(observer.scalar(sql.to_owned()).await?) {
            return Ok(true);
        }
        if Instant::now() >= until {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Engine-specific observer queries for the fault sections. MySQL/MariaDB: PROCESSLIST and the
/// general log (`Query`/`Execute` rows; prepares excluded). PostgreSQL: `pg_stat_activity` and
/// the fixture's csvlog (`log_statement = all`), read by the superuser observer, whose own
/// statements are not logged. Every value interpolated here is a fixed tag or a server id.
struct Spy {
    mysql: bool,
}
const PG_LOG: &str = "regexp_split_to_table(pg_read_file('log/stoneql.csv'), E'\\n') AS line";
impl Spy {
    /// Writer session currently running a statement containing `tag` (0 if none).
    fn running(&self, tag: &str) -> String {
        if self.mysql {
            format!(
                "SELECT CAST(COALESCE(MAX(ID), 0) AS SIGNED) FROM information_schema.PROCESSLIST WHERE USER = 'stoneql_writer' AND INFO LIKE '%{tag}%'"
            )
        } else {
            format!(
                "SELECT COALESCE(MAX(pid), 0)::int8 FROM pg_stat_activity WHERE usename = 'stoneql_writer' AND state = 'active' AND query LIKE '%{tag}%'"
            )
        }
    }
    fn exists(&self, session: i64) -> String {
        if self.mysql {
            format!("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID = {session}")
        } else {
            format!("SELECT COUNT(*) FROM pg_stat_activity WHERE pid = {session}")
        }
    }
    fn sessions(&self) -> &'static str {
        if self.mysql {
            "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER = 'stoneql_writer'"
        } else {
            "SELECT COUNT(*) FROM pg_stat_activity WHERE usename = 'stoneql_writer'"
        }
    }
    /// Executions (not prepares) of a statement containing `tag` by the writer.
    fn executions(&self, tag: &str) -> String {
        if self.mysql {
            format!(
                "SELECT COUNT(*) FROM mysql.general_log WHERE command_type IN ('Query', 'Execute') AND thread_id <> CONNECTION_ID() AND argument LIKE '%{tag}%'"
            )
        } else {
            format!(
                "SELECT COUNT(*) FROM {PG_LOG} WHERE line LIKE '%,\"execute %: %{tag}%' AND split_part(line, ',', 2) = '\"stoneql_writer\"'"
            )
        }
    }
    /// The writer session that executed a statement containing `tag` (0 if none).
    fn session_of(&self, tag: &str) -> String {
        if self.mysql {
            format!(
                "SELECT CAST(COALESCE(MAX(thread_id), 0) AS SIGNED) FROM mysql.general_log WHERE command_type IN ('Query', 'Execute') AND thread_id <> CONNECTION_ID() AND argument LIKE '%{tag}%'"
            )
        } else {
            format!(
                "SELECT COALESCE(MAX(split_part(line, ',', 4)::int8), 0) FROM {PG_LOG} WHERE line LIKE '%,\"execute %: %{tag}%' AND split_part(line, ',', 2) = '\"stoneql_writer\"'"
            )
        }
    }
    /// Times `statement` (exact text) was sent as a whole query on one session.
    fn statements_on(&self, session: i64, statement: &str) -> String {
        if self.mysql {
            format!(
                "SELECT COUNT(*) FROM mysql.general_log WHERE thread_id = {session} AND command_type = 'Query' AND CONVERT(argument USING utf8mb4) = '{statement}'"
            )
        } else {
            format!(
                "SELECT COUNT(*) FROM {PG_LOG} WHERE split_part(line, ',', 4) = '{session}' AND line LIKE '%,\"statement: {statement}\",%'"
            )
        }
    }
    fn kill(&self, session: i64) -> String {
        if self.mysql {
            format!("KILL {session}")
        } else {
            format!("SELECT pg_terminate_backend({session})")
        }
    }
    /// The COMMIT text StoneQL sends (PostgreSQL: savepoint-checked, R-10).
    fn commit(&self) -> &'static str {
        if self.mysql {
            "COMMIT"
        } else {
            "SAVEPOINT stoneql_commit; COMMIT"
        }
    }
}

/// P3 (M006 blocker 3, Contract 4.5): a lost-COMMIT `unknown` (relay `sever_on_reply`: COMMIT
/// forwarded, reply never delivered) is kept, listable and queryable, through more confirmed
/// lease ends than both retention bounds (32 per owner, 256 overall), which do evict the
/// oldest confirmed ones. Acknowledging it hides it without changing the recorded outcome.
async fn unknown_outcome_retention(
    run: &Run,
    relay_handle: ConnectionId,
    observer: &mut Observer,
    journaled_target: DirectServer,
) -> Result<serde_json::Value, Failure> {
    let (connections, owner) = (&run.connections, run.owner);
    relay("normal").await?;
    step("unknown outcome retention: begin");
    let lease = connections.begin_transaction(owner, relay_handle).await?;
    step("unknown outcome retention: insert");
    let tag = "(9001, 1)";
    let (_, update) = run
        .run_on(
            relay_handle,
            &format!("INSERT INTO exec_item(id, parent) VALUES {tag}"),
            Some(lease),
        )
        .await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Succeeded),
        "retention lease insert",
    )?;
    relay("sever_on_reply").await?;
    step("unknown outcome retention: lost commit");
    let lost = connections.end_transaction(owner, lease, true).await?;
    relay("sever").await?;
    relay("normal").await?;
    let expected = TransactionEnd {
        outcome: Outcome::Unknown,
        reason: EndReason::Requested,
        commit_sent: true,
        rollback_sent: false,
    };
    ensure(lost == expected, "lost COMMIT is unknown with commitSent")?;
    let churn = ExecutionLimits::MAX_ENDED_LEASES + ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER;
    let started = Instant::now();
    let mut first = None;
    for _ in 0..churn {
        step("unknown outcome retention: churn begin");
        let churned = connections.begin_transaction(owner, run.handle).await?;
        step("unknown outcome retention: churn end");
        first.get_or_insert(churned);
        let end = connections.end_transaction(owner, churned, false).await?;
        ensure(
            end.outcome == Outcome::RolledBack,
            "churn rollback confirmed",
        )?;
    }
    let churn_ms = started.elapsed().as_secs_f64() * 1000.0;
    step("unknown outcome retention: after churn");
    let first = first.ok_or("churn")?;
    // The oldest confirmed end was evicted; the unknown one was not.
    let evicted =
        connections.end_transaction(owner, first, true).await == Err(ExecutionError::Forbidden);
    ensure(evicted, "confirmed ends beyond the bounds are evicted")?;
    let listed = connections
        .unresolved_outcomes(owner)
        .outcomes
        .into_iter()
        .find(|outcome| outcome.outcome_id == lease.opaque())
        .ok_or("lost COMMIT still listed after churn")?;
    ensure(
        listed.transaction == Some(expected.into()) && listed.job.is_none(),
        "listed facts unchanged",
    )?;
    ensure(
        connections.end_transaction(owner, lease, false).await? == expected,
        "lost COMMIT still queryable after churn",
    )?;
    let applied = observer
        .scalar("SELECT COUNT(*) FROM exec_item WHERE id = 9001".into())
        .await?;
    let remaining = connections.acknowledge_outcome(owner, OutcomeId::parse(&lease.opaque())?)?;
    ensure(
        remaining
            .outcomes
            .iter()
            .all(|outcome| outcome.outcome_id != lease.opaque())
            && connections.end_transaction(owner, lease, true).await? == expected,
        "acknowledgement hides the outcome without changing it",
    )?;
    let restart = journal_restart(observer, journaled_target).await?;
    Ok(json!({
        "lostCommit": {"outcome": "unknown", "commitSent": true, "rowCommittedObserved": applied},
        "confirmedEndsChurned": churn, "churnMs": churn_ms,
        "oldestConfirmedEvicted": evicted,
        "unknownListedAfterChurn": true, "unknownQueryableAfterChurn": true,
        "acknowledgementChangesOutcome": false,
        "journalRestart": restart,
    }))
}

/// Durable journal on a real server: a lost-COMMIT `unknown` (`sever_on_reply`) is journaled by
/// a registry with a journal in a private temporary directory; that registry is dropped without
/// shutdown (a crash), a new one is created from the journal, and the outcome is listed for
/// the new owner with unchanged facts, marked as recorded before a restart.
async fn journal_restart(
    observer: &mut Observer,
    target: DirectServer,
) -> Result<serde_json::Value, Failure> {
    let root = env::temp_dir().join(format!("stoneql-fixture-journal-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root)?;
    let dir = root.join("outcome-journal");
    let result = async {
        let fresh = || Connections::new(2, Duration::from_secs(3), Duration::from_secs(3600));
        let crashed = fresh()?;
        crashed.require_outcome_journal();
        let owner = OwnerId::new();
        ensure(
            crashed.open_outcome_journal(&dir, owner).is_none(),
            "journal opened",
        )?;
        let handle = crashed
            .connect_with_access(owner, target, None, None, Access::ReadWrite)
            .await?;
        relay("normal").await?;
        step("journal restart: begin");
        let lease = crashed.begin_transaction(owner, handle).await?;
        let mut options = JobOptions::new("INSERT INTO exec_item(id, parent) VALUES (9002, 1)");
        options.lease = Some(lease);
        let (id, _) = crashed.submit_job(owner, handle, options)?;
        let mut done = false;
        for _ in 0..30 {
            let update = crashed.fetch_job(owner, id, Duration::from_secs(1)).await?;
            if update.done {
                ensure(
                    update.outcome == Some(ExecutionOutcomeDto::Succeeded),
                    "journaled lease insert",
                )?;
                done = true;
                break;
            }
        }
        ensure(done, "journaled lease insert finished")?;
        relay("sever_on_reply").await?;
        step("journal restart: lost commit");
        let lost = crashed.end_transaction(owner, lease, true).await?;
        relay("sever").await?;
        relay("normal").await?;
        let expected = TransactionEnd {
            outcome: Outcome::Unknown,
            reason: EndReason::Requested,
            commit_sent: true,
            rollback_sent: false,
        };
        ensure(lost == expected, "journaled lost COMMIT is unknown")?;
        let before = crashed
            .unresolved_outcomes(owner)
            .outcomes
            .into_iter()
            .find(|outcome| outcome.outcome_id == lease.opaque())
            .ok_or("lost COMMIT listed before the crash")?;
        // Crash: no shutdown, nothing concluded or flushed by the runtime on the way out.
        drop(crashed);
        step("journal restart: restored");
        let restarted = fresh()?;
        let next = OwnerId::new();
        ensure(
            restarted.open_outcome_journal(&dir, next).is_none(),
            "journal reopened",
        )?;
        let listed = restarted.unresolved_outcomes(next);
        let after = listed
            .outcomes
            .iter()
            .find(|outcome| outcome.outcome_id == lease.opaque())
            .ok_or("lost COMMIT restored after the restart")?;
        ensure(
            after.restored
                && !after.app_exited
                && after.transaction == Some(expected.into())
                && after.transaction == before.transaction
                && (after.engine.clone(), after.database.clone())
                    == (before.engine.clone(), before.database.clone())
                && after.engine.is_some(),
            "restored facts unchanged",
        )?;
        ensure(
            restarted.end_transaction(next, lease, false).await? == expected,
            "restored outcome answers as recorded and sends nothing",
        )?;
        let applied = observer
            .scalar("SELECT COUNT(*) FROM exec_item WHERE id = 9002".into())
            .await?;
        restarted.acknowledge_outcome(next, OutcomeId::parse(&lease.opaque())?)?;
        let modes = {
            use std::os::unix::fs::MetadataExt;
            (
                std::fs::metadata(&dir)?.mode() & 0o777,
                std::fs::metadata(dir.join("unknown-outcomes.journal"))?.mode() & 0o777,
            )
        };
        ensure(modes == (0o700, 0o600), "journal modes")?;
        drop(restarted);
        Ok::<_, Failure>(json!({
            "lostCommitJournaled": true, "restoredAfterDrop": true,
            "factsUnchanged": true, "restoredMarked": true,
            "rowCommittedObserved": applied, "modes": [format!("{:o}", modes.0), format!("{:o}", modes.1)],
            "otherRestoredOutcomes": listed.outcomes.len() - 1,
        }))
    }
    .await;
    let _ = std::fs::remove_dir_all(&root);
    result
}

/// Requests cancellation and returns (cancel→terminal latency, final update).
async fn import_done(
    connections: &Connections,
    owner: OwnerId,
    id: JobId,
) -> Result<SqlImportStatus, Failure> {
    for _ in 0..3000 {
        let status = connections.sql_import_status(owner, id)?;
        if status.done {
            return Ok(status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("import did not finish".into())
}
/// M007 d5 SQL import on the real server (MySQL/MariaDB, A20): a StoneQL SQL export of
/// `export_values` (CREATE TABLE and data, edge values) is renamed in the text, imported from one
/// validated snapshot and compared NULL-safely and byte-exactly; a failing statement stops the
/// import with its number and line; a refused statement means nothing runs.
async fn sql_import(
    connections: &Connections,
    owner: OwnerId,
    handle: ConnectionId,
    observer: &mut Observer,
    report: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), Failure> {
    let mut exported = Vec::new();
    let progress = ExportProgress::default();
    connections
        .export_table(
            owner,
            handle,
            &ExportRequest {
                table: Some("export_values".into()),
                format: ExportFormat::Sql,
                structure: true,
                data: true,
            },
            &mut exported,
            &progress,
        )
        .await
        .map_err(|error| format!("export: {error:?}"))?;
    let text = String::from_utf8(exported)?.replace("`export_values`", "`import_values`");
    // Whole database (table: None): every base table's CREATE TABLE and rows in one snapshot,
    // foreign-key checks off while importing; CSV is refused for a whole database.
    let mut whole = Vec::new();
    connections
        .export_table(
            owner,
            handle,
            &ExportRequest {
                table: None,
                format: ExportFormat::Sql,
                structure: true,
                data: true,
            },
            &mut whole,
            &ExportProgress::default(),
        )
        .await
        .map_err(|error| format!("whole-database export: {error:?}"))?;
    let whole = String::from_utf8(whole)?;
    ensure(
        whole.contains("CREATE TABLE `export_values`")
            && whole.contains("CREATE TABLE `exec_text`")
            && whole.contains("INSERT INTO `export_values`")
            && whole.contains("SET FOREIGN_KEY_CHECKS = 0;")
            && whole.find("SET FOREIGN_KEY_CHECKS = 0;") < whole.find("CREATE TABLE")
            && whole.trim_end().ends_with("SET FOREIGN_KEY_CHECKS = 1;"),
        "whole-database export carries every table",
    )?;
    ensure(
        connections
            .export_table(
                owner,
                handle,
                &ExportRequest {
                    table: None,
                    format: ExportFormat::Csv,
                    structure: false,
                    data: true,
                },
                &mut Vec::new(),
                &ExportProgress::default(),
            )
            .await
            == Err(stoneql_runtime::TransferError::InvalidRequest),
        "whole-database CSV export is refused",
    )?;
    ensure(
        text.contains("CREATE TABLE `import_values`"),
        "export carries structure",
    )?;
    let (id, count) = connections
        .start_sql_import(owner, handle, text.into_bytes())
        .map_err(|error| format!("{error:?}"))?;
    let status = import_done(connections, owner, id).await?;
    ensure(
        status.outcome == Some(ExecutionOutcomeDto::Succeeded) && status.completed == count,
        "round-trip import succeeded",
    )?;
    let columns = ["t", "d", "dt", "zd", "e", "s", "j", "f"];
    let same = format!(
        "SELECT COUNT(*) FROM export_values a JOIN import_values b USING (id) WHERE {}",
        columns
            .iter()
            .map(|c| format!("BINARY a.{c} <=> BINARY b.{c}"))
            .collect::<Vec<_>>()
            .join(" AND ")
    );
    ensure(
        observer
            .scalar("SELECT COUNT(*) FROM import_values".into())
            .await?
            == 5
            && observer.scalar(same).await? == 5,
        "imported rows equal the source exactly",
    )?;
    // Statement 3 (line 3) fails on the server: 1–2 stay committed (autocommit), 4 never runs.
    let (id, _) = connections
        .start_sql_import(
            owner,
            handle,
            b"INSERT INTO exec_text (id, label) VALUES (31, 'a');\nINSERT INTO exec_text (id, label) VALUES (32, 'b');\nINSERT INTO exec_text (id, label) VALUES (31, 'dup');\nINSERT INTO exec_text (id, label) VALUES (33, 'c');"
                .to_vec(),
        )
        .map_err(|error| format!("{error:?}"))?;
    let status = import_done(connections, owner, id).await?;
    let failed = status.failed.clone().ok_or("failed statement reported")?;
    ensure(
        status.outcome == Some(ExecutionOutcomeDto::Failed)
            && status.completed == 2
            && status.affected_rows == "2"
            && (failed.index, failed.line) == (2, 3)
            && failed.failure.as_ref().and_then(|f| f.sql_state.as_deref()) == Some("23000"),
        "first failure stops the import with its position",
    )?;
    ensure(
        observer
            .scalar("SELECT COUNT(*) FROM exec_text WHERE id IN (31, 32)".into())
            .await?
            == 2
            && observer
                .scalar("SELECT COUNT(*) FROM exec_text WHERE id = 33".into())
                .await?
                == 0,
        "statements before the failure committed, after it not run",
    )?;
    // A refused statement anywhere: nothing is sent.
    let refused = connections
        .start_sql_import(
            owner,
            handle,
            b"INSERT INTO exec_text (id) VALUES (34);\nUSE mysql;".to_vec(),
        )
        .err()
        .ok_or("USE refused")?;
    ensure(
        matches!(&refused, ImportError::Refused(r) if r.statement == Some(2) && r.line == Some(2)),
        "refusal names statement and line",
    )?;
    ensure(
        observer
            .scalar("SELECT COUNT(*) FROM exec_text WHERE id = 34".into())
            .await?
            == 0,
        "nothing of a refused file ran",
    )?;
    report.insert(
        "sqlImport".into(),
        json!({"roundTripRows": 5, "roundTripStatements": count, "stopAtFailure": {"statement": 3, "line": 3, "sqlState": "23000"}, "refusedNothingRan": true}),
    );
    Ok(())
}
async fn cancel_and_wait(run: &Run, id: JobId) -> Result<(f64, JobUpdate), Failure> {
    let requested = Instant::now();
    run.connections.cancel_job(run.owner, id)?;
    let mut terminal = None;
    loop {
        let update = run
            .connections
            .fetch_job(run.owner, id, Duration::from_millis(20))
            .await?;
        if terminal.is_none() && update.outcome.is_some() {
            terminal = Some(requested.elapsed().as_secs_f64() * 1000.0);
        }
        if update.done {
            run.settle(id, &update)?;
            return Ok((terminal.ok_or("terminal")?, update));
        }
    }
}

/// R-2/R-3 on MySQL/MariaDB. Every refused input carries a unique tag; the general log (all
/// command types, prepares included) proves nothing containing it reached the server.
async fn policy_and_state(
    run: &Run,
    observer: &mut Observer,
    writer: MySqlConnectOptions,
) -> Result<serde_json::Value, Failure> {
    let (connections, owner, handle) = (&run.connections, run.owner, run.handle);
    observer
        .execute("SET GLOBAL log_output = 'TABLE'".into())
        .await?;
    observer
        .execute("SET GLOBAL general_log = 'ON'".into())
        .await?;
    let sent = |tag: &str| {
        format!(
            "SELECT COUNT(*) FROM mysql.general_log WHERE thread_id <> CONNECTION_ID() AND argument LIKE '%{tag}%'"
        )
    };
    // Positive control: a statement that is sent appears in the log under its tag.
    run.run("SELECT 1 /* stoneqlr2x0 */", None).await?;
    ensure(
        observer.scalar(sent("stoneqlr2x0")).await? >= 1,
        "general log records sent statements",
    )?;
    let probes = [
        (
            "set_statement_commit",
            "SET STATEMENT max_statement_time=60 FOR COMMIT /* TAG */",
            ScriptError::SetStatement,
        ),
        (
            "set_statement_create_table",
            "SET STATEMENT max_statement_time=60 FOR CREATE TABLE exec_never_TAG(i int)",
            ScriptError::SetStatement,
        ),
        (
            "set_statement_prepare",
            "SET STATEMENT max_statement_time=60 FOR PREPARE s FROM 'COMMIT /* TAG */'",
            ScriptError::SetStatement,
        ),
        (
            "case_commit",
            "DELIMITER //\nCASE WHEN 1 THEN COMMIT; END CASE /* TAG */ //",
            ScriptError::CompoundStatement,
        ),
        (
            "case_autocommit",
            "DELIMITER //\nCASE WHEN 1 THEN SET @x = 1; END CASE /* TAG */ //",
            ScriptError::CompoundStatement,
        ),
        (
            "call_autocommit_off",
            "CALL exec_autocommit_off() /* TAG */; INSERT INTO exec_item(id, parent) VALUES (7001, 1)",
            ScriptError::Call,
        ),
        (
            "call_start_transaction",
            "CALL exec_start_txn() /* TAG */; INSERT INTO exec_item(id, parent) VALUES (7002, 1)",
            ScriptError::Call,
        ),
    ];
    let mut refused = serde_json::Map::new();
    let mut number = 0;
    for (name, template, error) in probes {
        for in_lease in [false, true] {
            number += 1;
            let tag = format!("stoneqlr2x{number}");
            let lease = if in_lease {
                Some(connections.begin_transaction(owner, handle).await?)
            } else {
                None
            };
            let mut options = JobOptions::new(template.replace("TAG", &tag));
            options.lease = lease;
            let result = connections.submit_job(owner, handle, options);
            if let Some(lease) = lease {
                let end = connections.end_transaction(owner, lease, false).await?;
                ensure(
                    end.outcome == Outcome::RolledBack,
                    "probe lease rolled back",
                )?;
            }
            ensure(
                result == Err(ExecutionError::Script(0, error)),
                "wrapper/compound/CALL refused",
            )?;
            ensure(
                observer.scalar(sent(&tag)).await? == 0,
                "refused input not sent",
            )?;
            refused.insert(
                format!("{name}{}", if in_lease { "_lease" } else { "" }),
                json!({"refused": format!("{error:?}"), "generalLogRows": 0}),
            );
        }
    }
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id IN (7001, 7002)")
            .await?
            == 0,
        "refused CALL scripts inserted nothing",
    )?;
    observer
        .execute("SET GLOBAL general_log = 'OFF'".into())
        .await?;

    // The runtime's session-state probe against real sessions (the review's R-3 procedures run
    // directly on a writer connection, outside StoneQL): it must report a mismatch for every
    // session that would have discarded or committed work behind StoneQL's back.
    let mut state = serde_json::Map::new();
    let mut connection = MySqlConnection::connect_with(&writer).await?;
    ensure(
        mysql_state_matches(&mut connection, false).await
            && !mysql_state_matches(&mut connection, true).await,
        "fresh autocommit session matches",
    )?;
    connection.close().await?;
    state.insert("autocommitSessionMatches".into(), json!(true));
    for (name, procedure, id) in [
        ("procedureAutocommitOff", "CALL exec_autocommit_off()", 7101),
        ("procedureStartTransaction", "CALL exec_start_txn()", 7102),
    ] {
        let mut connection = MySqlConnection::connect_with(&writer).await?;
        let rows = |sql: String| sqlx::raw_sql(sqlx::AssertSqlSafe(sql));
        rows(procedure.into()).execute(&mut connection).await?;
        let inserted = rows(format!(
            "INSERT INTO exec_item(id, parent) VALUES ({id}, 1)"
        ))
        .execute(&mut connection)
        .await?
        .rows_affected();
        let detected = !mysql_state_matches(&mut connection, false).await;
        connection.close().await?;
        let survived = observer
            .scalar(format!("SELECT COUNT(*) FROM exec_item WHERE id = {id}"))
            .await?;
        ensure(
            inserted == 1 && detected && survived == 0,
            "diverged session detected",
        )?;
        state.insert(
            name.into(),
            json!({"insertReportedRows": inserted, "probeMismatch": detected, "rowsAfterClose": survived}),
        );
    }
    let mut connection = MySqlConnection::connect_with(&writer).await?;
    sqlx::raw_sql("START TRANSACTION")
        .execute(&mut connection)
        .await?;
    let open = mysql_state_matches(&mut connection, true).await;
    sqlx::raw_sql("COMMIT").execute(&mut connection).await?;
    let committed = !mysql_state_matches(&mut connection, true).await;
    connection.close().await?;
    ensure(
        open && committed,
        "lease transaction ended behind the lease",
    )?;
    state.insert(
        "leaseEndedBehindStoneql".into(),
        json!({"openMatches": open, "afterCommitMismatch": committed}),
    );
    Ok(
        json!({"generalLogPositiveControl": true, "refusedBeforeSend": refused, "sessionStateProbe": state}),
    )
}

/// Lost-reply runs per relay mode (Contract 7.1: `sever_on_reply`, the COMMIT reached the
/// server; 7.2: `sever`, it did not). `drop_s2c` waits out the full COMMIT deadline per run,
/// so it keeps a small sample.
fn lost_reply_runs(mode: &str) -> usize {
    if mode == "drop_s2c" { 3 } else { 50 }
}

/// B-5, requirement 8, L-3 and B-4 (Contracts 5.3, 7.1, 7.2) on every network engine. Data
/// states are reported as observed evidence; nothing here claims a rollback the server did not
/// confirm.
async fn write_faults(
    run: &Run,
    observer: &mut Observer,
    mut locker: Observer,
    relayed: DirectServer,
    journaled: DirectServer,
) -> Result<serde_json::Map<String, serde_json::Value>, Failure> {
    let (connections, owner) = (&run.connections, run.owner);
    let spy = Spy { mysql: run.mysql() };
    let mut report = serde_json::Map::new();
    let relay_handle = connections
        .connect_with_access(owner, relayed, None, None, Access::ReadWrite)
        .await?;
    if spy.mysql {
        observer
            .execute("SET GLOBAL log_output = 'TABLE'".into())
            .await?;
        observer
            .execute("SET GLOBAL general_log = 'ON'".into())
            .await?;
    }
    // Positive control: an executed statement is visible in the server's statement log.
    run.run("SELECT 1 WHERE 4242 = 4242", None).await?;
    ensure(
        wait_for(
            observer,
            &spy.executions("4242 = 4242"),
            |count| count == 1,
            Duration::from_secs(5),
        )
        .await?,
        "statement log records executions",
    )?;
    let sessions = spy.sessions();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let baseline = observer.scalar(sessions.into()).await?;
    let initial = observer
        .scalar("SELECT v FROM exec_lock WHERE id = 1".into())
        .await?;

    step("write cancel");
    // A. Long-running write blocked on a row lock held by another session, then cancelled.
    let (mut samples, mut kills, mut unchanged, mut discarded, mut once) = (Vec::new(), 0, 0, 0, 0);
    for sample in 0..30 {
        locker.execute("BEGIN".into()).await?;
        locker
            .execute("SELECT v FROM exec_lock WHERE id = 1 FOR UPDATE".into())
            .await?;
        let tag = format!("AND {0} = {0}", 1000 + sample);
        let (id, _) = connections.submit_job(
            owner,
            run.handle,
            JobOptions::new(format!("UPDATE exec_lock SET v = v + 1 WHERE id = 1 {tag}")),
        )?;
        ensure(
            wait_for(
                observer,
                &spy.running(&tag),
                |session| session > 0,
                Duration::from_secs(5),
            )
            .await?,
            "write blocked on the lock",
        )?;
        let session = observer.scalar(spy.running(&tag)).await?;
        let (latency, update) = cancel_and_wait(run, id).await?;
        samples.push(latency);
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Unknown)
                && update.cancellation == CancellationDto::Confirmed
                && update.server_cancel_sent
                && statuses(&update) == [StatementStatusDto::Unknown],
            "write cancel outcome",
        )?;
        kills += usize::from(
            wait_for(
                observer,
                &spy.running(&tag),
                |session| session == 0,
                Duration::from_secs(2),
            )
            .await?,
        );
        discarded += usize::from(
            wait_for(
                observer,
                &spy.exists(session),
                |count| count == 0,
                Duration::from_secs(3),
            )
            .await?
                && wait_for(
                    observer,
                    sessions,
                    // The background expiry lease may end meanwhile; never above baseline.
                    |count| count <= baseline,
                    Duration::from_secs(3),
                )
                .await?,
        );
        locker.execute("ROLLBACK".into()).await?;
        unchanged += usize::from(
            observer
                .scalar("SELECT v FROM exec_lock WHERE id = 1".into())
                .await?
                == initial,
        );
        once += usize::from(
            wait_for(
                observer,
                &spy.executions(&tag),
                |count| count >= 1,
                Duration::from_secs(2),
            )
            .await?
                && observer.scalar(spy.executions(&tag)).await? == 1,
        );
    }
    let n = samples.len();
    ensure(
        kills == n && unchanged == n && discarded == n && once == n,
        "write cancel evidence",
    )?;
    let max = samples.iter().copied().fold(0.0, f64::max);
    report.insert(
        "writeCancel".into(),
        json!({
            "n": n, "p50Ms": percentile(&mut samples.clone(), 0.5),
            "p95Ms": percentile(&mut samples.clone(), 0.95), "maxMs": max,
            "outcome": "unknown", "cancellation": "confirmed",
            "serverInterrupt": if spy.mysql { "KILL QUERY" } else { "pg_cancel_backend" },
            "interruptDelivered": format!("{kills}/{n}"), "connectionDiscarded": format!("{discarded}/{n}"),
            "rowUnchanged": format!("{unchanged}/{n}"), "executedOnce": format!("{once}/{n}"),
            "sessionsBaseline": baseline,
            "note": "rowUnchanged is the observed data state after the lock holder rolled back (statement never acquired the lock); not a claimed rollback",
        }),
    );
    ensure(
        percentile(&mut samples, 0.95) <= 2000.0,
        "write cancel p95 within proposed 2 s",
    )?;

    step("bulk cancel");
    // A2. Large INSERT … SELECT cancelled mid-way: statement atomicity observed afterwards.
    let mut bulk = Vec::new();
    for sample in 0..3 {
        let tag = format!("WHERE {0} = {0}", 3000 + sample);
        let (id, _) = connections.submit_job(
            owner,
            run.handle,
            JobOptions::new(format!(
                "INSERT INTO exec_bulk(n) SELECT a.n + 10 * b.n + 100 * c.n + 1000 * d.n + 10000 * e.n + 100000 * f.n + 1000000 * g.n FROM exec_digit a CROSS JOIN exec_digit b CROSS JOIN exec_digit c CROSS JOIN exec_digit d CROSS JOIN exec_digit e CROSS JOIN exec_digit f CROSS JOIN exec_digit g {tag}"
            )),
        )?;
        tokio::time::sleep(Duration::from_millis(700)).await;
        let running = connections.fetch_job(owner, id, Duration::ZERO).await?;
        ensure(running.outcome.is_none(), "bulk insert still running")?;
        let (latency, update) = cancel_and_wait(run, id).await?;
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Unknown)
                && statuses(&update) == [StatementStatusDto::Unknown],
            "bulk cancel unknown",
        )?;
        // Wait for the server to finish the statement's own rollback before observing.
        let settled = wait_for(
            observer,
            &spy.running(&tag),
            |session| session == 0,
            Duration::from_secs(60),
        )
        .await?;
        let rows = observer
            .scalar("SELECT COUNT(*) FROM exec_bulk".into())
            .await?;
        ensure(settled && rows == 0, "cancelled bulk insert left rows")?;
        bulk.push(json!({
            "cancelToTerminalMs": latency, "cancellation": update.cancellation,
            "serverCancelSent": update.server_cancel_sent, "rowsAfterSettle": rows,
            "executedOnce": observer.scalar(spy.executions(&tag)).await? == 1,
        }));
        // The cancelled insert's dead rows stay in the table's files; the fixture's data
        // directory is memory-backed, so they are released before the next sample.
        observer.execute("TRUNCATE TABLE exec_bulk".into()).await?;
    }
    report.insert("bulkCancel".into(), json!(bulk));

    step("kill failure");
    // B. The cancel connection cannot be opened: requested, not confirmed; no hang. With a 5 s
    // statement deadline, PostgreSQL's server statement_timeout (L-3, deadline - margin) must
    // end the abandoned statement server-side, after the abandon and before the client
    // deadline; MySQL/MariaDB have none and the observer ends it.
    locker.execute("BEGIN".into()).await?;
    locker
        .execute("SELECT v FROM exec_lock WHERE id = 1 FOR UPDATE".into())
        .await?;
    let tag = "AND 2000 = 2000";
    let mut options = JobOptions::new(format!("UPDATE exec_lock SET v = v + 1 WHERE id = 1 {tag}"));
    let deadline = Duration::from_secs(5);
    options.statement_deadline = deadline;
    let (id, _) = connections.submit_job(owner, relay_handle, options)?;
    let started = Instant::now();
    ensure(
        wait_for(
            observer,
            &spy.running(tag),
            |session| session > 0,
            Duration::from_secs(5),
        )
        .await?,
        "relayed write blocked",
    )?;
    let session = observer.scalar(spy.running(tag)).await?;
    relay("refuse_new").await?;
    let (latency, update) = cancel_and_wait(run, id).await?;
    relay("normal").await?;
    let bound =
        (ExecutionLimits::CANCEL_GRACE + Duration::from_millis(1000)).as_secs_f64() * 1000.0;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Unknown)
            && update.cancellation == CancellationDto::Requested
            && !update.server_cancel_sent
            && statuses(&update) == [StatementStatusDto::Unknown]
            && latency <= bound,
        "failed interrupt is requested, unknown and bounded",
    )?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let still_running = observer.scalar(spy.running(tag)).await? == session;
    let backstop = ExecutionLimits::postgres_statement_timeout(deadline);
    let ended_by_server = !spy.mysql
        && wait_for(
            observer,
            &spy.running(tag),
            |running| running == 0,
            backstop + Duration::from_secs(2),
        )
        .await?;
    let ended_after = started.elapsed().as_secs_f64() * 1000.0;
    if spy.mysql && still_running {
        observer.execute(spy.kill(session)).await?;
    }
    if !spy.mysql {
        ensure(
            still_running
                && ended_by_server
                && ended_after >= backstop.as_secs_f64() * 1000.0
                && ended_after < deadline.as_secs_f64() * 1000.0,
            "server statement_timeout ended the abandoned statement before the client deadline",
        )?;
        // The abandoned session itself is ended by the observer (PostgreSQL does not notice a
        // closed client while waiting on a lock).
        observer.execute(spy.kill(session)).await?;
    }
    locker.execute("ROLLBACK".into()).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let value = observer
        .scalar("SELECT v FROM exec_lock WHERE id = 1".into())
        .await?;
    report.insert(
        "killFailure".into(),
        json!({
            "cancelToTerminalMs": latency, "boundMs": bound, "outcome": "unknown",
            "cancellation": update.cancellation, "serverCancelSent": update.server_cancel_sent,
            "statementStillRunningAfterAbandon": still_running,
            "killedByObserver": spy.mysql && still_running,
            "serverBackstop": if spy.mysql { json!(null) } else {
                json!({"statementDeadlineMs": deadline.as_millis(), "statementTimeoutMs": backstop.as_millis(), "endedByServer": ended_by_server, "endedAfterSubmitMs": ended_after})
            },
            "rowApplied": value != initial,
            "executions": observer.scalar(spy.executions(tag)).await?,
        }),
    );

    step("postgres server timeout");
    // C. PostgreSQL only: the server statement_timeout fires before the client deadline.
    if !spy.mysql {
        let timeout = postgres_server_timeout(run, observer, &mut locker, &spy, baseline).await?;
        report.insert("postgresServerTimeout".into(), timeout);
    }

    step("lost end replies");
    // D. Lost COMMIT/ROLLBACK replies through the relay: unknown, never retried.
    let mut lost = serde_json::Map::new();
    let mut marker = 8000;
    for commit in [true, false] {
        for mode in ["sever_on_reply", "sever", "drop_s2c"] {
            let (mut unknown, mut applied, mut ended_logged, mut repeat_same, mut times) =
                (0, 0, Vec::new(), 0, Vec::new());
            let runs = lost_reply_runs(mode);
            for _ in 0..runs {
                marker += 1;
                relay("normal").await?;
                let lease = connections.begin_transaction(owner, relay_handle).await?;
                let tag = format!("({marker}, 1)");
                let (_, update) = run
                    .run_on(
                        relay_handle,
                        &format!("INSERT INTO exec_item(id, parent) VALUES {tag}"),
                        Some(lease),
                    )
                    .await?;
                ensure(
                    update.outcome == Some(ExecutionOutcomeDto::Succeeded),
                    "relayed lease insert",
                )?;
                ensure(
                    wait_for(
                        observer,
                        &spy.session_of(&tag),
                        |session| session > 0,
                        Duration::from_secs(5),
                    )
                    .await?,
                    "lease session",
                )?;
                let session = observer.scalar(spy.session_of(&tag)).await?;
                relay(mode).await?;
                let started = Instant::now();
                let end = connections.end_transaction(owner, lease, commit).await?;
                times.push(started.elapsed().as_secs_f64() * 1000.0);
                ensure(
                    end.outcome == Outcome::Unknown
                        && end.reason == EndReason::Requested
                        && end.commit_sent == commit
                        && end.rollback_sent == !commit,
                    "lost reply is unknown with sent facts",
                )?;
                unknown += 1;
                relay("sever").await?;
                relay("normal").await?;
                ensure(
                    wait_for(
                        observer,
                        &spy.exists(session),
                        |count| count == 0,
                        Duration::from_secs(10),
                    )
                    .await?,
                    "lease session discarded",
                )?;
                let statement = if commit { spy.commit() } else { "ROLLBACK" };
                let other = if commit { "ROLLBACK" } else { spy.commit() };
                // The server log may trail the session's end slightly.
                tokio::time::sleep(Duration::from_millis(if spy.mysql { 0 } else { 150 })).await;
                let logged = observer
                    .scalar(spy.statements_on(session, statement))
                    .await?;
                // Asking again returns the recorded outcome and sends nothing.
                let again = connections.end_transaction(owner, lease, !commit).await?;
                repeat_same += usize::from(again == end);
                run.acknowledge(&lease.opaque(), "lost end reply listed until acknowledged")?;
                ensure(
                    logged <= 1
                        && observer
                            .scalar(spy.statements_on(session, statement))
                            .await?
                            == logged
                        && observer.scalar(spy.statements_on(session, other)).await? == 0
                        && observer.scalar(spy.executions(&tag)).await? == 1,
                    "no re-send",
                )?;
                ended_logged.push(logged);
                applied += observer
                    .scalar(format!(
                        "SELECT COUNT(*) FROM exec_item WHERE id = {marker}"
                    ))
                    .await?;
            }
            ensure(
                repeat_same == runs,
                "repeat end returns the recorded outcome",
            )?;
            if !commit {
                ensure(applied == 0, "lost rollback applied the insert")?;
            }
            if commit && mode == "sever_on_reply" {
                // Contract 7.1: the test must be able to produce the dangerous case.
                ensure(applied >= 1, "no lost-reply COMMIT was applied")?;
            }
            if mode == "sever" {
                ensure(applied == 0, "a COMMIT that never left was applied")?;
            }
            let logged_runs = ended_logged.iter().filter(|&&logged| logged == 1).count();
            lost.insert(
                format!("{}_{mode}", if commit { "commit" } else { "rollback" }),
                json!({
                    "n": runs, "unknown": format!("{unknown}/{runs}"),
                    "commitSent": commit, "rollbackSent": !commit,
                    "rowCommittedObserved": applied,
                    "endStatementReachedServer": format!("{logged_runs}/{runs}"),
                    "repeatEndSameOutcome": format!("{repeat_same}/{runs}"),
                    "endMsMax": times.iter().copied().fold(0.0, f64::max),
                    "endMsP50": percentile(&mut times.clone(), 0.5),
                }),
            );
        }
    }
    report.insert("lostEndReplies".into(), serde_json::Value::Object(lost));

    step("unknown outcome retention");
    report.insert(
        "unknownOutcomeRetention".into(),
        unknown_outcome_retention(run, relay_handle, observer, journaled).await?,
    );
    report.insert(
        "noResend".into(),
        json!({
            "source": if spy.mysql { "mysql.general_log (Query/Execute; prepares excluded)" } else { "PostgreSQL csvlog, log_statement = all (execute and simple-query statements; parse excluded)" },
            "writeCancelExecutedOnce": format!("{once}/{n}"), "lostReplyEndStatementsAtMostOnce": true,
        }),
    );
    if spy.mysql {
        observer
            .execute("SET GLOBAL general_log = 'OFF'".into())
            .await?;
    }
    connections.disconnect(owner, relay_handle).await?;
    Ok(report)
}

/// Submits a job and polls it every 20 ms; returns (submit→terminal ms, final update).
async fn timed(run: &Run, options: JobOptions) -> Result<(f64, JobUpdate), Failure> {
    let (id, _) = run.connections.submit_job(run.owner, run.handle, options)?;
    let submitted = Instant::now();
    let mut terminal = None;
    loop {
        let update = run
            .connections
            .fetch_job(run.owner, id, Duration::from_millis(20))
            .await?;
        if terminal.is_none() && update.outcome.is_some() {
            terminal = Some(submitted.elapsed().as_secs_f64() * 1000.0);
        }
        if update.done {
            run.settle(id, &update)?;
            return Ok((terminal.ok_or("terminal")?, update));
        }
    }
}

/// L-3 / Contract 5.3 ordering on PostgreSQL 17.6: StoneQL's server `statement_timeout`
/// (statement deadline - margin) ends a lock-blocked write on the server with SQLSTATE 57014
/// before the client deadline. The job is `failed` (authoritative: the statement ended with an
/// error before any commit, and the session check confirms the transaction state), not
/// `unknown`; in a lease the block is aborted and the lease becomes rollback-only, and the
/// rollback is reported only as the server confirms it. Cancellation still works before the
/// server timeout; a script that disables the server timeout still meets the client deadline
/// and interrupt (the backstop); an external cancel (57014 for another reason) is not reported
/// as a deadline. Writer sessions return to baseline with none left in a transaction.
async fn postgres_server_timeout(
    run: &Run,
    observer: &mut Observer,
    locker: &mut Observer,
    spy: &Spy,
    baseline: i64,
) -> Result<serde_json::Value, Failure> {
    let deadline = Duration::from_secs(3);
    let server = ExecutionLimits::postgres_statement_timeout(deadline);
    let (server_ms, deadline_ms) = (
        server.as_secs_f64() * 1000.0,
        deadline.as_secs_f64() * 1000.0,
    );
    let initial = observer
        .scalar("SELECT v FROM exec_lock WHERE id = 1".into())
        .await?;
    let timeout_errors = format!(
        "SELECT COUNT(*) FROM {PG_LOG} WHERE split_part(line, ',', 2) = '\"stoneql_writer\"' AND line LIKE '%,ERROR,57014,\"canceling statement due to statement timeout\"%'"
    );
    let errors_before = observer.scalar(timeout_errors.clone()).await?;
    let lock = async |locker: &mut Observer| -> Result<(), Failure> {
        locker.execute("BEGIN".into()).await?;
        locker
            .execute("SELECT v FROM exec_lock WHERE id = 1 FOR UPDATE".into())
            .await
    };
    let blocked = |tag: &str| format!("UPDATE exec_lock SET v = v + 1 WHERE id = 1 AND {tag}");
    let is_timeout = |update: &JobUpdate, index: usize| {
        update.outcome == Some(ExecutionOutcomeDto::Failed)
            && update.statements[index].status == StatementStatusDto::Failed
            && update.statements[index]
                .failure
                .as_ref()
                .and_then(|failure| failure.sql_state.as_deref())
                == Some("57014")
            && update.cancellation == CancellationDto::None
            && !update.server_cancel_sent
            && update.deadline_exceeded
    };

    step("postgres server timeout C1 autocommit");
    // C1. Autocommit, 10 samples: failed by the server timeout, before the client deadline.
    let mut samples = Vec::new();
    let mut once = 0;
    for sample in 0..10 {
        let tag = format!("2100{sample} = 2100{sample}");
        lock(locker).await?;
        let mut options = JobOptions::new(blocked(&tag));
        options.statement_deadline = deadline;
        let (terminal, update) = timed(run, options).await?;
        locker.execute("ROLLBACK".into()).await?;
        ensure(
            is_timeout(&update, 0),
            "server statement_timeout ends the write as failed (57014)",
        )?;
        ensure(
            terminal >= server_ms && terminal < deadline_ms,
            "server timeout fires before the client deadline",
        )?;
        once += i64::from(observer.scalar(spy.executions(&tag)).await? == 1);
        samples.push(terminal);
    }
    ensure(once == 10, "timed-out writes executed once")?;
    let value = observer
        .scalar("SELECT v FROM exec_lock WHERE id = 1".into())
        .await?;
    ensure(value == initial, "timed-out autocommit writes not applied")?;
    let errors = observer.scalar(timeout_errors.clone()).await? - errors_before;
    ensure(errors >= 10, "server log records the statement timeouts")?;

    step("postgres server timeout C2 lease");
    // C2. In a lease: the block is aborted (server-observed), the lease rollback-only, and the
    // requested rollback is confirmed by the server.
    let lease = run
        .connections
        .begin_transaction(run.owner, run.handle)
        .await?;
    let (_, update) = run
        .run(
            "INSERT INTO exec_item(id, parent) VALUES (2201, 1)",
            Some(lease),
        )
        .await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Succeeded),
        "lease insert before the timeout",
    )?;
    lock(locker).await?;
    let mut options = JobOptions::new(blocked("2202 = 2202"));
    options.lease = Some(lease);
    options.statement_deadline = deadline;
    let (lease_terminal, update) = timed(run, options).await?;
    locker.execute("ROLLBACK".into()).await?;
    ensure(
        is_timeout(&update, 0)
            && update.lease == Some(LeaseStateDto::RollbackOnly)
            && lease_terminal >= server_ms
            && lease_terminal < deadline_ms,
        "in-lease server timeout makes the lease rollback-only",
    )?;
    let aborted = observer
        .scalar_i64("SELECT COUNT(*) FROM pg_stat_activity WHERE usename = 'stoneql_writer' AND state = 'idle in transaction (aborted)'")
        .await?;
    ensure(aborted == 1, "server shows the lease block aborted")?;
    ensure(
        matches!(
            run.connections.submit_job(run.owner, run.handle, {
                let mut options = JobOptions::new("SELECT 1");
                options.lease = Some(lease);
                options
            }),
            Err(ExecutionError::RollbackOnly)
        ),
        "rollback-only lease refuses further statements",
    )?;
    let end = run
        .connections
        .end_transaction(run.owner, lease, false)
        .await?;
    ensure(
        end.outcome == Outcome::RolledBack && !end.commit_sent,
        "rollback confirmed by the server",
    )?;
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 2201")
            .await?
            == 0,
        "rolled-back lease work not visible",
    )?;

    step("postgres server timeout C3 cancel");
    // C3. A cancel before the server timeout still takes the client interrupt path.
    lock(locker).await?;
    let mut options = JobOptions::new(blocked("2203 = 2203"));
    options.statement_deadline = deadline;
    let (id, _) = run.connections.submit_job(run.owner, run.handle, options)?;
    tokio::time::sleep(Duration::from_millis(700)).await;
    let (cancel_ms, update) = cancel_and_wait(run, id).await?;
    locker.execute("ROLLBACK".into()).await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Unknown)
            && update.cancellation == CancellationDto::Confirmed
            && update.server_cancel_sent
            && !update.deadline_exceeded,
        "cancel before the server timeout is confirmed by the interrupt",
    )?;

    step("postgres server timeout C4 backstop");
    // C4. A script that disables the server timeout meets the client deadline and interrupt.
    lock(locker).await?;
    let mut options = JobOptions::new(format!(
        "SET statement_timeout = 0; {}",
        blocked("2204 = 2204")
    ));
    options.statement_deadline = deadline;
    let (backstop_ms, update) = timed(run, options).await?;
    locker.execute("ROLLBACK".into()).await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Unknown)
            && statuses(&update) == [StatementStatusDto::Succeeded, StatementStatusDto::Unknown]
            && update.deadline_exceeded
            && update.server_cancel_sent
            && backstop_ms >= deadline_ms,
        "client deadline and interrupt remain the backstop",
    )?;

    step("postgres server timeout C5 external cancel");
    // C5. 57014 from someone else's cancel is a failure, not a deadline, even when it arrives
    // after the server-timeout duration (here the script disabled the server timeout, and the
    // external cancel lands between the server timeout and the client deadline).
    lock(locker).await?;
    let tag = "2205 = 2205";
    let mut options = JobOptions::new(format!("SET statement_timeout = 0; {}", blocked(tag)));
    options.statement_deadline = deadline;
    let (id, _) = run.connections.submit_job(run.owner, run.handle, options)?;
    let submitted = Instant::now();
    ensure(
        wait_for(
            observer,
            &spy.running(tag),
            |session| session > 0,
            Duration::from_secs(2),
        )
        .await?,
        "write blocked for external cancel",
    )?;
    let session = observer.scalar(spy.running(tag)).await?;
    tokio::time::sleep_until((submitted + server + Duration::from_millis(300)).into()).await;
    observer
        .execute(format!("SELECT pg_cancel_backend({session})"))
        .await?;
    let (_, update) = run.drain(id).await?;
    let external_ms = submitted.elapsed().as_secs_f64() * 1000.0;
    locker.execute("ROLLBACK".into()).await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Failed)
            && update.statements[1]
                .failure
                .as_ref()
                .and_then(|failure| failure.sql_state.as_deref())
                == Some("57014")
            && update.cancellation == CancellationDto::None
            && !update.deadline_exceeded
            && external_ms < deadline_ms,
        "external cancel is not reported as a deadline",
    )?;

    step("postgres server timeout cleanup");
    // Cleanup: no writer session left in a transaction; the count never exceeds the baseline.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let sessions = observer.scalar(spy.sessions().into()).await?;
    let in_transaction = observer
        .scalar_i64("SELECT COUNT(*) FROM pg_stat_activity WHERE usename = 'stoneql_writer' AND state LIKE 'idle in transaction%'")
        .await?;
    ensure(
        sessions <= baseline && in_transaction == 0,
        "server-timeout jobs leave no sessions behind",
    )?;
    let value = observer
        .scalar("SELECT v FROM exec_lock WHERE id = 1".into())
        .await?;
    ensure(value == initial, "no timed-out or cancelled write applied")?;
    Ok(json!({
        "statementDeadlineMs": deadline.as_millis(),
        "statementTimeoutMs": server.as_millis(),
        "marginMs": ExecutionLimits::POSTGRES_TIMEOUT_MARGIN.as_millis(),
        "autocommit": {
            "n": samples.len(), "outcome": "failed", "sqlState": "57014",
            "cancellation": "none", "deadlineExceeded": true,
            "submitToTerminalMsMin": samples.iter().copied().fold(f64::MAX, f64::min),
            "submitToTerminalMsP50": percentile(&mut samples.clone(), 0.5),
            "submitToTerminalMsMax": samples.iter().copied().fold(0.0, f64::max),
            "executedOnce": format!("{once}/10"), "serverLogTimeoutErrors": errors,
            "rowApplied": false,
        },
        "lease": {
            "submitToTerminalMs": lease_terminal, "leaseState": "rollback_only",
            "serverStateAbortedBlock": aborted == 1, "rollbackConfirmed": true,
            "leaseWorkVisibleAfter": false,
        },
        "cancelBeforeServerTimeout": {"cancelToTerminalMs": cancel_ms, "cancellation": "confirmed"},
        "clientDeadlineBackstop": {"submitToTerminalMs": backstop_ms, "outcome": "unknown", "deadlineExceeded": true},
        "externalCancelAfterServerTimeoutNotDeadline": {"submitToTerminalMs": external_ms, "outcome": "failed", "sqlState": "57014", "deadlineExceeded": false},
        "writerSessionsAfter": sessions, "writerSessionsBaseline": baseline,
        "writerSessionsInTransactionAfter": in_transaction,
    }))
}

/// Item 6 (MySQL/MariaDB): a COMMIT the server refuses with a database error (here ER 1290,
/// `read_only` switched on by the fixture owner while the lease is open; the writer has no
/// SUPER/CONNECTION_ADMIN) is `unknown`, never `rolled_back` by assumption, and no ROLLBACK
/// follows. The data state afterwards is reported as observed.
async fn mysql_commit_refused(
    run: &Run,
    observer: &mut Observer,
) -> Result<serde_json::Value, Failure> {
    let (connections, owner) = (&run.connections, run.owner);
    let lease = connections.begin_transaction(owner, run.handle).await?;
    let (_, update) = run
        .run(
            "INSERT INTO exec_item(id, parent) VALUES (7201, 1)",
            Some(lease),
        )
        .await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Succeeded),
        "lease insert before refused commit",
    )?;
    observer.execute("SET GLOBAL read_only = ON".into()).await?;
    let end = connections.end_transaction(owner, lease, true).await;
    // Restored before any assertion can stop the run.
    observer
        .execute("SET GLOBAL read_only = OFF".into())
        .await?;
    let end = end?;
    let again = connections.end_transaction(owner, lease, false).await?;
    let rows = observer
        .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 7201")
        .await?;
    ensure(
        end.outcome == Outcome::Unknown
            && end.reason == EndReason::Requested
            && end.commit_sent
            && !end.rollback_sent
            && again == end,
        "refused COMMIT is unknown and nothing else is sent",
    )?;
    Ok(json!({
        "serverError": "ER 1290 (read_only)", "outcome": "unknown", "commitSent": true,
        "rollbackSent": false, "repeatEndSameOutcome": true, "rowsObservedAfter": rows,
        "note": "rowsObservedAfter is the observed data state; StoneQL does not claim it",
    }))
}

async fn pg_count(connection: &mut PgConnection, id: i32) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM exec_item WHERE id = $1")
        .bind(id)
        .fetch_one(connection)
        .await
}

/// Item 1 and R-10 (PostgreSQL): the runtime's session-state probe and checked COMMIT against
/// a real server, on raw writer connections outside StoneQL.
async fn postgres_state_and_commit(
    writer: &PgConnectOptions,
) -> Result<serde_json::Value, Failure> {
    let raw = |sql: &'static str| sqlx::raw_sql(sqlx::AssertSqlSafe(sql).into_sql_str());
    let sql_state = |error: &sqlx::Error| {
        error
            .as_database_error()
            .and_then(|error| error.code())
            .map(|code| code.into_owned())
    };

    let mut c = PgConnection::connect_with(writer).await?;
    // Outside a block: matches autocommit only.
    let fresh = postgres_state_matches(&mut c, false, false).await
        && !postgres_state_matches(&mut c, true, false).await
        && !postgres_state_matches(&mut c, true, true).await;
    // Inside a healthy block: matches a lease only; the probe leaves the block as it was.
    raw("BEGIN").execute(&mut c).await?;
    raw("INSERT INTO exec_item(id, parent) VALUES (7301, 1)")
        .execute(&mut c)
        .await?;
    let healthy = postgres_state_matches(&mut c, true, false).await
        && !postgres_state_matches(&mut c, false, false).await
        && postgres_state_matches(&mut c, true, false).await;
    postgres_commit(&mut c).await?;
    let committed = pg_count(&mut c, 7301).await? == 1;
    // Aborted block: a mismatch after a succeeded job, accepted after a failed one.
    raw("BEGIN").execute(&mut c).await?;
    raw("INSERT INTO exec_item(id, parent) VALUES (7302, 1)")
        .execute(&mut c)
        .await?;
    let _ = raw("SELECT 1 / 0").execute(&mut c).await;
    let aborted = !postgres_state_matches(&mut c, true, false).await
        && postgres_state_matches(&mut c, true, true).await
        && !postgres_state_matches(&mut c, false, true).await;
    // R-10: StoneQL's COMMIT is refused on the aborted block (25P02) and leaves it aborted.
    let checked = postgres_commit(&mut c).await;
    let checked_state = checked.as_ref().err().and_then(sql_state);
    let still_aborted = !postgres_state_matches(&mut c, true, false).await
        && postgres_state_matches(&mut c, true, true).await;
    raw("ROLLBACK").execute(&mut c).await?;
    // Positive control: a plain COMMIT on an aborted block is answered OK (tag ROLLBACK).
    raw("BEGIN").execute(&mut c).await?;
    raw("INSERT INTO exec_item(id, parent) VALUES (7303, 1)")
        .execute(&mut c)
        .await?;
    let _ = raw("SELECT 1 / 0").execute(&mut c).await;
    let plain_ok = raw("COMMIT").execute(&mut c).await.is_ok();
    // Outside a block the checked COMMIT is refused (25P01) and runs nothing.
    let outside = postgres_commit(&mut c).await;
    let outside_state = outside.as_ref().err().and_then(sql_state);
    let back_to_autocommit = postgres_state_matches(&mut c, false, false).await;
    let rows = (pg_count(&mut c, 7302).await?, pg_count(&mut c, 7303).await?);
    c.close().await?;
    ensure(
        fresh && healthy && committed && aborted,
        "PostgreSQL session-state probe",
    )?;
    ensure(
        checked_state.as_deref() == Some("25P02")
            && still_aborted
            && plain_ok
            && outside_state.as_deref() == Some("25P01")
            && back_to_autocommit
            && rows == (0, 0),
        "PostgreSQL checked COMMIT (R-10)",
    )?;
    Ok(json!({
        "probe": {"autocommitMatches": fresh, "healthyBlockMatchesLeaseOnly": healthy,
                  "abortedBlockMismatchAfterSuccess": aborted, "healthyCommitApplied": committed},
        "r10": {"plainCommitOnAbortedBlockAnsweredOk": plain_ok, "checkedCommitOnAbortedBlock": checked_state,
                "blockStillAbortedAfterRefusal": still_aborted, "checkedCommitOutsideBlock": outside_state,
                "rowsAppliedFromAbortedBlocks": rows.0 + rows.1},
    }))
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let started = Instant::now();
    harness().await.map_err(|error| {
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!(
            "step `{}` failed {:.1} s into the harness (unix {:.3}): {error} ({error:?})",
            STEP.lock().map(|step| *step).unwrap_or("unknown"),
            started.elapsed().as_secs_f64(),
            at.as_secs_f64(),
        )
        .into()
    })
}

async fn harness() -> Result<(), Failure> {
    let engine = match env::var("STONEQL_LIFECYCLE_ENGINE")?.as_str() {
        "mysql" => Engine::MySql,
        "mariadb" => Engine::MariaDb,
        "postgresql" => Engine::PostgreSql,
        _ => return Err("fixture engine required".into()),
    };
    let port: u16 = env::var("STONEQL_LIFECYCLE_PORT")?.parse()?;
    let password = env::var("STONEQL_FIXTURE_PASSWORD")?;
    let marker = env::var("STONEQL_SPIKE_FIXTURE_ID")?;
    let ca = std::fs::read(env::var("STONEQL_LIFECYCLE_CA")?)?;
    let database = "stoneql_m001_fixture";
    // Observer: the fixture's administrative account over verified TLS; ownership checked.
    let pg = |username: &str| {
        PgConnectOptions::new_without_pgpass()
            .host("127.0.0.1")
            .port(port)
            .username(username)
            .password(&password)
            .database(database)
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert_from_pem(ca.clone())
    };
    let admin = async || -> Result<Observer, Failure> {
        Ok(match engine {
            Engine::PostgreSql => {
                let mut connection = PgConnection::connect_with(&pg("stoneql_fixture")).await?;
                // The observer's own queries stay out of the statement log it reads.
                sqlx::raw_sql("SET log_statement = 'none'")
                    .execute(&mut connection)
                    .await?;
                Observer::Postgres(connection)
            }
            _ => Observer::MySql(
                MySqlConnection::connect_with(&admin_options(port, &password, &ca)).await?,
            ),
        })
    };
    let mut observer = admin().await?;
    let owned: String = match &mut observer {
        Observer::MySql(connection) => {
            sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
                .fetch_one(connection)
                .await?
        }
        Observer::Postgres(connection) => {
            sqlx::query_scalar("SELECT id FROM stoneql_fixture_marker")
                .fetch_one(connection)
                .await?
        }
    };
    ensure(owned == marker, "fixture ownership mismatch")?;
    let config = || DirectServer {
        engine,
        host: "127.0.0.1".into(),
        port,
        username: "stoneql_writer".into(),
        password: password.clone(),
        database: database.into(),
        root_certificate_pem: Some(ca.clone()),
        tls: true,
    };
    let connections = Connections::new(4, Duration::from_secs(3), Duration::from_secs(3600))?;
    let owner = OwnerId::new();
    let handle = connections
        .connect_with_access(owner, config(), None, None, Access::ReadWrite)
        .await?;
    let run = Run {
        engine,
        connections: connections.clone(),
        owner,
        handle,
    };
    let mysql = run.mysql();
    let limits = QueryLimits {
        rows: 10,
        bytes: 65_536,
        deadline: Duration::from_secs(3),
    };
    let mut report = serde_json::Map::new();

    step("session state and COMMIT");
    // Session-state and COMMIT checks first, so a regression fails the run early.
    if mysql {
        report.insert(
            "commitRefused".into(),
            mysql_commit_refused(&run, &mut observer).await?,
        );
    } else {
        report.insert(
            "postgresSessionState".into(),
            postgres_state_and_commit(&pg("stoneql_writer")).await?,
        );
    }

    step("renderer CRUD statements");
    // M006 live CRUD: the exact statements apps/frontend/src/services/liveMutations.ts generates
    // (pinned here) run as read-write jobs; values round-trip exactly; a stale row is 0 rows.
    let crud: [&str; 4] = if mysql {
        [
            r#"INSERT INTO `exec_text` (`id`, `label`, `amount`) VALUES (_utf8mb4 X'37', _utf8mb4 X'4f27427269656e205c20c3a9', _utf8mb4 X'31323334353637382e3930');"#,
            r#"UPDATE `exec_text` SET `label` = NULL WHERE `id` = _utf8mb4 X'37' LIMIT 1;"#,
            r#"UPDATE `exec_text` SET `label` = _utf8mb4 X'78' WHERE `id` = _utf8mb4 X'38' LIMIT 1;"#,
            r#"DELETE FROM `exec_text` WHERE `id` = _utf8mb4 X'37' LIMIT 1;"#,
        ]
    } else {
        [
            r#"INSERT INTO "exec_text" ("id", "label", "amount") VALUES (E'7', E'O''Brien \\ é', E'12345678.90');"#,
            r#"UPDATE "exec_text" SET "label" = NULL WHERE "id" = E'7';"#,
            r#"UPDATE "exec_text" SET "label" = E'x' WHERE "id" = E'8';"#,
            r#"DELETE FROM "exec_text" WHERE "id" = E'7';"#,
        ]
    };
    let affected = |update: &JobUpdate| update.statements.first().map(|s| s.affected_rows.clone());
    let mut crud_report = Vec::new();
    for (index, (sql, rows)) in crud.iter().zip(["1", "1", "0", "1"]).enumerate() {
        let (_, update) = run.run(sql, None).await?;
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Succeeded),
            "CRUD statement succeeded",
        )?;
        ensure(
            affected(&update).as_deref() == Some(rows),
            "CRUD affected rows",
        )?;
        if index == 0 {
            // O'Brien \ é as UTF-8 bytes, and the exact decimal.
            let exact = if mysql {
                "SELECT COUNT(*) FROM exec_text WHERE id = 7 AND HEX(label) = '4F27427269656E205C20C3A9' AND amount = 12345678.90"
            } else {
                "SELECT COUNT(*) FROM exec_text WHERE id = 7 AND encode(convert_to(label, 'UTF8'), 'hex') = '4f27427269656e205c20c3a9' AND amount = 12345678.90"
            };
            ensure(
                observer.scalar(exact.into()).await? == 1,
                "CRUD insert stored exact values",
            )?;
        }
        if index == 1 {
            ensure(
                observer
                    .scalar("SELECT COUNT(*) FROM exec_text WHERE id = 7 AND label IS NULL".into())
                    .await?
                    == 1,
                "CRUD update stored NULL",
            )?;
        }
        crud_report.push(rows);
    }
    ensure(
        observer
            .scalar("SELECT COUNT(*) FROM exec_text".into())
            .await?
            == 0,
        "CRUD delete removed the row",
    )?;
    report.insert("rendererCrud".into(), json!({"statements": crud.len(), "affected": crud_report, "exactText": true, "staleRowAffected": "0"}));

    step("renderer CSV import batches");
    // M007 live CSV import: the exact batches apps/frontend/src/services/liveImport.ts builds
    // (pinned) run inside one lease; a committed batch keeps NULL / '' / 'NULL' distinct, and a
    // rejected batch rolls the whole import back.
    let [import_ok, import_dup]: [&str; 2] = if mysql {
        [
            r#"INSERT INTO `exec_text` (`id`, `label`, `amount`) VALUES
(_utf8mb4 X'3231', NULL, _utf8mb4 X'312e3530'),
(_utf8mb4 X'3232', _utf8mb4 X'', _utf8mb4 X'2d302e3031'),
(_utf8mb4 X'3233', _utf8mb4 X'4e554c4c', NULL);"#,
            r#"INSERT INTO `exec_text` (`id`, `label`, `amount`) VALUES
(_utf8mb4 X'3234', _utf8mb4 X'78', _utf8mb4 X'31'),
(_utf8mb4 X'3234', _utf8mb4 X'79', _utf8mb4 X'32');"#,
        ]
    } else {
        [
            r#"INSERT INTO "exec_text" ("id", "label", "amount") VALUES
(E'21', NULL, E'1.50'),
(E'22', E'', E'-0.01'),
(E'23', E'NULL', NULL);"#,
            r#"INSERT INTO "exec_text" ("id", "label", "amount") VALUES
(E'24', E'x', E'1'),
(E'24', E'y', E'2');"#,
        ]
    };
    let lease = connections.begin_transaction(owner, handle).await?;
    let (_, update) = run.run(import_ok, Some(lease)).await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Succeeded),
        "import batch succeeded",
    )?;
    ensure(
        update.statements.first().map(|s| s.affected_rows.as_str()) == Some("3"),
        "import batch inserted three rows",
    )?;
    let end = connections.end_transaction(owner, lease, true).await?;
    ensure(end.outcome == Outcome::Committed, "import committed")?;
    ensure(
        observer
            .scalar("SELECT COUNT(*) FROM exec_text WHERE (id = 21 AND label IS NULL AND amount = 1.50) OR (id = 22 AND label = '' AND amount = -0.01) OR (id = 23 AND label = 'NULL' AND amount IS NULL)".into())
            .await?
            == 3,
        "import kept NULL, empty and 'NULL' distinct",
    )?;
    let lease = connections.begin_transaction(owner, handle).await?;
    let (_, update) = run.run(import_dup, Some(lease)).await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Failed),
        "duplicate batch rejected",
    )?;
    let end = connections.end_transaction(owner, lease, false).await?;
    ensure(
        end.outcome == Outcome::RolledBack,
        "rejected import rolled back",
    )?;
    ensure(
        observer
            .scalar("SELECT COUNT(*) FROM exec_text WHERE id = 24".into())
            .await?
            == 0,
        "nothing of the rejected import applied",
    )?;
    report.insert(
        "rendererCsvImport".into(),
        json!({"committedRows": 3, "nullEmptyTextDistinct": true, "rejectedBatchRolledBack": true}),
    );

    if mysql {
        step("sql import");
        sql_import(&connections, owner, handle, &mut observer, &mut report).await?;
    }

    step("expiry lease start");
    // Lease expiry runs in the background on a second handle for the real LEASE_IDLE.
    let expiry_handle = connections
        .connect_with_access(owner, config(), None, None, Access::ReadWrite)
        .await?;
    let expiry_lease = connections.begin_transaction(owner, expiry_handle).await?;
    let expiry_started = Instant::now();
    let mut options = JobOptions::new("INSERT INTO exec_item(id, parent) VALUES (900, 1)");
    options.lease = Some(expiry_lease);
    let (id, _) = connections.submit_job(owner, expiry_handle, options)?;
    let (_, update) = run.drain(id).await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Succeeded),
        "expiry lease insert",
    )?;

    step("read-only handles");
    // Read-only handles and the M003 path are unchanged.
    let read_only = connections.connect(owner, config()).await?;
    ensure(
        connections.submit_job(owner, read_only, JobOptions::new("SELECT 1"))
            == Err(ExecutionError::ReadOnlyHandle),
        "read-only handle refuses jobs",
    )?;
    connections
        .read_only(owner, read_only, "SELECT 1", limits)
        .await?;
    connections
        .read_only(owner, handle, "SELECT 1", limits)
        .await?;

    step("owner isolation");
    // Owner isolation.
    let other = OwnerId::new();
    let (id, _) = connections.submit_job(owner, handle, JobOptions::new(run.slow()))?;
    ensure(
        connections.fetch_job(other, id, Duration::ZERO).await == Err(ExecutionError::Forbidden)
            && connections.cancel_job(other, id) == Err(ExecutionError::Forbidden)
            && connections.submit_job(other, handle, JobOptions::new("SELECT 1"))
                == Err(ExecutionError::Forbidden),
        "owner isolation",
    )?;
    // Busy, never queued: neither another job nor the read-only path runs beside it.
    ensure(
        connections.submit_job(owner, handle, JobOptions::new("SELECT 1"))
            == Err(ExecutionError::Busy)
            && connections
                .read_only(owner, handle, "SELECT 1", limits)
                .await
                == Err(ReadError::Busy)
            && connections.begin_transaction(owner, handle).await == Err(ExecutionError::Busy),
        "busy handle refuses work",
    )?;
    connections.cancel_job(owner, id)?;
    run.drain(id).await?;
    report.insert("ownerIsolation".into(), json!(true));
    report.insert("busyNotQueued".into(), json!(true));

    step("exact values");
    // Exact values through the read-write path.
    let exact = if mysql {
        "SELECT s, u, d, dt, zd, tm, n FROM raw_values WHERE id = 1"
    } else {
        r#"SELECT i8, n, ts, tz, dt, d FROM "Mixed ""Case"" values" ORDER BY id"#
    };
    let (batches, _) = run.run(exact, None).await?;
    let rows = &batches[0].rows;
    let expected: Vec<Vec<Option<&str>>> = if mysql {
        vec![vec![
            Some("-9223372036854775808"),
            Some("18446744073709551615"),
            Some("123456789012345678901234567890.4500"),
            Some("2024-02-29 12:34:56.123456"),
            Some("0000-00-00"),
            Some("-26:03:04.123456"),
            None,
        ]]
    } else {
        vec![
            vec![
                Some("-9223372036854775808"),
                Some("123456789012345678901234567890.4500"),
                Some("2024-02-29 12:34:56.123456"),
                Some("2024-02-29 07:04:56.123456+00"),
                Some("0044-03-15 BC"),
                Some("fallback"),
            ],
            vec![
                Some("9223372036854775807"),
                Some("-0.0001"),
                Some("infinity"),
                Some("-infinity"),
                Some("infinity"),
                None,
            ],
        ]
    };
    let actual: Vec<Vec<Option<&str>>> = rows
        .iter()
        .map(|row| row.iter().map(text).collect())
        .collect();
    ensure(actual == expected, "exact values")?;
    ensure(
        rows.iter().flatten().any(|cell| *cell == CellValue::Null),
        "NULL distinct",
    )?;
    report.insert("exactValues".into(), json!(true));

    step("multiple result sets");
    // Multiple result sets: a script, and a routine.
    let (batches, update) = run
        .run("SELECT 1 AS a; SELECT 'two' AS b, 2 AS c", None)
        .await?;
    ensure(
        batches.iter().filter(|batch| batch.last).count() == 2
            && update
                .statements
                .iter()
                .all(|report| report.result_sets == 1),
        "script result sets",
    )?;
    // MySQL/MariaDB CALL is refused in read-write jobs (R-3); PostgreSQL procedures still run.
    if mysql {
        ensure(
            connections.submit_job(owner, handle, JobOptions::new("CALL exec_multi()"))
                == Err(ExecutionError::Script(0, ScriptError::Call)),
            "MySQL CALL refused",
        )?;
        report.insert("procedureResultSets".into(), json!("refused (CALL)"));
    } else {
        let (batches, update) = run.run("CALL exec_proc(NULL)", None).await?;
        let sets = batches.iter().filter(|batch| batch.last).count();
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Succeeded) && sets == 1,
            "procedure results",
        )?;
        report.insert("procedureResultSets".into(), json!(sets));
    }
    step("text protocol");
    // MySQL statements that cannot be prepared (USE) use the text protocol.
    if mysql {
        let (batches, update) = run
            .run(
                "USE information_schema; SELECT DATABASE(), 18446744073709551615",
                None,
            )
            .await?;
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Succeeded)
                && text(&batches[0].rows[0][0]) == Some("information_schema"),
            "text protocol fallback",
        )?;
    }

    step("DDL partial outcomes");
    // Partial outcomes with DDL; authoritative unique and foreign-key failures.
    let (_, update) = run
        .run(
            "CREATE TABLE exec_ddl(id INT PRIMARY KEY); INSERT INTO exec_ddl VALUES (1); INSERT INTO exec_ddl VALUES (1); INSERT INTO exec_ddl VALUES (2)",
            None,
        )
        .await?;
    ensure(
        statuses(&update)
            == [
                StatementStatusDto::Succeeded,
                StatementStatusDto::Succeeded,
                StatementStatusDto::Failed,
                StatementStatusDto::NotRun,
            ]
            && update.statements[2]
                .failure
                .as_ref()
                .and_then(|f| f.constraint)
                == Some(ConstraintKindDto::Unique)
            && update.outcome == Some(ExecutionOutcomeDto::Failed),
        "DDL partial outcome",
    )?;
    ensure(
        observer.scalar_i64("SELECT COUNT(*) FROM exec_ddl").await? == 1,
        "DDL partial durable rows",
    )?;
    let (_, update) = run
        .run("INSERT INTO exec_item(id, parent) VALUES (1, 404)", None)
        .await?;
    let failure = update.statements[0].failure.clone().ok_or("FK failure")?;
    ensure(
        failure.constraint == Some(ConstraintKindDto::ForeignKey)
            && failure.sql_state.as_deref() == Some(if mysql { "23000" } else { "23503" }),
        "FK failure",
    )?;
    report.insert("ddlPartialOutcome".into(), json!(true));
    report.insert(
        "constraintFailures".into(),
        json!(["unique", "foreign_key"]),
    );

    step("transactions");
    // Transactions.
    let lease = connections.begin_transaction(owner, handle).await?;
    ensure(
        connections.submit_job(owner, handle, JobOptions::new("SELECT 1"))
            == Err(ExecutionError::Busy)
            && connections
                .read_only(owner, handle, "SELECT 1", limits)
                .await
                == Err(ReadError::Busy),
        "lease makes handle exclusive",
    )?;
    if mysql {
        let mut options = JobOptions::new("CREATE TABLE exec_never(id INT)");
        options.lease = Some(lease);
        ensure(
            connections.submit_job(owner, handle, options)
                == Err(ExecutionError::Script(
                    0,
                    ScriptError::NotAllowedInTransaction,
                )),
            "MySQL DDL refused in lease",
        )?;
    }
    let (_, update) = run
        .run(
            if mysql {
                "INSERT INTO exec_item(id, parent) VALUES (10, 1)"
            } else {
                "INSERT INTO exec_item(id, parent) VALUES (10, 1); CREATE TABLE exec_tx_ddl(id int)"
            },
            Some(lease),
        )
        .await?;
    ensure(
        update.outcome == Some(ExecutionOutcomeDto::Succeeded),
        "lease insert",
    )?;
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 10")
            .await?
            == 0,
        "uncommitted invisible",
    )?;
    let end = connections.end_transaction(owner, lease, true).await?;
    ensure(
        end.outcome == Outcome::Committed && end.commit_sent,
        "commit confirmed",
    )?;
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 10")
            .await?
            == 1,
        "committed visible",
    )?;
    let lease = connections.begin_transaction(owner, handle).await?;
    let (_, update) = run
        .run(
            if mysql {
                "INSERT INTO exec_item(id, parent) VALUES (11, 1); INSERT INTO exec_item(id, parent) VALUES (10, 1)"
            } else {
                "CREATE TABLE exec_rolled(id int); INSERT INTO exec_item(id, parent) VALUES (11, 1); INSERT INTO exec_item(id, parent) VALUES (10, 1)"
            },
            Some(lease),
        )
        .await?;
    ensure(
        update.lease == Some(LeaseStateDto::RollbackOnly)
            && connections.end_transaction(owner, lease, true).await
                == Err(ExecutionError::RollbackOnly),
        "rollback-only after failure",
    )?;
    let end = connections.end_transaction(owner, lease, false).await?;
    ensure(
        end.outcome == Outcome::RolledBack && !end.commit_sent,
        "rollback confirmed",
    )?;
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 11")
            .await?
            == 0,
        "rolled back invisible",
    )?;
    if !mysql {
        ensure(
            observer
                .scalar_i64("SELECT COUNT(*) FROM pg_class WHERE relname = 'exec_rolled'")
                .await?
                == 0,
            "transactional DDL rolled back",
        )?;
    }
    report.insert(
        "transactions".into(),
        json!(["commit", "rollback", "rollback_only"]),
    );

    step("connection loss in a lease");
    // Connection loss mid-transaction: the server session is killed by the observer.
    let lease = connections.begin_transaction(owner, handle).await?;
    run.run(
        "INSERT INTO exec_item(id, parent) VALUES (12, 1)",
        Some(lease),
    )
    .await?;
    if mysql {
        // The newest open InnoDB transaction is this lease (the expiry lease started first).
        let session = observer
            .scalar_i64("SELECT CAST(trx_mysql_thread_id AS SIGNED) FROM information_schema.INNODB_TRX ORDER BY trx_started DESC LIMIT 1")
            .await?;
        observer.execute(format!("KILL {session}")).await?;
    } else {
        observer
            // The newest open transaction is this lease (the expiry lease started first); its
            // last query is the session check, not the INSERT.
            .execute("SELECT pg_terminate_backend(pid) FROM (SELECT pid FROM pg_stat_activity WHERE usename = 'stoneql_writer' AND state = 'idle in transaction' ORDER BY xact_start DESC LIMIT 1) AS newest".into())
            .await?;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_, update) = run.run("SELECT 1", Some(lease)).await?;
    // PostgreSQL loses the session at the job's statement_timeout setup (L-3), before the
    // user statement is sent; MySQL/MariaDB at the statement itself.
    ensure(
        if mysql {
            statuses(&update) == [StatementStatusDto::Unknown]
        } else {
            statuses(&update) == [StatementStatusDto::NotRun]
                && update.outcome == Some(ExecutionOutcomeDto::Failed)
        } && update.lease == Some(LeaseStateDto::Ended),
        "lost lease connection",
    )?;
    let end = connections.end_transaction(owner, lease, true).await?;
    ensure(
        end.outcome == Outcome::Unknown
            && end.reason == EndReason::ConnectionLost
            && !end.commit_sent,
        "connection loss is unknown",
    )?;
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 12")
            .await?
            == 0,
        "lost transaction not applied",
    )?;
    report.insert("connectionLossUnknown".into(), json!(true));

    step("cancel latency");
    // Cancellation latency: cancel request to terminal state, 100 samples.
    let mut samples = Vec::new();
    let mut signals = 0;
    for _ in 0..100 {
        let (id, count) = connections.submit_job(
            owner,
            handle,
            JobOptions::new(format!(
                "{}; INSERT INTO exec_item(id, parent) VALUES (13, 1)",
                run.slow()
            )),
        )?;
        // Cancel once the slow statement itself is running (not its session setup), then
        // after a further 150 ms.
        for _ in 0..200 {
            let update = connections.fetch_job(owner, id, Duration::ZERO).await?;
            if update.statements[count - 2].status == StatementStatusDto::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        let requested = Instant::now();
        connections.cancel_job(owner, id)?;
        let mut terminal = None;
        let mut last = None;
        while last.as_ref().is_none_or(|update: &JobUpdate| !update.done) {
            let update = connections
                .fetch_job(owner, id, Duration::from_millis(20))
                .await?;
            if terminal.is_none() && update.outcome.is_some() {
                terminal = Some(requested.elapsed());
            }
            last = Some(update);
        }
        let update = last.ok_or("cancel update")?;
        run.settle(id, &update)?;
        samples.push(terminal.ok_or("terminal")?.as_secs_f64() * 1000.0);
        signals += usize::from(update.server_cancel_sent);
        let slow = &update.statements[count - 2];
        if update.outcome != Some(ExecutionOutcomeDto::Unknown)
            || update.cancellation != CancellationDto::Confirmed
        {
            // Statuses and flags only (no SQL text or values), for diagnosing a failed run.
            eprintln!(
                "cancel sample: outcome {:?}, cancellation {:?}, statuses {:?}, signal {}",
                update.outcome,
                update.cancellation,
                statuses(&update),
                update.server_cancel_sent
            );
        }
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Unknown)
                && update.cancellation == CancellationDto::Confirmed
                && slow.status == StatementStatusDto::Unknown
                && update.statements[count - 1].status == StatementStatusDto::NotRun,
            "cancel outcome",
        )?;
    }
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 13")
            .await?
            == 0,
        "cancelled job sent nothing after",
    )?;
    let mut deadline = JobOptions::new(run.slow());
    deadline.statement_deadline = Duration::from_secs(1);
    let (id, _) = connections.submit_job(owner, handle, deadline)?;
    let (_, update) = run.drain(id).await?;
    // MySQL/MariaDB: the client deadline interrupts. PostgreSQL: the server statement_timeout
    // (L-3) ends the statement first, so it fails with 57014 and nothing is cancelled.
    ensure(
        update.deadline_exceeded
            && if mysql {
                update.outcome == Some(ExecutionOutcomeDto::Unknown)
                    && update.cancellation == CancellationDto::Confirmed
            } else {
                update.outcome == Some(ExecutionOutcomeDto::Failed)
                    && update.cancellation == CancellationDto::None
                    && update.statements[0]
                        .failure
                        .as_ref()
                        .and_then(|failure| failure.sql_state.as_deref())
                        == Some("57014")
            },
        "deadline",
    )?;
    let max = samples.iter().copied().fold(0.0, f64::max);
    report.insert(
        "cancel".into(),
        json!({
            "samples": samples.len(),
            "serverSignalsSent": signals,
            "confirmed": samples.len(),
            "p50Ms": percentile(&mut samples.clone(), 0.5),
            "p95Ms": percentile(&mut samples.clone(), 0.95),
            "maxMs": max,
            "graceMs": ExecutionLimits::CANCEL_GRACE.as_millis(),
            "deadlinePath": if mysql { "client deadline interrupt (confirmed)" } else { "server statement_timeout (failed, 57014)" },
        }),
    );
    ensure(
        percentile(&mut samples, 0.95) <= 2000.0,
        "cancel p95 within proposed 2 s",
    )?;

    step("fault cycles");
    // 100 fault/transaction cycles; the next job must never see the previous session state.
    // Baseline writer sessions: the expiry lease plus idle, never-job-used pool connections.
    let writer_sessions = if mysql {
        "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE USER = 'stoneql_writer'"
    } else {
        "SELECT COUNT(*) FROM pg_stat_activity WHERE usename = 'stoneql_writer'"
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    let baseline = observer.scalar_i64(writer_sessions).await?;
    let mut committed = 0;
    for cycle in 0..100i64 {
        let lease = connections.begin_transaction(owner, handle).await?;
        let state = if mysql {
            format!(
                "SET @leak = {cycle}; CREATE TEMPORARY TABLE leak_t(x INT); USE information_schema; INSERT INTO stoneql_m001_fixture.exec_cycle VALUES ({cycle})"
            )
        } else {
            format!(
                "SET stoneql.leak = '{cycle}'; CREATE TEMP TABLE leak_t(x int); SET search_path = pg_catalog; INSERT INTO public.exec_cycle VALUES ({cycle})"
            )
        };
        let fault = cycle % 4;
        let script = match fault {
            2 => format!("{state}; SELECT * FROM missing_table_for_fault"),
            _ => state,
        };
        if fault == 3 {
            let mut options = JobOptions::new(format!(
                "{script}; SELECT {}",
                if mysql { "SLEEP(30)" } else { "pg_sleep(30)" }
            ));
            options.lease = Some(lease);
            let (id, _) = connections.submit_job(owner, handle, options)?;
            tokio::time::sleep(Duration::from_millis(150)).await;
            connections.cancel_job(owner, id)?;
            let (_, update) = run.drain(id).await?;
            // B-11: MySQL's killed SLEEP() returns 1 (success) and its row is read after the
            // interrupt, so it is withheld and flagged; PostgreSQL raises 57014 (unknown), and
            // MariaDB may do either.
            let sleep = update.statements.last().ok_or("sleep statement")?;
            let withheld_success =
                sleep.status == StatementStatusDto::Succeeded && sleep.results_withheld;
            ensure(
                match engine {
                    Engine::MySql => withheld_success,
                    Engine::MariaDb => {
                        withheld_success || sleep.status == StatementStatusDto::Unknown
                    }
                    _ => sleep.status == StatementStatusDto::Unknown,
                },
                "interrupted sleep status and withheld flag",
            )?;
        } else {
            run.run(&script, Some(lease)).await?;
        }
        let end = connections
            .end_transaction(owner, lease, fault == 0)
            .await?;
        let expected = if fault == 0 {
            Outcome::Committed
        } else {
            Outcome::RolledBack
        };
        ensure(end.outcome == expected, "cycle outcome confirmed")?;
        committed += i64::from(fault == 0);
        let probe = if mysql {
            "SELECT @leak IS NULL, DATABASE()"
        } else {
            "SELECT current_setting('stoneql.leak', true) IS NULL OR current_setting('stoneql.leak', true) = '', current_setting('search_path')"
        };
        let (batches, _) = run.run(probe, None).await?;
        let row = &batches[0].rows[0];
        ensure(
            matches!(&row[0], CellValue::Integer { text } if text == "1")
                || row[0] == CellValue::Boolean { value: true },
            "session variable leaked",
        )?;
        ensure(
            text(&row[1]) == Some(if mysql { database } else { "\"$user\", public" }),
            "schema/search_path leaked",
        )?;
        let (_, update) = run.run("SELECT * FROM leak_t", None).await?;
        ensure(
            update.outcome == Some(ExecutionOutcomeDto::Failed),
            "temporary table leaked",
        )?;
    }
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_cycle")
            .await?
            == committed,
        "cycle commits",
    )?;
    connections
        .read_only(owner, handle, "SELECT 1", limits)
        .await?;
    // Every job/lease/cancel connection was discarded: the session count never exceeds the
    // baseline (the background expiry lease may end meanwhile).
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sessions = observer.scalar_i64(writer_sessions).await?;
    ensure(sessions <= baseline, "dirty or leaked sessions remain")?;
    report.insert(
        "faultCycles".into(),
        json!({"cycles": 100, "committed": committed, "dirtyConnectionsReused": 0, "sessionStateLeaks": 0, "writerSessionsBefore": baseline, "writerSessionsAfter": sessions}),
    );

    step("backpressure");
    // Backpressure: a large result is paged and the statement waits for the consumer.
    let mut options = JobOptions::new(if engine == Engine::MariaDb {
        "SET SESSION max_recursive_iterations = 200000; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 100000) SELECT x, REPEAT('p', 64) FROM c"
    } else if mysql {
        "SET SESSION cte_max_recursion_depth = 200000; WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 100000) SELECT x, REPEAT('p', 64) FROM c"
    } else {
        "SELECT x, repeat('p', 64) FROM generate_series(1, 100000) AS x"
    });
    options.max_rows_per_result = ExecutionLimits::MAX_ROWS_PER_RESULT;
    let (id, count) = connections.submit_job(owner, handle, options)?;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let parked = connections.fetch_job(owner, id, Duration::ZERO).await?;
    ensure(
        parked.statements[count - 1].status == StatementStatusDto::Running,
        "statement waits for consumer",
    )?;
    let (mut batches, _) = run.drain(id).await?;
    batches.extend(parked.batch);
    let rows: usize = batches.iter().map(|batch| batch.rows.len()).sum();
    ensure(
        batches
            .iter()
            .all(|batch| batch.rows.len() <= ExecutionLimits::PAGE_ROWS)
            && batches.iter().any(|batch| batch.truncated)
            && rows < 100_000,
        "bounded pages",
    )?;
    report.insert(
        "backpressure".into(),
        json!({"pages": batches.len(), "rowsDelivered": rows, "truncated": true, "parkedWhileUnread": true}),
    );

    step("policy and session state");
    // R-2/R-3 on real servers: wrapper and compound statements refused before send, CALL
    // refused, and the session-state probe checked against real diverged sessions.
    if mysql {
        let writer = MySqlConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username("stoneql_writer")
            .password(&password)
            .database(database)
            .ssl_mode(MySqlSslMode::VerifyIdentity)
            .ssl_ca_from_pem(ca.clone());
        let probes = policy_and_state(&run, &mut observer, writer).await?;
        report.insert("policyAndSessionState".into(), json!(probes));
    }

    step("lease expiry");
    // Real lease expiry after LEASE_IDLE without use. Checked before the fault sections, whose
    // hundreds of lease ends would evict this owner's older ended-lease records (32 per owner).
    let wait = ExecutionLimits::LEASE_IDLE + Duration::from_secs(3);
    if let Some(remaining) = wait.checked_sub(expiry_started.elapsed()) {
        tokio::time::sleep(remaining).await;
    }
    let end = connections
        .end_transaction(owner, expiry_lease, true)
        .await?;
    ensure(
        end.outcome == Outcome::RolledBack && end.reason == EndReason::Expired,
        "lease expiry rolled back",
    )?;
    ensure(
        observer
            .scalar_i64("SELECT COUNT(*) FROM exec_item WHERE id = 900")
            .await?
            == 0,
        "expired lease not applied",
    )?;
    report.insert(
        "leaseExpiry".into(),
        json!({"idleSeconds": ExecutionLimits::LEASE_IDLE.as_secs(), "outcome": "rolled_back"}),
    );

    step("write faults setup");
    // Write cancellation, failed interrupt (and the PostgreSQL backstop), no re-send and lost
    // COMMIT/ROLLBACK replies, on every network engine.
    {
        let relayed = DirectServer {
            port: env::var("STONEQL_RELAY_PORT")?.parse()?,
            ..config()
        };
        let journaled = DirectServer {
            port: env::var("STONEQL_RELAY_PORT")?.parse()?,
            ..config()
        };
        let locker = admin().await?;
        let faults = write_faults(&run, &mut observer, locker, relayed, journaled).await?;
        for (key, value) in faults {
            report.insert(key, value);
        }
    }

    step("shutdown");
    connections.shutdown().await;
    ensure(
        connections.open_transactions() == 0,
        "transactions after shutdown",
    )?;
    report.insert("engine".into(), json!(format!("{engine:?}")));
    println!("{}", serde_json::Value::Object(report));
    Ok(())
}
