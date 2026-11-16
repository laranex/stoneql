//! Validation harness (uncommitted, test-only) for the durable unknown-outcome journal in real
//! OS processes, through the public runtime API only, against a disposable SQLite file and a
//! temporary journal directory named on the command line (never the app data directory).
//!
//! Modes (driven by scripts that SIGKILL and restart the process):
//! - `work <dir> <sqlite> <log> <seed>`: opens the journal, logs what it restored, then loops
//!   creating unknown outcomes (cancelled in-flight statements, leases left open), concluded
//!   work and acknowledgements. Every transition is appended to `<log>` with one `write(2)`
//!   before/after the call, so a SIGKILL never loses a line that was already logged.
//! - `hold <dir>`: opens the journal, prints the listing, then sleeps (holds the lock).
//! - `probe <dir> <sqlite>`: opens the journal, prints the listing and whether a transaction
//!   and a job are admitted (nothing else is written).
//! - `perf <dir|none> <sqlite-prefix> <workers> <seconds>`: realistic desktop load (each worker
//!   its own SQLite file: jobs of 1-5 statements and short transactions), listing polled every
//!   second, then an acknowledgement burst; prints latency samples as JSON.
use serde_json::{Value, json};
use sqlx::{Connection, Executor, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use stoneql_contracts::v1::{self, ExecutionOutcomeDto};
use stoneql_runtime::{
    Access, ConnectionId, Connections, ExecutionError, JobId, JobOptions, LeaseId, OutcomeId,
    OwnerId,
};

type Failure = Box<dyn std::error::Error>;

/// A long statement that only ends when interrupted (cancel → unknown outcome).
const ENDLESS: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct Log(File);
impl Log {
    fn line(&mut self, mut value: Value) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        value["t"] = json!(now.as_millis() as u64);
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        // One write(2) on an O_APPEND file: in the kernel once this returns.
        self.0.write_all(&bytes).unwrap();
    }
}

fn error(e: &ExecutionError) -> String {
    format!("{e:?}")
}

async fn create_sqlite(file: &Path) -> Result<(), Failure> {
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(file)
            .create_if_missing(true),
    )
    .await?;
    connection
        .execute("CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, v INTEGER)")
        .await?;
    connection.close().await?;
    Ok(())
}

fn registry() -> Connections {
    Connections::new(16, Duration::from_secs(3), Duration::from_secs(3600)).unwrap()
}

/// A journaled registry as the desktop host builds it: durability required, then opened.
fn journaled(dir: &Path, owner: OwnerId) -> (Connections, Option<v1::JournalProblemDto>) {
    let connections = registry();
    connections.require_outcome_journal();
    let problem = connections.open_outcome_journal(dir, owner);
    (connections, problem)
}

async fn handle(
    connections: &Connections,
    owner: OwnerId,
    file: &Path,
) -> Result<ConnectionId, Failure> {
    let grant = connections.grant_sqlite_file(owner, file)?;
    Ok(connections
        .connect_sqlite_with_access(owner, &grant, Access::ReadWrite)
        .await?)
}

