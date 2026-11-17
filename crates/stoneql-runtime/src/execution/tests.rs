//! Execution service tests on temporary SQLite files (no Docker). Network engines are covered
//! by the disposable-fixture example `examples/execution.rs`.
use super::*;
use sqlx::{Connection, Executor, SqliteConnection, sqlite::SqliteConnectOptions};
use stoneql_contracts::v1::{ExecutionOutcomeDto, SqlImportStatus};

struct Fixture(PathBuf);
impl Fixture {
    async fn new() -> Self {
        let root = std::env::temp_dir().join(format!("stoneql-exec-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let fixture = Self(std::fs::canonicalize(root).unwrap());
        let mut writer = fixture.observer().await;
        writer
            .execute(
                "CREATE TABLE parent(id INTEGER PRIMARY KEY);
                 CREATE TABLE item(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES parent(id), email TEXT UNIQUE, n INTEGER, amount TEXT, at TEXT, note TEXT);
                 INSERT INTO parent VALUES (1);",
            )
            .await
            .unwrap();
        writer.close().await.unwrap();
        fixture
    }
    fn file(&self) -> PathBuf {
        self.0.join("fixture.sqlite")
    }
    async fn observer(&self) -> SqliteConnection {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(self.file())
                .create_if_missing(true),
        )
        .await
        .unwrap()
    }
    async fn count(&self, sql: &'static str) -> i64 {
        let mut observer = self.observer().await;
        let value = sqlx::query_scalar(sql)
            .fetch_one(&mut observer)
            .await
            .unwrap();
        observer.close().await.unwrap();
        value
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn open(fixture: &Fixture, access: Access) -> (Connections, OwnerId, ConnectionId) {
    let connections =
        Connections::new(4, Duration::from_secs(3), Duration::from_secs(600)).unwrap();
    let owner = OwnerId::new();
    let grant = connections
        .grant_sqlite_file(owner, &fixture.file())
        .unwrap();
    let handle = connections
        .connect_sqlite_with_access(owner, &grant, access)
        .await
        .unwrap();
    (connections, owner, handle)
}
/// Fetches until done, returning every page and the final update.
async fn drain(
    connections: &Connections,
    owner: OwnerId,
    id: JobId,
) -> (Vec<ResultBatch>, JobUpdate) {
    let mut batches = Vec::new();
    for _ in 0..600 {
        let update = connections
            .fetch_job(owner, id, Duration::from_secs(1))
            .await
            .unwrap();
        if let Some(batch) = update.batch.clone() {
            batches.push(batch);
        }
        if update.done {
            return (batches, update);
        }
    }
    panic!("job did not finish");
}
async fn run(
    connections: &Connections,
    owner: OwnerId,
    handle: ConnectionId,
    options: JobOptions,
) -> (Vec<ResultBatch>, JobUpdate) {
    let (id, _) = connections.submit_job(owner, handle, options).unwrap();
    drain(connections, owner, id).await
}
fn statuses(update: &JobUpdate) -> Vec<StatementStatusDto> {
    update
        .statements
        .iter()
        .map(|report| report.status)
        .collect()
}
/// A statement that keeps SQLite's VM busy until interrupted.
const SLOW: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT max(x) FROM c";

#[tokio::test]
async fn read_only_handles_refuse_jobs_and_transactions_and_keep_concurrent_reads() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadOnly).await;
    assert_eq!(connections.access(owner, handle), Ok(Access::ReadOnly));
    assert_eq!(
        connections
            .submit_job(owner, handle, JobOptions::new("SELECT 1"))
            .unwrap_err(),
        ExecutionError::ReadOnlyHandle
    );
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::ReadOnlyHandle
    );
    let limits = crate::QueryLimits {
        rows: 10,
        bytes: 4096,
        deadline: Duration::from_secs(3),
    };
    let (first, second) = tokio::join!(
        connections.read_only(owner, handle, "SELECT 1", limits),
        connections.read_only(owner, handle, "SELECT 2", limits)
    );
    assert!(first.is_ok() && second.is_ok());
}

#[tokio::test]
async fn jobs_transactions_and_pages_are_owner_scoped() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let other = OwnerId::new();
    assert_eq!(
        connections
            .submit_job(other, handle, JobOptions::new("SELECT 1"))
            .unwrap_err(),
        ExecutionError::Forbidden
    );
    assert_eq!(
        connections
            .begin_transaction(other, handle)
            .await
            .unwrap_err(),
        ExecutionError::Forbidden
    );
    let (id, _) = connections
        .submit_job(owner, handle, JobOptions::new(SLOW))
        .unwrap();
    assert_eq!(
        connections
            .fetch_job(other, id, Duration::ZERO)
            .await
            .unwrap_err(),
        ExecutionError::Forbidden
    );
    assert_eq!(
        connections.cancel_job(other, id).unwrap_err(),
        ExecutionError::Forbidden
    );
    assert!(connections.cancel_job(owner, id).unwrap());
    drain(&connections, owner, id).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    assert_eq!(
        connections
            .end_transaction(other, lease, true)
            .await
            .unwrap_err(),
        ExecutionError::Forbidden
    );
    let mut options = JobOptions::new("SELECT 1");
    options.lease = Some(lease);
    assert_eq!(
        connections.submit_job(other, handle, options).unwrap_err(),
        ExecutionError::Forbidden
    );
    assert_eq!(
        connections
            .end_transaction(owner, lease, false)
            .await
            .unwrap()
            .outcome,
        Outcome::RolledBack
    );
    // Forged identifiers are refused, not interpreted.
    assert_eq!(
        JobId::parse("../job").unwrap_err(),
        ExecutionError::Forbidden
    );
}

#[tokio::test]
async fn scripts_report_each_statement_and_stop_at_an_authoritative_constraint_failure() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let (batches, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new(
            "INSERT INTO item(id, parent, email, n, amount, at, note) VALUES (1, 1, 'a@x', 9223372036854775807, '123456789012345678901234567890.4500', '2024-02-29 12:34:56.123456', NULL);
             SELECT id, n, amount, at, note FROM item; SELECT 2 AS second WHERE 0;
             INSERT INTO item(id, email) VALUES (2, 'a@x');
             INSERT INTO item(id) VALUES (3)",
        ),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Failed));
    assert_eq!(
        statuses(&update),
        [
            StatementStatusDto::Succeeded,
            StatementStatusDto::Succeeded,
            StatementStatusDto::Succeeded,
            StatementStatusDto::Failed,
            StatementStatusDto::NotRun
        ]
    );
    assert_eq!(update.statements[0].affected_rows, "1");
    assert_eq!(
        update.statements[3].failure.as_ref().unwrap().constraint,
        Some(ConstraintKindDto::Unique)
    );
    assert_eq!(update.cancellation, CancellationDto::None);
    // Exact values and NULL survive; a zero-row result set keeps its columns.
    let first = batches.iter().find(|batch| batch.statement == 1).unwrap();
    assert_eq!(first.columns.as_ref().unwrap().len(), 5);
    assert_eq!(
        first.rows,
        [vec![
            CellValue::Integer { text: "1".into() },
            CellValue::Integer {
                text: "9223372036854775807".into()
            },
            CellValue::Text {
                text: "123456789012345678901234567890.4500".into()
            },
            CellValue::Text {
                text: "2024-02-29 12:34:56.123456".into()
            },
            CellValue::Null,
        ]]
    );
    let empty = batches.iter().find(|batch| batch.statement == 2).unwrap();
    assert!(
        empty.rows.is_empty() && empty.last && empty.columns.as_ref().unwrap()[0].name == "second"
    );
    // Autocommit: the successful insert is durable; the refused and unsent ones are absent.
    assert_eq!(fixture.count("SELECT count(*) FROM item").await, 1);
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO item(id, parent) VALUES (4, 99)"),
    )
    .await;
    assert_eq!(
        update.statements[0].failure.as_ref().unwrap().constraint,
        Some(ConstraintKindDto::ForeignKey)
    );
}

#[tokio::test]
async fn policy_refusals_send_nothing() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    for (sql, index, error) in [
        (
            "INSERT INTO parent VALUES (2); COMMIT",
            1,
            ScriptError::TransactionControl,
        ),
        ("ATTACH 'other.db' AS other", 0, ScriptError::FileAccess),
        (
            "INSERT INTO parent VALUES (3); SAVEPOINT a",
            1,
            ScriptError::TransactionControl,
        ),
    ] {
        assert_eq!(
            connections
                .submit_job(owner, handle, JobOptions::new(sql))
                .unwrap_err(),
            ExecutionError::Script(index, error)
        );
    }
    assert_eq!(fixture.count("SELECT count(*) FROM parent").await, 1);
    let mut options = JobOptions::new("SELECT 1");
    options.statement_deadline = ExecutionLimits::MAX_STATEMENT_DEADLINE + Duration::from_secs(1);
    assert_eq!(
        connections.submit_job(owner, handle, options).unwrap_err(),
        ExecutionError::InvalidLimits
    );
}

#[tokio::test]
async fn busy_handles_refuse_instead_of_queueing() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let (id, _) = connections
        .submit_job(owner, handle, JobOptions::new(SLOW))
        .unwrap();
    assert_eq!(
        connections
            .submit_job(
                owner,
                handle,
                JobOptions::new("INSERT INTO parent VALUES (5)")
            )
            .unwrap_err(),
        ExecutionError::Busy
    );
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::Busy
    );
    let limits = crate::QueryLimits {
        rows: 10,
        bytes: 4096,
        deadline: Duration::from_secs(3),
    };
    assert_eq!(
        connections
            .read_only(owner, handle, "SELECT 1", limits)
            .await
            .unwrap_err(),
        ReadError::Busy
    );
    connections.cancel_job(owner, id).unwrap();
    drain(&connections, owner, id).await;
    // The refused insert was never sent, and the handle is free again.
    assert_eq!(fixture.count("SELECT count(*) FROM parent").await, 1);
    assert!(
        connections
            .read_only(owner, handle, "SELECT 1", limits)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn cancel_interrupts_a_running_statement_and_reports_unknown_effects() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let mut samples = Vec::new();
    for _ in 0..5 {
        let (id, _) = connections
            .submit_job(
                owner,
                handle,
                JobOptions::new(format!("{SLOW}; INSERT INTO parent VALUES (7)")),
            )
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let requested = std::time::Instant::now();
        assert!(connections.cancel_job(owner, id).unwrap());
        let (_, update) = drain(&connections, owner, id).await;
        samples.push(requested.elapsed());
        assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
        assert_eq!(update.cancellation, CancellationDto::Confirmed);
        assert!(update.server_cancel_sent);
        assert_eq!(
            statuses(&update),
            [StatementStatusDto::Unknown, StatementStatusDto::NotRun]
        );
        assert!(
            !connections
                .cancel_job(owner, id)
                .is_ok_and(|requested| requested)
        );
    }
    assert!(
        samples
            .iter()
            .all(|sample| *sample < Duration::from_secs(2)),
        "{samples:?}"
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 7")
            .await,
        0
    );
    // The statement deadline takes the same path.
    let mut options = JobOptions::new(SLOW);
    options.statement_deadline = Duration::from_millis(300);
    let (_, update) = run(&connections, owner, handle, options).await;
    assert!(update.deadline_exceeded);
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
    assert_eq!(update.cancellation, CancellationDto::Confirmed);
}

