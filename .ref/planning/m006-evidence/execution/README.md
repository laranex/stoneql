# M006 execution half — read-write jobs, transactions and cancellation evidence

Executed locally on 2026-10-26 under the user's continuation instruction; **uncommitted** for reconciliation review (user instruction: do not commit). macOS ARM (Darwin 27), Node 26.11.1, Rust 1.99.1, Docker Desktop (`desktop-linux`). Disposable digest-pinned MySQL 8.4.6, MariaDB 11.4.8 and PostgreSQL 17.6 TLS fixtures plus temporary SQLite files. No credentials, keys, certificates or user data are retained here.

Implementation choices, limits and every divergence from `../../m006-safety-contracts.md`: [decisions.md](./decisions.md). Public behaviour: `docs/development/sql-execution.md`.

## Files

- `execution-mysql.json`, `execution-mariadb.json`, `execution-postgresql.json`: `npm run test:execution` reports from `crates/stoneql-runtime/examples/execution.rs` (runtime service against the fixtures, observer = fixture admin account over verified TLS).
- `execution-cleanup-*.json`: containers/networks remaining (0) and temporary directories removed.

## Fixture design

`tools/test-connection-lifecycle.mjs --execution` (MySQL, MariaDB) and `tools/test-postgres-fixture.mjs --execution` (PostgreSQL) reuse the existing runners: generated CA, loopback ports, random credentials and marker, ownership check, filtered environment and verified cleanup. Added init: a writer account (`stoneql_writer`; MySQL gains CREATE/DROP/ALTER/INDEX/REFERENCES/TEMPORARY/EXECUTE/ROUTINE privileges on the fixture database, PostgreSQL a new role with CREATE on `public`), `exec_parent`/`exec_item` (FK) and `exec_cycle` tables, and `exec_multi()` (MySQL/MariaDB, two result sets) / `exec_proc(INOUT text)` (PostgreSQL). Outputs go to the ignored `artifacts/m006/`.

## Key results (all three engines)

| Check | Result |
| --- | --- |
| Read-only path | Read-only handle refuses jobs; `read_only` works on read-only and idle read-write handles |
| Owner isolation | Foreign owner cannot fetch, cancel or submit on another owner's job/handle (`FORBIDDEN`) |
| BUSY, no queue | While a job runs: second job, read-only query and `begin_transaction` all `BUSY`; same while a lease is open |
| Exact values | MySQL/MariaDB BIGINT min, BIGINT UNSIGNED max, DECIMAL(38,4), DATETIME(6), zero date, TIME(6) −26:03:04.123456, NULL; PostgreSQL int8 extremes, numeric, timestamp(6), timestamptz in UTC, BC date, ±infinity, NULL vs default |
| Multiple results | Two-statement script → 2 result sets; `CALL exec_multi()` → 2 (MySQL/MariaDB); `CALL exec_proc(NULL)` → 1 (PostgreSQL); MySQL `USE` uses the text-protocol fallback |
| DDL partial outcome | `CREATE TABLE; INSERT; INSERT dup; INSERT` → succeeded, succeeded, failed (unique), not_run; 1 durable row |
| Constraint failures | Unique and FK reported with constraint kind; SQLSTATE 23000 (MySQL/MariaDB) / 23503 (PostgreSQL) |
| Transactions | Commit confirmed and visible to the observer; rollback-only after a failed statement (COMMIT refused); rollback confirmed and invisible; MySQL DDL in a lease refused (implicit commit); PostgreSQL DDL in a lease rolled back |
| Connection loss | Observer kills the lease session (MySQL `KILL`, PostgreSQL `pg_terminate_backend`): next statement `unknown`, lease ended, `end_transaction` → `unknown`, `connection_lost`, `commitSent=false`; row absent |
| Cancellation | 100 samples per engine on a long statement: job `unknown`, cancellation `confirmed`, following statement `not_run`, server interrupt delivered 100/100; statement deadline (1 s) takes the same path |
| Fault cycles | 100 lease cycles per engine (commit / rollback / failing statement / cancel mid-statement): every outcome server-confirmed; next job never sees `@leak`/custom GUC, changed database/search_path or temporary table; 25 commits exactly; writer server sessions 2 → 2 (baseline: expiry lease + an idle, never-job-used pool connection) |
| Backpressure | 100,000-row result with no consumer for 800 ms: statement still running (parked); 17 pages ≤ 500 rows; delivered 8,397 rows, truncated at the 1 MiB per-result bound |
| Lease expiry | Real 60 s idle: rolled back (`expired`), row absent |
| Cleanup | 0 containers, 0 networks, temporary directories removed |