/// Fetches until done; returns the final outcome.
async fn drain(
    connections: &Connections,
    owner: OwnerId,
    id: JobId,
) -> Option<ExecutionOutcomeDto> {
    loop {
        match connections
            .fetch_job(owner, id, Duration::from_millis(500))
            .await
        {
            Ok(update) if update.done => return update.outcome,
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

fn listed(connections: &Connections, owner: OwnerId, id: &str) -> Value {
    connections
        .unresolved_outcomes(owner)
        .outcomes
        .into_iter()
        .find(|outcome| outcome.outcome_id == id)
        .map(|outcome| json!(outcome))
        .unwrap_or(Value::Null)
}

/// Submits `sql` (optionally in `lease`), cancels after `cancel_after` if given, drains it.
#[allow(clippy::too_many_arguments)]
async fn job(
    log: &mut Log,
    connections: &Connections,
    owner: OwnerId,
    handle: ConnectionId,
    sql: &str,
    lease: Option<LeaseId>,
    cancel_after: Option<Duration>,
    kind: &str,
) -> Option<ExecutionOutcomeDto> {
    let mut options = JobOptions::new(sql);
    options.lease = lease;
    log.line(json!({"ev": "submit-start", "kind": kind}));
    let id = match connections.submit_job(owner, handle, options) {
        Ok((id, _)) => id,
        Err(e) => {
            log.line(json!({"ev": "submit-err", "kind": kind, "err": error(&e)}));
            return None;
        }
    };
    log.line(json!({"ev": "submitted", "kind": kind, "id": id.opaque(), "lease": lease.map(LeaseId::opaque)}));
    if let Some(after) = cancel_after {
        tokio::time::sleep(after).await;
        let _ = connections.cancel_job(owner, id);
    }
    let outcome = drain(connections, owner, id).await;
    log.line(json!({"ev": "job-done", "id": id.opaque(), "outcome": outcome}));
    if outcome == Some(ExecutionOutcomeDto::Unknown) {
        log.line(json!({"ev": "unknown", "id": id.opaque(), "outcome": listed(connections, owner, &id.opaque())}));
    }
    outcome
}

async fn end(
    log: &mut Log,
    connections: &Connections,
    owner: OwnerId,
    lease: LeaseId,
    commit: bool,
) {
    log.line(json!({"ev": "end-start", "id": lease.opaque(), "commit": commit}));
    match connections.end_transaction(owner, lease, commit).await {
        Ok(end) => {
            let end = v1::TransactionEnd::from(end);
            log.line(json!({"ev": "ended", "id": lease.opaque(), "end": end}));
            if end.outcome == v1::OutcomeDto::Unknown {
                log.line(json!({"ev": "unknown", "id": lease.opaque(), "outcome": listed(connections, owner, &lease.opaque())}));
            }
        }
        Err(e) => log.line(json!({"ev": "end-err", "id": lease.opaque(), "err": error(&e)})),
    }
}

async fn work(dir: &Path, file: &Path, log: &Path, seed: u64) -> Result<(), Failure> {
    let mut log = Log(OpenOptions::new().create(true).append(true).open(log)?);
    let mut rng = Rng(seed | 1);
    let owner = OwnerId::new();
    log.line(json!({"ev": "open-start"}));
    let (connections, problem) = journaled(dir, owner);
    let restored = connections.unresolved_outcomes(owner);
    log.line(json!({"ev": "open", "problem": problem, "listing": restored}));
    let handle = handle(&connections, owner, file).await?;
    for round in 0u64.. {
        // Acknowledgements: the damage entry first (it blocks new work), bursts near the cap,
        // otherwise now and then.
        let outcomes = connections.unresolved_outcomes(owner).outcomes;
        let burst = outcomes.len() >= 9;
        let mut picks: Vec<String> = outcomes
            .iter()
            .filter(|o| o.journal_damage.is_some())
            .map(|o| o.outcome_id.clone())
            .collect();
        if burst {
            picks.extend(outcomes.iter().take(6).map(|o| o.outcome_id.clone()));
        } else if !outcomes.is_empty() && rng.below(5) == 0 {
            let pick = rng.below(outcomes.len() as u64) as usize;
            picks.push(outcomes[pick].outcome_id.clone());
        }
        picks.dedup();
        for id in picks {
            log.line(json!({"ev": "ack-start", "id": id, "burst": burst}));
            match connections.acknowledge_outcome(owner, OutcomeId::parse(&id)?) {
                Ok(_) => log.line(json!({"ev": "ack-done", "id": id})),
                Err(e) => log.line(json!({"ev": "ack-err", "id": id, "err": error(&e)})),
            }
        }
        match rng.below(4) {
            // An in-flight statement interrupted by cancel: an unknown job.
            0 => {
                let after = Duration::from_millis(5 + rng.below(25));
                job(
                    &mut log,
                    &connections,
                    owner,
                    handle,
                    ENDLESS,
                    None,
                    Some(after),
                    "cancelled",
                )
                .await;
            }
            // A transaction: writes, sometimes an interrupted statement inside it, then commit
            // or rollback (rollback-only after an interrupt).
            1 | 2 => {
                log.line(json!({"ev": "begin-start"}));
                let lease = match connections.begin_transaction(owner, handle).await {
                    Ok(lease) => lease,
                    Err(e) => {
                        log.line(json!({"ev": "begin-err", "err": error(&e)}));
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        continue;
                    }
                };
                log.line(json!({"ev": "begun", "id": lease.opaque()}));
                let sql = format!(
                    "INSERT INTO t(v) VALUES ({round}); UPDATE t SET v = v + 1 WHERE id = (SELECT max(id) FROM t)"
                );
                let ok = job(
                    &mut log,
                    &connections,
                    owner,
                    handle,
                    &sql,
                    Some(lease),
                    None,
                    "lease-write",
                )
                .await
                    == Some(ExecutionOutcomeDto::Succeeded);
                let interrupted = rng.below(3) == 0;
                if interrupted {
                    let after = Duration::from_millis(5 + rng.below(20));
                    job(
                        &mut log,
                        &connections,
                        owner,
                        handle,
                        ENDLESS,
                        Some(lease),
                        Some(after),
                        "lease-cancelled",
                    )
                    .await;
                }
                // Leave it open for a moment now and then (a kill here leaves it in flight).
                if rng.below(3) == 0 {
                    tokio::time::sleep(Duration::from_millis(rng.below(60))).await;
                }
                end(
                    &mut log,
                    &connections,
                    owner,
                    lease,
                    ok && !interrupted && rng.below(4) != 0,
                )
                .await;
            }
            // A multi-statement script outside a transaction (one journal write per statement).
            _ => {
                let sql = (0..1 + rng.below(4))
                    .map(|i| format!("INSERT INTO t(v) VALUES ({})", round * 10 + i))
                    .collect::<Vec<_>>()
                    .join("; ");
                job(
                    &mut log,
                    &connections,
                    owner,
                    handle,
                    &sql,
                    None,
                    None,
                    "script",
                )
                .await;
            }
        }
    }
    Ok(())
}

async fn hold(dir: &Path) {
    let owner = OwnerId::new();
    let (connections, problem) = journaled(dir, owner);
    println!(
        "{}",
        json!({"mode": "hold", "pid": std::process::id(), "problem": problem, "listing": connections.unresolved_outcomes(owner)})
    );
    std::io::stdout().flush().unwrap();
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

async fn probe(dir: &Path, file: &Path) -> Result<(), Failure> {
    let owner = OwnerId::new();
    let (connections, problem) = journaled(dir, owner);
    let handle = handle(&connections, owner, file).await?;
    let begin = connections
        .begin_transaction(owner, handle)
        .await
        .map(LeaseId::opaque)
        .map_err(|e| error(&e));
    let submit = connections
        .submit_job(
            owner,
            handle,
            JobOptions::new("INSERT INTO t(v) VALUES (-1)"),
        )
        .map(|(id, _)| id.opaque())
        .map_err(|e| error(&e));
    println!(
        "{}",
        json!({"mode": "probe", "problem": problem, "listing": connections.unresolved_outcomes(owner),
               "begin": format!("{begin:?}"), "submit": format!("{submit:?}")})
    );
    Ok(())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

async fn perf(
    dir: Option<PathBuf>,
    prefix: &str,
    workers: u64,
    seconds: u64,
) -> Result<(), Failure> {
    let owner = OwnerId::new();
    let connections = match &dir {
        Some(dir) => {
            let (connections, problem) = journaled(dir, owner);
            assert!(problem.is_none(), "journal problem {problem:?}");
            connections
        }
        None => registry(),
    };
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut tasks = Vec::new();
    for worker in 0..workers {
        let connections = connections.clone();
        let file = PathBuf::from(format!("{prefix}-{worker}.sqlite"));
        create_sqlite(&file).await?;
        let handle = handle(&connections, owner, &file).await?;
        tasks.push(tokio::spawn(async move {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (worker + 1));
            let (mut submit, mut begin, mut commit, mut jobs) = (vec![], vec![], vec![], vec![]);
            while Instant::now() < deadline {
                let statements = 1 + rng.below(5);
                let sql = (0..statements)
                    .map(|i| format!("INSERT INTO t(v) VALUES ({i})"))
                    .collect::<Vec<_>>()
                    .join("; ");
                if rng.below(3) == 0 {
                    let started = Instant::now();
                    let lease = connections.begin_transaction(owner, handle).await.unwrap();
                    begin.push(ms(started.elapsed()));
                    let mut options = JobOptions::new(sql);
                    options.lease = Some(lease);
                    let started = Instant::now();
                    let (id, _) = connections.submit_job(owner, handle, options).unwrap();
                    submit.push(ms(started.elapsed()));
                    drain(&connections, owner, id).await;
                    jobs.push((statements, ms(started.elapsed())));
                    let started = Instant::now();
                    connections
                        .end_transaction(owner, lease, true)
                        .await
                        .unwrap();
                    commit.push(ms(started.elapsed()));
                } else {
                    let started = Instant::now();
                    let (id, _) = connections
                        .submit_job(owner, handle, JobOptions::new(sql))
                        .unwrap();
                    submit.push(ms(started.elapsed()));
                    drain(&connections, owner, id).await;
                    jobs.push((statements, ms(started.elapsed())));
                }
                // Think time between user actions (UI round trip), 0-20 ms.
                tokio::time::sleep(Duration::from_millis(rng.below(20))).await;
            }
            json!({"submit": submit, "begin": begin, "commit": commit, "jobs": jobs})
        }));
    }
    // The renderer lists every second (while jobs are pending); poll exactly so.
    let mut listing = Vec::new();
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let started = Instant::now();
        std::hint::black_box(connections.unresolved_outcomes(owner));
        listing.push(ms(started.elapsed()));
    }
    // Tighter sampling (every 50 ms) for distribution only, in a second phase below.
    let mut results = Vec::new();
    for task in tasks {
        results.push(task.await?);
    }
    // Acknowledgement burst: 15 unknown outcomes (cancelled in-flight statements), then all
    // acknowledged back to back while the listing is polled every 50 ms.
    let file = PathBuf::from(format!("{prefix}-ack.sqlite"));
    create_sqlite(&file).await?;
    let ack_handle = handle(&connections, owner, &file).await?;
    let mut ids = Vec::new();
    for _ in 0..15 {
        let (id, _) = connections.submit_job(owner, ack_handle, JobOptions::new(ENDLESS))?;
        tokio::time::sleep(Duration::from_millis(10)).await;
        connections.cancel_job(owner, id)?;
        drain(&connections, owner, id).await;
        ids.push(id.opaque());
    }
    let mut acks = Vec::new();
    for id in &ids {
        let started = Instant::now();
        connections.acknowledge_outcome(owner, OutcomeId::parse(id)?)?;
        acks.push(ms(started.elapsed()));
    }
    println!(
        "{}",
        json!({"journal": dir.is_some(), "workers": workers, "seconds": seconds,
               "listing1s": listing, "acks": acks, "workers_samples": results})
    );
    connections.shutdown().await;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("work") => {
            let file = PathBuf::from(&args[2]);
            create_sqlite(&file).await?;
            work(
                Path::new(&args[1]),
                &file,
                Path::new(&args[3]),
                args[4].parse()?,
            )
            .await
        }
        Some("hold") => {
            hold(Path::new(&args[1])).await;
            Ok(())
        }
        Some("probe") => {
            let file = PathBuf::from(&args[2]);
            create_sqlite(&file).await?;
            probe(Path::new(&args[1]), &file).await
        }
        Some("perf") => {
            let dir = (args[1] != "none").then(|| PathBuf::from(&args[1]));
            perf(dir, &args[2], args[3].parse()?, args[4].parse()?).await
        }
        _ => Err("usage: journal_crash work|hold|probe|perf ...".into()),
    }
}
