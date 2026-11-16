//! SQL import (M007 d5; A20 approved 2026-11-16 for MySQL/MariaDB, which the host enforces).
//!
//! The file's bytes are read once by the host and handed over. The whole text is validated
//! first: UTF-8, size, every statement split with the dialect's splitter and checked against the
//! read-write script policy plus the import refusals (`USE`, `LOAD DATA/XML LOCAL`). If any
//! statement is refused, nothing runs. Otherwise the import runs from that same validated
//! snapshot (the same buffer and statement ranges), one statement at a time under autocommit on
//! its own never-pooled connection, as a job record: cancel, release, reload protection and the
//! durable unknown-outcome journal apply exactly as for jobs. It stops at the first failure.
//! Result sets are driven to their end and discarded.
use super::{
    Admitted, Classified, ConnectionId, Connections, ExecutionError, ExecutionLimits,
    ExecutionOutcome, Inner, Job, JobId, Origin, OwnerId, Physical, Reason, Session, Sink,
    StatementFailure, StatementStatusDto, Target, UnresolvedJob, cancellation, classify, finish,
    guarded, statement,
};
use crate::{
    read_only::Dialect,
    script::{self, ScriptError, Statement},
};
use std::{
    ops::Range,
    sync::{Arc, Weak},
    time::Duration,
};
use stoneql_contracts::v1::{ImportStatementDto, JobStateDto, SqlImportStatus};
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportRefusalReason {
    TooLarge,
    NotUtf8,
    Empty,
    TooManyStatements,
    StatementTooLarge,
    /// `USE` would silently move later statements to another database.
    Use,
    /// `LOAD DATA/XML LOCAL` asks the client to send one of its files.
    LocalFile,
    Script(ScriptError),
}
/// Why the file was refused before anything ran. `statement` (1-based) and `line` (1-based)
/// locate the refused statement when there is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportRefusal {
    pub reason: ImportRefusalReason,
    pub statement: Option<u32>,
    pub line: Option<u32>,
}
impl std::fmt::Display for ImportRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let (Some(statement), Some(line)) = (self.statement, self.line) {
            write!(f, "Statement {statement} (line {line}): ")?;
        }
        match self.reason {
            ImportRefusalReason::TooLarge => write!(
                f,
                "The file exceeds the {} MiB import limit.",
                ExecutionLimits::MAX_IMPORT_BYTES / (1024 * 1024)
            ),
            ImportRefusalReason::NotUtf8 => f.write_str("The file is not valid UTF-8 text."),
            ImportRefusalReason::Empty => f.write_str("The file contains no statements."),
            ImportRefusalReason::TooManyStatements => write!(
                f,
                "The file exceeds the {} statement import limit.",
                ExecutionLimits::MAX_IMPORT_STATEMENTS
            ),
            ImportRefusalReason::StatementTooLarge => write!(
                f,
                "The statement exceeds the {} MiB statement limit.",
                ExecutionLimits::MAX_IMPORT_STATEMENT_BYTES / (1024 * 1024)
            ),
            ImportRefusalReason::Use => f.write_str(
                "USE is not accepted in an import; it would move the following statements to another database.",
            ),
            ImportRefusalReason::LocalFile => f.write_str(
                "LOAD DATA/XML LOCAL is not accepted: it reads a file from this computer.",
            ),
            ImportRefusalReason::Script(error) => write!(f, "{error}"),
        }?;
        f.write_str(" Nothing was run.")
    }
}
impl std::error::Error for ImportRefusal {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Refused(ImportRefusal),
    Execution(ExecutionError),
}
impl From<ExecutionError> for ImportError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}

/// The validated file: the bytes as read, and the range of every statement in it.
pub(super) struct ImportSnapshot {
    /// The bytes as read, owned once (no copy: the snapshot is never shared).
    text: String,
    spans: Vec<Range<usize>>,
}
impl ImportSnapshot {
    fn line(&self, offset: usize) -> u32 {
        line(&self.text, offset)
    }
}
fn line(text: &str, offset: usize) -> u32 {
    let lines = text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    u32::try_from(lines + 1).unwrap_or(u32::MAX)
}