Cancel request → terminal state (ms; debug build, loopback Docker, machine concurrently running other checks; characterization only, not an approved budget):

| Engine | p50 | p95 | max |
| --- | --- | --- | --- |
| MySQL 8.4.6 | 6.8 | 61.3 | 139.5 |
| MariaDB 11.4.8 | 6.4 | 8.6 | 57.6 |
| PostgreSQL 17.6 | 39.6 | 46.1 | 53.9 |

The proposed profile (p95 ≤ 2 s) is asserted by the harness; earlier runs of the same harness measured MySQL p95 5.7–11.4 ms and MariaDB 6.5 ms.

## Safety-fix pass (2026-10-26, uncommitted, at the approval gate)

Re-run after the V-1/V-2/V-3 and B-1..B-5 fixes: `npm run test:execution` exit 0 on all three engines (the JSON files here are from that run; cleanup reports show 0 containers, 0 networks, temporary directories removed). **Read-write remains gated OFF in the desktop host**; the harness drives the ungated runtime service directly. New harness pieces: `tools/tcp-fault-relay.mjs` (test-owned, 127.0.0.1 only, TLS passes through undecrypted; control commands `normal`, `drop_s2c`, `sever_on_reply`, `sever`, `refuse_new`, `stats`), fixture tables `exec_lock`, `exec_digit`, `exec_bulk`, and the server general log (table output) enabled only for the new sections to count executions. New report fields: `writeCancel`, `bulkCancel`, `killFailure`, `lostEndReplies`, `noResend` (MySQL/MariaDB only; PostgreSQL unchanged).

Write cancellation (B-5): an `UPDATE` blocked on a row lock held by the fixture admin, cancelled; cancel → terminal state (ms, debug build, characterization only):

| Engine | n | p50 | p95 | max | KILL delivered (query gone) | Job connection discarded, sessions at baseline (3) | Executed once (general log) | Row value unchanged after lock holder rolled back |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| MySQL 8.4.6 | 30 | 5.9 | 7.7 | 8.9 | 30/30 | 30/30 | 30/30 | 30/30 |
| MariaDB 11.4.8 | 30 | 5.7 | 7.6 | 7.8 | 30/30 | 30/30 | 30/30 | 30/30 |

Every sample: job `unknown`, statement `unknown`, cancellation `confirmed`, `serverCancelSent=true`. The unchanged row is the observed data state (the statement never got the lock), **not** a claimed rollback.

Large `INSERT … SELECT` (10M-row cross join) cancelled after ~700 ms: cancel → terminal MySQL 800/850/878 ms, MariaDB 1360/1296/1259 ms; every sample `unknown`/`confirmed`/executed once; 0 rows remained after the server settled (InnoDB statement atomicity as observed).

Failed KILL connection (relay `refuse_new` after the job connected through it): `unknown`, cancellation `requested` (not confirmed), `serverCancelSent=false`, terminal after 1503 ms (both engines; bound asserted 2500 ms). **Finding:** on both engines the abandoned `UPDATE` was still running on the server 500 ms after StoneQL dropped its connection; the fixture admin killed it before releasing the lock (row not applied). Without that kill it would very likely have applied after the lock was released. The UI therefore says the statement "may still be running on the server" (B-6).

Lost COMMIT/ROLLBACK replies (B-4), n=3 per mode per engine, identical results on MySQL and MariaDB:

