# M007 — Streaming transfers and compression

## Status
**In progress — alpha slice implemented, not closable (reviewed 2026-11-17; record "Acceptance record — 2026-11-17" at the end).** Live export Quick/Custom with SQL/CSV/JSON, GZIP, table subsets and SQL/CSV options (JSON + GZIP approved 2026-11-17, A20; uncommitted at review time); SQL import for MySQL/MariaDB; CSV import into a table; usable in shipped builds since A15 (2026-11-17). Met: G2 and G6 for SQL/CSV. Not met, engineering only: GZIP real-server round trip (no gzip test located); JSON/whole-database export cases on PostgreSQL and server restores of the other Custom options; `flate2` licence/advisory review; million-row RSS, slow-consumer and disk-full runs. Blocked: G3 matrix (A3, A19, remaining A20 items), G4/G5 quotas and SQL import limits (A18, A20 limit sign-off). The 2026-11-16 line "Blocked — all unblocked work done" is superseded. Earlier status text follows.

Planned / not started. The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## v0.1.0 Alpha slice (D022, Approved product decision 2026-10-26)
- **Required:** basic SQL and CSV import/export for MySQL/MariaDB with bounded streaming and native file grants. Task breakdown, acceptance and quotas: `.ref/planning/v0.1.0-alpha-plan.md` workstream (d) (Proposed).
- *2026-11-17 note: JSON and GZIP export are now approved and implemented (A20, 2026-11-17); ZIP, XLSX and GZIP import stay Pending under A20. The next lines are kept as history.*
- **Deferred (approved deferral):** XLSX import. **Proposed deferrals:** XLSX and JSON export, ZIP, GZIP import (unless quotas apply to decompressed bytes with a tested expansion cap). **Proposed to keep:** GZIP export.
- SQL import depends on the M006 gate for unrestricted read-write SQL (MySQL write cancellation and SSH cancellation-slot exhaustion resolved, D022); CSV import depends on M006 owned transactions.
- PostgreSQL/SQLite transfers follow those engines' alpha status (optional / Deferred). The full M007 scope below remains for later releases.

## Objective and scope
Stream SQL/CSV/JSON/XLSX and ZIP/GZIP with accurate progress, file authorization and failure outcomes. Reuse format configuration and mock UX.

## Dependencies and prerequisites
M006 execution/schema contracts; approved D008 library, format, snapshot and quota decisions. M005 only for credential-backed/SSH integration; Direct transfer core need not wait for SSH. Host-specific file grants approved before host integration.

## Technical approach
Bound parsers, queues, compressors and sinks; centralize schema validation and exact-value encoding. Desktop file grants and later server upload tokens remain separate host adapters. Choose dump tools versus logical export from recorded evidence.

## Deliverables
Transfer services, native file binding and independently verified engine/format round trips with quota/cleanup reports.

## Acceptance criteria
- G1: reject unauthorized paths/symlinks, malformed archives, expansion bombs and executable spreadsheet formulas; libraries/licenses reviewed; no secrets in summaries/temp names/logs.
- G2: exact values, NULL/empty/default semantics and ordering preserved; committed/rejected/partial/unknown counts truthful; no whole-script rollback promise for nontransactional DDL.
- G3: approved engine/format/compression/OS matrix passes independent round trips; unsupported structure/data combinations rejected; generated DTO compatibility retained.
- G4/G5: meet approved million-row/slow-consumer memory and cleanup profile, enforce queue/disk/byte quotas, and survive disk-full/cancel/restart without orphan resources or hidden partial writes.
- G6: restore/read-back evidence exists for every format advertised as a backup; simulated successes are not accepted as proof.

## Required tests and verification
Engine round trips and independent restore/read-back; precision/encoding fixtures, malformed inputs, quota/archive adversarial cases, slow consumers, cancellation, disk full and restart cleanup; measured RSS/queue/temp usage.

## Risks and limitations
XLSX and ZIP may require bounded temporary storage; SQL export snapshot consistency varies. A library or native dump tool is not approved by this plan.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.

## Acceptance record — 2026-11-16 (alpha slice; macOS arm64; MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6)
*Statuses in this table are superseded by "Acceptance record — 2026-11-17" at the end of this file; kept as history.*