fn refusal(reason: ImportRefusalReason) -> ImportRefusal {
    ImportRefusal {
        reason,
        statement: None,
        line: None,
    }
}
fn import_policy(statement: &Statement, dialect: Dialect) -> Result<(), ImportRefusalReason> {
    script::check_one(statement, dialect, false).map_err(ImportRefusalReason::Script)?;
    let words = &statement.words;
    let first = words.first().map_or("", String::as_str);
    if first == "USE" {
        return Err(ImportRefusalReason::Use);
    }
    // LOAD DATA [LOW_PRIORITY | CONCURRENT] LOCAL INFILE …, LOAD XML … LOCAL INFILE …
    if first == "LOAD" && words.iter().take(4).any(|word| word == "LOCAL") {
        return Err(ImportRefusalReason::LocalFile);
    }
    Ok(())
}

/// Validates the whole file. Statements are checked as the splitter yields them, so only one
/// statement's words are held at a time; the snapshot keeps the text and the ranges.
pub(super) fn validate(bytes: Vec<u8>, dialect: Dialect) -> Result<ImportSnapshot, ImportRefusal> {
    if bytes.len() > ExecutionLimits::MAX_IMPORT_BYTES {
        return Err(refusal(ImportRefusalReason::TooLarge));
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) => {
            let valid = error.utf8_error().valid_up_to();
            let lines = error.as_bytes()[..valid]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count();
            return Err(ImportRefusal {
                reason: ImportRefusalReason::NotUtf8,
                statement: None,
                line: Some(u32::try_from(lines + 1).unwrap_or(u32::MAX)),
            });
        }
    };
    // A UTF-8 byte order mark is not part of the first statement.
    let body = if text.starts_with('\u{feff}') { 3 } else { 0 };
    let mut spans: Vec<Range<usize>> = Vec::new();
    let mut refused: Option<(usize, ImportRefusalReason)> = None;
    let split = script::split_each(&text[body..], dialect, &mut |statement| {
        let span = statement.span.start + body..statement.span.end + body;
        let reason = if span.len() > ExecutionLimits::MAX_IMPORT_STATEMENT_BYTES {
            ImportRefusalReason::StatementTooLarge
        } else if spans.len() >= ExecutionLimits::MAX_IMPORT_STATEMENTS {
            ImportRefusalReason::TooManyStatements
        } else if let Err(reason) = import_policy(&statement, dialect) {
            reason
        } else {
            spans.push(span);
            return Ok(());
        };
        refused = Some((span.start, reason));
        // Stops the splitter; the refusal recorded above is what is reported.
        Err(ScriptError::Empty)
    });
    let failed = match (refused, split) {
        (Some(refused), _) => Some(refused),
        (None, Err((offset, error))) => {
            // Locate the statement by its first non-blank character.
            let rest = &text[body + offset..];
            let blank = rest.len() - rest.trim_start().len();
            Some((body + offset + blank, ImportRefusalReason::Script(error)))
        }
        (None, Ok(())) => None,
    };
    if let Some((offset, reason)) = failed {
        return Err(ImportRefusal {
            reason,
            statement: Some(u32::try_from(spans.len() + 1).unwrap_or(u32::MAX)),
            line: Some(line(&text, offset)),
        });
    }
    if spans.is_empty() {
        return Err(refusal(ImportRefusalReason::Empty));
    }
    Ok(ImportSnapshot { text, spans })
}

#[derive(Clone)]
pub(super) struct ImportStatement {
    index: u32,
    line: u32,
    failure: Option<StatementFailure>,
}
impl From<&ImportStatement> for ImportStatementDto {
    fn from(statement: &ImportStatement) -> Self {
        Self {
            index: statement.index,
            line: statement.line,
            failure: statement.failure.clone(),
        }
    }
}
pub(super) struct ImportProgress {
    total: u32,
    completed: u32,
    affected: u64,
    failed: Option<ImportStatement>,
    unknown: Option<ImportStatement>,
}

struct ImportPlan {
    id: JobId,
    job: Arc<Job>,
    snapshot: ImportSnapshot,
    target: Target,
    origin: Origin,
    /// The handle's exclusive work permit, released before the record's page channel closes.
    _permit: Option<OwnedSemaphorePermit>,
    connect_deadline: Duration,
    sink: Sink,
    registry: Weak<Inner>,
}

