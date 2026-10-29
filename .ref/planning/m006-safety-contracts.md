# M006 — Safety contracts for advanced SQL execution and transactions

**Status: Proposed / awaiting approval.** This is planning only, dated 2026-10-26 and based on commit `e908c49`. No M006 code, test, contract, vendor, Cargo, docs, milestone, decision-register or progress-file change was made. **No acceptance test in this document has been executed.** Every "current behavior" claim cites the file and line it comes from. Values are labelled:

- **IMPLEMENTED** — present in code today (with its source). This is not a release approval.
- **APPROVED** — explicitly approved by the user and recorded in `architecture-decisions.md`.
- **PROPOSED** — a new value for review. Under D010 and AGENTS.md, no value may be treated as approved by inference, including from the continuation instruction ("continue to the end of the M07. I already approved").

Scope: M006 (`.ref/ms/M006-schema-execution.md`) covers advanced execution, jobs, transactions and cancellation. Schema metadata and caches are referenced only where they share generation and ownership rules.

**Citation basis.** All `file:line` citations refer to committed `HEAD` `e908c49`, where no M006 code existed.

**Concurrent working-tree changes (observed, not reviewed).** While this proposal was being written, uncommitted changes from another session appeared:

- `crates/stoneql-runtime/src/script.rs` (new): splitter and transaction policy;
- `crates/stoneql-contracts/src/v1.rs` (+213 lines): `AccessModeDto`, `SubmitJobRequest`, `JobStateDto {Queued, Running, Finished}`, `LeaseStateDto {Active, RollbackOnly, Ended}`, `ExecutionOutcomeDto`, `StatementStatusDto`, `TransactionLease`;
- `crates/stoneql-core/src/lib.rs`: `ExecutionOutcome {Succeeded, Failed, Cancelled, Unknown}`;
- `mysql.rs`, `sqlite.rs`, `read_only.rs` (the `Dialect` refactor shifts `read_only.rs` lines after 75 by −7);
- `architecture-decisions.md` (D021).

None of it was modified by this task. Before either path proceeds, reconcile these divergences from the contracts below:

1. **`Queued` job state vs. Contract 1.3 (no queue).**
2. **`ExecutionOutcome::Cancelled` vs. Contract 8.** Here, a confirmed cancellation of a *write* stays `Unknown` unless a confirmed rollback follows. The draft core comment ("cancellation was confirmed (the statement was observed to end)") does not say whether effects are excluded.
3. **Savepoints inside leases** (the draft `SavepointOutsideTransaction` implies support) vs. Contract 3.5 (not offered in the first slice).
4. **Script splitting implemented directly** vs. Contract 2.3 (Phase A single statement first).
5. **Contract changes added to closed v1** without a recorded D003 versioning decision (prerequisite 1).

The draft also independently matches several contracts here: API-only transaction control, the MySQL implicit-commit refusal inside leases, SQLite `ATTACH`/`DETACH`/`VACUUM INTO` refusal, and an explicit `AccessMode`.

---

## 0. Current baseline (what M006 starts from)

| Area | Current behavior | Source |
| --- | --- | --- |
| Ownership | One `OwnerId` per desktop `NativeSession`; every handle is checked against its owner on each call | `apps/desktop/src-tauri/src/lib.rs:50,80`; `crates/stoneql-runtime/src/lib.rs:173-181,552-560,578-588` |
| Capacity | Registry of 8 logical handles (configurable range 1..=64), with a 3 s setup deadline and a 3600 s absolute lifetime | `apps/desktop/src-tauri/src/lib.rs:81-85`; `crates/stoneql-runtime/src/lib.rs:223-246` |
| Lease shown to the UI | `expires_after_seconds: 3600` is hard-coded separately from the registry lifetime. These are **two sources of truth**, and an M006 lease contract must unify them | `apps/desktop/src-tauri/src/database.rs:180` vs `apps/desktop/src-tauri/src/lib.rs:84` |
| Expiry | Absolute expiry is set at registration (no idle timer, no renewal). A spawned timer removes and closes the entry | `crates/stoneql-runtime/src/lib.rs:511,533-549` |
| Physical connections | Every pool has `max_connections(1)` | `mysql.rs:55`, `postgres.rs:65`, `sqlite.rs:69` |
| Session-state isolation | Every query calls `close_on_drop()` and then `close()`; no connection is ever reused | `crates/stoneql-runtime/src/read_only.rs:124-126,183-185` |
| Read-only boundary | Restricted SELECT lexer (`read_only.rs:226-353`) plus engine enforcement: MySQL/MariaDB `START TRANSACTION READ ONLY` (`read_only.rs:88-98`); PostgreSQL read-only transaction with `SET LOCAL statement_timeout` (`postgres.rs:105-118`, D012); SQLite `read_only(true)` and `query_only=ON` (`sqlite.rs:47-58`, D015) | as cited |
| Query limits | rows 1..=1000, bytes 1..=1 MiB, deadline >0 and ≤30 s (runtime). The desktop fixes the deadline at 3 s | `read_only.rs:22-23,68-74`; `database.rs:185-191` |
| Deadline | `tokio::time::timeout` drops the future and the connection closes. There is **no server-side kill** for MySQL/MariaDB. PostgreSQL gets a server `statement_timeout`; SQLite has a progress-handler interrupt | `read_only.rs:86-105`; `postgres.rs:107-113`; `sqlite.rs:107-121` |
| Host admission | 16 concurrent connect/query requests per session | `database.rs:31-39` |
| Renderer wait | 8 s for database commands and 120 s for interactive ones. A renderer timeout is reported locally as `DEADLINE_EXCEEDED` while the host keeps working | `apps/frontend/src/services/nativeHost.ts:73-77` |
| Generations | A registry generation counter is bumped under the entries lock on reset, and late registrations are refused | `crates/stoneql-runtime/src/lib.rs:201,276,517-519,592-603` |
| Stale replies | The host rechecks the session after a query; if stale, it returns `FORBIDDEN` with outcome `unknown` | `database.rs:213-217` |
| Frontend stale guard | The `liveGenerations` map; results are discarded if the generation changed | `apps/frontend/src/stores/workspace.ts:97,129-145` (message at `:135`) |
| Renderer reload | A new nonce triggers `connections.reset()`, which revokes handles, grants and pending attempts and closes pools and tunnels | `apps/desktop/src-tauri/src/lib.rs:137-165`; `crates/stoneql-runtime/src/lib.rs:589-603` |
| Shutdown | `shutdown_and_exit` awaits `connections.shutdown()` and then exits | `apps/desktop/src-tauri/src/lib.rs:215-229`; `crates/stoneql-runtime/src/lib.rs:605-621` |
| SSH tunnel | Keepalive every 30 s with max 3 (dead path detected in ≈90 s). At most 4 concurrent forwards; beyond that, accepted streams are **silently dropped** | `crates/stoneql-runtime/src/ssh.rs:150-151,27,403-405` |
| Outcomes | `Outcome {Rejected, RolledBack, Committed, Unknown}`; connection loss never claims rollback (tested) | `crates/stoneql-core/src/lib.rs:130-136,151-156,183-190` |
| Error codes | 20 allowlisted codes, including `Cancelled`, `DeadlineExceeded`, `ConnectionLost`, `Conflict` | `crates/stoneql-core/src/lib.rs:66-88`; DTO `crates/stoneql-contracts/src/v1.rs:166-187` |
| Retry flag | `retryable` is always `false` | `crates/stoneql-contracts/src/v1.rs:202,238` |
| Query error mapping | Refusals → `Rejected`; every execution failure (timeout, database, packet) → `Unknown` | `database.rs:58-78` |
| Capabilities | `Mutations`, `Transactions`, `Cancellation` and `MultipleResults` exist; a capability with no recorded support is `Unverified` and fails closed | `crates/stoneql-core/src/lib.rs:20-59,168-182` |
| Multiple result sets | `QueryResult.result_sets: Vec<ResultSet>` exists as a DTO, but the live command returns a single `ResultSet` | `v1.rs:160-164`; `database.rs:197` |
| Contract evolution | The v1 decoder is deliberately closed (unknown fields are rejected), so additions are a versioning decision | `docs/architecture/api-contract.md:9` |
| PROD safeguards | Renderer-only modal with a "PROD — I understand" checkbox, used only for **mock** mutations. `environment` is profile metadata and is **not sent over IPC** | `App.vue:529-538,577,600-615,892-905,2617-2627`; `types/index.ts:56`; `connectionPreferences.ts:11-17,136-141`; `DirectConnectionRequest` has no environment field (`v1.rs:327-348`) |
| SQLite files | Typed owner-scoped grants (`GrantKind::{Sqlite, SshKey}`), capped at 32 and re-verified at every open | `crates/stoneql-runtime/src/lib.rs:190-194,221-222,402-423`; `sqlite.rs:24-41`; D014 |
| Metadata allocation | The aggregate metadata cap is **Proposed / awaiting approval**, and the driver reserves from a server-supplied count | `.ref/planning/m003-metadata-limit/findings.md` §1, §7 |

---

## Implementation status per contract item (2026-10-26 safety-fix pass; R-1..R-3 fix pass and follow-up pass 2026-10-29)

Legend: **Implemented-verified** = in code (uncommitted) with an executed test or fixture that would fail without it. **Implemented-unverified** = in code, but no executed test proves the item. **Proposed** = not implemented (or implemented differently, noted). None of these is an approval. **Read-write SQL is gated OFF natively** (`READ_WRITE_SQL_ENABLED = false`, desktop host), so none of it is reachable from the shipped UI until the user approves enabling it. Evidence: `m006-evidence/execution/` and `m006-reconciliation.md`.

