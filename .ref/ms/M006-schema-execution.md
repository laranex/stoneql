# M006 — Advanced SQL, schema metadata and integrity

## Status
**In progress. Not complete; no acceptance criterion is signed off.** (Updated 2026-10-29.)

- **Execution half: implemented locally, uncommitted.** This covers read-write jobs, scripts, multiple result sets, transaction leases, cancellation and outcomes. The user instructed that it not be committed before reconciliation. Builder evidence (MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6, SQLite; macOS ARM; debug build) is in `.ref/planning/m006-evidence/execution/`. The reconciliation review re-ran only the runtime suite (45 passed), the desktop lib + SQLite execution tests (6 + 1 passed) and `liveJobs.test.ts` (4 passed). The Docker fixtures, browser suite, lint/boundary checks and `contracts:check` were not re-run by the review.
- **Reconciliation verdict: CONFORMS-WITH-GAPS** against the D022 approved principles (`.ref/planning/m006-reconciliation.md`). There are three **must-fix-before-commit** defects: V-1 (a lease outcome is lost when the 5 s release bound cuts off rollback, and `end_transaction` then returns `FORBIDDEN`), V-2 (the script-policy bypass `SET …, autocommit=0` beyond 8 words, plus unrefused MySQL `PREPARE`/`EXECUTE`, can give false `committed`/`rolled_back`), and V-3 (the UI shows "executed successfully" when the job connection could not be opened). Before-alpha items are B-1 to B-11 in that document, including lost-COMMIT fault injection (Contract 7.1/7.2) and MySQL write-cancel evidence (Contract 5.3).
- **Safety-fix pass (2026-10-26, uncommitted, at the approval gate):** V-1, V-2, V-3 fixed with regression tests; B-1, B-2, B-3, B-6, B-10 and L-5, L-7 fixed; B-4 (lost COMMIT/ROLLBACK through a TCP relay) and B-5 (MySQL/MariaDB write cancel and failed KILL connection) executed on real fixtures. Per-item status: `.ref/planning/m006-safety-contracts.md` § Implementation status; evidence: `.ref/planning/m006-evidence/execution/`. Open: B-7, B-11, L-1..L-4, L-6, L-8, Contract 3.7/4.5/5.2, Phase A vs B, all limits.
- **R-1..R-3 fix pass (2026-10-29, uncommitted, at the approval gate):** the three critical findings of the independent review (`.ref/planning/m006-fix-review.md`) are fixed with regression tests and mutants: R-1 SQLite text is run only if SQLite parses it as one statement (prepare-prefix check before send; splitter trigger rule hardened); R-2 MySQL/MariaDB in-lease allow-list, `SET STATEMENT … FOR`, top-level compound statements and `CALL` refused before send (MySQL 8.4.6 / MariaDB 11.4.8 fixtures: zero general-log rows); R-3 job outcomes and lease ends are claimed only after a server session-state check (MySQL/MariaDB, SQLite; PostgreSQL not probed). Contract 3.7 is implemented for those engines. Still open: B-4 at n=50 + PostgreSQL lost-COMMIT/write-cancel, B-1 ordering test, B-7, B-11, R-4..R-10, L-3, L-4, L-8, Contract 4.5/5.2, Phase A vs B, all limits, and the user's decision to enable the gate.
- **Read-write SQL is gated OFF natively** (`READ_WRITE_SQL_ENABLED = false` in the desktop host): `access: read_write` is refused with `UNSUPPORTED_CAPABILITY` before any Keychain lookup or network I/O, for every engine and route. The UI shows the option disabled ("pending safety approval"). Enabling it requires the user's approval. SSH read-write stays refused independently until cancellation under forwarding-slot exhaustion (Contract 5.2) is implemented and fixture-tested.
- **Follow-up pass (2026-10-29, uncommitted, at the approval gate):** PostgreSQL session-state check (`SAVEPOINT`) and savepoint-checked COMMIT (R-10); PostgreSQL `statement_timeout` backstop (L-3, Proposed value); in-flight interrupt discards the connection (L-8); atomic per-owner lease count (L-4); job records per owner 8 (B-7, Proposed); `resultsWithheld` contract field (B-11); ended outcomes 32 per owner / 256 overall (R-6); deterministic B-1 ordering test; fixtures: MySQL/MariaDB refused COMMIT → `unknown`, lost COMMIT/ROLLBACK n=50 per mode on all three network engines, PostgreSQL write cancel and backstop. Evidence and mutants: `.ref/planning/m006-evidence/execution/README.md` § Follow-up pass. Contract 3.7 is now implemented for all four engines. Still open: R-4, R-7, R-8 (renderer), R-9/SSH, Contract 4.5/5.2, Phase A vs B, approval of all limits, and the user's decision to enable the gate.
- **Renderer pass (2026-10-29):** R-4 (warning before window close or browser reload with an open lease or running live job; native close uses the in-app dialog), R-7 (outcome queried and shown when disconnect fails), R-8 (truthful lease toolbar text) fixed with tests and mutants. Combined-tree verification 2026-10-29: rust:build/fmt/lint/test (97)/boundaries/vendor, contracts:check, desktop:test, npm test, build, test:browser (30/30), docs:build all pass.
- **User decisions 2026-10-29:** commit the execution foundation with read-write disabled; limits approved or conditionally approved as alpha-testing baselines; PostgreSQL timeout ordering to be revisited. Details: `architecture-decisions.md` § D025 "User decisions, 2026-10-29".
- **Remaining blockers before enabling read-write (follow-up):**
  1. PostgreSQL timeout ordering: server `statement_timeout` firing before the application deadline (Contract 5.3), with regression tests; the current deadline + 2 s is not approved.
  2. Reload protection: in-webview reload (Cmd+R / devtools) with an open lease or running job is not blocked or verified in WKWebView; needs a native navigation/reload guard or Contract 4.5.
  3. UNKNOWN outcome retention and reload behaviour: unresolved `unknown` outcomes must not be evicted silently by the 32/256 bound, and must be reportable after a renderer reload (Contract 4.5).
  4. SSH cancellation under forwarding-slot exhaustion (Contract 5.2) and the desktop SSH read-write branch test (R-9); SSH read-write stays refused.
  5. Test and wording gaps: browser test stubs lack `resultsWithheld`; a job on an idle-expired lease reports "connection was lost" instead of the expiry.
  6. Remaining D025 release measurements: silent-drop lost-COMMIT beyond n=3, release-build budgets, and final release values for every limit.
  7. Explicit user decision to enable read-write (Direct first; SSH separately).
