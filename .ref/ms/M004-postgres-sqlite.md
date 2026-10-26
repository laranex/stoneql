# M004 — PostgreSQL and SQLite adapters

## Status
Local implementation and evidence recorded (2026-10-26, under the user's continuation instruction); not complete. Open gates: approved engine/version/OS matrix (only PostgreSQL 17.6 and bundled SQLite 3.51.3 on macOS ARM were executed; Windows/Linux, including Windows verbatim SQLite paths, unverified); real native file-picker and graphical WebView-to-database interaction (tests use Tauri MockRuntime and a trusted grant); approved G4/G5 lifecycle/basic-query numerical budgets and the 100,000-row resource profile for the new engines; PostgreSQL/SQLite per-message allocation bounds (the receive-limit patch is MySQL-only); SQLite WAL files in read-only directories and network filesystems; same-user SQLite path-replacement race; PostgreSQL extensions/arrays/ranges/interval text (explicit unknown today); hostile database definitions/operators; review of decisions D011–D015. No push, publication or deployment is authorized.

## Objective and scope
Extend the verified lifecycle/basic read-only slice to PostgreSQL and SQLite. Preserve engine-specific capabilities and typed values; advanced schema/object workflows remain M006.

## Dependencies and prerequisites
M003 connection/read-only pattern reviewed and M001 contracts; approved PostgreSQL/SQLite version, schema and file-access policy. No SSH or Axum prerequisite; may run alongside M005.

## Technical approach
Implement PostgreSQL native type/schema identity mapping and SQLite bounded workers/authorized files. Probe missing SQLx capabilities before adopting them for each engine. Read-only opening/enforcement and explicit unsupported behavior reuse core rules.

## Deliverables
Adapters, lossless type fixtures, lifecycle/read-only evidence and capability-gap report; no duplicate host logic.

## Acceptance criteria
- G1: verify PostgreSQL TLS/privilege boundaries and SQLite canonical path/read-only file restrictions; reject unauthorized files and mutation paths.
- G2: preserve PostgreSQL ENUM/unknown types, temporal text and SQLite dynamic/strict values; NULL/default meanings survive round trips; locking failures never imply successful writes.
- G3: approved engine/version/OS matrix passes; SQLite routines/events and other unsupported capabilities are rejected, not presented as functional.
- G4/G5: repeat approved lifecycle/basic-query budgets; SQLite work does not block async execution, busy/timeout behavior is bounded and failed connections/workers release resources.
- Generated v1 contracts and MySQL/MariaDB regression fixtures still pass. Native object DDL/materialized-view behavior is gated in M006.

## Required tests and verification
Disposable PostgreSQL and temporary authorized SQLite files; strict/affinity/ENUM/quoted identifiers/raw precision; TLS, traversal, read-only and busy-lock negatives; lifecycle/query measurement profiles and adapter parity.

## Risks and limitations
Unknown PostgreSQL extensions and SQLite filesystem semantics need explicit support limits. No claim of identical transaction or cancellation behavior across engines.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.

## Incremental evidence — 2026-10-26
- Runtime split into `mysql.rs`, `postgres.rs`, `sqlite.rs` behind one registry and one bounded executor; dialect-aware lexer. Contracts add `SqliteFileRequest`/`SqliteFileGrant`/`SqliteConnectionRequest`; desktop adds `pick_sqlite_file`/`connect_sqlite` and accepts `postgresql` in `connect_direct`. UI enables PostgreSQL/SQLite in Live mode with a native **Choose file…** control.
- G1: PostgreSQL untrusted CA/hostname mismatch rejected as certificate failures, wrong password 28P01, server plaintext refusal, SELECT-only 42501, definer/nested/sequence positive controls with 25006 refusal (mutation without the read-only transaction fails). SQLite relative/`..`/symlink/directory/missing/foreign/forged grants refused; symlink swap after grant refused at open and query; engine `SQLITE_READONLY` refusal independent of the lexer.
- G2: exact int8/NUMERIC (scale, NaN)/float specials, ENUM label, INET as explicit unknown, temporal text including BC/24:00/±infinity/timetz/UTC timestamptz, jsonb, uuid, quoted identifiers, NULL/empty/default; SQLite STRICT/ANY/affinity storage classes, REAL from NUMERIC affinity reported as float. Busy lock fails after the 1 s timeout with unknown outcome and later succeeds.
- G3: SQLite routines/events/`CALL`/`PRAGMA`/`ATTACH`/CTEs and PostgreSQL `CALL`/casts/dollar quoting refused, not emulated. MySQL/MariaDB native, lifecycle (incl. expired certificates) fixtures and generated contracts re-passed.
- G4/G5 (characterization only, not approved budgets): 100 PostgreSQL IPC TLS cycles p50 28.4 ms/p95 31.8 ms (debug, SCRAM); 100 SQLite IPC cycles p50 0.36 ms/p95 0.58 ms. Deadline interrupt releases the SQLite worker (mutation without it fails); PostgreSQL statement timeout stopped server work; lease expiry, reload cleanup and eight-slot reuse verified.
- Safe evidence: `.ref/planning/m004-evidence/`; decisions D011–D015 in `.ref/planning/architecture-decisions.md`; public page `docs/development/postgres-sqlite.md`.
