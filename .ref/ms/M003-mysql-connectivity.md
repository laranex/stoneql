# M003 — MySQL/MariaDB lifecycle and read-only SQL

## Status
**Blocked — all unblocked work done (2026-11-16).** Remaining criteria need user decisions A1, A3, A6, A7 in `.ref/planning/approvals-ledger.md`. Earlier status text follows.

Authorized continuation; in progress. Shared lifecycle and restricted read-only adapter verified against local disposable fixtures. Native Direct MySQL/MariaDB SQL binding is implemented; complete native GUI/OS, memory and trust acceptance gates remain pending.

## Objective and scope
A usable Direct vertical slice: connect, inspect minimal server identity, run one bounded read-only statement, display typed results/errors, and disconnect. Include basic read-only SQL now; preserve advanced execution for M006.

## Dependencies and prerequisites
M001 contracts and SQLx capability evidence for this slice; approved TLS/engine/limits profile. M002 is required for desktop UI binding, not for accepting the independent M003-L lifecycle checkpoint.

## Technical approach
Build isolated bounded connections/pools and runtime SQLx execution for arbitrary result shapes. Use least-privilege database accounts plus verified engine read-only enforcement; do not rely on a SELECT prefix. Return a bounded result with column names/types and explicit row/byte limits. Do not expose mutation, CALL, multi-statement scripts, user transaction leases, query jobs or execution cancellation. Internal read-only safety guards are not a public transaction workflow.

## Deliverables
Direct connection lifecycle, basic read-only query/result binding, disposable MySQL/MariaDB fixtures and M003-L acceptance record.

## Acceptance criteria
- **M003-L** (accept independently to unblock M005): verified connect/auth-failure/timeout/disconnect/expiry cleanup, isolated pools, TLS verification and redacted diagnostics; pass the lifecycle profile for both MySQL and MariaDB. No M002 UI or M004 prerequisite for this checkpoint.
- G1: reject invalid certificates/targets, DML/DDL/CALL/stacked or unsafe unsupported SQL; test misleading comments/prefixes and SELECT side effects. Database permissions and read-only enforcement, not string heuristics, define the safety boundary; privilege requirements and residual limitations are explicit.
- G2: exact BIGINT/DECIMAL/temporal/NULL values displayed; unknown types are explicit/read-only, not coerced. Row/byte caps truncate the result set explicitly, never individual raw values silently.
- G3: approved MySQL and MariaDB version fixtures have separate capability/error evidence; generated v1 contract and existing UI/mock behavior remain compatible.
- G4/G5: meet approved lifecycle/basic-SQL budgets; fail closed on deadline/protocol errors, drain/reset or discard uncertain connections, and never retry SQL automatically. A client timeout/close is not confirmed server-side cancellation.
- Full schema discovery/cache/revisions, advanced jobs, multiple result sets, routines, transactions and execution cancellation remain M006; only minimal connection/result metadata is introduced here.

## Required tests and verification
Disposable MySQL/MariaDB TLS/privilege matrix; 100 lifecycle and bounded SELECT samples; boundary/raw type fixtures; SQL read-only negative cases; timeout/disconnect cleanup; actual existing SQL UI result/error workflow on supported native runners.

## Risks and limitations
Read-only SQL can invoke side effects through functions or privileges; unsupported forms must fail conservatively and engine-level enforcement must be demonstrated. Driver deadline/cancel limitations cannot be hidden. M003-L does not imply full M003 completion.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.

## Incremental evidence — 2026-10-25
- Host-independent stoneql-runtime contains owner-scoped bounded MySQL/MariaDB pools, verified-identity TLS, deadlines, expiry, disconnect and shutdown. No frontend/native commands expose it yet; no credential persistence.
- Nine Rust regression tests cover ownership/capacity, expiry, connect/shutdown race, stalled handshake, configuration, SELECT refusal policy and raw temporal decoding.
- Live fixtures: 100 verified-TLS lifecycle samples for each pinned M001 engine. SELECT-only grants deny DELETE (1142); a separately privileged fixture account in a read-only transaction denies DELETE (1792). Exact integer/decimal/binary/ENUM/NULL/empty and zero/fractional/extended temporal cases passed. Empty metadata and row/byte truncation passed.
- MySQL read-only transaction setup requires the static text-protocol command; the initially attempted prepared form failed. Live regression now exercises the corrected path.
- Driver limit found: SQLx 0.9 reads/concatenates complete packets before application byte checks and has no public receive limit. Evaluate a narrow receive-limit patch before live UI exposure. Output caps alone are not an allocation bound.
- Original M003-L checkpoint left hostname/expired-certificate and real cleanup checks open; those local gaps are addressed in the hardening evidence below. OS compatibility and complete performance profiles remain. M003 UI binding, packet bounds and residual view/function trust review remain open.
- Safe raw summaries: `.ref/planning/m003-evidence/`. See `docs/development/connection-lifecycle.md`; debug microbenchmarks are not release gates.

