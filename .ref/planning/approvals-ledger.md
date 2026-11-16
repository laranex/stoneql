# Approvals and sign-offs ledger (M002–M007)

Started 2026-11-16 during the "finish M002–M007" run. Each row is a decision or sign-off only the
user can give. Nothing here is approved by inference; status stays **Pending** until the user
answers in chat. "Unblocks" names the acceptance criterion or work item it clears.

User decisions recorded in this run:
- 2026-11-16 — Supported engines for M002–M007 work: MySQL, MariaDB, PostgreSQL. SQLite keeps its
  code and regressions but receives no new work; its criteria are out of scope. Platform stays
  macOS only (D021). No OS change was stated.
- 2026-11-16 — Constraints carried over: no keyboard/window focus use, minimal storage, short tests.

| # | Milestone | Decision or sign-off needed | Unblocks | Status |
|---|---|---|---|---|
| A1 | M002–M006 | Permission to open real StoneQL windows (takes focus) | M002 G3 native E2E, OS fallback close dialog, 30-launch startup profile; M006 physical Cmd+R/context-menu/inspector reload; M005 Keychain prompts and native key-file picker | Pending |
| A2 | M002 | Signing identity / signed release `.app` (M009 custody) | M002 G3 "E2E on the signed release .app"; M005 Keychain ACL/data-protection checks | Pending |
| A3 | M002–M005 | Approve the Proposed numerical budgets and version matrix: M002 startup/IPC, M003 lifecycle/basic-SQL (MySQL 8.4.6, MariaDB 11.4.8 on macOS arm64), M004 PostgreSQL 17.6 lifecycle/query, M005 SSH transport budget and auth-method matrix | G4/G5 and G3 sign-off for M002–M005 | Pending |
| A4 | M002/M005 | russh RUSTSEC-2026-0495 (agent *server* flaw; StoneQL uses only the agent client, not reachable): approve upgrade `=0.64.1` → 0.65.x (breaking 0.x bump, "russh upgrade policy" from M005 review) or record risk acceptance | Clean macOS dependency audit (M002 alpha blocker) | Pending |
| A5 | M002/M004 | libsqlite3-sys 0.37 RUSTSEC-2026-0459 (bundled SQLite < 3.54, local crafted DB+WAL pair leaks stale memory). sqlx 0.9.0 (latest) requires `=0.37`; no upgrade path. Decide: risk acceptance while SQLite is out of scope, or remove/feature-gate SQLite from the desktop build (conflicts with D022 "keep SQLite regressions") | Clean macOS dependency audit (M002 alpha blocker) | Pending |
| A6 | M003/M006/M007 | Adopt the metadata-limit vendor patch (D002 extension, alpha plan e1): 4,096 columns / 4 MiB descriptor budget per result, `experiments/sqlx-metadata-limit/metadata-limit.patch` (re-verified to apply 2026-11-16). Note: the hostile huge-column-count abort was reasoned, not executed. | M003 G4/G5 aggregate memory bound; same driver path for M006 browse/CRUD and M007 export | Pending |
| A7 | M003 | Accept residual read-only trust limits (hostile UDF/plugin/external effects not certifiable; advisory locks possible via definitions) as documented in `docs/development/read-only-safety.md` | M003 G1 sign-off | Pending |
| A8 | M004/M006/M007 | Adopt the sqlx-postgres message-limit patch as `vendor/sqlx-postgres` (D002-class extension) with a 2 MiB runtime limit mapped to `PacketTooLarge`: fixes a hostile-server panic (length < 4) and unbounded per-message buffering. `.ref/planning/m004-pg-message-limit/findings.md` | M004 G1/G4 message bound; same driver path for PostgreSQL browse (M006) and transfers (M007) | Pending |
| A9 | M004 | Review/approve D011 (PostgreSQL TLS/env refusal), D012 (read-only boundary), D013 (value decoding, interval/arrays/ranges stay unknown) | M004 G1–G3 sign-off | Pending |
| A10 | M004/M009 | PostgreSQL alpha go/no-go (D022: optional, never blocks the alpha). The 2026-11-16 scope instruction covers milestone work; it is not taken as the alpha go decision | Whether PostgreSQL ships enabled in the alpha | Pending |
| A11 | M005 | Approve the alpha-fix mechanisms implemented as Proposed (m005-alpha-fix-plan open questions 1–6): native app-data host-key file (not Keychain); native confirmation dialog on trust; re-enrollment instead of import; fingerprint binding for `ssh_password` only (F-08 JSON scopes not adopted); 60 s native secret deadline / 75 s renderer wait with a saved reference; "Remove anyway" when credential deletion fails; trust shared per SSH endpoint; limits 512 hosts / 8 KiB key / 1 MiB file, 16 pending scans / 5 min | M005 F-01/F-02/F-04/F-10 sign-off (implementation and tests done) | Pending |
| A12 | M005 | RSA policy (stay unsupported recommended / accept RUSTSEC-2023-0071 for client signing / aws-lc-rs) and keyboard-interactive policy (detect-and-refuse recommended / implement) | M005 G3 auth-method matrix | Pending |
| A13 | M005 | Review/approve D005, D006 and D016–D020 (desktop secret provider, trust/agent policy, SSH transport decisions) | M005 sign-off | Pending |
| A14 | M005/M009 | Settle the bundle identifier (`app.dbadmin.desktop` today, D021 open item): it fixes the Keychain service name and the host-key store directory; changing it later de-enrolls every host and orphans saved secrets | M005 alpha blocker note; M009 packaging | Pending |
| A15 | M006/M007 | Enable unrestricted read-write SQL in builds (`READ_WRITE_SQL_ENABLED`), Direct first; SSH read-write separately | Live CRUD and SQL-tab writes in shipped builds; M007 SQL/CSV import (D022 dependency) | Pending |
| A16 | M006 | Approve the remaining Proposed M006 decisions and values: D025-11, D025-12, D025-13 (durable journal), `ExecutionLimits` release values, alpha-plan Q1–Q9 | M006 G4/G5 sign-off | Pending |
| A17 | M006 | Approve the live CRUD and metadata design implemented as Proposed: rows addressed by full primary key only (keyless tables read-only), `_utf8mb4 X'…'` / `E'…'` literals, binary columns read-only, 0 affected rows = conflict, one DELETE per selected row stopping at the first failure, catalog refresh on DDL keywords | M006 CRUD sign-off (implemented and fixture-verified) | Pending |
| A18 | M007 | Approve transfer quotas implemented as Proposed defaults: CSV import file ≤ 64 MiB (renderer-held), INSERT batches < 60 KiB, export SQL batches of 500 rows, 8 outstanding export grants / 16 transfer records per session; and the alpha-plan quotas still unimplemented (1 GiB import, free-space floor, +256 MiB RSS for 1M rows) | M007 G4/G5 sign-off | Pending |
| A19 | M007 | Approve the CSV convention (every value quoted, NULL = unquoted `NULL`; binary columns refused in CSV) and PostgreSQL data-only SQL export (structure would need pg_dump) | M007 G2/G3 format sign-off | Pending |
| A20 | M007 | D008 transfer library/format decisions: GZIP export (Proposed keep), ZIP, JSON/XLSX export, GZIP import (all Deferred/not offered); and whether live SQL import (d5) is built natively before the alpha | Remaining M007 formats and d5 | Pending |