#[tokio::test]
async fn leases_commit_and_roll_back_only_as_the_server_confirms() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let in_lease = |lease, sql: &str| {
        let mut options = JobOptions::new(sql);
        options.lease = Some(lease);
        options
    };
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::Busy
    );
    assert_eq!(
        connections
            .submit_job(owner, handle, JobOptions::new("SELECT 1"))
            .unwrap_err(),
        ExecutionError::Busy
    );
    let (_, update) = run(
        &connections,
        owner,
        handle,
        in_lease(
            lease,
            "INSERT INTO parent VALUES (10); CREATE TABLE made_in_lease(x)",
        ),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    assert!(update.statements.iter().all(|report| report.in_transaction));
    assert_eq!(update.lease, Some(LeaseStateDto::Active));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 10")
            .await,
        0
    );
    let end = connections
        .end_transaction(owner, lease, true)
        .await
        .unwrap();
    assert_eq!((end.outcome, end.commit_sent), (Outcome::Committed, true));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 10")
            .await,
        1
    );
    // A failed statement makes the lease rollback-only; rollback is confirmed.
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let (_, update) = run(
        &connections,
        owner,
        handle,
        in_lease(
            lease,
            "INSERT INTO parent VALUES (11); INSERT INTO parent VALUES (1)",
        ),
    )
    .await;
    assert_eq!(update.lease, Some(LeaseStateDto::RollbackOnly));
    assert_eq!(
        connections
            .submit_job(owner, handle, in_lease(lease, "SELECT 1"))
            .unwrap_err(),
        ExecutionError::RollbackOnly
    );
    assert_eq!(
        connections
            .end_transaction(owner, lease, true)
            .await
            .unwrap_err(),
        ExecutionError::RollbackOnly
    );
    let end = connections
        .end_transaction(owner, lease, false)
        .await
        .unwrap();
    assert_eq!(
        (end.outcome, end.commit_sent, end.rollback_sent),
        (Outcome::RolledBack, false, true)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 11")
            .await,
        0
    );
    // B-2: a repeated end (for example after a lost reply) reports the recorded outcome and
    // sends nothing; it is never a second COMMIT/ROLLBACK and never FORBIDDEN.
    for commit in [false, true] {
        assert_eq!(
            connections.end_transaction(owner, lease, commit).await,
            Ok(end)
        );
    }
    // A cancelled statement inside a lease is unknown until a confirmed rollback.
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let (id, _) = connections
        .submit_job(
            owner,
            handle,
            in_lease(lease, &format!("INSERT INTO parent VALUES (12); {SLOW}")),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    connections.cancel_job(owner, id).unwrap();
    let (_, update) = drain(&connections, owner, id).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
    assert_eq!(update.lease, Some(LeaseStateDto::RollbackOnly));
    assert_eq!(
        connections
            .end_transaction(owner, lease, false)
            .await
            .unwrap()
            .outcome,
        Outcome::RolledBack
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 12")
            .await,
        0
    );
    assert_eq!(connections.open_transactions(), 0);
}

#[tokio::test]
async fn disconnect_rolls_back_open_leases_and_expiry_is_recorded() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let mut options = JobOptions::new("INSERT INTO parent VALUES (20)");
    options.lease = Some(lease);
    run(&connections, owner, handle, options).await;
    connections.disconnect(owner, handle).await.unwrap();
    let end = connections
        .end_transaction(owner, lease, true)
        .await
        .unwrap();
    assert_eq!(
        (end.outcome, end.reason),
        (Outcome::RolledBack, EndReason::Closed)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 20")
            .await,
        0
    );
    // Expiry uses the same confirmed-rollback path; driven here without waiting LEASE_IDLE.
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let entry = connections.0.execution.leases.lock().unwrap()[&lease].clone();
    entry.state.lock().unwrap().last_used =
        Instant::now() - ExecutionLimits::LEASE_IDLE - Duration::from_secs(1);
    tokio::time::timeout(
        Duration::from_secs(5),
        expire(
            Arc::downgrade(&connections.0),
            lease,
            Arc::downgrade(&entry),
        ),
    )
    .await
    .unwrap();
    drop(entry);
    let end = connections
        .end_transaction(owner, lease, false)
        .await
        .unwrap();
    assert_eq!(
        (end.outcome, end.reason),
        (Outcome::RolledBack, EndReason::Expired)
    );
    assert!(
        connections
            .submit_job(owner, handle, JobOptions::new("SELECT 1"))
            .is_ok()
    );
}

/// V-1: a release cut off by `RELEASE_BOUND` (here: the lease connection is held, so its
/// ROLLBACK cannot even be sent) still leaves a recorded `unknown` outcome for the lease, on
/// disconnect, reset (renderer reload) and shutdown alike. Never FORBIDDEN, never rolled back.
#[tokio::test]
async fn a_release_cut_off_by_its_bound_records_unknown_instead_of_losing_the_outcome() {
    let fixture = Fixture::new().await;
    for (index, path) in ["disconnect", "reset", "shutdown"].into_iter().enumerate() {
        let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
        let lease = connections.begin_transaction(owner, handle).await.unwrap();
        let mut options = JobOptions::new(format!("INSERT INTO parent VALUES ({})", 30 + index));
        options.lease = Some(lease);
        run(&connections, owner, handle, options).await;
        let slot = connections.0.execution.leases.lock().unwrap()[&lease]
            .slot
            .clone();
        let held = slot.lock_owned().await;
        let started = std::time::Instant::now();
        match path {
            "disconnect" => connections.disconnect(owner, handle).await.unwrap(),
            "reset" => connections.reset().await,
            _ => connections.shutdown().await,
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed >= ExecutionLimits::RELEASE_BOUND
                && elapsed < ExecutionLimits::RELEASE_BOUND + Duration::from_secs(3),
            "{path}: {elapsed:?}"
        );
        let expected = TransactionEnd {
            outcome: Outcome::Unknown,
            reason: EndReason::Closed,
            commit_sent: false,
            rollback_sent: false,
        };
        for commit in [true, false] {
            assert_eq!(
                connections.end_transaction(owner, lease, commit).await,
                Ok(expected),
                "{path}"
            );
        }
        assert_eq!(connections.open_transactions(), 0);
        // Dropping the abandoned connection lets SQLite discard the transaction; this is the
        // observed data state, not a claimed rollback.
        drop(held);
        assert_eq!(
            fixture
                .count("SELECT count(*) FROM parent WHERE id >= 30")
                .await,
            0,
            "{path}"
        );
    }
}

/// V-1: every bounded path records `unknown` (with what is about to be sent) before COMMIT or
/// ROLLBACK is sent, and replaces it only with the server-confirmed outcome.
#[tokio::test]
async fn conclude_records_unknown_before_sending_commit_or_rollback() {
    let fixture = Fixture::new().await;
    for commit in [true, false] {
        let mut physical = Target::Sqlite(fixture.file()).open().await.unwrap();
        physical.begin().await.unwrap();
        let seen = Mutex::new(Vec::new());
        let end = physical
            .conclude(commit, EndReason::Closed, false, |end| {
                seen.lock().unwrap().push(end);
                true
            })
            .await;
        let unknown = TransactionEnd {
            outcome: Outcome::Unknown,
            reason: EndReason::Closed,
            commit_sent: commit,
            rollback_sent: !commit,
        };
        let confirmed = TransactionEnd {
            outcome: if commit {
                Outcome::Committed
            } else {
                Outcome::RolledBack
            },
            ..unknown
        };
        assert_eq!(*seen.lock().unwrap(), [unknown, confirmed]);
        assert_eq!(end, confirmed);
    }
}

/// B-1: a submit or begin racing a disconnect is refused or released; after both finish no job
/// is still running and no transaction is open on the closed handle.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disconnect_racing_submit_and_begin_never_leaves_untracked_work() {
    let fixture = Fixture::new().await;
    let mut refused = 0;
    for round in 0..60 {
        let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let (closer, racer) = (connections.clone(), connections.clone());
        let gate = barrier.clone();
        let close = tokio::spawn(async move {
            gate.wait().await;
            closer.disconnect(owner, handle).await
        });
        let gate = barrier.clone();
        let race = tokio::spawn(async move {
            gate.wait().await;
            if round % 2 == 0 {
                racer
                    .submit_job(owner, handle, JobOptions::new(SLOW))
                    .map(|_| ())
            } else {
                racer.begin_transaction(owner, handle).await.map(|_| ())
            }
        });
        close.await.unwrap().unwrap();
        let raced = race.await.unwrap();
        if let Err(error) = raced {
            assert!(
                matches!(error, ExecutionError::Forbidden | ExecutionError::Busy),
                "{error:?}"
            );
            refused += 1;
        }
        // Whatever was admitted before the close has been released by it.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(connections.open_transactions(), 0, "round {round}");
        assert!(
            connections
                .0
                .execution
                .jobs
                .lock()
                .unwrap()
                .values()
                .all(|entry| entry.job.is_finished()),
            "round {round}"
        );
        // After the close, new work on the handle is refused.
        assert_eq!(
            connections
                .submit_job(owner, handle, JobOptions::new("SELECT 1"))
                .unwrap_err(),
            ExecutionError::Forbidden
        );
        assert_eq!(
            connections
                .begin_transaction(owner, handle)
                .await
                .unwrap_err(),
            ExecutionError::Forbidden
        );
    }
    assert!(refused > 0, "the race never reached the close");
}

#[tokio::test]
async fn session_state_never_reaches_the_next_job_across_fault_cycles() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    for cycle in 0..100 {
        let lease = connections.begin_transaction(owner, handle).await.unwrap();
        let mut options = JobOptions::new(format!(
            "CREATE TEMP TABLE leak(x); PRAGMA cache_size = 100; INSERT INTO parent VALUES ({}){}",
            1000 + cycle,
            if cycle % 2 == 0 {
                "; INSERT INTO parent VALUES (1)"
            } else {
                ""
            }
        ));
        options.lease = Some(lease);
        run(&connections, owner, handle, options).await;
        let commit = cycle % 4 == 1;
        let end = connections
            .end_transaction(owner, lease, commit)
            .await
            .unwrap();
        assert_eq!(
            end.outcome,
            if commit {
                Outcome::Committed
            } else {
                Outcome::RolledBack
            }
        );
        let (batches, _) = run(
            &connections,
            owner,
            handle,
            JobOptions::new("SELECT count(*) FROM sqlite_temp_master; PRAGMA cache_size"),
        )
        .await;
        assert_eq!(
            batches[0].rows,
            [vec![CellValue::Integer { text: "0".into() }]],
            "cycle {cycle}"
        );
        assert_ne!(
            batches[1].rows,
            [vec![CellValue::Integer { text: "100".into() }]],
            "cycle {cycle}"
        );
    }
    // Only odd cycles where cycle % 4 == 1 committed (no failing statement there).
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id >= 1000")
            .await,
        25
    );
    assert_eq!(connections.open_transactions(), 0);
    assert_eq!(connections.retained_jobs(), 0);
}