| Mode | End took | Outcome | Sent flags | COMMIT/ROLLBACK in server log per run | Row present afterwards (observer) | Repeat `end_transaction` |
| --- | --- | --- | --- | --- | --- | --- |
| commit, reply dropped (`drop_s2c`) | ~10.0 s (COMMIT deadline) | unknown 3/3 | commitSent | 1 | **3/3 committed** | same outcome 3/3, nothing sent |
| commit, severed after the server replied | ~1 ms | unknown 3/3 | commitSent | 1 | **3/3 committed** | same 3/3 |
| commit, severed before the bytes left | <1 ms | unknown 3/3 | commitSent (attempted) | 0 | 0/3 | same 3/3 |
| rollback, reply dropped | ~2.0 s (ROLLBACK deadline) | unknown 3/3 | rollbackSent | 1 | 0/3 | same 3/3 |
| rollback, severed after reply | ~1 ms | unknown 3/3 | rollbackSent | 1 | 0/3 | same 3/3 |
| rollback, severed before send | <1 ms | unknown 3/3 | rollbackSent (attempted) | 0 | 0/3 | same 3/3 |

Never `committed` or `rolled_back` without the server reply; the server did commit in 6/6 runs per engine where COMMIT arrived, which is exactly the case `unknown` exists for. `commitSent`/`rollbackSent` mean "attempted": a sever before transmission still sets them. No re-send: the inserted row's statement exactly once and each end statement at most once in the general log.

Other cancel loop (100 samples, unchanged harness): MySQL p50/p95/max 4.9/6.5/20.8 ms, MariaDB 5.2/6.4/9.6 ms, PostgreSQL 55.2/61.1/62.6 ms.

## Defects found and fixed during evidence runs

- MySQL `information_schema.INNODB_TRX` thread ids are unsigned (harness decode).
- MariaDB `max_recursive_iterations` defaults low; the harness sets it per session (no runtime change).
- PostgreSQL reports a terminated session as a database error (57P01) rather than an I/O error; such errors (class `08`, `57P*`, MySQL 1053/1927) are now treated as connection loss (`unknown`, connection discarded) instead of a confirmed statement failure.
- Session-count assertion now compares against a measured baseline (idle pool connections are legitimate).

## Not executed / open

SSH routing of jobs, leases and cancel connections (read-write over SSH is refused instead); Linux/Windows; GUI-driven native runs; reviewer approval of limits and decisions; user approval to enable the read-write gate. (PostgreSQL lost-COMMIT/write-cancel and the MySQL refused-COMMIT fixture were added in the 2026-10-29 follow-up pass below.)

## R-1..R-3 fix pass (2026-10-29, uncommitted, at the approval gate)

Fixes for the critical findings of `../../m006-fix-review.md`; choices in [decisions.md](./decisions.md). `npm run test:execution` exit 0 on all three engines (the JSON files here are from this run; cleanup reports 0 containers, 0 networks, temporary directories removed). Cancel p95 this run: MySQL 5.3 ms, MariaDB 5.4 ms, PostgreSQL 59.8 ms (characterization only). **Read-write remains gated OFF.**

New report field `policyAndSessionState` (MySQL/MariaDB):

- `generalLogPositiveControl`: a tagged statement sent through the runtime appears in `mysql.general_log` (so zero rows below means "not sent").
- `refusedBeforeSend` (7 inputs × in/out of a lease = 14 per engine, all refused at `submit_job`, general-log rows of any command type containing the tag = 0): `SET STATEMENT max_statement_time=60 FOR COMMIT`, `… FOR CREATE TABLE`, `… FOR PREPARE s FROM 'COMMIT'` (`SetStatement`); `DELIMITER // CASE WHEN 1 THEN COMMIT; END CASE //` and `… SET @x = 1; END CASE` (`CompoundStatement`); `CALL exec_autocommit_off(); INSERT …` and `CALL exec_start_txn(); INSERT …` (`Call`). The rows those scripts would insert are absent.
- `sessionStateProbe`: the runtime's `mysql_state_matches` against real writer sessions: a fresh session matches autocommit and not "in transaction"; after the review's procedures (`SET autocommit = 0` / `START TRANSACTION` inside a procedure, called directly outside StoneQL) followed by an `INSERT` that reported 1 row, the probe reports a mismatch and the row is gone after close (0 rows) on both MySQL 8.4.6 and MariaDB 11.4.8; an open transaction matches a lease and a `COMMIT` behind the lease's back is a mismatch.
- `procedureResultSets`: `"refused (CALL)"` on MySQL/MariaDB (was 2 result sets); PostgreSQL `CALL exec_proc` unchanged (1). Fixture init adds `exec_autocommit_off()` and `exec_start_txn()`.