impl Connections {
    /// Validates the whole file and, only if every statement passes, starts running it from the
    /// same snapshot. Returns the import's job id and statement count. CPU-bound for large
    /// files: hosts call it off their async executor.
    pub fn start_sql_import(
        &self,
        owner: OwnerId,
        handle: ConnectionId,
        bytes: Vec<u8>,
    ) -> Result<(JobId, u32), ImportError> {
        let dialect = self.writable(owner, handle)?.0.dialect();
        let snapshot = validate(bytes, dialect).map_err(ImportError::Refused)?;
        let Admitted {
            pool,
            engine,
            permit,
            ..
        } = self.admit_work(owner, handle, None)?;
        let total = snapshot.spans.len();
        let count = u32::try_from(total).unwrap_or(u32::MAX);
        let (tx, rx) = mpsc::channel(1);
        let job = Job::new(
            owner,
            handle,
            ExecutionLimits::IMPORT_DEADLINE,
            Vec::new(),
            None,
            Some(ImportProgress {
                total: count,
                completed: 0,
                affected: 0,
                failed: None,
                unknown: None,
            }),
        );
        let id = JobId(Uuid::new_v4());
        let target = Target::of(&pool);
        let origin = Origin::of(engine, &target);
        self.register_job(owner, handle, id, &job, &origin, total, false, rx)?;
        let mut sink = Sink::new(tx, 1);
        sink.discard = true;
        tokio::spawn(run_import(ImportPlan {
            id,
            job,
            snapshot,
            target,
            origin,
            _permit: permit,
            connect_deadline: self.0.deadline,
            sink,
            registry: Arc::downgrade(&self.0),
        }));
        Ok((id, count))
    }

    /// The import's progress. Once it has finished, this final status is delivered with
    /// `done` and the record is released (until then it counts as active work).
    pub fn sql_import_status(
        &self,
        owner: OwnerId,
        id: JobId,
    ) -> Result<SqlImportStatus, ExecutionError> {
        let execution = &self.0.execution;
        let job = execution
            .jobs
            .lock()
            .expect("job lock")
            .get(&id)
            .filter(|entry| entry.job.owner == owner)
            .map(|entry| entry.job.clone())
            .ok_or(ExecutionError::Forbidden)?;
        let reason = job.control.reason();
        let status = {
            let progress = job.progress.lock().expect("job progress lock");
            let import = progress.import.as_ref().ok_or(ExecutionError::Forbidden)?;
            SqlImportStatus {
                import_id: id.opaque(),
                state: progress.state,
                outcome: progress.outcome.map(Into::into),
                cancellation: cancellation(reason, progress.stop_confirmed),
                server_cancel_sent: progress.server_cancel_sent,
                deadline_exceeded: reason == Some(Reason::Deadline),
                statement_count: import.total,
                completed: import.completed,
                affected_rows: import.affected.to_string(),
                failed: import.failed.as_ref().map(Into::into),
                unknown: import.unknown.as_ref().map(Into::into),
                done: progress.state == JobStateDto::Finished,
            }
        };
        if status.done {
            execution.jobs.lock().expect("job lock").remove(&id);
        }
        Ok(status)
    }
}

fn update(job: &Job, change: impl FnOnce(&mut ImportProgress)) {
    job.update(|progress| {
        if let Some(import) = progress.import.as_mut() {
            change(import);
        }
    });
}