#[tokio::test]
async fn large_results_apply_backpressure_and_bounds() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let mut options = JobOptions::new(
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 100000) SELECT x, 'payload-payload' FROM c",
    );
    options.max_rows_per_result = ExecutionLimits::MAX_ROWS_PER_RESULT;
    let (id, _) = connections.submit_job(owner, handle, options).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    // The worker is parked on the full page buffer: only BUFFERED_PAGES pages exist.
    let pages = connections.0.execution.jobs.lock().unwrap()[&id]
        .pages
        .clone();
    assert_eq!(pages.lock().await.len(), ExecutionLimits::BUFFERED_PAGES);
    assert_eq!(
        connections
            .fetch_job(owner, id, Duration::ZERO)
            .await
            .unwrap()
            .statements[0]
            .status,
        StatementStatusDto::Running
    );
    let (batches, update) = drain(&connections, owner, id).await;
    let rows: usize = batches.iter().map(|batch| batch.rows.len()).sum();
    assert!(
        batches
            .iter()
            .all(|batch| batch.rows.len() <= ExecutionLimits::PAGE_ROWS)
    );
    assert!(rows > 1000 && rows < 100_000, "{rows}");
    assert!(batches.last().unwrap().truncated && batches.last().unwrap().last);
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
}

/// Models a splitter miss: the whole text reaches the runtime as one checked statement.
fn unsplit(sql: &str) -> Statement {
    Statement {
        sql: sql.into(),
        span: 0..sql.len(),
        words: vec!["CREATE".into()],
        compound: false,
        semicolon: false,
        label: false,
    }
}
async fn start(
    connections: &Connections,
    owner: OwnerId,
    handle: ConnectionId,
    statements: Vec<Statement>,
    lease: Option<LeaseId>,
) -> JobUpdate {
    let mut options = JobOptions::new("");
    options.lease = lease;
    let (id, _) = connections
        .start_job(owner, handle, statements, options)
        .unwrap();
    drain(connections, owner, id).await.1
}
/// The fixed review input (R-1): `begin`/`end` identifiers inside a trigger body.
const SMUGGLE: &str = "CREATE TEMP TRIGGER rv AFTER INSERT ON parent BEGIN SELECT 1 AS begin; END; COMMIT; BEGIN; SELECT 1 AS end, 2 AS end";

/// R-1: SQLx's SQLite driver runs every statement in the text it is given, so a statement the
/// splitter mis-split must be refused before anything in it runs, whatever the splitter did.
#[tokio::test]
async fn r1_sqlite_text_with_more_than_one_statement_never_runs() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    // Inside a lease: refused before execution; the rollback is then truthful.
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let mut options = JobOptions::new("INSERT INTO parent VALUES (77)");
    options.lease = Some(lease);
    run(&connections, owner, handle, options).await;
    // The hardened splitter already separates the smuggled COMMIT (policy refusal) …
    let mut options = JobOptions::new(SMUGGLE);
    options.lease = Some(lease);
    assert_eq!(
        connections.submit_job(owner, handle, options).unwrap_err(),
        ExecutionError::Script(1, ScriptError::TransactionControl)
    );
    // … and the runtime refuses the text even when the splitter missed it.
    let update = start(
        &connections,
        owner,
        handle,
        vec![unsplit(SMUGGLE)],
        Some(lease),
    )
    .await;
    assert_eq!(statuses(&update), [StatementStatusDto::Failed]);
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Failed));
    assert_eq!(update.lease, Some(LeaseStateDto::RollbackOnly));
    let end = connections
        .end_transaction(owner, lease, false)
        .await
        .unwrap();
    assert_eq!(
        (end.outcome, end.commit_sent, end.rollback_sent),
        (Outcome::RolledBack, false, true)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 77")
            .await,
        0
    );
    // Outside a lease: refused, never Succeeded, and the smuggled insert did not run.
    let autocommit = "CREATE TEMP TRIGGER rv AFTER INSERT ON parent BEGIN SELECT 1 AS begin; END; BEGIN; INSERT INTO parent VALUES (78); SELECT 1 AS end, 2 AS end";
    assert!(
        connections
            .submit_job(owner, handle, JobOptions::new(autocommit))
            .is_err()
    );
    let update = start(&connections, owner, handle, vec![unsplit(autocommit)], None).await;
    assert_eq!(statuses(&update), [StatementStatusDto::Failed]);
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Failed));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 78")
            .await,
        0
    );
    // Other smuggling shapes: a plain second statement, a NUL byte, a `;` in a comment (refused
    // conservatively: SQLite would accept the prefix).
    for sql in [
        "INSERT INTO parent VALUES (79); INSERT INTO parent VALUES (80)",
        "INSERT INTO parent VALUES (79)\0; ATTACH 'x' AS x",
        "INSERT INTO parent VALUES (79) /* ; */",
    ] {
        let update = start(&connections, owner, handle, vec![unsplit(sql)], None).await;
        assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Failed), "{sql:?}");
    }
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id >= 79")
            .await,
        0
    );
    // A `;` inside strings, identifiers and trigger bodies is still one statement.
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new(
            "INSERT INTO item(id, parent, note) VALUES (5, 1, 'a;b'); SELECT 1 AS [x;y], \"p;q\"; CREATE TRIGGER tr AFTER INSERT ON parent BEGIN UPDATE item SET n = 1 WHERE note = 'a;b'; SELECT 1 AS end; END; INSERT INTO parent VALUES (81)",
        ),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM item WHERE id = 5 AND n = 1")
            .await,
        1
    );
}

/// R-3: an outcome is reported only when the session's transaction state still matches the
/// model. The divergent statements are injected past the policy to model any future miss.
#[tokio::test]
async fn r3_a_diverged_session_is_never_reported_committed_or_rolled_back() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let parts = |sql: &str| script::split(sql, crate::read_only::Dialect::Sqlite).unwrap();
    // Outside a lease: a transaction left open is discarded with the connection.
    let update = start(
        &connections,
        owner,
        handle,
        parts("INSERT INTO parent VALUES (90); BEGIN; INSERT INTO parent VALUES (91)"),
        None,
    )
    .await;
    assert!(
        update
            .statements
            .iter()
            .all(|report| report.status == StatementStatusDto::Succeeded)
    );
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id IN (90, 91)")
            .await,
        1
    );
    // Inside a lease: the transaction was committed by a statement, not by StoneQL.
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let update = start(
        &connections,
        owner,
        handle,
        parts("INSERT INTO parent VALUES (92); COMMIT"),
        Some(lease),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
    assert_eq!(update.lease, Some(LeaseStateDto::RollbackOnly));
    let end = connections
        .end_transaction(owner, lease, false)
        .await
        .unwrap();
    assert_eq!(
        (end.outcome, end.commit_sent, end.rollback_sent),
        (Outcome::Unknown, false, false)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 92")
            .await,
        1
    );
    // A matching session keeps its outcomes.
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO parent VALUES (93)"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
}

/// Waits (bounded) until `condition` holds.
async fn eventually(condition: impl Fn() -> bool, what: &str) {
    for _ in 0..500 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what}");
}

/// L-8: when the server interrupt is still in flight at the end of the cancel grace (written,
/// no reply), it may reach the session later and hit whatever runs next. The lease connection
/// is then discarded and the lease ended `unknown`, never kept for ROLLBACK or another job.
#[tokio::test]
async fn l8_an_interrupt_still_in_flight_never_leaves_its_lease_connection_in_use() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let mut options = JobOptions::new("INSERT INTO parent VALUES (60)");
    options.lease = Some(lease);
    run(&connections, owner, handle, options).await;
    let slot = connections.0.execution.leases.lock().unwrap()[&lease]
        .slot
        .clone();
    {
        let mut guard = slot.lock().await;
        let physical = guard.as_mut().unwrap();
        let Interrupt::Sqlite(flag) = &physical.interrupt else {
            unreachable!()
        };
        physical.interrupt = Interrupt::Stalled(flag.clone());
    }
    let mut options = JobOptions::new(SLOW);
    options.lease = Some(lease);
    let (id, _) = connections.submit_job(owner, handle, options).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(connections.cancel_job(owner, id).unwrap());
    let (_, update) = drain(&connections, owner, id).await;
    assert_eq!(statuses(&update), [StatementStatusDto::Unknown]);
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
    assert!(!update.server_cancel_sent);
    assert_eq!(update.lease, Some(LeaseStateDto::Ended));
    let lost = TransactionEnd {
        outcome: Outcome::Unknown,
        reason: EndReason::ConnectionLost,
        commit_sent: false,
        rollback_sent: false,
    };
    assert_eq!(
        connections.end_transaction(owner, lease, false).await,
        Ok(lost)
    );
    let mut options = JobOptions::new("INSERT INTO parent VALUES (61)");
    options.lease = Some(lease);
    assert_eq!(
        connections.submit_job(owner, handle, options).unwrap_err(),
        ExecutionError::Forbidden
    );
    assert_eq!(connections.open_transactions(), 0);
}

/// L-4: the per-owner lease count is reserved before the connection is opened, so concurrent
/// begins on different handles never exceed `MAX_LEASES_PER_OWNER`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l4_concurrent_begins_never_exceed_the_per_owner_lease_count() {
    let fixture = Fixture::new().await;
    let connections =
        Connections::new(4, Duration::from_secs(3), Duration::from_secs(600)).unwrap();
    let owner = OwnerId::new();
    let grant = connections
        .grant_sqlite_file(owner, &fixture.file())
        .unwrap();
    let mut handles = Vec::new();
    for _ in 0..=ExecutionLimits::MAX_LEASES_PER_OWNER {
        handles.push(
            connections
                .connect_sqlite_with_access(owner, &grant, Access::ReadWrite)
                .await
                .unwrap(),
        );
    }
    for round in 0..20 {
        let barrier = Arc::new(tokio::sync::Barrier::new(handles.len()));
        let begins: Vec<_> = handles
            .iter()
            .map(|&handle| {
                let (connections, barrier) = (connections.clone(), barrier.clone());
                tokio::spawn(async move {
                    barrier.wait().await;
                    connections.begin_transaction(owner, handle).await
                })
            })
            .collect();
        let mut opened = Vec::new();
        for begin in begins {
            match begin.await.unwrap() {
                Ok(lease) => opened.push(lease),
                Err(error) => assert_eq!(error, ExecutionError::Capacity, "round {round}"),
            }
        }
        assert_eq!(
            opened.len(),
            ExecutionLimits::MAX_LEASES_PER_OWNER,
            "round {round}"
        );
        assert_eq!(
            connections.open_transactions(),
            ExecutionLimits::MAX_LEASES_PER_OWNER
        );
        for lease in opened {
            connections
                .end_transaction(owner, lease, false)
                .await
                .unwrap();
        }
        assert!(connections.0.execution.opening.lock().unwrap().is_empty());
    }
}