## Receive-limit evaluation
- Initial isolated patch (before the adoption approval recorded below) checked logical packet sizes before payload reads, including aggregate continuation size and arithmetic overflow. At that evaluation checkpoint, root SQLx remained registry 0.9.0.
- Two adversarial socket tests plus one upstream-copy boundary unit test passed. Both real TLS fixtures rejected oversized rows and hostname/untrusted-CA failures; fresh connections worked afterward. Prepared source/checksum provenance and MIT attribution retained.
- Evidence in `.ref/planning/m003-packet-evidence/`; reproducible scripts `test:receive-limit` / `test:receive-limit:live`; source/diff in `experiments/sqlx-receive-limit/`.
- Production patch adoption/maintenance, broader protocol/OS/memory profiles and native UI binding remain pending. No dependency exception or milestone completion inferred.

## Lifecycle hardening — local verification
- Added explicit v3 CA generation, exact hostname/issuer failure checks, expired server certificates with fixed past dates and a negative certificate-classification unit test. Application TLS policy is unchanged; no system clock or trust-store changes.
- Physical expiry cleanup is observed through the fixture account's process list; the one-second lease removes the server session and releases capacity. No unrelated sessions are inspected.
- Fixed early setup/teardown temporary-directory leaks in both runners, with failure-first command-boundary regressions. Added a readiness regression for the socket-only initialization server and a TCP-listener requirement.
- Fixed the effective-Docker-context guard to honor DOCKER_CONTEXT over DOCKER_HOST; both runner regressions refuse a remote context before container operations.
- The expired MariaDB readiness observer uses only its authenticated container-local Unix socket, independently of StoneQL's verified TCP rejection.
- Safe summaries/checksums are retained separately under `.ref/planning/m003-lifecycle-hardening/`. Final checks are recorded in local progress. M003 remains in progress: production receive-limit adoption, native binding, OS/profile coverage and indirect function/view trust are not accepted by this increment.

## Approved receive-limit adoption
- User explicitly approved the tested patch for the main runtime on 2026-10-25 (“do it”). D002 records its limited scope; no release/security/OS waiver.
- Root Cargo patch points to reviewed, MIT-attributed SQLx MySQL 0.9.0 source. Core/contracts still import no driver/runtime/host framework. Source/provenance drift checks run in CI.
- Runtime cap: 2 MiB per logical incoming packet, independent of 1 MiB result serialization. Oversize produces `PacketTooLarge`, discards the physical connection and does not retry SQL. Fixture regression selects a 3 MiB stored value and then verifies a subsequent small query on a fresh connection.
- Packet tests share the same source as the runtime. Full memory/metadata profiles, native binding and other acceptance gates remain open; M003 not complete.

## Native live binding increment

- Explicit Live/Mock mode is wired into the active connection dialog and existing SQL workspace. Default browser/mock workflows remain unchanged. Direct MySQL/MariaDB only; configured database context, no simulated schema discovery or live mutation controls.
- Strict generated v1 commands enforce native identity/origin/ownership, bounded admission and shutdown cleanup. Raw results preserve precision, temporal text, NULL and duplicate column labels. Password/PEM/handles stay out of local profile persistence; reconnect is manual.
- Added Light/Dark live-bridge browser coverage and actual-handler integration against owned TLS fixtures. Evidence distinguishes simulated browser bridge, Tauri MockRuntime with real SQLx/database, and real Wry startup. None substitutes for full native GUI-to-database testing.
- New narrow-window browser regression exposed a scrolling connection footer; only the form body now scrolls, preserving readable controls and reachable actions.
- M003 remains in progress. Full OS/native GUI coverage, aggregate metadata/memory bounds, indirect view/function trust and renderer-reload lease reconciliation are not accepted by this increment. M002's two Linux GTK audit findings remain unresolved.

Verification: 151 JavaScript tests, 30 default Rust tests, 21 browser scenarios, both-engine actual-handler fixtures, Rust formatting/Clippy/contracts/boundaries/provenance, frontend/native/docs builds and real macOS Wry startup smoke (1,597 ms) passed. Safe evidence: `.ref/planning/m003-native-binding/`. The owning-fixture test is ignored in the default Rust suite and executed separately by `test:native-database`.

## Indirect read-only trust verification

- Both pinned TLS fixtures passed actual native-handler checks for definer functions invoked through views, a nested view and InnoDB/MyISAM writes. Positive controls prove the SELECT-only reader can trigger the definitions outside read-only mode; independent engine1792 checks prove refusal is not simply a missing privilege or rollback.
- Actual native queries refuse all three with safe QUERY_REJECTED/unknown, leave counters unchanged and permit subsequent safe reads on fresh connections. Indirect advisory locking is demonstrably possible and the tested lock is released on physical close. Built-in allowlists do not apply transitively to database definitions.
- Mutation test omitting the private read-only transaction failed on the new native refusal assertion; original runtime restored and both engine fixtures rerun successfully. No product code/contract/UI change.
- Reproducible safe evidence: `.ref/planning/m003-read-only-trust/`; public boundary: `docs/development/read-only-safety.md`. This narrows the persistent-write trust gap; hostile UDF/external effects, native GUI/OS/resource profiles and reload reconciliation remain open. No blanket trust acceptance or milestone completion.