SQLite (R-1, R-3) is covered by runtime unit tests on temporary files; there is no SQLite fixture section. Mutants run (each made its regression test fail, then reverted): SQLite single-statement check disabled (`r1_sqlite_text_with_more_than_one_statement_never_runs`: job `Succeeded`); old SQLite trigger depth rule (`r1_sqlite_trigger_bodies_end…`); `SET STATEMENT` check removed, compound check removed, in-lease allow-list bypassed (`r2_…` tests); session probe disabled (`r3_a_diverged_session…`: `Succeeded`); renderer "(committed)" text restored and diverged-text branch removed (`liveJobs.test.ts` R-3 test).

## Follow-up pass (2026-10-29, uncommitted, at the approval gate)

Closes the remaining items of `../../m006-fix-review.md` §5 except R-4, R-7, R-8 (renderer, owned by a separate pass), SSH and approvals. `npm run test:execution` exit 0 on all three engines; **the report JSON files here are from this run** (cleanup: 0 containers, 0 networks, temporary directories removed; `execution-cleanup-mysql.json` was last rewritten by the item-6 mutant run, which fails before its report, with identical content). **Read-write remains gated OFF.** Cancel p95 (100 samples): MySQL 5.4 ms, MariaDB 5.8 ms, PostgreSQL 68.6 ms (characterization only).