/// B-7: finished jobs whose pages were never fetched count against the owner's bound (8,
/// PROPOSED), which bounds the bytes an owner can leave buffered: the 9th is refused, and
/// fetching one to the end frees its slot.
#[tokio::test]
async fn b7_undrained_jobs_beyond_the_owner_bound_are_refused() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let mut ids = Vec::new();
    for _ in 0..8 {
        let (id, _) = connections
            .submit_job(owner, handle, JobOptions::new("SELECT 1"))
            .unwrap();
        let job = connections.0.execution.jobs.lock().unwrap()[&id]
            .job
            .clone();
        job.wait_finished().await;
        // The permit is released just after the job finishes.
        eventually(|| handle_idle(&connections, handle), "permit released").await;
        ids.push(id);
    }
    assert_eq!(
        connections
            .submit_job(owner, handle, JobOptions::new("SELECT 1"))
            .unwrap_err(),
        ExecutionError::Capacity
    );
    drain(&connections, owner, ids[0]).await;
    let (id, _) = connections
        .submit_job(owner, handle, JobOptions::new("SELECT 1"))
        .unwrap();
    drain(&connections, owner, id).await;
}
fn handle_idle(connections: &Connections, handle: ConnectionId) -> bool {
    connections.0.entries.lock().unwrap()[&handle]
        .work
        .available_permits()
        == ExecutionLimits::RUNNING_JOBS_PER_HANDLE
}

/// R-6: resolved ended-lease outcomes are kept per owner, so many ends by one owner (or many
/// owners) never evict another owner's recent outcome; all owners together stay bounded.
#[test]
fn r6_ended_lease_records_are_kept_per_owner_and_bounded_overall() {
    let state = ExecutionState::new();
    let end = TransactionEnd {
        outcome: Outcome::RolledBack,
        ..TransactionEnd::unknown(EndReason::Requested)
    };
    let (a, b) = (OwnerId::new(), OwnerId::new());
    let kept = LeaseId(Uuid::new_v4());
    state.record_end(kept, a, "db", end);
    let ids: Vec<_> = (0..40).map(|_| LeaseId(Uuid::new_v4())).collect();
    for &id in &ids {
        state.record_end(id, b, "db", end);
    }
    assert_eq!(state.recorded(kept, a), Some(end));
    let evicted = ids.len() - ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER;
    assert!(
        ids[..evicted]
            .iter()
            .all(|&id| state.recorded(id, b).is_none())
    );
    assert!(
        ids[evicted..]
            .iter()
            .all(|&id| state.recorded(id, b) == Some(end))
    );
    // Replacing a record (unknown → confirmed) keeps the bounds.
    let confirmed = TransactionEnd {
        outcome: Outcome::Committed,
        ..end
    };
    state.record_end(kept, a, "db", confirmed);
    assert_eq!(state.recorded(kept, a), Some(confirmed));
    assert_eq!(
        state.ended.lock().unwrap().len(),
        1 + ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER
    );
    for _ in 0..ExecutionLimits::MAX_ENDED_LEASES {
        state.record_end(LeaseId(Uuid::new_v4()), OwnerId::new(), "db", end);
    }
    assert_eq!(
        state.ended.lock().unwrap().len(),
        ExecutionLimits::MAX_ENDED_LEASES
    );
    assert_eq!(
        state.recorded(kept, a),
        None,
        "the oldest resolved record overall goes first"
    );
}

fn job_facts() -> UnresolvedJob {
    UnresolvedJob {
        statement_count: 1,
        unknown_statement: Some(0),
        in_transaction: false,
        cancellation: CancellationDto::Requested,
        server_cancel_sent: false,
        deadline_exceeded: false,
    }
}
fn listed(state: &ExecutionState, owner: OwnerId) -> Vec<String> {
    state
        .unresolved(owner)
        .outcomes
        .into_iter()
        .map(|outcome| outcome.outcome_id)
        .collect()
}

/// P3: unacknowledged unknown outcomes (lease ends and jobs) are never evicted by the 32/256
/// bounds, however many resolved outcomes this or other owners record; resolved ones still are.
#[test]
fn p3_unresolved_unknowns_survive_eviction_pressure_and_resolved_ones_are_evicted() {
    let state = ExecutionState::new();
    let owner = OwnerId::new();
    let unknown = TransactionEnd {
        commit_sent: true,
        ..TransactionEnd::unknown(EndReason::Requested)
    };
    let resolved = TransactionEnd {
        outcome: Outcome::RolledBack,
        ..TransactionEnd::unknown(EndReason::Requested)
    };
    let lease = LeaseId(Uuid::new_v4());
    let job = JobId(Uuid::new_v4());
    state.record_end(lease, owner, "app", unknown);
    state.record_job(job, owner, "app", job_facts());
    let own: Vec<_> = (0..3 * ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER)
        .map(|_| LeaseId(Uuid::new_v4()))
        .collect();
    for &id in &own {
        state.record_end(id, owner, "app", resolved);
    }
    for _ in 0..2 * ExecutionLimits::MAX_ENDED_LEASES {
        state.record_end(LeaseId(Uuid::new_v4()), OwnerId::new(), "other", resolved);
    }
    assert_eq!(state.recorded(lease, owner), Some(unknown));
    assert_eq!(listed(&state, owner), [lease.opaque(), job.opaque()]);
    let outcomes = state.unresolved(owner).outcomes;
    assert_eq!(outcomes[0].transaction, Some(unknown.into()));
    assert_eq!(
        (outcomes[0].database.as_str(), &outcomes[0].job),
        ("app", &None)
    );
    assert_eq!(outcomes[1].job, Some(job_facts()));
    assert_eq!(outcomes[1].transaction, None);
    // Resolved ones are still bounded per owner and overall.
    let ended = state.ended.lock().unwrap();
    assert_eq!(
        ended.iter().filter(|r| !r.unresolved()).count(),
        ExecutionLimits::MAX_ENDED_LEASES
    );
    assert_eq!(ended.len(), 2 + ExecutionLimits::MAX_ENDED_LEASES);
    drop(ended);
    assert!(
        own[..own.len() - 1]
            .iter()
            .all(|&id| state.recorded(id, owner).is_none())
    );
    assert!(listed(&state, OwnerId::new()).is_empty(), "owner-scoped");
    // Acknowledged with the owner's bound already full, it stays queryable (newest evictable).
    state
        .acknowledge(owner, OutcomeId::parse(&lease.opaque()).unwrap())
        .unwrap();
    assert_eq!(listed(&state, owner), [job.opaque()]);
    assert_eq!(state.recorded(lease, owner), Some(unknown));
}

/// P3: acknowledgement is owner-scoped and idempotent, hides the outcome without changing it
/// (unknown is never turned into success or rollback) and only then makes it evictable. A
/// record that changes while still unknown (COMMIT now attempted) must be acknowledged again.
#[test]
fn p3_acknowledgement_hides_without_changing_the_outcome() {
    let state = ExecutionState::new();
    let (owner, other) = (OwnerId::new(), OwnerId::new());
    let lease = LeaseId(Uuid::new_v4());
    let unknown = TransactionEnd::unknown(EndReason::Closed);
    state.record_end(lease, owner, "app", unknown);
    let id = OutcomeId::parse(&lease.opaque()).unwrap();
    assert_eq!(state.acknowledge(other, id), Err(ExecutionError::Forbidden));
    assert_eq!(listed(&state, owner), [lease.opaque()]);
    for _ in 0..2 {
        assert_eq!(state.acknowledge(owner, id), Ok(()));
        assert!(listed(&state, owner).is_empty());
        assert_eq!(state.recorded(lease, owner), Some(unknown));
    }
    // The end is still being concluded: COMMIT is now attempted, a different unknown.
    let sent = TransactionEnd {
        commit_sent: true,
        ..unknown
    };
    state.record_end(lease, owner, "app", sent);
    assert_eq!(listed(&state, owner), [lease.opaque()]);
    state.acknowledge(owner, id).unwrap();
    // Recording the same unknown again keeps the acknowledgement.
    state.record_end(lease, owner, "app", sent);
    assert!(listed(&state, owner).is_empty());
    assert_eq!(state.recorded(lease, owner), Some(sent));
    // Acknowledged, it is evictable like a resolved outcome.
    let resolved = TransactionEnd {
        outcome: Outcome::RolledBack,
        ..unknown
    };
    for _ in 0..ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER {
        state.record_end(LeaseId(Uuid::new_v4()), owner, "app", resolved);
    }
    assert_eq!(state.recorded(lease, owner), None);
    assert_eq!(state.acknowledge(owner, id), Err(ExecutionError::Forbidden));
    assert_eq!(
        state.acknowledge(
            owner,
            OutcomeId::parse(&Uuid::new_v4().to_string()).unwrap()
        ),
        Err(ExecutionError::Forbidden)
    );
}

/// P3: at `MAX_UNRESOLVED_OUTCOMES_PER_OWNER` unacknowledged unknowns new jobs and
/// transactions are refused before anything is sent, and nothing is dropped; another owner is
/// unaffected; acknowledging one admits work again.
#[tokio::test]
async fn p3_the_unresolved_cap_refuses_new_work_and_never_drops_an_outcome() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let state = &connections.0.execution;
    let ids: Vec<_> = (0..ExecutionLimits::MAX_UNRESOLVED_OUTCOMES_PER_OWNER)
        .map(|_| JobId(Uuid::new_v4()))
        .collect();
    for &id in &ids {
        state.record_job(id, owner, "fixture.sqlite", job_facts());
    }
    for _ in 0..3 {
        assert_eq!(
            connections
                .submit_job(
                    owner,
                    handle,
                    JobOptions::new("INSERT INTO parent VALUES (51)")
                )
                .unwrap_err(),
            ExecutionError::UnresolvedOutcomes
        );
        assert_eq!(
            connections
                .begin_transaction(owner, handle)
                .await
                .unwrap_err(),
            ExecutionError::UnresolvedOutcomes
        );
    }
    assert_eq!(connections.retained_jobs(), 0);
    assert_eq!(connections.open_transactions(), 0);
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 51")
            .await,
        0
    );
    let all = connections.unresolved_outcomes(owner);
    assert_eq!(
        all.limit as usize,
        ExecutionLimits::MAX_UNRESOLVED_OUTCOMES_PER_OWNER
    );
    assert_eq!(
        all.outcomes
            .iter()
            .map(|o| o.outcome_id.clone())
            .collect::<Vec<_>>(),
        ids.iter().map(|id| id.opaque()).collect::<Vec<_>>()
    );
    // Another owner's work is not refused by this owner's outcomes.
    let other = OwnerId::new();
    let grant = connections
        .grant_sqlite_file(other, &fixture.file())
        .unwrap();
    let other_handle = connections
        .connect_sqlite_with_access(other, &grant, Access::ReadWrite)
        .await
        .unwrap();
    let (_, update) = run(
        &connections,
        other,
        other_handle,
        JobOptions::new("SELECT 1"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    // Acknowledging one admits new work.
    let remaining = connections
        .acknowledge_outcome(owner, OutcomeId::parse(&ids[0].opaque()).unwrap())
        .unwrap();
    assert_eq!(remaining.outcomes.len(), ids.len() - 1);
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO parent VALUES (51)"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
}

/// P3 + P2: a job interrupted by a renderer reset ends `unknown`; the reset releases its job
/// record (so it no longer holds reload), yet the unknown outcome stays listable for the same
/// owner (the next renderer session) with SQL-free facts, and never for another owner.
#[tokio::test]
async fn p3_a_job_unknown_survives_the_reset_that_releases_its_record() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    assert!(!connections.has_active_work(owner));
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    assert!(connections.has_active_work(owner));
    let mut options = JobOptions::new(format!("INSERT INTO parent VALUES (61); {SLOW}"));
    options.lease = Some(lease);
    let (job, _) = connections.submit_job(owner, handle, options).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    connections.reset().await;
    assert!(
        !connections.has_active_work(owner),
        "reset releases job records and leases"
    );
    assert_eq!(connections.retained_jobs(), 0);
    assert_eq!(
        connections
            .fetch_job(owner, job, Duration::ZERO)
            .await
            .unwrap_err(),
        ExecutionError::Forbidden
    );
    let outcomes = connections.unresolved_outcomes(owner).outcomes;
    let job_outcome = outcomes
        .iter()
        .find(|outcome| outcome.outcome_id == job.opaque())
        .expect("the interrupted job is listed");
    assert_eq!(job_outcome.database, "fixture.sqlite");
    assert_eq!(
        job_outcome.job,
        Some(UnresolvedJob {
            statement_count: 2,
            unknown_statement: Some(1),
            in_transaction: true,
            cancellation: CancellationDto::Confirmed,
            server_cancel_sent: true,
            deadline_exceeded: false,
        })
    );
    // The lease's ROLLBACK was confirmed: resolved, so not listed.
    assert!(
        outcomes
            .iter()
            .all(|outcome| outcome.outcome_id != lease.opaque())
    );
    assert_eq!(
        connections
            .end_transaction(owner, lease, true)
            .await
            .unwrap()
            .outcome,
        Outcome::RolledBack
    );
    assert!(
        connections
            .unresolved_outcomes(OwnerId::new())
            .outcomes
            .is_empty()
    );
    let id = OutcomeId::parse(&job.opaque()).unwrap();
    assert_eq!(
        connections
            .acknowledge_outcome(OwnerId::new(), id)
            .unwrap_err(),
        ExecutionError::Forbidden
    );
    assert!(
        connections
            .acknowledge_outcome(owner, id)
            .unwrap()
            .outcomes
            .is_empty()
    );
    assert!(
        connections
            .acknowledge_outcome(owner, id)
            .unwrap()
            .outcomes
            .is_empty()
    );
}