async fn run_import(mut plan: ImportPlan) {
    let job = plan.job.clone();
    let control = &job.control;
    let open = tokio::time::timeout(plan.connect_deadline, plan.target.open());
    // An import interrupted before it starts (cancel, close) never opens a connection.
    let physical = tokio::select! {
        biased;
        _ = control.interrupted() => None,
        opened = open => opened.ok().and_then(Result::ok),
    };
    let Some(mut connection) = physical else {
        // Nothing was sent: an interrupt is a confirmed stop, otherwise a confirmed failure.
        let outcome = if control.is_interrupted() {
            ExecutionOutcome::Cancelled
        } else {
            ExecutionOutcome::Failed
        };
        if let Some(inner) = plan.registry.upgrade() {
            inner.execution.concluded(plan.id.0);
        }
        finish(&job, outcome, true, None);
        return;
    };
    connection.interrupt.rearm();
    let total = plan.snapshot.spans.len();
    let mut outcome = ExecutionOutcome::Succeeded;
    let mut stop_confirmed = true;
    let mut reusable = true;
    let mut sent = false;
    let mut in_flight: Option<usize> = None;
    for index in 0..total {
        if control.is_interrupted() {
            outcome = ExecutionOutcome::Cancelled;
            break;
        }
        let span = plan.snapshot.spans[index].clone();
        let at = |failure: Option<StatementFailure>| ImportStatement {
            index: index as u32,
            line: plan.snapshot.line(span.start),
            failure,
        };
        // Write-ahead: the statement about to be sent is journaled first (statement 0 was at
        // start). If that fails nothing more is sent and the import fails here.
        if index > 0
            && !plan.registry.upgrade().is_some_and(|inner| {
                inner
                    .execution
                    .journal_job(plan.id, &plan.origin, total, index, false)
            })
        {
            let failed = at(None);
            update(&job, |import| import.failed = Some(failed));
            outcome = ExecutionOutcome::Failed;
            break;
        }
        control.arm(ExecutionLimits::IMPORT_STATEMENT_DEADLINE);
        let sql = &plan.snapshot.text[span.clone()];
        let Physical { session, interrupt } = &mut connection;
        // R-1: nothing is sent unless SQLite parses the text as exactly one statement.
        #[cfg(feature = "sqlite")]
        if let Session::Sqlite(sqlite) = session
            && !super::sqlite_single_statement(sqlite, sql).await
        {
            let failed = at(Some(StatementFailure {
                constraint: None,
                sql_state: None,
            }));
            update(&job, |import| import.failed = Some(failed));
            outcome = ExecutionOutcome::Failed;
            break;
        }
        #[cfg(not(feature = "sqlite"))]
        let _: &Session = session;
        sent = true;
        in_flight = Some(index);
        let guarded = guarded(
            statement(session, sql, &mut plan.sink, control),
            control,
            &*interrupt,
        )
        .await;
        let (status, failure, completed): Classified = classify(&guarded);
        if guarded.interrupted {
            stop_confirmed = guarded.result.is_some();
            // L-8: a statement that did not end, or an interrupt still in flight, leaves a
            // session nothing else may run on.
            if guarded.result.is_none() || guarded.signal.is_none() {
                reusable = false;
            }
        }
        job.update(|progress| progress.server_cancel_sent |= guarded.signal == Some(true));
        match status {
            StatementStatusDto::Succeeded => {
                let affected = completed.map_or(0, |(affected, _)| affected);
                update(&job, |import| {
                    import.completed += 1;
                    import.affected = import.affected.saturating_add(affected);
                });
                in_flight = None;
                continue;
            }
            StatementStatusDto::Failed => {
                let failed = at(failure);
                update(&job, |import| import.failed = Some(failed));
                in_flight = None;
                outcome = ExecutionOutcome::Failed;
            }
            _ => {
                let unknown = at(None);
                update(&job, |import| import.unknown = Some(unknown));
                outcome = ExecutionOutcome::Unknown;
                if !guarded.interrupted {
                    reusable = false;
                }
            }
        }
        break;
    }
    // R-3: a reported outcome is kept only if the session's transaction state still matches
    // autocommit with no open transaction; otherwise what the statements became is unknown.
    let mut diverged = false;
    if !reusable {
        if sent {
            outcome = ExecutionOutcome::Unknown;
        }
    } else if sent
        && outcome != ExecutionOutcome::Unknown
        && !connection
            .state_matches(false, outcome == ExecutionOutcome::Succeeded)
            .await
    {
        outcome = ExecutionOutcome::Unknown;
        diverged = true;
    }
    if reusable && !diverged {
        connection.close().await;
    } else {
        drop(connection);
    }
    let registry = plan.registry.upgrade();
    if outcome != ExecutionOutcome::Unknown
        && let Some(inner) = &registry
    {
        inner.execution.concluded(plan.id.0);
    }
    if outcome == ExecutionOutcome::Unknown
        && let Some(inner) = registry
    {
        let reason = control.reason();
        let server_cancel_sent = job
            .progress
            .lock()
            .expect("job progress lock")
            .server_cancel_sent;
        inner.execution.record_job(
            plan.id,
            job.owner,
            &plan.origin,
            UnresolvedJob {
                statement_count: total as u32,
                unknown_statement: in_flight.map(|index| index as u32),
                in_transaction: false,
                cancellation: cancellation(reason, stop_confirmed),
                server_cancel_sent,
                deadline_exceeded: reason == Some(Reason::Deadline),
            },
        );
    }
    finish(&job, outcome, stop_confirmed, None);
    drop(plan);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(sql: &str) -> ImportRefusal {
        validate(sql.as_bytes().to_vec(), Dialect::MySql)
            .err()
            .unwrap()
    }
    fn texts(sql: &str) -> Vec<String> {
        let snapshot = validate(sql.as_bytes().to_vec(), Dialect::MySql).unwrap();
        snapshot
            .spans
            .iter()
            .map(|span| snapshot.text[span.clone()].to_owned())
            .collect()
    }

    #[test]
    fn validates_the_whole_file_and_keeps_exact_statement_ranges() {
        let sql = "\u{feff}-- dump\r\nINSERT INTO t VALUES ('a;b', \"c\");\r\n\
                   /*!40101 SET NAMES utf8mb4 */;\n\
                   DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END//\nDELIMITER ;\n\
                   INSERT INTO t VALUES (X'00ff')";
        assert_eq!(
            texts(sql),
            [
                "-- dump\r\nINSERT INTO t VALUES ('a;b', \"c\")",
                "/*!40101 SET NAMES utf8mb4 */",
                "CREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END",
                "INSERT INTO t VALUES (X'00ff')",
            ]
        );
    }

    #[test]
    fn any_refused_statement_refuses_the_file_with_its_number_and_line() {
        let refusal = refused(
            "INSERT INTO t VALUES (1);\nINSERT INTO t VALUES (2);\n\nCOMMIT;\nINSERT INTO t VALUES (3);",
        );
        assert_eq!(refusal.statement, Some(3));
        assert_eq!(refusal.line, Some(4));
        assert_eq!(
            refusal.reason,
            ImportRefusalReason::Script(ScriptError::TransactionControl)
        );
        assert!(refusal.to_string().starts_with("Statement 3 (line 4): "));
        assert!(refusal.to_string().ends_with("Nothing was run."));
        assert_eq!(
            refused("SELECT 1;\nUSE other;").reason,
            ImportRefusalReason::Use
        );
        for sql in [
            "LOAD DATA LOCAL INFILE '/etc/passwd' INTO TABLE t",
            "LOAD DATA CONCURRENT LOCAL INFILE 'x' INTO TABLE t",
            "LOAD XML LOCAL INFILE 'x' INTO TABLE t",
        ] {
            assert_eq!(refused(sql).reason, ImportRefusalReason::LocalFile, "{sql}");
        }
        for (sql, reason) in [
            ("SET autocommit = 0", ScriptError::TransactionControl),
            ("START TRANSACTION", ScriptError::TransactionControl),
            ("CALL p()", ScriptError::Call),
            ("PREPARE s FROM 'COMMIT'", ScriptError::PreparedStatement),
        ] {
            assert_eq!(
                refused(sql).reason,
                ImportRefusalReason::Script(reason),
                "{sql}"
            );
        }
        let unterminated = refused("SELECT 1;\nSELECT 2;\nINSERT INTO t VALUES ('open");
        assert_eq!(
            (
                unterminated.reason,
                unterminated.statement,
                unterminated.line
            ),
            (
                ImportRefusalReason::Script(ScriptError::Unterminated),
                Some(3),
                Some(3)
            )
        );
    }

    #[test]
    fn size_encoding_and_count_limits_refuse_before_anything_runs() {
        assert_eq!(refused("").reason, ImportRefusalReason::Empty);
        assert_eq!(
            refused(" ;\n-- only a comment\n").reason,
            ImportRefusalReason::Empty
        );
        let invalid = validate(b"SELECT 1;\nSELECT '\xff';".to_vec(), Dialect::MySql)
            .err()
            .unwrap();
        assert_eq!(
            (invalid.reason, invalid.line),
            (ImportRefusalReason::NotUtf8, Some(2))
        );
        let big = format!(
            "SELECT 1;\nINSERT INTO t VALUES ('{}');",
            "x".repeat(ExecutionLimits::MAX_IMPORT_STATEMENT_BYTES)
        );
        let refusal = refused(&big);
        assert_eq!(
            (refusal.reason, refusal.statement, refusal.line),
            (ImportRefusalReason::StatementTooLarge, Some(2), Some(2))
        );
        let at_limit = format!(
            "INSERT INTO t VALUES ('{}')",
            "x".repeat(ExecutionLimits::MAX_IMPORT_STATEMENT_BYTES - 25)
        );
        assert_eq!(at_limit.len(), ExecutionLimits::MAX_IMPORT_STATEMENT_BYTES);
        assert_eq!(texts(&at_limit).len(), 1);
        let many = "SELECT 1;".repeat(ExecutionLimits::MAX_IMPORT_STATEMENTS + 1);
        assert_eq!(
            refused(&many).reason,
            ImportRefusalReason::TooManyStatements
        );
        assert_eq!(
            texts(&"SELECT 1;".repeat(ExecutionLimits::MAX_IMPORT_STATEMENTS)).len(),
            ExecutionLimits::MAX_IMPORT_STATEMENTS
        );
        let mut huge = Vec::new();
        huge.resize(ExecutionLimits::MAX_IMPORT_BYTES + 1, b' ');
        assert_eq!(
            validate(huge, Dialect::MySql).err().unwrap().reason,
            ImportRefusalReason::TooLarge
        );
    }
}