| Item | Status | Evidence / note |
| --- | --- | --- |
| 1.1 opaque, owner/handle-scoped job ids | Implemented-verified | runtime `jobs_transactions_and_pages_are_owner_scoped`; desktop `sqlite_execution.rs`; fixture owner isolation |
| 1.2 job states | Implemented-verified (different shape) | `Running \| Finished` + outcome + `cancellation`; no `cancelling`/`expired` states (divergence 4) |
| 1.3 no queue, `BUSY` | Implemented-verified | runtime `busy_handles_refuse_instead_of_queueing`; fixture "BUSY, no queue" on 3 engines |
| 1.4 idempotent submit per request id | Proposed | not implemented; the renderer never resubmits (`liveJobs.test.ts` B-10/no-resubmission) |
| 1.5 bounded retention | Implemented-verified; per-owner bound via record count (B-7) | 2-page channel, 4 MiB/job; **follow-up:** `MAX_JOB_RECORDS_PER_OWNER` 8 (Proposed) bounds buffered bytes per owner to ≈6 MiB typical / 24 MiB worst (single-row 1 MiB pages); runtime `b7_undrained_jobs_beyond_the_owner_bound_are_refused` (mutant: 64 → fails). No separate 8 MiB byte cap |
| 1.6 job never outlives its handle | Implemented-verified | B-1 fix: disconnect/expiry remove the handle before release; submit/begin insert-then-recheck; runtime `disconnect_racing_submit_and_begin_never_leaves_untracked_work` (60 rounds) **Follow-up:** the removal-before-release order is tested deterministically for disconnect and the lifetime watcher (`b1_disconnect_and_handle_expiry_remove_the_handle_before_releasing_its_work`; both order-reversal mutants fail it) |
| 2.1 mode separation + host gate | Implemented-verified | read-only default unchanged; **native gate OFF**: desktop `read_write_is_refused_natively_while_the_gate_is_off` (all engines/routes, before Keychain/network) |
| 2.2 `Capabilities::require` gating | Proposed | replaced by access mode + native gate; capability rows not recorded `Supported` |
| 2.3 splitting authority | Implemented-verified (Phase B) | scripts split client-side; Phase A vs B still a user question (Q2). Custom-DELIMITER statements that hide a second statement outside a BEGIN … END body are refused (`MultipleStatements`), because SQLx enables server multi-statements for the text fallback. **R-1 (2026-10-29):** the splitter is no longer the authority on SQLite: before each statement is sent the runtime prepares every prefix ending at a `;` and refuses the statement unless each fails with a text-only parse error (`execution.rs::sqlite_single_statement`); SQLite trigger bodies now end only at `; END ;`. Runtime `r1_sqlite_text_with_more_than_one_statement_never_runs` (mutant: check disabled → job `Succeeded`, test fails), `script.rs` `r1_sqlite_trigger_bodies_end_only_at_semicolon_end_semicolon` (mutant: old depth rule → fails) |
| 2.4 transaction control API-only | Implemented-verified with divergences | `BEGIN`/`START`/`COMMIT`/`ROLLBACK`/`XA`/savepoints refused; `SET … autocommit` refused anywhere in the statement (all words, quoting, scope; V-2a); MySQL/MariaDB `PREPARE`/`EXECUTE`/`DEALLOCATE`/`DROP PREPARE` refused in and out of leases (V-2b). Divergences: `LOCK`/`UNLOCK TABLES` refused only inside a lease; `SET TRANSACTION` allowed outside a lease only. **R-2 (2026-10-29):** MariaDB `SET STATEMENT … FOR` (any inner statement), top-level compound statements (`CASE`/`IF`/`LOOP`/`WHILE`/`REPEAT`/`FOR`, labels) and MySQL/MariaDB `CALL` refused in and out of leases; PREPARE/EXECUTE stay refused when wrapped (`r2_mariadb_wrappers_and_compound_statements…`; mutants removing the SET STATEMENT or compound check fail it); fixture: refused before send with zero general-log rows on MySQL 8.4.6 and MariaDB 11.4.8. `script.rs` tests `autocommit_changes_are_refused_anywhere…`, `mysql_prepared_statements_are_refused…`, `mysql_custom_delimiters_cannot_smuggle…`, `other_transaction_and_implicit_commit_vectors_stay_refused` (mutation: restoring the 8-word window fails the autocommit test) |
| 2.5 refused regardless of mode (USE, session identity, LOAD LOCAL, COPY, SQLite ATTACH…) | Partly implemented | SQLite `ATTACH`/`DETACH`/`VACUUM INTO` refused (verified); `LOAD DATA LOCAL` refused at protocol level by SQLx capabilities (code reading only); `USE`/`SET sql_mode`/`search_path`/`COPY` **not refused** (job connections are discarded, so no leak to other jobs — fixture fault cycles) — Proposed |
| 2.6 per-statement outcome, stop at first failure | Implemented-verified | runtime/fixture DDL partial outcome |
| 2.7 MySQL implicit commit refused in a lease | Implemented-verified (**allow-list**, R-2 2026-10-29) | in-lease leading keywords: SELECT, WITH, VALUES, TABLE, SHOW, DESCRIBE/DESC, EXPLAIN, INSERT, UPDATE, DELETE, REPLACE, USE, CREATE/DROP TEMPORARY TABLE, `SET @user` only; no `;` anywhere. `r2_inside_a_mysql_lease_only_allowlisted_statements_run` (mutant: allow-list bypassed → fails); fixture "MySQL DDL refused in lease" (`NotAllowedInTransaction`) |
| 2.8 `CALL` / multiple results | MySQL/MariaDB `CALL` **refused** in read-write scripts (R-3, 2026-10-29); PostgreSQL `CALL` implemented-verified | the residual "routine starts a transaction and reports `succeeded`" is closed twice: refusal (fixture: zero general-log rows, rows absent) and the session check (3.7). Multi-result `CALL` on MySQL/MariaDB is therefore not offered |
| 2.9 SQLite read-write decision | Implemented-verified (D025-10, unapproved) | SQLite deferred from alpha by D022 |
| 3.1 lease ownership | Implemented-verified | owner + handle scoped |
| 3.2 pinning 3-A | Implemented-verified (dedicated never-pooled connection, divergence 1) | fixture sessions back to baseline |
| 3.3 idle/max durations | Implemented-verified | real 60 s expiry fixture |
| 3.4 counts | Implemented-verified | per handle; per owner reserved atomically before the connection opens (L-4, follow-up): `l4_concurrent_begins_never_exceed_the_per_owner_lease_count` (20 rounds × 3 concurrent begins; mutant without the reservation fails) |
| 3.5 no savepoints | Implemented-verified | |
| 3.6 discard always | Implemented-verified | 100 fault cycles per engine |
| 3.7 server transaction-state verification | Implemented-verified for MySQL/MariaDB, SQLite and PostgreSQL (R-3 and follow-up, 2026-10-29) | no vendor change: after every job (not already `unknown`) MySQL/MariaDB `SELECT @@autocommit` + `SET TRANSACTION READ WRITE` (ER 1568 ⇔ active transaction; checked on 8.4.6 and 11.4.8), SQLite `BEGIN` (+ immediate ROLLBACK). Mismatch → job `unknown`; in a lease also `diverged` → rollback-only, end sends nothing, `unknown`. Runtime `r3_a_diverged_session_is_never_reported_committed_or_rolled_back` (mutant: probe disabled → `Succeeded`, fails); fixture `policyAndSessionState.sessionStateProbe` (the review's procedures: INSERT reported 1 row, 0 after close, probe mismatch). Not re-checked immediately before COMMIT/ROLLBACK (nothing runs between); a deadlock that rolls back the transaction is reported `unknown` (conservative) **Follow-up (PostgreSQL):** `SAVEPOINT stoneql_state` (+ `RELEASE`): 25P01 outside a block, success inside a healthy one, 25P02 (aborted) accepted only after a failed statement; R-10 COMMIT sent as `SAVEPOINT stoneql_commit; COMMIT`. Fixture `postgresSessionState` on 17.6 (positive control: plain COMMIT on an aborted block answered OK); mutants (aborted accepted after success; plain COMMIT) fail it |
| 4.1 triggers (expiry, handle expiry, disconnect, reload, shutdown, loss) | Implemented-verified | runtime + desktop (disconnect, reload); fixture expiry and session kill |
| 4.2 bounded procedure; record outcome | Implemented-verified | **V-1 fix:** `unknown` (with `commitSent`/`rollbackSent`) is recorded when the lease is removed and again before COMMIT/ROLLBACK is sent; only server confirmation overwrites. Runtime `a_release_cut_off_by_its_bound_records_unknown…` (disconnect, reset, shutdown; mutation: removing the record yields FORBIDDEN) and `conclude_records_unknown_before_sending…`; expiry uses the same `take_lease` path (ordering verified by code and the conclude test; not forced to time out) |
| 4.3 rollback only when confirmed | Implemented-verified | fixture lost-ROLLBACK replies: unknown, `rollbackSent=true`, never `rolled_back`. MySQL COMMIT refused with a database error is now `unknown` (L-7) — unit-unverified on a real server (no fixture provokes it) **Follow-up:** MySQL/MariaDB COMMIT refused with ER 1290 (`read_only`) is `unknown`, `commitSent`, no ROLLBACK sent — fixture `commitRefused` on both engines (mutant removing the MySQL guard fails it); PostgreSQL 25P01 on COMMIT → `unknown`, no ROLLBACK |
| 4.4 non-transactional effects stated | Implemented-unverified (docs only) | |
| 4.5 post-reload reporting | Implemented-verified (unknown outcomes only, in memory; values Proposed) | **P3 (2026-10-29):** unresolved `unknown` lease ends and job outcomes are recorded SQL-free per owner, never evicted by the 32/256 bounds (which now apply only to resolved or acknowledged records), listed by `list_unresolved_outcomes` and hidden only by `acknowledge_outcome` (idempotent, owner-scoped, never changes the outcome); at 16 unacknowledged per owner new jobs/leases are refused (`UNRESOLVED_OUTCOMES`), nothing dropped. The desktop runtime owner outlives renderer rotations, so the next renderer lists the previous session's unknowns; the UI shows them above every workspace until acknowledged. Confirmed `rolled_back`/`committed` ends are not listed (differs from the §5 "counts of rolled_back" idea). Process exit loses them. Runtime `p3_…` (4 tests; 7 mutants killed), desktop `reload_guard.rs` rotation test (3 mutants killed), `unresolvedOutcomes.test.ts`, browser `unresolved-outcomes.spec.ts` (Light/Dark), fixture `unknownOutcomeRetention` (lost COMMIT kept through 288 confirmed ends). **Review F-2/F-3 (2026-10-29):** a job a reset detached (past `RELEASE_BOUND`) is reported in the additive `UnresolvedOutcomes.pending` until it finishes (counted before outcomes are read; a job records its unknown before it is marked finished), and the renderer re-lists every 1 s while `pending > 0` (runtime `detached_jobs_are_pending_…`, `unresolvedOutcomes.test.ts`, browser late-outcome spec Light/Dark; mutants killed). On macOS a terminated WebKit content process runs `renderer_lost` (same rotation as a confirmed reload) then reloads, instead of Tauri's default reload the guard refused (`reload_guard.rs` + `real_window_reload` SIGKILL phase; handler-removed mutant reproduces the blank window) |
| 4.6 shutdown bound | Implemented-verified | 5 s bound test (shutdown path) |
| 5.1 best-effort cancel, latency | Implemented-verified | 100 samples/engine + 30 blocked-write samples/engine (README) |
| 5.2 SSH cancel under slot exhaustion | Proposed | SSH read-write refused natively (verified) |
| 5.3 write deadline/cancel issues a server interrupt | Implemented-verified (MySQL, MariaDB, PostgreSQL) | **B-5:** fixture blocked UPDATE (30/30 interrupt delivered, session gone, connection discarded, executed once) and large `INSERT…SELECT` cancel (0 rows after; statement atomicity observed, not claimed as rollback). **Follow-up:** same sections on PostgreSQL 17.6 (`pg_cancel_backend`, executed once per csvlog) **2026-10-29 ordering (L-3):** PostgreSQL server `statement_timeout` < client statement deadline < job deadline; fixture (PostgreSQL 17.6, 3 s deadline, 2 s server timeout): 10/10 autocommit lock-blocked `UPDATE`s ended by the server, `failed` 57014, cancellation `none`, no interrupt sent, submit→terminal 2035–2079 ms over four passing runs (≈ 920 ms before the client deadline), executed once, not applied, 10 csvlog statement-timeout errors; in a lease → rollback-only, server shows the block aborted, ROLLBACK confirmed; cancel before the timeout still confirmed by `pg_cancel_backend`; after a script's `SET statement_timeout = 0` the client deadline + interrupt end it (`unknown`, 3066–3110 ms); an external `pg_cancel_backend` arriving after the server-timeout duration is `failed` without `deadlineExceeded`; writer sessions back to baseline, none in a transaction. Mutants killed: deadline + 2 s restored (fixture "deadline" + unit test); timeout-message check removed (fixture "external cancel is not reported as a deadline") |
| 5.4 cancel when KILL connection fails | Implemented-verified | relay refuses new connections: `requested`, `serverCancelSent=false`, `unknown`, terminal ≈1.5 s. **Finding:** the abandoned statement was still running on the server; the UI now says it may still be running (B-6) **Follow-up (L-3), superseded 2026-10-29:** the earlier deadline + 2 s was not approved. PostgreSQL jobs now set `statement_timeout` = statement deadline − 1 s (at most half the deadline, never 0; Proposed), so the server ends a statement **before** the client deadline (5.3 ordering); fixture: abandoned statement ended by the server 4064–4076 ms after submit (5 s deadline, 4 s server timeout). MySQL/MariaDB have no backstop. **L-8:** an interrupt still unanswered at the end of the grace period discards the connection (lease → `unknown`), runtime `l8_…` (mutant fails); answered interrupts on idle sessions were probed harmless on all three engines |
| 5.5 terminal cancel states | Implemented-verified | `cancellation: requested \| confirmed` |
| 6.1–6.3 loss and stale sessions | Implemented-verified | desktop stale-session refusals; fixture session kill |
| 6.4 loss detection while idle | Implemented-unverified | detected on next use; SSH keepalive unchanged |
| 7.1 lost COMMIT reply → unknown, `commitSent` | Implemented-verified (n=50 per network engine) | **B-4 follow-up:** relay `sever_on_reply` (COMMIT forwarded, reply never delivered), n=50 per engine on MySQL 8.4.6, MariaDB 11.4.8 and PostgreSQL 17.6: 50/50 `unknown`, `commitSent`; COMMIT in the server log 50/50 and the row committed 50/50 (the dangerous case is produced); 0 re-sends. 7.2 (`sever`, before COMMIT leaves) n=50 per engine: `unknown` with `commitSent` (attempted), COMMIT absent from the log, 0 rows. Same for ROLLBACK (never `rolled_back`). `drop_s2c` n=3 (10 s deadline each). `commitSent` means "attempted" |
| 7.2 never auto-retry | Implemented-verified | general-log counts: the job statement and COMMIT/ROLLBACK each at most once; repeat `end_transaction` returns the recorded outcome and sends nothing **Follow-up:** PostgreSQL csvlog counts too (all lost-reply runs on 3 engines) |
| 7.3 read-only retry hint | Proposed | `retryable` always false |
| 7.4 reconciliation UX | Implemented-verified (unit/browser) | `describeTransactionEnd`, renderer commit-timeout text (B-2) |
| 7.5 duplicate-submit protection | Proposed | see 1.4 |
| 8.x taxonomy | Implemented-verified | V-3 fix: UI derives text from the outcome; "executed successfully" only for `succeeded` (unit + browser both themes) **Follow-up (B-11):** `StatementReport.resultsWithheld` flags rows/pages read but not delivered after an interrupt; renderer marks those result sets truncated (`b11_…` runtime and `liveJobs` B-11 tests, mutants fail; MySQL fixture: killed `SLEEP()` `succeeded` with the flag) |
| 9.1–9.2 PROD UX, read-only default | Implemented-verified (UX) | typed name confirmation and persistent banner Proposed (checkbox implemented) |
| 9.3 environment over IPC | Proposed | divergence 6 |
| 9.4 destructive classification | Implemented-unverified as specified | keyword confirmation only; classifier classes Proposed |
| 10 limits | Implemented as working defaults | every value PROPOSED |

## Contract 1 — Background job lifecycle and concurrency

**Policy.**

1. A *job* is one accepted execution request: a single statement, or an approved script per Contract 2. It is identified by an opaque, unguessable `JobId`, owned by exactly one `OwnerId` and bound to one `ConnectionId` and one registry generation.
2. States: `accepted → running → {succeeded | failed | cancelling → cancelled | unknown | expired}`. `cancelling` is the only intermediate state that a user action can enter. Terminal states are immutable.
3. **No hidden queue for write-capable jobs.** If a handle is busy (a running job or an open transaction owned by another job), submission is refused with `BUSY` (PROPOSED code). It is never queued. A queued write running later, after the user believes it was abandoned, is the hazard this rule prevents. Read-only jobs may also be refused rather than queued, keeping one simple rule.
4. Submission is idempotent per `(owner, request_id)` within a generation. A duplicate submit returns the existing `JobId` and never executes twice.
5. Results are retained in a bounded per-job buffer until the owner fetches or releases them, the retention TTL elapses, or the owner or generation is reset. Fetching is paged with an opaque cursor from **retained** data. M006 does not hold a server cursor open across page fetches; streaming beyond caps belongs to M007. As today, once a row or byte cap is reached the runtime stops reading, marks the result truncated and discards the connection (`read_only.rs:163-185`).
6. A job never outlives its handle or generation. Handle expiry, disconnect, reset or shutdown moves every non-terminal job to a terminal state under Contracts 4 and 8.

**Rationale.** The live path today is synchronous request/response under a 3 s runtime deadline (`database.rs:189`), a 16-request admission limit (`database.rs:31-39`) and an 8 s renderer timeout (`nativeHost.ts:75`). Long DDL, procedures and transactions cannot fit that model, and a renderer timeout must not hide a still-running write. Because each handle has exactly one physical connection (`mysql.rs:55`, `postgres.rs:65`, `sqlite.rs:69`), one running job per handle is the physical truth, not an extra restriction.

**Enforcement point.**

- **Runtime** (`stoneql-runtime`, framework-independent): job registry, ownership, state machine, bounds, retention and terminal transitions.
- **Core:** state and outcome vocabulary.
- **Host:** envelope validation, the session token on every submit, poll, fetch and cancel call (`database.rs:14-24`), and admission.
- **UI:** display only.

Job submit, poll and fetch framing (and any push or event channel) is a **new contract surface** that needs a D003 versioning decision. Any push channel must still carry and check the session token.

**Outcome codes.** Existing: `FORBIDDEN` (foreign or stale job id), `CAPACITY`, `INVALID_REQUEST`. PROPOSED: `BUSY` (handle has a running job or open lease), `JOB_EXPIRED` (results released or TTL elapsed).

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 1.1 | Runtime unit test: owner B polls, fetches, cancels and releases owner A's `JobId`; malformed and unknown ids | 100% `FORBIDDEN`; no state change observed by A |
| 1.2 | Runtime unit test: submit while a job is running on the same handle | `BUSY`; the second statement is never sent (fixture-side statement counter unchanged) |
| 1.3 | Duplicate `request_id` submit, 1000 iterations with concurrent tasks | Exactly one execution per id (server-side counter) |
| 1.4 | Retention: fill buffers to the per-owner byte cap, then submit | Refused with `CAPACITY`; RSS growth bounded per Contract 10 |
| 1.5 | Reset or disconnect with jobs in every non-terminal state (paused-time test) | Every job terminal within the cleanup bound; job count 0 after release; capacity reusable |
| 1.6 | Renderer timeout simulation: host job continues after the renderer gives up | Job reaches a terminal state that remains retrievable by the same owner until reset; no duplicate execution |

**Values.**

| Name | Current | Proposed | Status |
| --- | --- | --- | --- |
| Running jobs per handle | n/a (1 physical connection: `mysql.rs:55` etc.) | 1 | PROPOSED |
| Queue depth per handle | n/a | 0 (refuse, never queue) | PROPOSED |
| Concurrent jobs per owner | 16 requests (`database.rs:35`) | ≤ 8 (= handles) | PROPOSED |
| Retained result bytes per job | 1 MiB per result set (`read_only.rs:23`, IMPLEMENTED) | 1 MiB per result set, ≤ 16 result sets, job total ≤ 4 MiB (the job cap wins: later result sets are truncated and flagged) | PROPOSED |
| Retained result bytes per owner | n/a (8 × 1 MiB characterized in `resource-profiles.md:16`) | 8 MiB | PROPOSED |
| Terminal-result retention TTL | n/a | 300 s after terminal, or until fetch/release | PROPOSED |
| Terminal job records per owner | n/a | 64 (oldest released first; never a running job) | PROPOSED |

---

## Contract 2 — Read-write and multi-statement execution

**Policy.**

1. **Mode separation.** Execution mode is explicit and per handle: `read_only` (default) or `read_write`, chosen at connect time and fixed for the handle's life. The existing `read_only_query` command and `Connections::read_only` path (`read_only.rs:61-107`) stay **byte-for-byte unchanged in behavior**: same lexer, engine read-only transaction, limits, 3 s deadline and discard. Write execution uses a new runtime entry point and new host command. It never relaxes the read-only lexer or prelude. A `read_only` handle refuses every write command with `FORBIDDEN`.
2. **Capability gating.** Write and transaction commands call `Capabilities::require(Mutations | Transactions | MultipleResults | Routines)` (`core/lib.rs:52-58`). Until per-engine evidence records `Supported`, they fail closed as `UNVERIFIED_CAPABILITY`.
3. **Statement-splitting authority.**
   - **Phase A (recommended first):** exactly one statement per job. No client-side splitting and no server multi-statement mode. A trailing `;` is tolerated, as today (`read_only.rs:283-288`); anything after it is refused.
   - **Phase B (separate approval):** scripts split **client-side in the runtime** by a dialect-aware tokenizer extended from the existing lexer. It sends one statement per round trip, refuses on ambiguity (MySQL `DELIMITER`, unsupported quoting, PostgreSQL dollar quoting until supported, unterminated literals), and never falls back to the server's multi-statement mode.
   - Server multi-statement execution stays disabled because its semantics differ by engine. A PostgreSQL simple-query string with several statements runs as one implicit transaction; MySQL/MariaDB autocommit each statement. Either way, per-statement outcome reporting and cancellation granularity are lost.
4. **Transaction control is API-only.** User SQL containing `BEGIN`, `START TRANSACTION`, `COMMIT`, `ROLLBACK`, `SAVEPOINT`, `RELEASE`, `SET autocommit`, `SET TRANSACTION`, `LOCK TABLES`/`UNLOCK TABLES`, `XA` or `PREPARE`/`EXECUTE`/`DEALLOCATE` is refused (`UNSUPPORTED_SQL`). Otherwise lease state (Contract 3) would diverge from server state.
5. **Refused regardless of mode (first slice):**
   - `USE`, `SET`/`RESET` of session identity or safety settings (`sql_mode`, `ROLE`, `SESSION AUTHORIZATION`, `search_path`), `DISCARD`, `HANDLER`.
   - MySQL `LOAD DATA LOCAL` (the server could request client files). It must also be refused at protocol level — **spike**.
   - PostgreSQL `COPY ... FROM STDIN/TO STDOUT/PROGRAM`.
   - SQLite `ATTACH`/`DETACH`, `load_extension` and write `PRAGMA`s. `ATTACH` would bypass the D014 grant boundary, so it must also be refused at engine level, for example via the authorizer or `SQLITE_LIMIT_ATTACHED=0` — **spike**.
6. **Per-statement outcome.** Every executed statement produces `{index, kind (result|affected), outcome, error_code?}`. Scripts stop at the first failure (Phase B default; continue-on-error is not offered). Statements after a failure are reported `not_executed`, never omitted.
7. **DDL implicit commit (MySQL/MariaDB).** Outside a lease, DDL runs autocommitted and is reported per statement. **Inside a lease, the implicit-commit class is refused** (Contract 3). The conservative rule for MySQL/MariaDB leases is: only `SELECT`/`INSERT`/`UPDATE`/`DELETE`/`REPLACE` are allowed; everything else is refused. PostgreSQL and SQLite DDL is transactional and may run inside a lease. A multi-object schema operation is never reported as atomic on MySQL/MariaDB (`docs/architecture/data-integrity.md:21`).
8. **Procedures and multiple result sets.**
   - `CALL` is allowed only in `read_write` mode, outside a lease, with the `Routines` capability `Supported`.
   - A MySQL/MariaDB procedure may itself `COMMIT`/`START TRANSACTION`, and a PostgreSQL procedure may `COMMIT` when not inside a transaction block. A `CALL` is therefore classed as **may-commit**: its outcome is `committed` only if the call completed with server OK, and `unknown` otherwise.
   - Every result set returned is kept in order, each with its own index, row and byte caps, up to the job caps. The response uses `QueryResult.result_sets` (`v1.rs:160-164`).
9. **SQLite read-write is a new decision**, not a flag flip. It needs a separate writable open mode and a new grant kind (today only `Sqlite`/`SshKey`, `lib.rs:190-194`, all opened read-only, `sqlite.rs:47-58`). It also needs directory-write implications (journal/WAL files) and a D014/D015 amendment.

**Rationale.** M003 deliberately excluded mutation, `CALL`, multi-statement scripts and leases (`.ref/ms/M003-mysql-connectivity.md:13`). Indirect writes through definer views were proven possible outside the read-only transaction (`docs/development/read-only-safety.md:11-17`), so the read-only path's safety depends on its prelude and must not be shared with write code. Client-side splitting and API-only transaction control keep the runtime the single authority on what was sent and in what transaction.

**Enforcement point.**

- **Runtime:** mode check, classifier refusal and statement accounting.
- **Engine and privileges:** authoritative.
- **Core:** capability gating.
- **Host:** mode carried in the connect request and checked on every command.
- **UI:** displays the classification; never authorization.

**Outcome codes.** Existing: `UNSUPPORTED_SQL`, `UNSUPPORTED_CAPABILITY`, `UNVERIFIED_CAPABILITY`, `FORBIDDEN`, `QUERY_REJECTED`. PROPOSED: per-statement `not_executed` status (statement-level, not an error code).

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 2.1 | Re-run the whole M003/M004 read-only suite (lexer unit tests `read_only.rs:367-463`, `test:native-database`, `test:native-postgres`, SQLite runtime tests) after write code lands | 100% pass with no test edits; diff review shows `read_only.rs` execution logic unchanged |
| 2.2 | Mutation test: route a write command through a `read_only` handle | `FORBIDDEN`; fixture row counter unchanged |
| 2.3 | Refusal corpus (≥ 60 cases: transaction control, `SET autocommit=0`, `USE`, `LOAD DATA LOCAL`, `COPY`, `ATTACH`, comments hiding a second statement, `DELIMITER`) on all 4 engines | 100% refused before send (statement counter unchanged) |
| 2.4 | Engine-level `ATTACH` refusal with the classifier disabled (mutation) | SQLite engine refuses; no file created |
| 2.5 | MySQL/MariaDB: DDL inside a lease | Refused; lease still open; a subsequent `ROLLBACK` confirms prior DML undone |
| 2.6 | Phase B script of 5 statements, failure at #3 | Per-statement report `ok, ok, failed, not_executed, not_executed`; server state matches exactly |
| 2.7 | `CALL` returning 3 result sets plus OK on MySQL 8.4.6/MariaDB 11.4.8; PostgreSQL `CALL` with an internal `COMMIT` | All result sets in order with caps; outcome `committed` only on OK; procedure `COMMIT` documented |
| 2.8 | `cargo tree`/`rust:boundaries` | No Tauri/Axum in shared crates (unchanged gate) |

---

## Contract 3 — Transaction ownership

**Policy.**

1. **Lease.** `begin_transaction(owner, handle)` returns an opaque `LeaseId` bound to `(owner, ConnectionId, generation, server session identity)`. Server session identity is the MySQL `CONNECTION_ID()`, the PostgreSQL backend PID, or the SQLite worker. Every statement, commit, rollback and cancel call checks all four.
2. **Pinning model — a decision is required. Both options are PROPOSED; this document does not pick one silently:**

   | Option | Description | Pros | Cons |
   | --- | --- | --- | --- |
   | **3-A handle-exclusive (recommended)** | The lease takes the handle's single pooled connection. While it is open, every non-lease command on that handle returns `BUSY` | No change to pool size or the 8-handle capacity arithmetic; one physical session per handle stays true | Users who want to read while a transaction is open need a second handle |
   | 3-B pool of 2 | Raise `max_connections` to 2 for `read_write` handles | Reads continue alongside the lease | Physical sessions rise to 16 against the 8-slot registry; capacity math, observers and the SSH `MAX_FORWARDS = 4` budget (`ssh.rs:27`) all change |

3. **Durations.** Each lease has an idle timeout (reset only by a statement on the lease), a maximum lifetime (no renewal beyond it), and cannot outlive its handle's absolute expiry (`lib.rs:511`). Renewal is implicit by activity only; there is no keep-alive API. Leases hold locks, so long idle leases are a server-side hazard.
4. **Counts.** At most one open lease per handle, and at most N per owner (Contract 10).
5. **Nesting and savepoints.** `begin` on a handle with an open lease returns `BUSY`. Savepoints are **not offered in the first slice**; user `SAVEPOINT` SQL is refused (Contract 2.4). A later slice may add an API with runtime-generated savepoint names.
6. **Session-state isolation.** Within a lease, session state (user variables, temp tables, advisory or `GET_LOCK` locks, PostgreSQL `SET LOCAL`) persists across that lease's statements, which is expected. **Across leases and jobs, nothing persists**: a lease's physical connection is **always discarded** at lease end (commit, rollback, expiry or failure) and never returned to the pool. This extends the existing `close_on_drop()` rule (`read_only.rs:124-126`). No reset protocol (`COM_RESET_CONNECTION`, `DISCARD ALL`) is relied on in M006.
7. **Server-state verification.** After each lease statement, the runtime checks the server's transaction status. Sources: the MySQL `SERVER_STATUS_IN_TRANS` OK-packet flag, the PostgreSQL ReadyForQuery status (`I`/`T`/`E`), and SQLite autocommit state. Whether SQLx exposes these is a **spike**. If the server says "not in transaction" while the lease says open, the lease is terminated with outcome `unknown` (unexpected implicit commit or rollback). PostgreSQL status `E` (aborted, SQLSTATE 25P02 on later statements) puts the lease in `aborted`; only `rollback` is then accepted.

**Rationale.**

- D004 (`architecture-decisions.md`, register row D004) assigns principal-owned pools, leases and generations to M006. It is still a **Recommendation**, so the lease concept itself is unapproved until D004 is reviewed.
- `docs/architecture/data-integrity.md:19` and `docs/architecture/connection-management.md:23` already require one pinned physical connection per lease and "reset verifiably or discard".
- Today's discard-always policy makes the reset proof trivial. Keeping discard (rather than introducing reset) means the proof obligation is only "discard happens on every exit path", which is testable.
- The cost of discard is reconnect latency. For example, PostgreSQL IPC TLS p50 was 28.4 ms (debug characterization, `.ref/ms/M004-postgres-sqlite.md`, evidence section).

**Enforcement point.**

- **Runtime:** the lease registry inside `Connections`, owner, generation and session checks, timers, discard.
- **Engine:** authoritative transaction state.
- **Host:** token check.
- **UI:** displays the "transaction open" banner and countdown only.

**Outcome codes.** PROPOSED: `BUSY`, `LEASE_EXPIRED`, `TRANSACTION_ABORTED` (PostgreSQL `E` state, or MySQL deadlock 1213 which the server reports as a whole-transaction rollback; see Open question Q4). Existing: `FORBIDDEN` (foreign or stale lease), `CONFLICT`.

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 3.1 | Foreign owner, stale generation, a different handle, and a reconnected handle (same profile) each use the `LeaseId` | 100% `FORBIDDEN`; the server session sees no statement |
| 3.2 | **Session-state sentinel**, 100 cycles per engine: lease N creates a temp table, user variable, advisory lock and `SET` value; then commit or rollback; lease or job N+1 on the same handle checks for all four | 0/400 sentinels visible; independent observer (`processlist`/`pg_stat_activity`) shows lease N's session gone ≤ 5 s after lease end |
| 3.3 | Mutation test: remove the discard on lease end | 3.2 fails |
| 3.4 | Idle timeout and max lifetime under paused time; activity just before timeout extends idle but never max | Lease terminated at the bound ± one timer tick; rollback per Contract 4 |
| 3.5 | Handle-exclusive (3-A): a read job during an open lease | `BUSY`; no second physical session observed |
| 3.6 | PostgreSQL: a statement error inside a lease, then another statement | Lease `aborted`; the second statement is refused before send; rollback confirmed |
| 3.7 | Unexpected-state detection: fixture-only trigger that commits implicitly (MySQL, classifier mutation) | Lease ends `unknown` and is never reported committed or rolled back |

**Values.**

| Name | Current | Proposed | Status |
| --- | --- | --- | --- |
| Open leases per handle | n/a | 1 | PROPOSED |
| Open leases per owner | n/a | 2 | PROPOSED |
| Lease idle timeout | n/a | 60 s | PROPOSED |
| Lease max lifetime | n/a (handle lifetime 3600 s, `lib.rs:84`) | 15 min, capped by the handle's remaining lifetime | PROPOSED |
| Savepoints | n/a | not offered | PROPOSED |
| Connection reuse after a lease | never (`read_only.rs:126`) | never (discard) | IMPLEMENTED pattern, PROPOSED to keep |

---

## Contract 4 — Automatic rollback and cleanup

**Policy.**

1. **Triggers:** lease idle or max expiry; handle expiry (`lib.rs:533-549`); explicit disconnect (`lib.rs:578-588`); renderer reload (`apps/desktop/src-tauri/src/lib.rs:158` → `lib.rs:592-603`); shutdown (`apps/desktop/src-tauri/src/lib.rs:215-229` → `lib.rs:606-621`); connection or tunnel loss (`lib.rs:173-181`, `ssh.rs:320`).
2. **Procedure, every trigger, bounded:**
   1. Mark the lease `closing` and refuse new statements.
   2. If a statement is in flight, request engine cancellation (Contract 5) and wait ≤ the cancel window.
   3. If the connection is believed alive, send `ROLLBACK` with a rollback deadline.
   4. **Discard the physical connection in every case.**
   5. Record a terminal outcome.
3. **Rollback is reported only when confirmed.** `RolledBack` requires that an explicit `ROLLBACK` round trip returned server OK on the leased session. Any other path (socket close, pool close, tunnel death, deadline, process exit) is `Unknown`, with a fact field `commit_sent: bool`. When `commit_sent=false` the UI may say: "StoneQL never sent COMMIT; the server normally discards uncommitted work when the session ends, but this was not confirmed."
   - This generalizes the existing tested precedent (`core/lib.rs:183-190`) and the `reset()` doc comment (`lib.rs:589-591`).
   - The comment at `read_only.rs:183` ("Closing also rolls back this private safety transaction") applies to a READ ONLY transaction that cannot have written. It is **not** a confirmation mechanism for user transactions and must not be cited as one.
4. **Known non-transactional effects** are never covered by a confirmed rollback, and the result must say so: MyISAM and other non-transactional tables, PostgreSQL sequence advances, advisory locks taken and released, external or UDF effects.
5. **Reporting across reload.** The renderer that owned the lease is gone after reload, and its session token is invalidated (`apps/desktop/src-tauri/src/lib.rs:151-165`). The host keeps a bounded, SQL-free summary of terminal outcomes from the previous generation (counts of `rolled_back` / `unknown` with `commit_sent`). The next bootstrap shows it once. This is PROPOSED; the content is open question Q6. **Implementation note (P3, 2026-10-29, Proposed / awaiting approval):** instead of a one-time count summary, the host keeps each unresolved `unknown` (lease end or job) with SQL-free facts and the database/file name, lists it to every later renderer of the same process until the user acknowledges it, and refuses new read-write work at 16 unacknowledged per owner. Resolved outcomes are not reported after reload. Answer proposed for Q6: no SQL, values, host, user or credentials; database or SQLite file name only (no profile name, which the host never receives).
6. **Shutdown bound.** Shutdown must finish within a fixed bound even with a hung server or tunnel. The rollback attempt is best-effort inside that bound; after it, connections are dropped and the outcome is `unknown`. The renderer close confirmation (`request_close`, `apps/desktop/src-tauri/src/lib.rs:231-232`) should warn while leases are open (UX only).

**Rationale.** A lease holds locks and uncommitted work on a server the user may share. Silent abandonment is acceptable only if bounded and honestly reported. Today, `reset`/`shutdown` await `Entry::close` → `pool.close()` (`lib.rs:183-188,600-602,618-620`). With a leased connection checked out, that await must stay bounded.

**Enforcement point.** Runtime (single close path, `Entry::close` extended), with the host triggering reset and shutdown. UI is informational.

**Outcome codes.** Existing `Outcome::RolledBack` (confirmed only) and `Outcome::Unknown`. PROPOSED: `LEASE_EXPIRED`; fact fields `commit_sent` and `rollback_confirmed`.

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 4.1 | For each trigger × each engine (MySQL, MariaDB, PostgreSQL, SQLite; plus MySQL/PostgreSQL over SSH): an open lease with an uncommitted insert, then trigger | Independent observer: row absent, session gone ≤ 5 s; reported outcome is `rolled_back` **only** when the fixture log shows `ROLLBACK` OK, otherwise `unknown` |
| 4.2 | Fault injection: drop the TCP/socket (`iptables`-free: kill the fixture proxy/tunnel) before `ROLLBACK` | Outcome `unknown`, `commit_sent=false`; never `rolled_back` |
| 4.3 | Shutdown with a lease whose server is paused (`docker pause`) | Process exits ≤ shutdown bound; outcome recorded `unknown` |
| 4.4 | Mutation: report `RolledBack` on pool close | A test fails |
| 4.5 | 100 mixed lease/fault cycles per engine (roadmap advanced-execution profile) | 0 dirty connections reused (Contract 3.2 sentinel), 0 orphan sessions/tunnels (`open_tunnels()`, `lib.rs:623-631`) |

**Values.**

| Name | Current | Proposed | Status |
| --- | --- | --- | --- |
| Rollback deadline | n/a | 2 s | PROPOSED |
| Shutdown total bound | unbounded await of `pool.close()` (`lib.rs:618-620`) | 5 s | PROPOSED |
| Session-gone observation bound | 5 s cleanup (roadmap lifecycle, `roadmap.md:77`, proposed) | 5 s | PROPOSED (not approved, D010) |

---

## Contract 5 — Cancellation and timeouts

**Policy.**

1. `cancel(owner, job|lease)` is **best effort**. Accepting the request moves the job to `cancelling`; it never implies termination or rollback.
2. **Per-engine mechanism.** Every one requires a spike before the `Cancellation` capability can be recorded `Supported`; `docs/architecture/database-adapters.md:22` already requires this.

   | Engine | Mechanism | Confirmation signal | Constraints |
   | --- | --- | --- | --- |
   | MySQL / MariaDB | `KILL QUERY <id>` on a **second, ephemeral** connection built from the handle's own connect options (the pattern of `pool.connect_options()` already used at `sqlite.rs:104`). Session id captured at acquire (`CONNECTION_ID()`, or from the handshake if SQLx exposes it — spike) | Running statement fails with ER 1317 (query interrupted); MariaDB `max_statement_time` 1969 | Cannot use the pool (`max_connections(1)`). Same-user kill needs no `PROCESS`, but this must be verified per engine. Through SSH it consumes one of 4 forwards (`ssh.rs:27`); forwards beyond 4 are silently dropped (`ssh.rs:403-405`), so the cancel connection must use a short connect deadline and fail fast |
   | PostgreSQL | Protocol CancelRequest (backend PID + secret key) to the same endpoint. Through SSH that is the tunnel's Unix socket (D018). Whether SQLx exposes it publicly, and its TLS behavior, is unknown (spike). Fallback: `pg_cancel_backend(pid)` on a second authenticated connection | SQLSTATE 57014 on the running statement | Same forward budget. Server `statement_timeout` (`postgres.rs:107-113`) stays as a backstop |
   | SQLite | Interrupt flag checked by the existing progress handler (`sqlite.rs:109-114`) | `SQLITE_INTERRUPT` | Inside an explicit transaction, an interrupted write rolls back the **whole** transaction. That must be reported per Contract 4, not as a statement-only failure |

3. **Deadlines, in strict order:** server-side timeout (where available) < per-statement client deadline < per-job deadline. The renderer **must not** run a timer that implies an outcome for jobs. Submission returns a `JobId` immediately, and the 8 s renderer wait (`nativeHost.ts:75`) applies only to the submit and poll calls themselves. On a client deadline the runtime takes the same path as a user cancel: cancel → wait for the cancel window → discard.
4. **MySQL/MariaDB write deadline.** Today a timeout only drops the future (`read_only.rs:86-105`). For a write, the server may run the statement to completion and autocommit it. Therefore a write deadline **must** issue `KILL QUERY` before discard, and still reports `unknown` unless 1317 is observed and the statement was in a lease that is then rolled back with confirmation.
5. **Terminal cancel states:** `cancelled_confirmed` (the server signal above was observed) and `cancel_requested_unconfirmed` (no signal within the window; connection discarded). Neither implies "no effect". Confirmed cancellation means the server stopped the statement; partial or non-transactional effects stay possible (Contract 4.4).

**Rationale.** M003 explicitly says "A client timeout/close is not confirmed server-side cancellation" (`.ref/ms/M003-mysql-connectivity.md:23`), and the core message already says so (`core/lib.rs:100-102`). M006 must add a real cancel path without overclaiming.

**Enforcement point.** Runtime (engine modules own the mechanism; registry owns state). Host passes cancel requests with the token. Core capability gating applies.

**Outcome codes.** Existing `CANCELLED`, `DEADLINE_EXCEEDED`. PROPOSED: a `cancellation: none | requested | confirmed` field on job and statement results (preferred over new `Outcome` variants; see Contract 8).

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 5.1 | Per engine (plus MySQL and PostgreSQL over SSH): `SLEEP`/`pg_sleep`/CPU-bound recursive query, cancel after 500 ms, 100 samples | Cancel accepted → terminal client state p95 ≤ 2 s; server observer confirms the statement stopped in ≥ 99/100 samples; any non-confirmed sample is reported `cancel_requested_unconfirmed` |
| 5.2 | Cancel with all 4 SSH forwards busy | Terminal ≤ cancel window + 1 s, `cancel_requested_unconfirmed`, connection discarded; no hang |
| 5.3 | MySQL autocommit `UPDATE` (long) hits the client deadline | Observer shows `KILL` issued; outcome `unknown` unless 1317 observed; never `committed`/`rejected` by assumption |
| 5.4 | SQLite interrupt inside a lease | Lease reported terminated; whole transaction rolled back confirmed by state check, or `unknown` |
| 5.5 | Foreign-owner cancel | `FORBIDDEN`; job unaffected |
| 5.6 | Read-only path regression: existing 3 s deadline tests (`postgres-sqlite.md:46-48`) | Unchanged results |

**Values.**

| Name | Current | Proposed | Status |
| --- | --- | --- | --- |
| Cancel → terminal p95 | n/a | ≤ 2 s | PROPOSED in `roadmap.md:79`; **not approved** (D010) |
| Cancel confirmation window | n/a | 1.5 s | PROPOSED |
| Cancel connection connect deadline | n/a (setup deadline 3 s, `apps/desktop/src-tauri/src/lib.rs:83`) | 1 s | PROPOSED |
| Concurrent cancel connections | n/a | 1 per handle, 4 per owner | PROPOSED |
| Per-statement deadline (read-write) | read-only fixed 3 s (`database.rs:189`), runtime cap 30 s (`read_only.rs:71`) | default 30 s, max 300 s | PROPOSED |
| Per-job deadline | n/a | 600 s | PROPOSED |
| PostgreSQL server timeout | `SET LOCAL statement_timeout` = client deadline (`postgres.rs:107-113`, D012); read-write jobs: session `statement_timeout` = statement deadline − 1 s, at most half the deadline, never 0 (L-3, revised 2026-10-29; the earlier deadline + 2 s was not approved) | same, for read-write statements | IMPLEMENTED (read path, unchanged) / implemented-verified server-first, value PROPOSED (write) |
| MySQL server timeout | none on the read path | MariaDB `max_statement_time`; MySQL `max_execution_time` applies to SELECT only | PROPOSED (spike) |

---

## Contract 6 — Connection loss and stale-session invalidation

**Policy.**

1. A connection identity is `(OwnerId, ConnectionId, registry generation)`. Every reconnect mints a new `ConnectionId` (`lib.rs:509`). Jobs and leases bind to it and become permanently invalid when it closes.
2. **Reconnect never resumes a transaction.** No automatic reconnect for write-capable handles (today: manual reconnect only, `api-contract.md:98`). A lost lease is terminal `unknown` (Contract 4). A new handle starts with no lease.
3. Stale replies are refused at three layers, all preserved and extended to jobs and leases:
   - the runtime generation check (`lib.rs:517-519`);
   - the host session recheck after execution, returning `unknown` (`database.rs:213-217`);
   - the frontend generation guard (`workspace.ts:129-145`).
   A stale success is **never** shown as success.
4. **Loss detection:** driver I/O error; `Entry::usable` (pool closed or tunnel closed, `lib.rs:173-181`); SSH keepalive (≈90 s worst case, `ssh.rs:150-151`). A detected loss with a lease or job in flight immediately takes the Contract 4 path. It does not wait for lease expiry.
5. Schema metadata cache entries are keyed by generation and invalidated on loss or reconnect (`connection-management.md:37`). This is shared with the metadata part of M006 and not detailed here.

**Rationale.** The reload repair (`.ref/ms/M003-mysql-connectivity.md:81-88`) proved that stale tokens must not resolve. Transactions raise the stakes, because a stale "committed" display is a data-integrity defect.

**Enforcement point.** Runtime (generation and identity), host (token), UI (display guard; not a security control).

**Outcome codes.** Existing `FORBIDDEN` + `unknown` for stale replies; `CONNECTION_LOST` + `unknown` (`core/lib.rs:151-156`).

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 6.1 | Reset during a running job and during an open lease (paused time + real fixture) | Late reply → `FORBIDDEN`/`unknown`; never `committed`; job and lease terminal |
| 6.2 | Kill the server session (`KILL <id>` / `pg_terminate_backend`) during a lease | `CONNECTION_LOST`/`unknown` within one statement or 5 s; a new handle has no lease |
| 6.3 | SSH tunnel death (kill sshd channel) during a lease | Detected ≤ keepalive bound; terminal `unknown`; 0 orphan tunnels |
| 6.4 | Frontend: result for a superseded generation (existing pattern) extended to jobs/leases | Discarded in 100% of browser scenarios, both themes |

---

## Contract 7 — Unknown commit outcomes and retry

**Policy.**

1. Once `COMMIT` (or a may-commit statement: autocommit DML/DDL, `CALL`) is **sent**, any failure to receive a definitive server reply is `Outcome::Unknown`, with `commit_sent=true`. This covers timeout, socket error, tunnel loss, process exit, renderer reload and a stale generation.
2. **Never auto-retry writes**: not in the runtime, the host, or the UI. No replay on reconnect. `retryable` stays `false` for every write and transaction outcome (as today, `v1.rs:238`).
3. **Read-only.** M006 keeps "no automatic retry" for all SQL (current policy, `connection-lifecycle.md:22`). `retryable=true` may be set **only** for read-only-mode requests that failed **before any statement was sent** (`CAPACITY`, `CONNECTION_UNAVAILABLE` at acquire). Even then it is a UI hint for a user-initiated re-run, not an automatic retry.
4. **Unknown outcomes require reconciliation UX:** the message states what is known (`commit_sent`, statement index) and directs the user to verify on the server. A one-click "retry commit" or "run again" action on an `unknown` write is not offered.
5. Duplicate-submit protection (Contract 1.4) covers renderer double-click and IPC retries. It is not a server-side idempotency guarantee.

**Rationale.** `data-integrity.md:19` says "Network loss during commit has an unknown outcome; expose that status and require reconciliation rather than retrying"; D002 says "No automatic retry of failed SQL".

**Enforcement point.** Runtime (outcome assignment; no retry loop exists), host (no retry), UI (no retry affordance on unknown writes; enforced by a test, not by authority).

**Outcome codes.** Existing `Outcome::Unknown`, `CONNECTION_LOST`, `DEADLINE_EXCEEDED`. PROPOSED fact field `commit_sent`.

**Acceptance criteria (unexecuted).**

| # | Test method | Pass threshold |
| --- | --- | --- |
| 7.1 | Fault proxy drops the connection **after** forwarding `COMMIT` to the server but before the OK reaches the client; 50 runs per network engine | 50/50 reported `unknown`, `commit_sent=true`; the server shows the commit applied in ≥ 1 run (proves the test can produce the dangerous case); 0 automatic re-sends (proxy statement log) |
| 7.2 | Same with the drop **before** `COMMIT` reaches the server | `unknown`, `commit_sent=true` (the client cannot distinguish); never `rolled_back` |
| 7.3 | Static/grep test: no loop or reconnect around write execution in runtime/host; UI has no retry action for `unknown` writes | Review checklist + browser test pass |
| 7.4 | `retryable` matrix test over every error constructor | `true` only for the pre-send read-only cases in 7.3 above |

---

## Contract 8 — Outcome taxonomy

**Policy: every job, statement and lease reports exactly one of the following.**

| Taxonomy state | Meaning | `Outcome` (existing `core/lib.rs:131-136`) | Error code | Extra facts (PROPOSED) |
| --- | --- | --- | --- | --- |
| Confirmed success (read) | Server completed; rows within caps | — (success payload) | — | `truncated` (exists, `v1.rs:158`) |
| Confirmed success (write, autocommit or `COMMIT` OK) | Server OK received | `Committed` (in success payload, PROPOSED field) | — | `affected_rows` (exists as string, `v1.rs:157`; currently hard-coded `"0"` at `read_only.rs:152`; to be populated for writes — semantics spike: MySQL found vs changed rows) |
| Confirmed failure, no effect | Server error before effect, or a refusal before send | `Rejected` | `QUERY_REJECTED`, `UNSUPPORTED_SQL`, `CONFLICT`, … | `statement_index` |
| Confirmed failure inside a lease | Statement error; lease still open (MySQL) or aborted (PostgreSQL) | `Rejected` (statement) | `QUERY_REJECTED` / `TRANSACTION_ABORTED` (PROPOSED) | `transaction_state` |
| Rolled back (confirmed) | Explicit `ROLLBACK` OK | `RolledBack` | — or `LEASE_EXPIRED` (PROPOSED) | `non_transactional_effects_possible` |
| Cancelled — confirmed | Server interrupt signal observed (Contract 5) | `Unknown` for writes outside a confirmed rollback; `Rejected` for read-only | `CANCELLED` | `cancellation: confirmed` |
| Cancelled — requested only | No server signal; connection discarded | `Unknown` | `CANCELLED` | `cancellation: requested` |
| Deadline exceeded | Client deadline; cancel path taken | `Unknown` (unless a server signal is confirmed, as above) | `DEADLINE_EXCEEDED` | `cancellation` |
| Unknown | Lost connection, commit lost, stale generation, shutdown | `Unknown` | `CONNECTION_LOST` / `FORBIDDEN` | `commit_sent` |
| Not executed | Earlier statement failed (script) | — | — | `statement_status: not_executed` |

Rules:

1. Read-only jobs keep today's mapping (`database.rs:58-78`).
2. `Committed` is never inferred. It requires server OK for the committing statement.
3. **Confirmed cancellation of a write is still `Unknown`** unless followed by a confirmed rollback, because MySQL/MariaDB non-transactional engines and PostgreSQL sequences can retain partial effects.

**Needed additions (all PROPOSED; each is a v1 contract change requiring a D003 compatibility/version decision because the decoder is closed, `api-contract.md:9`):**

| Addition | Kind | Needed for | Necessity |
| --- | --- | --- | --- |
| `BUSY` | `ErrorCode`/`ErrorCodeDto` | 1, 3 | Required (or reuse `CONFLICT`; see Q3) |
| `LEASE_EXPIRED` | ErrorCode | 3, 4 | Required |
| `TRANSACTION_ABORTED` | ErrorCode | 3, 8 | Required for PostgreSQL |
| `JOB_EXPIRED` | ErrorCode | 1 | Optional (could be `FORBIDDEN`) |
| `cancellation: none\|requested\|confirmed` | field on job/statement result | 5, 8 | Required. Preferred over new `Outcome` variants, which would conflate effect with mechanism |
| `commit_sent: bool`, `transaction_state` | fields | 4, 7 | Required |
| `outcome` on **success** payloads | field | 8 | Required for writes. Today only `ErrorDto` carries an outcome (`v1.rs:199-204`) |
| Job, lease, statement-result and fetch-page DTOs | new DTOs | 1–3 | Required; generated TS, golden fixtures |

No new `Outcome` variant is proposed. Open question Q5 asks whether a `NotCommitted` variant is worth the version bump.

**Enforcement point.** Core (vocabulary, `CoreError` constructors), contracts (DTO + golden fixtures), runtime (assignment), host (projection, fixed messages).

**Acceptance criteria (unexecuted).**

| # | Test | Threshold |
| --- | --- | --- |
| 8.1 | Golden serde + generated TS for every new DTO/variant; `contracts:check` deterministic regeneration | Zero diff; strict decoder rejects unknown fields |
| 8.2 | Table-driven runtime test: each fault scenario (Contracts 4, 5, 7) → exact taxonomy row | 100% match; no `Committed`/`RolledBack` without a server-OK fixture log |
| 8.3 | Diagnostics test: no SQL, row data or driver text in any error/outcome DTO | 0 occurrences (extends `read_only.rs:371-383`) |

---

## Contract 9 — Production write safeguards (UX, not authorization)

**Policy.**

1. **Authority is the database.** Server privileges and accounts are authoritative. StoneQL safeguards reduce accidents; they cannot stop a compromised renderer. The documentation recommends a separate least-privilege (ideally read-only) account for PROD profiles (`read-only-safety.md:35`).
2. **Read-only default.** Every live handle opens `read_only` unless the connect request explicitly asks for `read_write` (host-enforced mode, Contract 2.1). For profiles tagged `Production`:
   - the renderer defaults to read-only;
   - enabling read-write requires a typed confirmation of the profile name at connect time;
   - the session shows a persistent PROD read-write banner.
3. **Environment over IPC (PROPOSED, advisory).** Add an optional `environment` field to the connect request so the host can require `acknowledge_production_write: true` for PROD read-write opens. This is defence-in-depth against **UI bugs only**, since the renderer controls both fields. It must be documented as not being authorization. Today `environment` never leaves the renderer (`v1.rs:327-348`).
4. **Destructive classification (UX).**
   - A runtime classifier (shared lexer) labels each statement: `read`, `dml`, `dml_unbounded` (`UPDATE`/`DELETE` without `WHERE`), `ddl`, `destructive` (`DROP`, `TRUNCATE`, `ALTER … DROP`, `DELETE`/`UPDATE` without `WHERE`), `routine` (`CALL`) or `unclassified`.
   - **`unclassified` is treated as `destructive`.**
   - On PROD read-write, `destructive`, `routine` and `unclassified` statements require a per-job confirmation that names the profile, reusing the existing modal pattern (`App.vue:529-538,2617-2627`). The host requires the per-job `confirmed_destructive` flag for those classes (again only against UI bugs).
   - Misclassification affects prompts only, never execution authority.
5. Script size is limited (Contract 10) so a confirmation summarises a reviewable amount of SQL.
6. Mock-mode safeguards stay unchanged (`App.vue:600-615,892-905`).

**Rationale.** `connection-management.md:39` and `data-integrity.md:15` state that PROD tags and confirmations are UX safeguards, not authorization. M006's G1 criterion says the same ("Destructive UI safeguards do not substitute for authorization").

**Enforcement point.**

- **Database privileges:** authoritative.
- **Host:** mode and flag checks (non-authoritative).
- **Runtime:** classification.
- **UI:** prompts.

**Outcome codes.** `FORBIDDEN` (write on a read-only handle; missing acknowledgement flag). PROPOSED: `CONFIRMATION_REQUIRED` (or reuse `INVALID_REQUEST`; see Q3).

**Acceptance criteria (unexecuted).**

| # | Test | Threshold |
| --- | --- | --- |
| 9.1 | Browser scenarios, Light and Dark, keyboard-only: PROD profile defaults read-only; enabling read-write requires the typed name; destructive job prompt; Cancel leaves the server untouched | All scenarios pass; focus trap and Escape behave as existing dialogs do |
| 9.2 | Host: PROD read-write open without the acknowledgement; destructive job without the flag | `FORBIDDEN`/refused; fixture statement counter unchanged |
| 9.3 | Classifier corpus (≥ 100 statements, all engines, including comment- and quote-obfuscated `DROP`) | 0 destructive statements classified `read`/`dml`; unknown → `destructive` |
| 9.4 | Privilege boundary: read-write handle with a SELECT-only account | Server denies (MySQL 1142 / PostgreSQL 42501) → `QUERY_REJECTED`/`rejected`; proves the database, not the UI, decided |
| 9.5 | Docs test: public pages say "not authorization" | Present in `security.md`/user guide |

---

## Contract 10 — Limits table

| Name | Current implemented value + source | Proposed value | Measurement method | Approval status |
| --- | --- | --- | --- | --- |
| Result rows per result set | 1..=1000 (`read_only.rs:22,68`; DTO `v1.rs:131`) | 1000 (unchanged) | Existing truncation fixtures | IMPLEMENTED; not release-approved |
| Result bytes per result set (serialized) | 1..=1 MiB (`read_only.rs:23,69`; `v1.rs:132`) | 1 MiB | Existing truncation fixtures | IMPLEMENTED |
| Result sets per job | 1 (`database.rs:197`) | 16 | `CALL` with N+1 results → explicit truncation flag | PROPOSED |
| Retained bytes per job / per owner | 4 MiB delivered per job; per owner bounded by 8 job records (≈6 MiB typical, 24 MiB worst), no byte cap (B-7, 2026-10-29) | 1 MiB per result set, ≤ 16 result sets, job total ≤ 4 MiB (job cap wins); 8 MiB per owner | RSS sampling (`resource-profiles.md` method) with 8 retained max-size jobs | PROPOSED |
| Incoming packet (MySQL/MariaDB) | 2 MiB (`mysql.rs:30-32`) | unchanged | Existing `test:receive-limit:live` | APPROVED (D002, scoped) |
| Per-message cap (PostgreSQL/SQLite) | none (`postgres-sqlite.md:50`) | open | Spike | Open gap |
| Result metadata (columns / descriptor bytes) | none in the driver; post-prepare JSON check only (`read_only.rs:155-160`) | 4,096 columns / 4 MiB (or 16,384) per `findings.md` §6–7 | `findings.md` §9 runner; applies equally to the write path | **Proposed / awaiting approval** (`m003-metadata-limit/findings.md` §7) |
| SQL text per statement | 64 KiB (`read_only.rs:227`; `v1.rs:129`) | 64 KiB | Boundary unit test | IMPLEMENTED |
| Script size (Phase B) | n/a | 64 KiB envelope (`apps/desktop/src-tauri/src/lib.rs:121`), ≤ 100 statements | Boundary unit test | PROPOSED |
| IPC envelope | 64 KiB (`apps/desktop/src-tauri/src/lib.rs:121`); query DTO 128 KiB (`v1.rs:100`) | unchanged | Existing | IMPLEMENTED |
| Per-statement deadline | read-only fixed 3 s (`database.rs:189`); runtime max 30 s (`read_only.rs:71`) | read-write default 30 s, max 300 s | Contract 5.1 / 5.3 fixtures | PROPOSED |
| Per-job deadline | n/a | 600 s | Paused-time unit test | PROPOSED |
| Connect / setup deadline | 3 s; SSH whole setup ≤ 2× (`apps/desktop/src-tauri/src/lib.rs:83`; `lib.rs:309-314`) | unchanged | Existing lifecycle fixtures | IMPLEMENTED |
| Cancel → terminal p95 | n/a | ≤ 2 s | Contract 5.1, 100 samples per engine | PROPOSED (`roadmap.md:79`, not approved, D010) |
| Rollback deadline / shutdown bound | n/a / unbounded await | 2 s / 5 s | Contracts 4.1 and 4.3 | PROPOSED |
| SQLite busy timeout | 1 s (`sqlite.rs:20`, D015) | 1 s read-only; read-write 5 s | Busy-lock fixture | IMPLEMENTED / PROPOSED |
| Logical handles (sessions) | 8 (`apps/desktop/src-tauri/src/lib.rs:81-85`); runtime range 1..=64 (`lib.rs:228`) | 8 | Existing slot-reuse tests | IMPLEMENTED |
| Physical connections per handle | 1 (`mysql.rs:55`, `postgres.rs:65`, `sqlite.rs:69`) | 1 + 1 ephemeral cancel connection (3-A) | Observer session count during cancel | PROPOSED |
| Concurrent host requests | 16 (`database.rs:35`) | 16 (submit/poll/fetch/cancel are short) | Existing slot test | IMPLEMENTED |
| Concurrent running jobs | n/a (synchronous) | 1 per handle, ≤ 8 per owner | Contract 1.2 | PROPOSED |
| Job queue depth | n/a | 0 | Contract 1.2 | PROPOSED |
| Open transactions | n/a | 1 per handle, 2 per owner | Contract 3.1 | PROPOSED |
| Lease idle / max lifetime | handle absolute 3600 s (`apps/desktop/src-tauri/src/lib.rs:84`; UI copy `database.rs:180`) | 60 s / 15 min | Contract 3.4 | PROPOSED; also unify the two 3600 s sources |
| Terminal job records retained | 8 per owner (running + unread), 300 s TTL (2026-10-29; was 64); released by a renderer reset (P3) | 64 per owner, 300 s TTL | Unit test (`b7_…`) | PROPOSED |
| Ended-lease outcomes retained | 32 per owner / 256 overall, resolved or acknowledged only (P3) | unchanged | Unit tests (`r6_…`, `p3_…`) | APPROVED as alpha baseline (2026-10-29) on condition unknowns are not lost |
| Unacknowledged `unknown` outcomes | 16 per owner, then new jobs/leases refused (`UNRESOLVED_OUTCOMES`), never evicted; in memory per app process (P3) | 16 | Unit test (`p3_the_unresolved_cap…`), fixture `unknownOutcomeRetention` | PROPOSED |
| SSH forwards per tunnel | 4 (`ssh.rs:27`) | unchanged; cancel must fit | Contract 5.2 | IMPLEMENTED (D018) |
| SSH dead-path detection | keepalive 30 s × 3 (`ssh.rs:150-151`) | unchanged | Contract 6.3 | IMPLEMENTED |
| File grants | 32 (`lib.rs:222`) | unchanged; writable SQLite grant kind counts toward it | Existing | IMPLEMENTED (D014) |
| Memory / RSS | not capped; characterized 22.9–23.4 MiB peak (`resource-profiles.md:34-37`) | Basic-SQL +64 MiB above idle (`roadmap.md:78`); advanced execution: +128 MiB with 8 handles × max retained results | `/usr/bin/time` method from `resource-profiles.md:17` | PROPOSED (roadmap values not approved) |
| Fault/transaction cycles | n/a | 100 per engine, 0 dirty connections returned | Contract 4.5 | PROPOSED (`roadmap.md:79`) |

---

## Gate matrix

M006 acceptance criteria (`.ref/ms/M006-schema-execution.md`, Acceptance criteria):

- **AC1** = G1 ownership, capabilities and diagnostics
- **AC2** = G2 constraints, atomicity, cancel/DDL/lost-commit outcomes
- **AC3** = G3 multiple results, versions, contracts
- **AC4** = G4/G5 bounded profile, no dirty pooled connection, cancel vs termination, no stale leaks
- **AC5** = M003 read-only regressions and no Tauri/Axum in shared crates

| Contract | G1 | G2 | G5 | M006 AC |
| --- | --- | --- | --- | --- |
| 1 Jobs | owner-scoped jobs (1.1) | duplicate-submit prevention (1.3) | bounded retention, cleanup on reset (1.4–1.6) | AC1, AC4 |
| 2 Read-write / multi-statement | mode, refusal corpus, `ATTACH`/`LOCAL INFILE` (2.2–2.4) | per-statement outcomes, DDL implicit commit (2.5–2.7) | — | AC1, AC2, AC3, AC5 (2.1, 2.8) |
| 3 Transactions | lease ownership (3.1) | aborted state, unexpected implicit commit (3.6–3.7) | discard proof, timers (3.2–3.5) | AC1, AC2, AC4 |
| 4 Rollback/cleanup | — | confirmed-only rollback (4.1–4.2) | bounded shutdown, 100 fault cycles (4.3–4.5) | AC2, AC4 |
| 5 Cancellation | foreign cancel (5.5) | cancelled write = unknown (5.3–5.4) | latency, forward exhaustion (5.1–5.2) | AC2, AC4, AC5 (5.6) |
| 6 Loss / stale | stale token refusal (6.1) | never stale "committed" (6.1–6.2) | detection bounds (6.2–6.3) | AC4 |
| 7 Unknown commit | — | lost-commit unknown (7.1–7.2) | no auto retry (7.3–7.4) | AC2, AC4 |
| 8 Taxonomy | redacted diagnostics (8.3) | exact mapping (8.2) | — | AC2, AC3 (8.1) |
| 9 PROD | not authorization; privilege test (9.2, 9.4) | — | — | AC1 |
| 10 Limits | — | — | all numeric bounds | AC4 |

---

## Prerequisites and decisions before any read-write implementation (ranked by risk)

1. **Outcome and contract versioning (Contracts 1, 7, 8).** Decide how job, lease, statement-result, `BUSY`/`LEASE_EXPIRED`/`TRANSACTION_ABORTED`, `cancellation` and `commit_sent` enter a closed v1 decoder (additive v1 with an agreed tolerant reader, or v2). Wrong or late choices make every outcome claim untestable. *Highest risk: data-integrity misreporting.*
2. **D004 review, then pinning model 3-A vs 3-B and discard-always (Contract 3).** D004 (principal-owned pools, leases and generations) is still a Recommendation, and lease ownership depends on it. This changes capacity arithmetic, the SSH forward budget and the dirty-connection proof.
3. **Per-engine SQLx capability spikes** (required by `database-adapters.md:22` and the M006 prerequisites):
   - MySQL `KILL QUERY` from an ephemeral connection, and session-id capture;
   - PostgreSQL CancelRequest exposure, its TLS behavior and Unix-socket/SSH path;
   - transaction-status flag exposure;
   - multiple result sets and `CALL` via SQLx for MySQL/MariaDB/PostgreSQL;
   - `affected_rows` semantics;
   - protocol-level refusal of `LOAD DATA LOCAL`;
   - SQLite writable open plus engine-level `ATTACH` refusal and interrupt-in-transaction behavior;
   - whether `raw_sql`/prepare can send more than one statement.
   Without these, capabilities stay `Unverified` and fail closed (`core/lib.rs:52-58`).
4. **Refusal list and splitting authority (Contract 2.3–2.5).** Phase A single statement vs Phase B scripts; the MySQL/MariaDB in-lease allowlist.
5. **SQLite read-write file policy.** New grant kind, writable open, journal/WAL directory writes, and the same-user replace race (D014) now with write consequences. Amends D014/D015.
6. **Numeric limits in Contract 10** (D010): especially lease idle/max, deadlines, retention, and the 2 s cancel target.
7. **Metadata cap adoption** (`findings.md` §7). The write path reuses the same driver metadata reads; an unbounded server-supplied count is a process-abort risk for any execution mode.
8. **PROD/mode UX policy (Contract 9),** including whether `environment` crosses IPC.
9. **Post-reload outcome reporting (Contract 4.5)** and its retention.

## What must be preserved from M003, M004 and M005

- The read-only path unchanged: lexer and allowlists (`read_only.rs:226-353`); engine read-only preludes (MySQL text-protocol `START TRANSACTION READ ONLY` `read_only.rs:88-98`, PostgreSQL D012, SQLite D015); `close_on_drop` discard (`read_only.rs:124-126`); limits and the 3 s deadline; `ReadError` → DTO mapping (`database.rs:58-78`).
- Owner scoping on every call; opaque UUID handles; registry generation and late-registration refusal; reset on new bootstrap nonce; bounded shutdown order (pool then tunnel, `lib.rs:182-188`).
- Verified TLS only (MySQL `VerifyIdentity`, PostgreSQL `VerifyFull`), refusal of inherited libpq environment (D011), the 2 MiB packet cap (D002).
- SSH: enrolled host keys only (D017), one tunnel per handle with a private 0700 Unix socket and the original host as TLS identity (D018), agent used for signing only (D019), cancellable bounded setup.
- Secrets: Keychain references resolved only in `connect_direct` (D020); no secrets, SQL or row data in errors or logs; fixed static messages (`core/lib.rs:89-128`).
- SQLite: renderer never sees paths; typed, capped grants revoked on reset/shutdown (D014).
- No automatic SQL retry; client timeout ≠ server cancellation; connection loss never claims rollback.
- Strict 64 KiB envelopes and the 16-request admission limit; browser/mock mode independent; no Tauri/Axum in shared crates (`rust:boundaries`).

## Open questions

- **Q1.** Pinning 3-A (handle-exclusive) or 3-B (two connections)? Recommendation: 3-A.
- **Q2.** Phase A only (single statement) for M006, deferring scripts? Recommendation: yes for the first increment.
- **Q3.** New error codes (`BUSY`, `CONFIRMATION_REQUIRED`, `JOB_EXPIRED`) or reuse of `CONFLICT`/`INVALID_REQUEST`/`FORBIDDEN`? Fewer codes means a smaller contract change but vaguer UX.
- **Q4.** MySQL deadlock (1213) and lock-wait timeout (1205, which depends on `innodb_rollback_on_timeout`): report the server-declared rollback as `RolledBack`, or require an explicit `ROLLBACK` OK as in every other case? Recommendation: always send `ROLLBACK` and confirm.
- **Q5.** Is an `Outcome::NotCommitted` variant ("COMMIT never sent; server-side discard unconfirmed") worth a contract change, versus `Unknown` + `commit_sent=false`?
- **Q6.** What may the post-reload summary contain without SQL or profile secrets (counts only? profile display name?)
- **Q7.** Should savepoints ever be offered, and should PostgreSQL in-lease DDL be allowed in the first slice?
- **Q8.** Does read-only M006 work (multiple result sets for SELECT, read-only cancel, schema metadata) proceed independently of write approval? Recommendation: yes. It needs Contracts 1, 5, 6, 8 and 10 but not 2.4–2.9, 3, 4, 7 or 9.
- **Q9.** Cancel-connection credentials: building it from the pool's retained connect options keeps the password in process memory for the handle's life, which is already the case inside SQLx pools. Is that acceptable under D020, or must cancel re-resolve the Keychain reference?

## Decision for the user: read-write execution in v0.1.0 Alpha (macOS-only)

The user approved v0.1.0 Alpha as macOS-only. The working-tree draft of D021 records this platform decision and explicitly lists the alpha **feature scope** (including M006) as *not approved*. Whether read-write execution is in alpha scope is **the user's decision**; it is not inferred here.

| Option | Scope | Prerequisites that must be approved first |
| --- | --- | --- |
| **A. Exclude read-write (recommended)** | Alpha ships the existing read-only live path. Optionally add read-only M006 items (read-only jobs/cancel, multiple SELECT result sets) if their spikes pass | Contracts 1/5/6/8/10 subset; no write contracts. **Costs no new code to stay safe**: `Mutations`/`Transactions` already fail closed as `Unverified` (`core/lib.rs:52-58`), and no write command exists |
| B. Autocommit single-statement writes, non-PROD only, no transactions | `read_write` mode, Phase A, no leases | Ranked prerequisites 1, 3 (cancel/kill and affected-rows spikes), 4, 6, 8, plus Contract 7 lost-commit fixtures. SQLite writes need prerequisite 5 or must be excluded |
| C. Full transactions | Contracts 1–10 | All nine prerequisites |

**Recommendation: Option A.** Option B or C would require, before any code, all of the following, none of which is approved today:

- a closed-decoder contract change (prerequisite 1);
- a pinning decision (prerequisite 2);
- unexecuted per-engine cancel and transaction-state spikes (prerequisite 3);
- a SQLite writable-open policy (prerequisite 5);
- an approved lost-commit outcome model.

M003/M004/M005 also still have open gates (native GUI, metadata cap, budgets). Option A keeps the alpha's safety story identical to what has been evidenced. Read-write can follow in a later pre-release once Contracts 2–4 and 7 pass their fixtures.