/// B-1: disconnect and the handle-lifetime watcher make the handle unreachable **before** they
/// release its work. The lease connection is held, so the release is parked while the
/// registry is inspected; with the old order the handle would still be registered.
#[tokio::test]
async fn b1_disconnect_and_handle_expiry_remove_the_handle_before_releasing_its_work() {
    let fixture = Fixture::new().await;
    for path in ["disconnect", "expiry"] {
        let lifetime = if path == "expiry" { 2 } else { 600 };
        let connections =
            Connections::new(4, Duration::from_secs(3), Duration::from_secs(lifetime)).unwrap();
        let owner = OwnerId::new();
        let grant = connections
            .grant_sqlite_file(owner, &fixture.file())
            .unwrap();
        let handle = connections
            .connect_sqlite_with_access(owner, &grant, Access::ReadWrite)
            .await
            .unwrap();
        let lease = connections.begin_transaction(owner, handle).await.unwrap();
        let slot = connections.0.execution.leases.lock().unwrap()[&lease]
            .slot
            .clone();
        let held = slot.lock_owned().await;
        let closer = connections.clone();
        let close = tokio::spawn(async move {
            if path == "disconnect" {
                closer.disconnect(owner, handle).await.unwrap();
            }
        });
        // Release has taken the lease and is parked on its held connection.
        for _ in 0..600 {
            if connections.open_transactions() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(connections.open_transactions(), 0, "{path}");
        assert!(
            !connections.0.entries.lock().unwrap().contains_key(&handle),
            "{path}: the handle is still registered while its work is released"
        );
        drop(held);
        close.await.unwrap();
        assert_eq!(
            connections
                .end_transaction(owner, lease, false)
                .await
                .unwrap()
                .reason,
            EndReason::Closed,
            "{path}"
        );
    }
}

/// B-11: rows and pages read after an interrupt are discarded, and the statement says so.
#[tokio::test]
async fn b11_results_read_after_an_interrupt_are_flagged_withheld() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let rows = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 100000) SELECT x FROM c";
    let mut options = JobOptions::new(rows);
    options.max_rows_per_result = ExecutionLimits::MAX_ROWS_PER_RESULT;
    let (id, _) = connections.submit_job(owner, handle, options).unwrap();
    let pages = connections.0.execution.jobs.lock().unwrap()[&id]
        .pages
        .clone();
    eventually(
        || {
            pages
                .try_lock()
                .is_ok_and(|pages| pages.len() == ExecutionLimits::BUFFERED_PAGES)
        },
        "worker parked on a full buffer",
    )
    .await;
    assert!(connections.cancel_job(owner, id).unwrap());
    let (batches, update) = drain(&connections, owner, id).await;
    assert_eq!(batches.len(), ExecutionLimits::BUFFERED_PAGES);
    assert!(!batches.iter().any(|batch| batch.last || batch.truncated));
    assert!(update.statements[0].results_withheld);
    // Without an interrupt nothing is withheld.
    let mut options = JobOptions::new(rows);
    options.max_rows_per_result = ExecutionLimits::MAX_ROWS_PER_RESULT;
    let (_, update) = run(&connections, owner, handle, options).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    assert!(!update.statements[0].results_withheld);
}

/// L-3 / Contract 5.3 ordering: PostgreSQL's server timeout fires strictly before the client
/// statement deadline, by the margin when the deadline allows it, and is never 0 ms (which
/// PostgreSQL treats as "disabled").
#[test]
fn postgres_statement_timeout_is_strictly_before_the_client_deadline_and_never_disabled() {
    let ms = Duration::from_millis;
    for seconds in [1, 2, 3, 30, 300] {
        let deadline = Duration::from_secs(seconds);
        let timeout = ExecutionLimits::postgres_statement_timeout(deadline);
        assert!(
            timeout < deadline,
            "{seconds} s: {timeout:?} not before the deadline"
        );
        assert!(
            timeout >= deadline / 2,
            "{seconds} s: {timeout:?} below half"
        );
        assert!(!timeout.is_zero());
    }
    assert_eq!(
        ExecutionLimits::postgres_statement_timeout(Duration::from_secs(30)),
        ms(29_000)
    );
    assert_eq!(
        ExecutionLimits::postgres_statement_timeout(Duration::from_secs(3)),
        ms(2_000)
    );
    assert_eq!(
        ExecutionLimits::postgres_statement_timeout(Duration::from_secs(1)),
        ms(500)
    );
    // Sub-millisecond deadlines (not reachable from the host, which sends whole seconds) still
    // get a server timeout rather than `0` = none.
    assert_eq!(
        ExecutionLimits::postgres_statement_timeout(Duration::from_micros(1)),
        ms(1)
    );
    assert_eq!(
        ExecutionLimits::postgres_statement_timeout(Duration::from_millis(3)),
        ms(1)
    );
}

/// An idle-expired lease is gone: a job on it is refused before anything is sent, and the
/// lease's recorded end (what the renderer then shows) states the expiry and the confirmed
/// rollback, without a second ROLLBACK being sent.
#[tokio::test]
async fn a_job_on_an_expired_lease_is_refused_and_the_recorded_end_states_the_expiry() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let entry = connections.0.execution.leases.lock().unwrap()[&lease].clone();
    entry.state.lock().unwrap().last_used =
        Instant::now() - ExecutionLimits::LEASE_IDLE - Duration::from_secs(1);
    tokio::time::timeout(
        Duration::from_secs(5),
        expire(
            Arc::downgrade(&connections.0),
            lease,
            Arc::downgrade(&entry),
        ),
    )
    .await
    .unwrap();
    drop(entry);
    let mut options = JobOptions::new("INSERT INTO parent VALUES (21)");
    options.lease = Some(lease);
    assert_eq!(
        connections.submit_job(owner, handle, options).err(),
        Some(ExecutionError::Forbidden)
    );
    for commit in [false, true] {
        let end = connections
            .end_transaction(owner, lease, commit)
            .await
            .unwrap();
        assert_eq!(
            (end.outcome, end.reason, end.commit_sent),
            (Outcome::RolledBack, EndReason::Expired, false)
        );
    }
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 21")
            .await,
        0
    );
}

/// F-3: a job still running when a reset released its record (its `RELEASE_BOUND` was
/// exceeded) records its unknown outcome later, after the next renderer may already have
/// listed. The list reports such detached jobs as `pending` until each has finished, and a
/// job is only counted finished after its unknown outcome is recorded, so `pending == 0`
/// means the listed outcomes are complete.
#[tokio::test]
async fn detached_jobs_are_pending_until_their_late_unknown_is_recorded() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let (job, _) = connections
        .submit_job(owner, handle, JobOptions::new(SLOW))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let running = connections.0.execution.jobs.lock().unwrap()[&job]
        .job
        .clone();
    // The reset's bounded wait ended before the job did: its record is released regardless.
    connections.0.execution.forget_jobs();
    assert!(
        !connections.has_active_work(owner),
        "detached work never holds reload"
    );
    let listed = connections.unresolved_outcomes(owner);
    assert_eq!((listed.outcomes.len(), listed.pending), (0, 1));
    assert_eq!(connections.unresolved_outcomes(OwnerId::new()).pending, 0);
    running.control.trip(Reason::Closed);
    running.wait_finished().await;
    let listed = connections.unresolved_outcomes(owner);
    assert_eq!(listed.pending, 0);
    assert_eq!(listed.outcomes.len(), 1);
    assert_eq!(listed.outcomes[0].outcome_id, job.opaque());
    connections.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Durable unknown-outcome journal (J-1..J-11). Journals live in the fixture's temporary
// directory, never in the repository. The journal store exists on Unix and Windows; elsewhere
// it fails closed (`j0`). J-4 changes Unix permissions and runs on Unix only.

#[cfg(any(unix, windows))]
use super::journal::Fault;

#[cfg(any(unix, windows))]
fn journal_dir(fixture: &Fixture) -> PathBuf {
    fixture.0.join("journal")
}
#[cfg(any(unix, windows))]
async fn open_journaled(fixture: &Fixture) -> (Connections, OwnerId, ConnectionId) {
    let (connections, owner, handle) = open(fixture, Access::ReadWrite).await;
    connections.require_outcome_journal();
    assert_eq!(
        connections.open_outcome_journal(&journal_dir(fixture), owner),
        None
    );
    (connections, owner, handle)
}
#[cfg(any(unix, windows))]
fn inject(connections: &Connections, fault: Option<Fault>) {
    connections
        .0
        .execution
        .journal
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .inject(fault);
}
#[cfg(any(unix, windows))]
fn full_disk() -> Option<Fault> {
    Some(Fault::Fail(std::io::ErrorKind::StorageFull))
}
#[cfg(any(unix, windows))]
fn sqlite_origin(fixture: &Fixture) -> Origin {
    Origin::of(Engine::Sqlite, &Target::Sqlite(fixture.file()))
}
#[cfg(any(unix, windows))]
fn journal_entries(fixture: &Fixture) -> Vec<Entry> {
    let bytes = std::fs::read(journal_dir(fixture).join(journal::FILE)).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    text.lines()
        .skip(1)
        .filter(|line| !line.starts_with("end "))
        .map(|line| serde_json::from_str(&line[9..]).unwrap())
        .collect()
}
/// Everything listed except what a restart legitimately changes (age, `restored`).
#[cfg(any(unix, windows))]
fn facts(outcome: &UnresolvedOutcome) -> UnresolvedOutcome {
    UnresolvedOutcome {
        seconds_ago: 0,
        restored: false,
        ..outcome.clone()
    }
}