## Renderer bootstrap lifecycle repair

- Verified root cause: host_info returned one host-lifetime session token; renderer reload lost its handles while pools remained until lease expiry. A new native regression failed before the fix because successive bootstrap nonces returned the same token.
- Fresh bootstrap nonce now invalidates the old token and resets/awaits existing pools before returning a new token. Same-nonce retries are idempotent. Registry generations reject late pending registrations; native post-connect revalidation closes late handles, and stale successful query replies retain unknown outcome.
- Existing global pool/admission caps and deadlines remain; pending attempts may retain permits until bounded cleanup. No DTO/wire version, capability, UI layout or mock behavior change. UUID-based frontend nonces avoid clock-resolution collisions.
- Tests cover registry reset/re-registration, native stale probes and real both-engine idle-session closure, old token/handle rejection and eight-slot reuse. Actual graphical reload/cross-OS, aggregate resource profiles and hostile UDF/external effects remain open; no milestone completion inferred.

Verified local record: `.ref/planning/m003-renderer-reload/` (checks executed2026-10-25, reviewed after pause2026-10-26):155 JS tests,33 default Rust tests,22 browser scenarios, both real TLS handler fixtures, formatting/Clippy/contracts/boundaries/provenance and frontend/native/docs builds passed. Initial real macOS smoke948ms; no graphical live-database reload or cross-OS acceptance claimed.

## Runtime resource characterization

- Opt-in `test:resources` provisions 100,000 1 KiB rows per owned TLS fixture, verifies PRIMARY range planning, records 100 200-row SELECT/serialization samples and observes standalone runtime RSS.
- Tests 16–2048-column empty metadata, explicit 128-byte metadata rejection/fresh-query recovery, and eight simultaneous row-boundary-truncated results under independent 1 MiB allowances. Records CPU/OS/RAM/tooling context, raw timings and cleanup; no credentials or original query data.
- Release numerical thresholds remain unapproved. This is debug runtime+JSON, excluding Tauri/WebView and Docker server memory. Existing packet cap does not bound cumulative driver descriptors; hostile metadata/protocol, native GUI/OS coverage and external/UDF gates remain open. No driver patch/adoption, production query behavior change or M003 completion.

Safe executed report: `.ref/planning/m003-resource-profile/` (2026-10-26). Both profiles passed; local debug runtime+JSON p95 MySQL14.583ms/MariaDB14.252ms, peak standalone RSS23.375/22.875MiB.157 JS/33 Rust tests and build/lint/contracts/boundaries/vendor/docs checks passed. Numerical budgets remain proposed and full native/aggregate-memory acceptance is open.

## Acceptance record — 2026-11-16 (scope: macOS; MySQL 8.4.6 / MariaDB 11.4.8 fixtures)
| Criterion | Evidence (executed) | Status |
|---|---|---|
| M003-L lifecycle checkpoint | Lifecycle hardening + 100-cycle samples (2026-10-25/26, above). | Verified (earlier evidence) |
| G1 TLS/target rejection, DML/DDL/CALL/stacked refusal, engine read-only enforcement | Earlier fixtures and `read_only.rs` tests; `npm run test:native-database` 2026-11-16 on commit `2cf6676`: both engines pass (UPDATE → UNSUPPORTED_SQL, 1792 trust checks). Residual limits documented in `docs/development/read-only-safety.md` (hostile UDF/external effects not certified). | Verified; residual risk needs acceptance (A7) |
| G2 exact values, explicit truncation | Same run: BIGINT UNSIGNED max, DECIMAL(38,4), zero date, NULL, duplicate labels, 1000-row and 1 MiB truncation exact. New: live catalog/browse/count statements decode over verified TLS on both engines (MySQL 8 binary-charset catalog cells handled in the renderer, `a187ead`). | Verified |
| G3 separate MySQL/MariaDB capability evidence; contract/UI compatibility | Both engines separately in the fixture run; browser suite 52/52; contracts unchanged. Version matrix itself (8.4.6 / 11.4.8, macOS arm64) is Proposed. | Verified; matrix approval A3 |
| G4/G5 lifecycle/basic-SQL budgets; fail closed; no retry | Characterization only (resource profile 2026-10-26). Budgets Proposed. Aggregate metadata allocation bound: prototype complete (`m003-metadata-limit/findings.md`, patch re-applied cleanly to current vendor 2026-11-16); adoption pending. | Blocked (A3, A6) |
| Native SQL UI workflow on supported runner | Simulated bridge + MockRuntime with real SQLx only; real-window GUI not run. | Blocked (A1) |

- Finding 2026-11-16 (open, M003 G2): the vendored driver names a column's type from its BINARY flag, which servers also set for `_bin` collations (MariaDB `JSON` = `LONGTEXT … utf8mb4_bin`), so such text columns are delivered as binary cells and the SQL tab shows them as hex. Export corrects this from the catalog's `CHARACTER_SET_NAME`; a general fix needs either catalog lookup for arbitrary query columns or a driver change (vendor patch → approval).
