# M004 — PostgreSQL and SQLite adapters

## Status
**Blocked — all unblocked work done (2026-11-16).** Scope narrowed by the user on 2026-11-16 to PostgreSQL (with MySQL/MariaDB); SQLite criteria are out of scope (code and regressions kept). Remaining PostgreSQL criteria need decisions A3, A9, A10 (A1 run, A8 adopted 2026-11-16) in `.ref/planning/approvals-ledger.md`. Earlier status text follows.

Local implementation and evidence recorded (2026-10-26, under the user's continuation instruction); not complete. Open gates: approved engine/version/OS matrix (only PostgreSQL 17.6 and bundled SQLite 3.51.3 on macOS ARM were executed; Windows/Linux, including Windows verbatim SQLite paths, unverified); real native file-picker and graphical WebView-to-database interaction (tests use Tauri MockRuntime and a trusted grant); approved G4/G5 lifecycle/basic-query numerical budgets and the 100,000-row resource profile for the new engines; PostgreSQL/SQLite per-message allocation bounds (the receive-limit patch is MySQL-only); SQLite WAL files in read-only directories and network filesystems; same-user SQLite path-replacement race; PostgreSQL extensions/arrays/ranges/interval text (explicit unknown today); hostile database definitions/operators; review of decisions D011–D015. No push, publication or deployment is authorized.

## v0.1.0 Alpha treatment (D022, Approved product decision 2026-10-26)
- **PostgreSQL — optional.** Admitted to the alpha only if, by the feature freeze, it passes the same alpha gates as MySQL/MariaDB for the capabilities it would advertise (read-only SQL at minimum; browse/CRUD/transfers are not planned for PostgreSQL in the alpha) on the signed artifact, including SSH parity and D011–D013 review. Otherwise it is capability-gated off in the alpha build and listed as an omission. It never blocks the alpha. Go/no-go: Proposed, decision pending.
- **SQLite — Deferred.** Implemented code, grants (D014/D015) and fixtures stay and must keep passing as regressions; the adapter is outside alpha support claims. The alpha UI must hide or clearly label it (Proposed: hidden behind a non-default flag). No writable-SQLite policy (M006 prerequisite 5) is needed for the alpha.
- Nothing here is complete; the open gates above remain for the release that ships each engine.

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

## Acceptance record — 2026-11-16 (PostgreSQL 17.6, macOS arm64; SQLite out of scope)
| Criterion | Evidence (executed) | Status |
|---|---|---|
| G1 PostgreSQL TLS/privilege boundaries, mutation refused | `npm run test:native-postgres` on `6b3a464` (2026-11-16): passed, 100 verified-TLS lifecycle samples, plus the 2026-10-26 untrusted-CA/hostname/28P01/42501/25006 evidence. **New risk found:** hostile-server backend header panics the driver (length < 4) and buffers unbounded (1 GiB declared → +411 MiB RSS) — `.ref/planning/m004-pg-message-limit/findings.md`; patch prototyped (4/4 adversarial tests). **A8 adopted 2026-11-16:** `vendor/sqlx-postgres` (provenance checked by `rust:vendor`), runtime 2 MiB limit → `PACKET_TOO_LARGE`; driver library suite 163/163; `test:native-postgres` (PostgreSQL 17.6): a 3 MiB value is refused and the next query on a fresh connection works. | Verified (TLS, privileges, message bound) |
| G2 ENUM/unknown types, temporal text, NULL/default | Earlier fixture evidence (D013); unchanged decoders. Arrays/ranges/interval/inet/extensions are explicit `unknown` by D013 (awaiting review). | Verified; D013 review A9 |
| G3 approved engine/version/OS matrix; unsupported capabilities rejected | PostgreSQL 17.6 on macOS arm64 executed; matrix approval pending. `CALL`/casts/dollar quoting refused (earlier). | Blocked (A3) |
| G4/G5 approved lifecycle/basic-query budgets; bounded resources | **New:** PostgreSQL 100,000-row resource profile (`npm run test:resources` now covers PostgreSQL via `tools/test-postgres-fixture.mjs --resource-profile`): PRIMARY index plan verified, 100 × 200-row selects p50 36.9 ms / p95 39.3 ms (debug, runtime+JSON, verified TLS), 16–1600-column metadata with rejection/recovery, 8 concurrent bounded results (8,386,400 B), max RSS 24.47 MiB, owned sessions and containers cleaned (`.ref/planning/m004-evidence/resource-postgresql.json`). Budgets Proposed. | Characterized; Blocked (A3, A8) |
| Generated contracts and MySQL/MariaDB regressions still pass | `test:native-database` both engines on `2cf6676`; desktop:test 24 passed (2026-11-16). | Verified |
| Native GUI to PostgreSQL | MockRuntime + real SQLx only. | Blocked (A1) |
| SQLite rows (canonical paths, WAL, path race, busy lock, Windows paths) | Out of scope per user 2026-11-16; existing regressions retained (desktop:test sqlite_* passed 2026-11-16). | Out of scope |