/// J-1: a crash (the registry dropped without shutdown) loses nothing: an open lease, a lease
/// end whose COMMIT may have been sent and an unknown job are restored by the next run with the
/// same ids and facts, marked as recorded before a restart, for the new run's owner.
#[cfg(not(any(unix, windows)))]
#[tokio::test]
async fn j0_without_a_journal_store_read_write_work_is_refused() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    connections.require_outcome_journal();
    assert_eq!(
        connections.open_outcome_journal(&fixture.0.join("journal"), owner),
        Some(JournalProblemDto::Unavailable)
    );
    assert_eq!(
        connections
            .submit_job(
                owner,
                handle,
                JobOptions::new("INSERT INTO parent VALUES (1)")
            )
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
}
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j1_a_crash_and_restart_restores_every_unknown_with_unchanged_facts() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open_journaled(&fixture).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let mut options = JobOptions::new("INSERT INTO parent VALUES (61)");
    options.lease = Some(lease);
    let (_, update) = run(&connections, owner, handle, options).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    let state = &connections.0.execution;
    let lost = LeaseId(Uuid::new_v4());
    let sent = TransactionEnd {
        commit_sent: true,
        ..TransactionEnd::unknown(EndReason::Requested)
    };
    assert!(state.record_end(lost, owner, sqlite_origin(&fixture), sent));
    let job = JobId(Uuid::new_v4());
    state.record_job(job, owner, sqlite_origin(&fixture), job_facts());
    let before = connections.unresolved_outcomes(owner).outcomes;
    assert_eq!(listed(state, owner), [lost.opaque(), job.opaque()]);
    drop(connections);

    let (restarted, next, _) = open(&fixture, Access::ReadWrite).await;
    assert_eq!(
        restarted.open_outcome_journal(&journal_dir(&fixture), next),
        None
    );
    let after = restarted.unresolved_outcomes(next);
    assert_eq!(after.journal, None);
    let ids: Vec<_> = after
        .outcomes
        .iter()
        .map(|o| o.outcome_id.clone())
        .collect();
    assert_eq!(ids, [lease.opaque(), lost.opaque(), job.opaque()]);
    assert!(after.outcomes.iter().all(|outcome| outcome.restored));
    assert_eq!(facts(&after.outcomes[1]), facts(&before[0]));
    assert_eq!(facts(&after.outcomes[2]), facts(&before[1]));
    let open_lease = &after.outcomes[0];
    assert!(open_lease.app_exited && !after.outcomes[1].app_exited);
    assert_eq!(
        open_lease.transaction,
        Some(TransactionEnd::unknown(EndReason::Closed).into())
    );
    assert_eq!(
        (open_lease.engine.clone(), open_lease.database.as_str()),
        (Some(EngineDto::Sqlite), "fixture.sqlite")
    );
    assert!(listed(&restarted.0.execution, OwnerId::new()).is_empty());
    // The restored lease end still answers as recorded (never a second COMMIT).
    assert_eq!(restarted.end_transaction(next, lost, true).await, Ok(sent));
}

/// J-2: acknowledgement is durable before it is reported. A failed one changes nothing (still
/// listed, now and after a restart); a successful one stays acknowledged after a restart.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j2_acknowledgement_is_journaled_before_it_is_reported() {
    let fixture = Fixture::new().await;
    let (connections, owner, _) = open_journaled(&fixture).await;
    let (first, second) = (JobId(Uuid::new_v4()), JobId(Uuid::new_v4()));
    for id in [first, second] {
        connections
            .0
            .execution
            .record_job(id, owner, "fixture.sqlite", job_facts());
    }
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    let id = OutcomeId::parse(&first.opaque()).unwrap();
    inject(&connections, full_disk());
    assert_eq!(
        connections.acknowledge_outcome(owner, id),
        Err(ExecutionError::JournalUnavailable)
    );
    assert_eq!(listed(&connections.0.execution, owner).len(), 2);
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    assert_eq!(
        listed(&connections.0.execution, owner),
        [first.opaque(), second.opaque()]
    );
    for _ in 0..2 {
        let remaining = connections.acknowledge_outcome(owner, id).unwrap();
        assert_eq!(remaining.outcomes.len(), 1);
    }
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    assert_eq!(listed(&connections.0.execution, owner), [second.opaque()]);
    // An outcome acknowledged in an earlier run no longer exists.
    assert_eq!(
        connections.acknowledge_outcome(owner, id),
        Err(ExecutionError::Forbidden)
    );
}

/// J-3: restored outcomes count toward the unresolved cap and are never evicted by the
/// 32/256 retention bounds; once acknowledged they are bounded like any other record.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j3_restored_outcomes_count_toward_the_cap_and_are_never_evicted() {
    let fixture = Fixture::new().await;
    let (connections, owner, _) = open_journaled(&fixture).await;
    let ids: Vec<_> = (0..ExecutionLimits::MAX_UNRESOLVED_OUTCOMES_PER_OWNER)
        .map(|_| JobId(Uuid::new_v4()))
        .collect();
    for &id in &ids {
        connections
            .0
            .execution
            .record_job(id, owner, "fixture.sqlite", job_facts());
    }
    drop(connections);
    let (connections, owner, handle) = open_journaled(&fixture).await;
    assert_eq!(
        connections
            .submit_job(
                owner,
                handle,
                JobOptions::new("INSERT INTO parent VALUES (62)")
            )
            .unwrap_err(),
        ExecutionError::UnresolvedOutcomes
    );
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::UnresolvedOutcomes
    );
    let state = &connections.0.execution;
    let resolved = TransactionEnd {
        outcome: Outcome::RolledBack,
        ..TransactionEnd::unknown(EndReason::Requested)
    };
    for _ in 0..2 * ExecutionLimits::MAX_ENDED_LEASES {
        state.record_end(LeaseId(Uuid::new_v4()), owner, "db", resolved);
    }
    assert_eq!(
        listed(state, owner),
        ids.iter().map(|id| id.opaque()).collect::<Vec<_>>()
    );
    assert_eq!(
        state.ended.lock().unwrap().len(),
        ids.len() + ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER
    );
    connections
        .acknowledge_outcome(owner, OutcomeId::parse(&ids[0].opaque()).unwrap())
        .unwrap();
    for _ in 0..ExecutionLimits::MAX_ENDED_LEASES_PER_OWNER {
        state.record_end(LeaseId(Uuid::new_v4()), owner, "db", resolved);
    }
    assert!(state.recorded(LeaseId(ids[0].0), owner).is_none());
    assert!(
        !state
            .ended
            .lock()
            .unwrap()
            .iter()
            .any(|record| record.id == ids[0].0),
        "acknowledged restored outcome evicted like a resolved one"
    );
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO parent VALUES (62)"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    // Resolved work leaves nothing in the journal.
    assert_eq!(journal_entries(&fixture).len(), ids.len() - 1);
}

/// J-4: a journal that cannot be used refuses new jobs and transactions before anything is
/// sent; read-only queries are unaffected. A failed write blocks admission until the
/// snapshot can be written again.
#[cfg(unix)]
#[tokio::test]
async fn j4_an_unusable_journal_refuses_new_work_before_anything_is_sent() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new().await;
    // Not opened yet, then insecure (group-accessible directory).
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    connections.require_outcome_journal();
    let refused = |connections: &Connections| {
        connections
            .submit_job(
                owner,
                handle,
                JobOptions::new("INSERT INTO parent VALUES (63)"),
            )
            .unwrap_err()
    };
    assert_eq!(refused(&connections), ExecutionError::JournalUnavailable);
    let dir = journal_dir(&fixture);
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert_eq!(
        connections.open_outcome_journal(&dir, owner),
        Some(JournalProblemDto::Insecure)
    );
    assert_eq!(refused(&connections), ExecutionError::JournalUnavailable);
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(
        connections.unresolved_outcomes(owner).journal,
        Some(JournalProblemDto::Insecure)
    );
    let limits = crate::QueryLimits {
        rows: 10,
        bytes: 4096,
        deadline: Duration::from_secs(3),
    };
    assert!(
        connections
            .read_only(owner, handle, "SELECT 1", limits)
            .await
            .is_ok()
    );
    assert_eq!(
        (connections.retained_jobs(), connections.open_transactions()),
        (0, 0)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 63")
            .await,
        0
    );
    drop(connections);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    // A write failure (full disk) at begin: nothing opened; then admission refused until the
    // snapshot can be written again.
    let (connections, owner, handle) = open_journaled(&fixture).await;
    inject(&connections, full_disk());
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(connections.open_transactions(), 0);
    assert_eq!(
        connections.unresolved_outcomes(owner).journal,
        Some(JournalProblemDto::WriteFailed)
    );
    assert_eq!(
        connections
            .submit_job(
                owner,
                handle,
                JobOptions::new("INSERT INTO parent VALUES (63)")
            )
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 63")
            .await,
        0
    );
    inject(&connections, None);
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO parent VALUES (63)"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    assert_eq!(connections.unresolved_outcomes(owner).journal, None);
}

/// J-5: COMMIT is never sent unless its potential unknown is journaled first. With the journal
/// failing, COMMIT is refused while the lease stays open (rollback still works); if the write
/// fails only at the COMMIT itself, the lease is rolled back instead and nothing is committed.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j5_commit_is_sent_only_after_its_unknown_is_journaled() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open_journaled(&fixture).await;
    let insert = |lease, id: u32| {
        let mut options = JobOptions::new(format!("INSERT INTO parent VALUES ({id})"));
        options.lease = Some(lease);
        options
    };
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let (_, update) = run(&connections, owner, handle, insert(lease, 64)).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    inject(&connections, full_disk());
    // The refused job inside the lease records the failure; the lease is untouched.
    assert_eq!(
        connections
            .submit_job(owner, handle, insert(lease, 65))
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(
        connections.end_transaction(owner, lease, true).await,
        Err(ExecutionError::JournalUnavailable)
    );
    assert_eq!(connections.open_transactions(), 1);
    let end = connections
        .end_transaction(owner, lease, false)
        .await
        .unwrap();
    assert_eq!((end.outcome, end.commit_sent), (Outcome::RolledBack, false));
    // Once the journal can be written again, COMMIT on the still-open lease works without any
    // other work running first (the refusal retries the write).
    inject(&connections, None);
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let (_, update) = run(&connections, owner, handle, insert(lease, 69)).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    inject(&connections, full_disk());
    assert_eq!(
        connections
            .submit_job(owner, handle, insert(lease, 70))
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(
        connections.end_transaction(owner, lease, true).await,
        Err(ExecutionError::JournalUnavailable)
    );
    inject(&connections, None);
    let end = connections
        .end_transaction(owner, lease, true)
        .await
        .unwrap();
    assert_eq!(end.outcome, Outcome::Committed);
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id IN (69, 70)")
            .await,
        1
    );
    // The write fails only once COMMIT is about to be sent.
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let (_, update) = run(&connections, owner, handle, insert(lease, 66)).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    inject(&connections, full_disk());
    let end = connections
        .end_transaction(owner, lease, true)
        .await
        .unwrap();
    assert_eq!(
        end,
        TransactionEnd {
            outcome: Outcome::RolledBack,
            reason: EndReason::Requested,
            commit_sent: false,
            rollback_sent: true,
        }
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id IN (64, 65, 66)")
            .await,
        0
    );

    // Ordering: when COMMIT is about to be sent its unknown (commitSent) is already on disk.
    inject(&connections, None);
    let state = &connections.0.execution;
    let id = LeaseId(Uuid::new_v4());
    let mut physical = Target::Sqlite(fixture.file()).open().await.unwrap();
    physical.begin().await.unwrap();
    let on_disk = Mutex::new(Vec::new());
    physical
        .conclude(true, EndReason::Requested, false, |end| {
            let durable = state.record_end(id, owner, sqlite_origin(&fixture), end);
            let entry = journal_entries(&fixture)
                .into_iter()
                .find(|entry| entry.id == id.opaque());
            on_disk
                .lock()
                .unwrap()
                .push(entry.and_then(|entry| entry.transaction));
            durable
        })
        .await;
    let sent = TransactionEnd {
        commit_sent: true,
        ..TransactionEnd::unknown(EndReason::Requested)
    };
    assert_eq!(*on_disk.lock().unwrap(), [Some(sent.into()), None]);
}