| Item (alpha plan d1–d6) | Commits | Executed evidence |
|---|---|---|
| d1 native save grants, temp file + atomic rename | `13a1ae9`, `3974ef9` | `pick_export_file` / trusted seam `authorize_export_file`: one-shot, owner-session grants; symlink destinations, dot files and missing directories refused; temp file `0600` created `create_new` beside the destination, fsync, rename refusing a non-regular destination; failure/cancel/reset/shutdown remove it (IPC test: failed export leaves no file or `.part`; fixtures: refused exports leave nothing). |
| d2 CSV export | same | Streaming on one connection in a read-only snapshot; NULL = unquoted `NULL`, everything else quoted; binary columns refused naming the column. |
| d3 SQL export | same | MySQL/MariaDB `SHOW CREATE TABLE` + 500-row INSERTs with `_utf8mb4 X'…'`/`X'…'` literals in `START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY`; PostgreSQL data only (`REPEATABLE READ, READ ONLY`, structure explicitly refused); unknown types refused naming the column. MariaDB `JSON` (`utf8mb4_bin`, BINARY flag) decoded as text from the catalog's character set. Shared literal vectors (`crates/stoneql-runtime/tests/fixtures/sql-literals.json`) pin the Rust and TypeScript encoders. |
| d6 independent read-back | same | `test:native-database` / `test:native-postgres`: exported files loaded by the server's own client (`mysql`/`mariadb` `SOURCE` + `LOAD DATA … ENCLOSED BY '"' ESCAPED BY ''`; `psql \\i` + `\\copy … NULL 'NULL'`) into scratch databases/schemas; MySQL and MariaDB 5/5 rows identical column-by-column (NULL-safe, binary) for SQL and CSV; PostgreSQL 5 rows, 0 differences both directions (EXCEPT) for SQL and CSV. Values: BIGINT max, DECIMAL(38,4), NUMERIC NaN, DATETIME(6), zero date, ±infinity, ENUM/SET, JSON/jsonb, float specials, boolean, uuid, quotes/commas/newlines/tab/backslash/non-ASCII, `''` vs NULL vs `'NULL'`. Evidence: `.ref/planning/m007-evidence/`. |
| d4 CSV import | `7378b86`, `05b30d2` | `liveImport.ts`: quoting-aware parser (NULL convention), header/binary/ragged/oversize checks with record numbers, batches < 60 KiB (job limit 64 KiB) inside one lease via `begin_transaction`/`submit_job`/`end_transaction` (gate, journal and unknown-outcome rules apply); any rejected batch rolls the import back. Exact batches pinned in `test:execution` (committed rows keep NULL/''/'NULL' distinct; duplicate batch rolled back, nothing applied) — see the final run in the verification note. |
| d5 SQL import (MySQL/MariaDB) | 2026-11-16 (uncommitted at time of writing) | Whole-file validation then execution from the same in-memory snapshot as a job record (`execution/import.rs`, `sql_import.rs`, `LiveSqlImport.vue`). Runtime unit tests (validation, refusals with number/line, limits, BOM/CRLF/DELIMITER/executable comments; SQLite runner: success with discarded result rows, stop at first failure, refusal sends nothing, cancel → unknown + journal, busy handle); desktop IPC/host tests (native-only grants, refusals before reading, symlink swap, oversize from size); real servers MySQL 8.4.6 and MariaDB 11.4.8 (`test:execution`: StoneQL export with CREATE TABLE round-trips 5 edge-value rows exactly, first failure stops at statement 3/line 3 SQLSTATE 23000 with 1–2 committed, a refused statement runs nothing); browser spec both themes. Usable only with the read-write gate (A15). |
| Renderer | `3974ef9`, `7378b86` | `LiveExport.vue` (Export tab, all live sessions) and Import tab (read-write sessions); browser specs `live-export.spec.ts`, `live-crud.spec.ts` (simulated bridge). |

| Criterion | Status |
|---|---|
| G1 unauthorized paths/symlinks refused; no secrets in summaries/temp names | Verified for export grants and temp names (uuid only). Archive/expansion and spreadsheet-formula criteria not applicable (ZIP/GZIP/XLSX not offered, D008). Library/licence review: no new library added. |
| G2 exact values, NULL/empty, truthful counts | Verified by independent restore (export) and pinned import batches; counts are server-reported. |
| G3 approved engine/format/OS matrix | Blocked (A3/A20): SQL+CSV on MySQL/MariaDB/PostgreSQL, macOS arm64 only. |
| G4/G5 million-row memory profile, quotas, disk-full/cancel/restart | Partially verified: streaming design (one batch held), cancel/failure cleanup tested; million-row RSS profile, disk-full on a small volume and slow-consumer sampling not run; quotas Proposed (A18). |
| G6 restore/read-back for every advertised format | Verified for SQL and CSV on all three engines. |

