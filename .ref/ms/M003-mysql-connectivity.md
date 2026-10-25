# M003 — MySQL/MariaDB lifecycle and read-only SQL

## Status
Authorized continuation; in progress. Shared lifecycle and restricted read-only adapter verified against local disposable fixtures. Native UI binding and complete acceptance gates remain pending.

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
- Host-independent dbadmin-runtime contains owner-scoped bounded MySQL/MariaDB pools, verified-identity TLS, deadlines, expiry, disconnect and shutdown. No frontend/native commands expose it yet; no credential persistence.
- Nine Rust regression tests cover ownership/capacity, expiry, connect/shutdown race, stalled handshake, configuration, SELECT refusal policy and raw temporal decoding.
- Live fixtures: 100 verified-TLS lifecycle samples for each pinned M001 engine. SELECT-only grants deny DELETE (1142); a separately privileged fixture account in a read-only transaction denies DELETE (1792). Exact integer/decimal/binary/ENUM/NULL/empty and zero/fractional/extended temporal cases passed. Empty metadata and row/byte truncation passed.
- MySQL read-only transaction setup requires the static text-protocol command; the initially attempted prepared form failed. Live regression now exercises the corrected path.
- Driver limit found: SQLx 0.9 reads/concatenates complete packets before application byte checks and has no public receive limit. Evaluate a narrow receive-limit patch before live UI exposure. Output caps alone are not an allocation bound.
- M003-L not fully accepted: hostname mismatch/expired certificates, OS matrix and complete lifetime/cleanup measurements remain. M003 UI binding, packet bounds and residual view/function trust review remain open.
- Safe raw summaries: `.ref/planning/m003-evidence/`. See `docs/development/connection-lifecycle.md`; debug microbenchmarks are not release gates.