/// J-6: each statement of a job is journaled before it is sent; when that write fails the job
/// stops and the next statement is never sent.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j6_a_statement_is_never_sent_unless_journaled() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open_journaled(&fixture).await;
    let mut blocker = fixture.observer().await;
    blocker.execute("BEGIN IMMEDIATE").await.unwrap();
    let (id, _) = connections
        .submit_job(
            owner,
            handle,
            JobOptions::new("INSERT INTO parent VALUES (67); INSERT INTO parent VALUES (68)"),
        )
        .unwrap();
    let pending = journal_entries(&fixture);
    assert_eq!(pending.len(), 1);
    assert_eq!(
        (pending[0].id.as_str(), pending[0].in_flight),
        (id.opaque().as_str(), true)
    );
    assert_eq!(pending[0].job.as_ref().unwrap().unknown_statement, Some(0));
    inject(&connections, full_disk());
    blocker.execute("ROLLBACK").await.unwrap();
    let (_, update) = drain(&connections, owner, id).await;
    assert_eq!(
        statuses(&update),
        [StatementStatusDto::Succeeded, StatementStatusDto::Failed]
    );
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Failed));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 67")
            .await,
        1
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 68")
            .await,
        0
    );
}

/// J-7: the journal never holds SQL, parameters, values, paths or credentials: real jobs and
/// a lease with planted canaries (succeeded, unknown after cancel, in flight at a crash).
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j7_the_journal_holds_no_sql_values_or_paths() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open_journaled(&fixture).await;
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let mut options = JobOptions::new(
        "INSERT INTO item(id, parent, email, note) VALUES (901, 1, 'canary-email@example.test', 'CANARY_VALUE_7f3')",
    );
    options.lease = Some(lease);
    let (_, update) = run(&connections, owner, handle, options).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    let end = connections
        .end_transaction(owner, lease, true)
        .await
        .unwrap();
    assert_eq!(end.outcome, Outcome::Committed);
    let (id, _) = connections
        .submit_job(
            owner,
            handle,
            JobOptions::new(format!(
                "{SLOW} /* CANARY_SQL_COMMENT */; UPDATE item SET note = 'CANARY_UPDATE_9q'"
            )),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    connections.cancel_job(owner, id).unwrap();
    let (_, update) = drain(&connections, owner, id).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Unknown));
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    let mut options = JobOptions::new("DELETE FROM item WHERE email = 'canary-email@example.test'");
    options.lease = Some(lease);
    run(&connections, owner, handle, options).await;
    // "Crash" with that lease open: its in-flight record stays.
    let entries = journal_entries(&fixture);
    assert_eq!(entries.len(), 2, "{entries:?}");
    let mut bytes = Vec::new();
    for file in std::fs::read_dir(journal_dir(&fixture)).unwrap() {
        bytes.extend(std::fs::read(file.unwrap().path()).unwrap());
    }
    let text = String::from_utf8_lossy(&bytes).to_lowercase();
    for canary in [
        "canary",
        "insert",
        "update",
        "delete",
        "select",
        "recursive",
        "item",
        "note",
        "email",
        "example.test",
        &fixture.0.to_string_lossy().to_lowercase(),
    ] {
        assert!(!text.contains(canary), "journal contains {canary:?}");
    }
    assert!(text.contains("fixture.sqlite"));
    drop(connections);
}

/// J-8: a damaged journal is preserved and reported through the registry, and new jobs and
/// transactions stay refused until the user acknowledges the damage (the fresh journal itself
/// is writable, so only this check refuses them). The report survives a restart until then.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j8_a_damaged_journal_refuses_new_work_until_acknowledged() {
    let fixture = Fixture::new().await;
    let (connections, owner, _) = open_journaled(&fixture).await;
    let (first, second) = (JobId(Uuid::new_v4()), JobId(Uuid::new_v4()));
    for id in [first, second] {
        connections
            .0
            .execution
            .record_job(id, owner, "fixture.sqlite", job_facts());
    }
    drop(connections);
    let file = journal_dir(&fixture).join(journal::FILE);
    let mut bytes = std::fs::read(&file).unwrap();
    let last = bytes.iter().rposition(|&b| b == b'{').unwrap();
    bytes[last + 2] ^= 0x04;
    std::fs::write(&file, &bytes).unwrap();

    let (connections, owner, handle) = open_journaled(&fixture).await;
    let reported = connections.unresolved_outcomes(owner);
    assert_eq!(reported.journal, None, "the fresh journal is writable");
    let damage = reported
        .outcomes
        .iter()
        .find(|outcome| outcome.journal_damage.is_some())
        .unwrap()
        .clone();
    let report = damage.journal_damage.clone().unwrap();
    assert_eq!(report.unreadable_records, Some(1));
    let preserved = journal_dir(&fixture).join(report.preserved_as.clone().unwrap());
    assert_eq!(std::fs::read(&preserved).unwrap(), bytes);
    assert_eq!(
        reported.outcomes.len(),
        2,
        "one record recovered plus the report"
    );
    let ids: Vec<_> = reported
        .outcomes
        .iter()
        .map(|o| o.outcome_id.clone())
        .collect();
    assert_eq!(
        connections
            .submit_job(
                owner,
                handle,
                JobOptions::new("INSERT INTO parent VALUES (71)")
            )
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    assert_eq!(
        (connections.retained_jobs(), connections.open_transactions()),
        (0, 0)
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 71")
            .await,
        0
    );
    drop(connections);
    // Still reported after a restart, and the work still refused.
    let (connections, owner, handle) = open_journaled(&fixture).await;
    assert_eq!(listed(&connections.0.execution, owner), ids);
    assert_eq!(
        connections
            .begin_transaction(owner, handle)
            .await
            .unwrap_err(),
        ExecutionError::JournalUnavailable
    );
    connections
        .acknowledge_outcome(owner, OutcomeId::parse(&damage.outcome_id).unwrap())
        .unwrap();
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO parent VALUES (71)"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    assert!(
        connections
            .unresolved_outcomes(owner)
            .outcomes
            .iter()
            .all(|outcome| outcome.journal_damage.is_none())
    );
    assert_eq!(
        std::fs::read(&preserved).unwrap(),
        bytes,
        "the damaged copy is kept"
    );
}

/// J-9: a crash or failed rewrite right after a damaged journal was preserved keeps the damage
/// at the journal path: every later start reports it, restores the verified record and refuses
/// new read-write work until the damage is acknowledged.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j9_damage_is_not_lost_when_the_rewrite_after_quarantine_fails() {
    let fixture = Fixture::new().await;
    let (connections, owner, _) = open_journaled(&fixture).await;
    let job = JobId(Uuid::new_v4());
    connections
        .0
        .execution
        .record_job(job, owner, "fixture.sqlite", job_facts());
    drop(connections);
    let dir = journal_dir(&fixture);
    let file = dir.join(journal::FILE);
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.truncate(bytes.len() - 4);
    std::fs::write(&file, &bytes).unwrap();
    std::fs::create_dir(dir.join("unknown-outcomes.journal.tmp")).unwrap();
    for planted in [true, false] {
        if !planted {
            std::fs::remove_dir(dir.join("unknown-outcomes.journal.tmp")).unwrap();
        }
        let (connections, owner, handle) = open_journaled_any(&fixture).await;
        let listed = connections.unresolved_outcomes(owner);
        assert!(
            listed.outcomes.iter().any(|o| o.journal_damage.is_some()),
            "planted={planted}"
        );
        assert!(listed.outcomes.iter().any(|o| o.outcome_id == job.opaque()));
        assert_eq!(
            connections
                .begin_transaction(owner, handle)
                .await
                .unwrap_err(),
            ExecutionError::JournalUnavailable
        );
        drop(connections);
    }
}
#[cfg(any(unix, windows))]
async fn open_journaled_any(fixture: &Fixture) -> (Connections, OwnerId, ConnectionId) {
    let (connections, owner, handle) = open(fixture, Access::ReadWrite).await;
    connections.require_outcome_journal();
    connections.open_outcome_journal(&journal_dir(fixture), owner);
    (connections, owner, handle)
}

/// J-10: a failed acknowledgement never becomes durable later. With the rewrite failing (a
/// directory planted at the temporary path) the acknowledgement is refused; once the plant is
/// gone another write (admission, new work) succeeds, yet after a restart the outcome is still
/// listed, unacknowledged. A real acknowledgement then persists.
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j10_a_failed_acknowledgement_never_becomes_durable_later() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open_journaled(&fixture).await;
    let job = JobId(Uuid::new_v4());
    connections
        .0
        .execution
        .record_job(job, owner, "fixture.sqlite", job_facts());
    let id = OutcomeId::parse(&job.opaque()).unwrap();
    let plant = journal_dir(&fixture).join("unknown-outcomes.journal.tmp");
    std::fs::create_dir(&plant).unwrap();
    assert_eq!(
        connections.acknowledge_outcome(owner, id),
        Err(ExecutionError::JournalUnavailable)
    );
    std::fs::remove_dir(&plant).unwrap();
    let (_, update) = run(
        &connections,
        owner,
        handle,
        JobOptions::new("INSERT INTO parent VALUES (72)"),
    )
    .await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    assert_eq!(listed(&connections.0.execution, owner), [job.opaque()]);
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    assert_eq!(listed(&connections.0.execution, owner), [job.opaque()]);
    connections.acknowledge_outcome(owner, id).unwrap();
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    assert!(listed(&connections.0.execution, owner).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Journal cost measurement (not a pass/fail test). Rerun:
//   cargo test --locked -p stoneql-runtime --lib perf_journal -- --ignored --nocapture

#[cfg(any(unix, windows))]
fn summary(name: &str, samples: &mut [Duration]) {
    samples.sort();
    let at = |q: f64| samples[((samples.len() as f64 * q).ceil() as usize).saturating_sub(1)];
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "{name}: n={} p50={:.2} ms p95={:.2} ms max={:.2} ms",
        samples.len(),
        ms(at(0.5)),
        ms(at(0.95)),
        ms(*samples.last().unwrap())
    );
}