## Acceptance record — 2026-11-17 (supersedes the 2026-11-16 statuses above; alpha slice; macOS arm64; MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6)
Evidence base: working tree on `b0bfea3` plus uncommitted JSON/GZIP export changes. `npm run test:execution` 2026-11-17, all three engines (`artifacts/m006/execution-*.json`, untracked; MySQL/MariaDB `wholeDatabaseExport`: `roundTrip`, `dropTableIfExists`, `json`, `jsonBinaryRefused`, `csvRefused`, `oversizedRowNamesTable` all true; `sqlImport` round trip 5 rows); `test:native-database` / `test:native-postgres` 2026-11-17; ledger "A20 — JSON and GZIP export".

| Criterion | Status | Evidence or gap |
|---|---|---|
| G1 unauthorized paths/symlinks; archives/expansion bombs; spreadsheet formulas; library/licence review; no secrets in names/logs | Not met (engineering) | Met parts: export grants, `0600` uuid temp files, symlink/dot-file refusal (2026-11-16); SQL import native-only grants and symlink swap refusal (`a6dd0f7`); GZIP output via a second private temp file then rename. No compressed or spreadsheet import is offered, so expansion/formula checks do not apply yet. Gap: `flate2` 1.1.10 (`rust_backend`) is new to the shipped binary (A20) with no recorded licence/advisory review or `audit:release` re-run. |
| G2 exact values; NULL/empty/default; ordering; truthful counts; no whole-script rollback promise | Met for SQL/CSV (three engines) and JSON (MySQL/MariaDB); CSV convention sign-off Blocked on A19 | Independent SQL/CSV restore on three engines (2026-11-16; re-run 2026-11-17, `exportRestore` in `artifacts/m003/native-*.json`, `artifacts/m004/native-postgresql.json`); whole-database SQL export (DROP TABLE + FK checks off) → drop → re-import exact (NULL/empty/binary), MySQL/MariaDB 2026-11-17; JSON export and binary refusal on real MySQL/MariaDB only (the PostgreSQL report has no JSON or whole-database export case — engineering gap). The other Custom options (INSERT IGNORE/REPLACE, one-row INSERT, AUTO_INCREMENT, CSV separator/enclosure/NULL text/header) are covered by runtime unit tests only, not restored against a server (engineering gap); SQL import stops at the first failure with earlier statements reported committed; transaction-wrapped exports cannot be re-imported (UI says so). A19's wording predates the new CSV options (separator, enclosure, NULL text, header) and needs updating. |
| G3 approved engine/format/compression/OS matrix; unsupported combinations rejected | Blocked on A3, A19, A20 (evidence executed) | SQL/CSV on three engines, JSON on MySQL/MariaDB, SQL import on MySQL/MariaDB; macOS arm64 only. Refused: binary in CSV/JSON, whole-database CSV, unknown types, PostgreSQL structure (A19). ZIP, XLSX and GZIP import still Pending under A20; PostgreSQL SQL import not offered. |
| G4/G5 million-row/slow-consumer memory and cleanup; queue/disk/byte quotas; disk-full/cancel/restart | Not met (engineering); quotas Blocked on A18, import limits on A20 | Streaming design (one batch held), cancel/failure cleanup, no export snapshot left open after a failed export (2026-11-17); SQL import benchmark 2026-11-16 (ledger A20 Results). Not run: million-row RSS profile, slow consumer, disk-full on a small volume, restart cleanup for exports. |
| G6 restore/read-back for every format advertised as a backup | Met for SQL and CSV (restored option combinations only); Not met for GZIP (engineering) | SQL/CSV independent restore on all three engines (2026-11-16, re-run 2026-11-17) and whole-database SQL round trip on MySQL/MariaDB (2026-11-17). GZIP: the ledger says "host unit path only"; a grep for `compress`/`flate2`/`gz` on 2026-11-17 found only the implementation in `apps/desktop/src-tauri/src/transfer.rs` (`gzip_finish`), no test there, in `apps/desktop/src-tauri/tests` or in the runtime, and no real-server `.gz` round trip. JSON is not presented as a backup format in `docs/user-guide/import-export.md`. |