- **Decisions:** the builder's choices are registered as D025-1..D025-10, plus a Proposed D003-M006 note, in `.ref/planning/architecture-decisions.md`: implemented (uncommitted), awaiting review. All `ExecutionLimits` values are PROPOSED.
- **Schema/metadata/browse/CRUD half: pending, not started.** This covers object/schema introspection, the metadata cache and its invalidation, live browse and generated CRUD (D022 alpha floor: live browsing + CRUD for MySQL/MariaDB). Advanced objects are deferred by D022.

The user authorized roadmap continuation and local milestone commits. Security and release acceptance gates still apply. No push, publication or deployment is authorized.

## Safety contracts — 2026-10-26
Proposal: `.ref/planning/m006-safety-contracts.md` (**Proposed / awaiting approval**). Its M006 principles were approved by the user under D022 (exclusive per-connection job ownership; BUSY instead of queuing; best-effort cancellation; never auto-retry uncertain writes; four-way outcome distinction; transaction ownership/expiry/cleanup; MySQL write cancellation and SSH cancellation-slot exhaustion resolved before unrestricted read-write). D022 also put CRUD and safe SQL execution in alpha scope, overriding the proposal's "Option A". The contract details, numeric values and Q1–Q9 remain Proposed. The execution-half implementation was reconciled against the proposal in `.ref/planning/m006-reconciliation.md` (11 divergences assessed; fix list and ranked open questions there).

## Objective and scope
Own advanced query jobs, streaming/batches, multiple result sets, transaction leases, cancellation, object/schema management and introspection/cache lifecycle. Extend M003 read-only results rather than replacing its working path.

## Dependencies and prerequisites
M003/M004 stable adapters; approved D003/D004 and per-engine SQLx capability spikes. M005 is required only for SSH/secret-backed integration coverage, not Direct core implementation. Approve job/resource/transaction/cancellation limits before implementation.

## Technical approach
Use framework-independent bounded job services, physical-connection transaction affinity, typed errors/outcomes and centralized schema/mutation rules. Add metadata revisions/refresh/invalidation and autocomplete discovery through generated DTOs. Keep IPC/HTTP framing at host boundaries.

## Deliverables
Native object/CRUD workflows, advanced execution lifecycle, metadata cache and authoritative constraint/error integration, with engine-specific capability report.

## Acceptance criteria
- G1: every handle/job/transaction/cache entry is owner-scoped; unauthorized actions and capabilities fail; diagnostics exclude SQL/row secrets by default. Destructive UI safeguards do not substitute for authorization.
- G2: FK/unique/composite constraints and optimistic conflicts are authoritative; batches have documented atomicity; raw values/defaults unchanged; cancellation, DDL implicit commits and lost commits report partial/unknown outcomes accurately.
- G3: applicable object/routine/index/materialized-view workflows verified on approved engine versions; multiple results and unknown types preserved; contract generation and unsupported-version tests pass.
- G4/G5: meet approved bounded-result/cancel profile; no dirty connection returned to pools; best-effort server cancellation is distinguished from confirmed termination; stale metadata and workspace replies cannot leak/cross generations.
- Basic M003 read-only queries still work through the same core contracts. No Tauri/Axum dependency enters shared services.

### Execution-half evidence against the criteria (2026-10-26; none signed off)
- G1: owner scoping of jobs, leases and pages has executed evidence (runtime, desktop IPC, 3-engine fixture). Cache entries are not implemented.
- G2: constraint kinds, partial DDL outcomes, MySQL in-lease implicit-commit refusal and connection-loss `unknown` have executed evidence. Lost-COMMIT/ROLLBACK fault injection (Contract 7.1/7.2) executed at n=50 per mode on MySQL, MariaDB and PostgreSQL (2026-10-29 follow-up pass).
- G3: multiple results (`CALL`, scripts) and exact values have executed evidence. Object/routine/index workflows are not started.
- G4/G5: backpressure, cancel p95 characterization (not an approved budget), 100 fault cycles with no session-state leakage, and best-effort vs confirmed cancellation have executed evidence. V-1 (lost outcome on release timeout) and MySQL/MariaDB write-cancel (Contract 5.3) now have executed tests/evidence (2026-10-26 safety-fix pass). Open: SSH cancellation (gated off). Server transaction-state verification (3.7) is implemented for MySQL/MariaDB/SQLite and, since the 2026-10-29 follow-up pass, PostgreSQL. Stale metadata is not started.
- M003 read-only path: unchanged for read-only handles (runtime suite). Read-write handles serialize it behind the work permit.

## Required tests and verification
Engine object matrix, FK/unique/conflict/NULL fixtures, multiple result sets and stored procedures, transaction/cancel/connection loss, stale cache/schema switching, bounded backpressure and owner isolation; repeat M003/M004 regressions.

## Risks and limitations
Raw SQL can mutate session state; pool reset/discard must be proved. Engine cancellation/DDL semantics differ. An M001 spike is capability evidence, not production feature completion.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