#[cfg(any(unix, windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "measurement; run with --ignored --nocapture"]
async fn perf_journal_writes_admission_and_listing() {
    let fixture = Fixture::new().await;
    // 1. Snapshot write latency at 1, 16 and the full per-owner bound (16 + 2 leases + 8 jobs).
    for size in [1usize, 16, 26] {
        let dir = fixture.0.join(format!("perf-{size}"));
        let mut journal = Journal::open(&dir);
        let origin = sqlite_origin(&fixture);
        let entry = |id: Uuid, index: u32| Entry {
            id: id.to_string(),
            recorded_at: journal::now(),
            engine: origin.engine.clone(),
            database: origin.database.clone(),
            in_flight: true,
            transaction: None,
            job: Some(UnresolvedJob {
                unknown_statement: Some(index),
                statement_count: 100,
                ..job_facts()
            }),
            damage: None,
        };
        for _ in 1..size {
            assert!(journal.put(entry(Uuid::new_v4(), 0)));
        }
        let id = Uuid::new_v4();
        let mut samples = Vec::new();
        for index in 0..200 {
            let started = std::time::Instant::now();
            assert!(journal.put(entry(id, index)));
            samples.push(started.elapsed());
        }
        let bytes = std::fs::metadata(dir.join(journal::FILE)).unwrap().len();
        summary(
            &format!("journal write, {size} records ({bytes} bytes)"),
            &mut samples,
        );
    }

    // 2. Admission (submit_job, which journals statement 0) with 8 concurrent submitters,
    //    each on its own owner and handle, with and without a journal.
    for journaled in [false, true] {
        let connections =
            Connections::new(16, Duration::from_secs(3), Duration::from_secs(600)).unwrap();
        if journaled {
            connections.require_outcome_journal();
            assert_eq!(
                connections.open_outcome_journal(&fixture.0.join("perf-admit"), OwnerId::new()),
                None
            );
        }
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let connections = connections.clone();
            let file = fixture.file();
            tasks.push(tokio::spawn(async move {
                let owner = OwnerId::new();
                let grant = connections.grant_sqlite_file(owner, &file).unwrap();
                let handle = connections
                    .connect_sqlite_with_access(owner, &grant, Access::ReadWrite)
                    .await
                    .unwrap();
                let mut samples = Vec::new();
                for _ in 0..25 {
                    let started = std::time::Instant::now();
                    let (id, _) = connections
                        .submit_job(owner, handle, JobOptions::new("SELECT 1"))
                        .unwrap();
                    samples.push(started.elapsed());
                    drain(&connections, owner, id).await;
                }
                samples
            }));
        }
        let mut samples = Vec::new();
        for task in tasks {
            samples.extend(task.await.unwrap());
        }
        summary(
            &format!(
                "submit_job admission, 8 concurrent submitters, {}",
                if journaled {
                    "journal"
                } else {
                    "no journal (baseline)"
                }
            ),
            &mut samples,
        );
    }

    // 3. Listing and acknowledging while another thread keeps writing the journal.
    let (connections, owner, _) = {
        let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
        connections.require_outcome_journal();
        connections.open_outcome_journal(&fixture.0.join("perf-list"), owner);
        (connections, owner, handle)
    };
    let state = &connections.0.execution;
    let ids: Vec<_> = (0..16).map(|_| JobId(Uuid::new_v4())).collect();
    for &id in &ids {
        state.record_job(id, owner, "fixture.sqlite", job_facts());
    }
    let mut idle = Vec::new();
    for _ in 0..200 {
        let started = std::time::Instant::now();
        std::hint::black_box(connections.unresolved_outcomes(owner));
        idle.push(started.elapsed());
    }
    summary("list_unresolved_outcomes, idle", &mut idle);
    let writer = {
        let connections = connections.clone();
        std::thread::spawn(move || {
            let origin = Origin::from("fixture.sqlite");
            for _ in 0..200 {
                connections
                    .0
                    .execution
                    .journal_job(JobId(Uuid::new_v4()), &origin, 1, 0, false);
            }
        })
    };
    let mut busy = Vec::new();
    while !writer.is_finished() {
        let started = std::time::Instant::now();
        std::hint::black_box(connections.unresolved_outcomes(owner));
        busy.push(started.elapsed());
        std::thread::sleep(Duration::from_millis(1));
    }
    writer.join().unwrap();
    summary("list_unresolved_outcomes, during journal writes", &mut busy);
    let mut acks = Vec::new();
    for id in ids {
        let started = std::time::Instant::now();
        connections
            .acknowledge_outcome(owner, OutcomeId::parse(&id.opaque()).unwrap())
            .unwrap();
        acks.push(started.elapsed());
    }
    summary("acknowledge_outcome (durable removal)", &mut acks);
}

/// J-11: removing the record of work whose outcome is now known (a job that succeeded, a lease
/// confirmed rolled back) is not undone by a failed write: it completes with the next
/// successful write, so a restart never restores resolved work as an unknown outcome (nor
/// counts it toward the cap). Acknowledgements keep the stricter rule (J-10).
#[cfg(any(unix, windows))]
#[tokio::test]
async fn j11_a_known_outcome_removal_completes_with_the_next_successful_write() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open_journaled(&fixture).await;
    let plant = journal_dir(&fixture).join("unknown-outcomes.journal.tmp");
    // A job journaled while writes work, finishing while they fail.
    let mut blocker = fixture.observer().await;
    blocker.execute("BEGIN IMMEDIATE").await.unwrap();
    let (job, _) = connections
        .submit_job(
            owner,
            handle,
            JobOptions::new("INSERT INTO parent VALUES (73)"),
        )
        .unwrap();
    std::fs::create_dir(&plant).unwrap();
    blocker.execute("ROLLBACK").await.unwrap();
    let (_, update) = drain(&connections, owner, job).await;
    assert_eq!(update.outcome, Some(ExecutionOutcomeDto::Succeeded));
    // A lease journaled while writes work, rolled back (confirmed) while they fail.
    std::fs::remove_dir(&plant).unwrap();
    let lease = connections.begin_transaction(owner, handle).await.unwrap();
    std::fs::create_dir(&plant).unwrap();
    let end = connections
        .end_transaction(owner, lease, false)
        .await
        .unwrap();
    assert_eq!(end.outcome, Outcome::RolledBack);
    assert_eq!(
        connections.unresolved_outcomes(owner).journal,
        Some(JournalProblemDto::WriteFailed)
    );
    std::fs::remove_dir(&plant).unwrap();
    // An unrelated successful write.
    let other = JobId(Uuid::new_v4());
    connections
        .0
        .execution
        .record_job(other, owner, "fixture.sqlite", job_facts());
    drop(connections);
    let (connections, owner, _) = open_journaled(&fixture).await;
    assert_eq!(listed(&connections.0.execution, owner), [other.opaque()]);
}

async fn import_done(connections: &Connections, owner: OwnerId, id: JobId) -> SqlImportStatus {
    for _ in 0..600 {
        let status = connections.sql_import_status(owner, id).unwrap();
        if status.done {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("import did not finish");
}

#[tokio::test]
async fn sql_import_runs_the_validated_snapshot_and_stops_at_the_first_failure() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    // A statement returning rows is driven to its end and discarded; nothing stalls. (It runs
    // first: SQLite reports the previous statement's change count for a SELECT.)
    let (id, count) = connections
        .start_sql_import(
            owner,
            handle,
            b"SELECT * FROM parent;\nINSERT INTO parent VALUES (2);\nINSERT INTO parent VALUES (3), (4);"
                .to_vec(),
        )
        .unwrap();
    assert_eq!(count, 3);
    assert!(connections.has_active_work(owner));
    let status = import_done(&connections, owner, id).await;
    assert_eq!(status.outcome, Some(ExecutionOutcomeDto::Succeeded));
    assert_eq!((status.completed, status.affected_rows.as_str()), (3, "3"));
    assert!(status.failed.is_none() && status.unknown.is_none());
    // The final status released the record.
    assert_eq!(
        connections.sql_import_status(owner, id).unwrap_err(),
        ExecutionError::Forbidden
    );
    assert!(!connections.has_active_work(owner));
    // Statement 2 (line 2) fails: statement 1 stays committed (autocommit), 3 is not run.
    let (id, _) = connections
        .start_sql_import(
            owner,
            handle,
            b"INSERT INTO parent VALUES (10);\nINSERT INTO parent VALUES (1);\nINSERT INTO parent VALUES (11);"
                .to_vec(),
        )
        .unwrap();
    let status = import_done(&connections, owner, id).await;
    assert_eq!(status.outcome, Some(ExecutionOutcomeDto::Failed));
    assert_eq!(status.completed, 1);
    let failed = status.failed.unwrap();
    assert_eq!((failed.index, failed.line), (1, 2));
    assert!(failed.failure.is_some());
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 10")
            .await,
        1
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 11")
            .await,
        0
    );
    // A refused statement anywhere means nothing runs.
    let refused = connections
        .start_sql_import(
            owner,
            handle,
            b"INSERT INTO parent VALUES (20);\nCOMMIT;".to_vec(),
        )
        .unwrap_err();
    let ImportError::Refused(refusal) = refused else {
        panic!("{refused:?}")
    };
    assert_eq!((refusal.statement, refusal.line), (Some(2), Some(2)));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 20")
            .await,
        0
    );
    assert!(!connections.has_active_work(owner));
    // Read-only handles never import.
    let (connections, owner, handle) = open(&fixture, Access::ReadOnly).await;
    assert_eq!(
        connections
            .start_sql_import(owner, handle, b"INSERT INTO parent VALUES (21)".to_vec())
            .unwrap_err(),
        ImportError::Execution(ExecutionError::ReadOnlyHandle)
    );
}

#[tokio::test]
async fn sql_import_cancel_reports_the_statement_in_flight_as_unknown_and_journals_it() {
    let fixture = Fixture::new().await;
    let (connections, owner, handle) = open(&fixture, Access::ReadWrite).await;
    let (id, _) = connections
        .start_sql_import(
            owner,
            handle,
            format!("INSERT INTO parent VALUES (29);\n{SLOW};\nINSERT INTO parent VALUES (30);")
                .into_bytes(),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Busy while the import runs: nothing is queued beside it.
    assert_eq!(
        connections
            .submit_job(owner, handle, JobOptions::new("SELECT 1"))
            .unwrap_err(),
        ExecutionError::Busy
    );
    assert!(connections.cancel_job(owner, id).unwrap());
    let status = import_done(&connections, owner, id).await;
    assert_eq!(status.outcome, Some(ExecutionOutcomeDto::Unknown));
    assert_eq!(status.cancellation, CancellationDto::Confirmed);
    assert_eq!(status.completed, 1);
    let unknown = status.unknown.unwrap();
    assert_eq!((unknown.index, unknown.line), (1, 2));
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 29")
            .await,
        1
    );
    assert_eq!(
        fixture
            .count("SELECT count(*) FROM parent WHERE id = 30")
            .await,
        0
    );
    let listed = connections.unresolved_outcomes(owner).outcomes;
    let job = listed
        .iter()
        .find_map(|outcome| outcome.job.clone())
        .expect("unknown import outcome listed");
    assert_eq!((job.statement_count, job.unknown_statement), (3, Some(1)));
}