Harness changes: the PostgreSQL runner now starts the same loopback fault relay and a csvlog (`logging_collector`, `log_destination = csvlog`) with `log_statement = 'all'` set **only for the `stoneql_writer` role** (`ALTER ROLE`, after the role's password statement, so no credential reaches the log); the observer reads it with `pg_read_file` as the fixture superuser with its own logging off. PostgreSQL init adds `exec_lock` (bigint value), `exec_digit`, `exec_bulk`. `write_faults` now runs on every network engine through engine-specific observer queries (`Spy`). The lease-expiry check moved before the fault sections (their 300+ lease ends would otherwise evict the expiry outcome, R-6). The 100-sample cancel loop now waits until the slow statement itself is running before cancelling (it had cancelled during the session-setup `SET` once, which is a correct `cancelled` but not the sampled path). Runner timeouts raised to 30 min. In write cancel, the "sessions back to baseline" check is now `≤ baseline`, because the background expiry lease may end during the section (a session count can then only drop; a leaked session would still exceed it).

New and changed report fields:

| Field | MySQL 8.4.6 | MariaDB 11.4.8 | PostgreSQL 17.6 |
| --- | --- | --- | --- |
| `commitRefused` (item 6): `SET GLOBAL read_only = ON` by the fixture owner between the lease `INSERT` and COMMIT | ER 1290; `unknown`, `commitSent`, **no ROLLBACK sent**, repeat end identical; row observed absent afterwards (server rolled back; not claimed) | same | — |
| `postgresSessionState` (item 1, R-10) on raw writer sessions | — | — | probe: autocommit session matches outside only; healthy block matches a lease only (probe repeated, block unchanged; COMMIT applied); aborted block is a mismatch after a succeeded job, a match after a failed one, and never outside. R-10: plain `COMMIT` on an aborted block answered OK (positive control); StoneQL's `SAVEPOINT stoneql_commit; COMMIT` refused with 25P02 and the block stays aborted; outside a block 25P01; 0 rows applied from aborted blocks |
| `writeCancel` (Contract 5.3), n=30, blocked `UPDATE` | p50/p95/max 6.0/7.5/8.7 ms; KILL QUERY delivered 30/30; discarded 30/30; executed once 30/30; row unchanged 30/30 | 5.0/7.4/7.8 ms; 30/30 each | **new**: 24.6/32.0/54.2 ms; `pg_cancel_backend` delivered 30/30; discarded 30/30; executed once 30/30 (csvlog); row unchanged 30/30 |
| `bulkCancel` (10M-row `INSERT…SELECT`, 3 samples) | 790/573/843 ms, 0 rows after settle | 1221/1225/1280 ms, 0 rows | **new**: 82/80/71 ms, 0 rows |
| `killFailure` (cancel connection refused by the relay; statement deadline 3 s) | `requested`, `unknown`, 1502 ms; still running after abandon → observer killed it | same (1502 ms) | `requested`, `unknown`, 1502 ms; still running after abandon, then **ended by the server's `statement_timeout` backstop (5 s) 5082 ms after submit (L-3)**; row not applied |
| `lostEndReplies` `sever_on_reply` (7.1), n=50 per action | COMMIT: unknown 50/50, COMMIT in server log 50/50, **row committed 50/50**; ROLLBACK: unknown 50/50, 0 rows | same | same (COMMIT text `SAVEPOINT stoneql_commit; COMMIT`) |
| `lostEndReplies` `sever` (7.2), n=50 per action | unknown 50/50 with `commitSent`/`rollbackSent` (attempted), end statement in log 0/50, 0 rows | same | same |
| `lostEndReplies` `drop_s2c`, n=3 per action | unknown 3/3 after the 10 s COMMIT / 2 s ROLLBACK deadline; COMMIT applied 3/3 | same | same |
| No re-send (all lost-reply runs) | the inserted statement exactly once, the end statement at most once, the other end statement never; repeat `end_transaction` returns the recorded outcome (all runs) | same | same (csvlog) |
| Fault cycle 3 (`SLEEP`/`pg_sleep` cancelled in a lease, B-11) | killed `SLEEP()` reported `succeeded` **with `resultsWithheld`** | `succeeded`+withheld or `unknown` accepted | `unknown` (57014) |
| Connection loss in a lease | unchanged | unchanged | the session is now lost at the job's `statement_timeout` setup: statement `not_run`, job `failed` (nothing sent), lease ended, end `unknown`/`connection_lost` |

Runtime unit tests added (SQLite, no Docker): `l8_an_interrupt_still_in_flight_never_leaves_its_lease_connection_in_use` (test-only `Interrupt::Stalled`), `l4_concurrent_begins_never_exceed_the_per_owner_lease_count` (20 rounds, 3 concurrent begins), `b7_undrained_jobs_beyond_the_owner_bound_are_refused`, `r6_ended_lease_records_are_kept_per_owner_and_bounded_overall`, `b1_disconnect_and_handle_expiry_remove_the_handle_before_releasing_its_work`, `b11_results_read_after_an_interrupt_are_flagged_withheld`; `liveJobs.test.ts` B-11; contract wire/TS consumers for the required `resultsWithheld`.

Mutants (each run against the real test or fixture, then reverted; all killed): L-8 in-flight check removed; L-4 reservation not counted; B-7 bound restored to 64; R-6 global FIFO of 32 restored; B-11 withheld flag removed (runtime) and truncation marking removed (renderer); B-1 release-before-remove in `disconnect`, and in the lifetime watcher; item 6 MySQL guard removed (fixture: "refused COMMIT is unknown…"); PostgreSQL probe accepting an aborted block after success (fixture: "PostgreSQL session-state probe"); R-10 plain `COMMIT` (fixture: "PostgreSQL checked COMMIT"); L-3 backstop set to 3600 s (fixture: "statement_timeout backstop ended the abandoned statement").

Probes on disposable unpublished containers (removed): PostgreSQL 17.6 `SAVEPOINT` outside/inside/aborted blocks and the one-message `SAVEPOINT; COMMIT`; `KILL QUERY` on an idle MySQL 8.4.6 / MariaDB 11.4.8 lease session and `pg_cancel_backend` on an idle PostgreSQL backend did **not** affect the next statement; MySQL/MariaDB COMMIT under `read_only` → ER 1290 with the transaction rolled back by the server.

