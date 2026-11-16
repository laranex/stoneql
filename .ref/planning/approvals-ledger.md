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
