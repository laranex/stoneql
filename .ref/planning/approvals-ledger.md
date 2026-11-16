# Approvals and sign-offs ledger (M002–M007)

Started 2026-11-16 during the "finish M002–M007" run. Each row is a decision or sign-off only the
user can give. Nothing here is approved by inference; status stays **Pending** until the user
answers in chat. "Unblocks" names the acceptance criterion or work item it clears.

User decisions recorded in this run:
- 2026-11-16 — Supported engines for M002–M007 work: MySQL, MariaDB, PostgreSQL. SQLite keeps its
  code and regressions but receives no new work; its criteria are out of scope. Platform stays
  macOS only (D021). No OS change was stated.
- 2026-11-16 — Constraints carried over: no keyboard/window focus use, minimal storage, short tests.
- 2026-11-16 — A2 and A4 approved. The SQLite advisory must stay visible in the full audit and
  must not be added to an ignore list.
- 2026-11-16 — A1 and A5 approved. Real windows, keyboard and window focus allowed for the GUI
  tests only; the other constraints stay.

| # | Milestone | Decision or sign-off needed | Unblocks | Status |
|---|---|---|---|---|
| A1 | M002–M006 | Permission to open real StoneQL windows (takes focus) | M002 G3 native E2E, OS fallback close dialog, 30-launch startup profile; M006 physical Cmd+R/context-menu/inspector reload; M005 Keychain prompts and native key-file picker | **Approved 2026-11-16** (user: "You may open real app windows and use keyboard and window focus as needed for those tests"). **Run 2026-11-16**: smoke, 30-launch profile, fallback dialog, reload paths and key picker passed; Keychain prompts and signed-app E2E still need A2 (see Results) |
| A2 | M002 | Signing identity / signed release `.app` (M009 custody) | M002 G3 "E2E on the signed release .app"; M005 Keychain ACL/data-protection checks | **Approved 2026-11-16** (user: "Sign the macOS app and run the signed-build checks"). **Partly run** (user chose ad-hoc signing for now): ad-hoc signed `.app` built, verified, launched and closed; Developer ID signing, notarization, Keychain prompts and full E2E still blocked on a signing certificate — see Results |
| A3 | M002–M005 | Approve the Proposed numerical budgets and version matrix: M002 startup/IPC, M003 lifecycle/basic-SQL (MySQL 8.4.6, MariaDB 11.4.8 on macOS arm64), M004 PostgreSQL 17.6 lifecycle/query, M005 SSH transport budget and auth-method matrix | G4/G5 and G3 sign-off for M002–M005 | Pending |
| A4 | M002/M005 | russh RUSTSEC-2026-0495 (agent *server* flaw; StoneQL uses only the agent client, not reachable): approve upgrade `=0.64.1` → 0.65.x (breaking 0.x bump, "russh upgrade policy" from M005 review) or record risk acceptance | Clean macOS dependency audit (M002 alpha blocker) | **Approved 2026-11-16** (user: "Upgrade `russh`, rerun the SSH tests, and audit the release dependency graph separately"). **Done 2026-11-16**: russh 0.65.1; release-graph audit clean (see Results) |
| A5 | M002/M004 | libsqlite3-sys 0.37 RUSTSEC-2026-0459 (bundled SQLite < 3.54, local crafted DB+WAL pair leaks stale memory). sqlx 0.9.0 (latest) requires `=0.37`; no upgrade path. Decide: risk acceptance while SQLite is out of scope, or remove/feature-gate SQLite from the desktop build (conflicts with D022 "keep SQLite regressions") | Clean macOS dependency audit (M002 alpha blocker) | **Approved 2026-11-16** (user: "Exclude SQLite from the release build and verify it is no longer bundled"). **Done 2026-11-16**: not in the release graph or binary; `cargo audit` still reports it from the dev/test lockfile (see Results) |
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

## Results

### A5 — SQLite excluded from the release build (done 2026-11-16)

- Runtime `sqlite` feature (default, used by the runtime's own tests) and desktop `sqlite` feature
  (off by default). `npm run desktop:release` builds without it; SQLite file grants and connections
  are compiled out; the SQLite IPC commands stay registered and refuse with `UNSUPPORTED_CAPABILITY`
  before any dialog; `HostInfo.sqlite=false` disables **Choose file…** with a notice.
- Not bundled: `cargo tree --locked -p stoneql-desktop --features custom-protocol -e normal,build
  -i libsqlite3-sys` (and `-i sqlx-sqlite`) is empty for `aarch64-apple-darwin` and
  `x86_64-apple-darwin`; the positive control with `,sqlite` shows `libsqlite3-sys v0.37.0`.
- Release binary `target/release/stoneql-desktop` (23,369,712 bytes, sha256 prefix
  `8e8fe60b703cc993`): 0 `sqlite3_` symbols, 0 `SQLite format 3` strings, no SQLite library in
  `otool -L`, 0 smoke-probe strings. Control (debug build with `sqlite`): 300 `sqlite3_` symbols.
- Regressions kept: `npm run desktop:test` runs the SQLite suites with the feature (55 passed) and
  the IPC suite without it (`sqlite_capability_matches_the_build`); runtime without SQLite 63
  passed; `rust:test` 158 passed; Clippy `-D warnings` clean for the workspace and for
  runtime ±sqlite, desktop +sqlite and desktop +smoke.
- Remaining: `cargo audit` still lists RUSTSEC-2026-0459 because it reads the whole lockfile
  (development/test graph). The M002 "clean macOS dependency audit" therefore needs either an
  audit scoped to the release graph or an ignore entry, which is a new decision (not taken).

### A1 — real-window GUI tests on macOS arm64 (run 2026-11-16, unsigned builds)

| Check | Command | Result |
|---|---|---|
| Native smoke (debug) | `npm run desktop:smoke` | Passed, 2,783 ms |
| 30-launch release startup profile | `npm run desktop:profile` (optimized build + non-shipped `smoke` feature) | 30/30 passed; host start → renderer ready min 562 / median 607 / p95 701 / max 715 ms; spawn → exit median 624 ms, max 1,152 ms (first launch). Sequential, no cache purge. Budgets still Proposed (A3) |
| OS fallback close dialog, unresponsive renderer | `journal_window fallback` | Passed: renderer frozen, window close → native "not responding" alert (rendered by macOS UserNotificationCenter); Cancel → app keeps running, alert gone; second close → alert again; Close StoneQL → exit status 0 |
| Physical reload paths while work is active | `journal_window reload`; `real_window_reload` test | Passed: Cmd+R (no reload), WebView context-menu Reload (real right-click, menu item chosen; refused, in-app dialog requested), Web Inspector Cmd+R (refused); after confirming, reloaded, lease rolled back, 2 unknown outcomes listed natively and in the UI |
| Native SSH key-file picker | `journal_window picker` (twice) | Passed: Escape → no grant; file chosen via Go to Folder → opaque grant, display name only |
| Keychain prompts | — | Not run: a meaningful prompt check needs the signed app (A2) |
| Native E2E on the signed `.app` (M002 G3) | — | Not run: needs A2 (signed artifact) |

Harness notes: the context menu needs a right-click outside the data grid (the grid has its own
menu); System Events cannot see the WKWebView menu or the CFUserNotification alert buttons, so the
harness uses the keyboard for the menu and AXPress in UserNotificationCenter for the alert. Early
harness runs left three orphaned alerts on screen; they were dismissed and the harness now answers
its alert on failure.

### A4 — russh upgrade and release-graph audit (done 2026-11-16)

- `russh` `=0.64.1` → `=0.65.1` (`crates/stoneql-runtime/Cargo.toml`); `cargo update -p russh`
  changed only russh (and dropped one unused `generic-array` edge). No source change was needed.
- SSH reruns: `npm run test:ssh` (Docker OpenSSH + private agent, per-run Keychain service) —
  MySQL 190 forwarded channels (full matrix), MariaDB 41 and PostgreSQL 41 (parity), all passed;
  `desktop:test` 55 passed (includes `live_ssh` negatives and `ssh_read_write`); `rust:test` 158
  passed; `rust:lint`, `rust:fmt`, `rust:vendor` clean.
- Release-graph audit: new `npm run audit:release` (`tools/audit-release.mjs`) prunes a temporary
  copy of `Cargo.lock` to the normal+build graph of `stoneql-desktop --features custom-protocol`
  for `aarch64-apple-darwin` and `x86_64-apple-darwin` (366 of 575 packages) and runs
  `cargo audit --file` on it: **0 vulnerabilities, 0 warnings**. Positive control (same script
  with `custom-protocol,sqlite`, 372 packages): reports RUSTSEC-2026-0459, exit 1.
- Full audit (`cargo audit`, 575 packages): **1 vulnerability, RUSTSEC-2026-0459 (libsqlite3-sys,
  test-only graph)** — left visible; no `audit.toml`/ignore list exists. RUSTSEC-2026-0495 (russh)
  no longer reported. Linux-only warnings RUSTSEC-2024-0370 and RUSTSEC-2024-0429 still listed.

### A2 — signed macOS app (approved 2026-11-16; ad-hoc interim run, Developer ID blocked)

Blockers found first:
1. `security find-identity -v -p codesigning` → 0 valid identities: no Developer ID Application
   certificate (or any signing certificate) is installed on this Mac. Installing one needs the
   user's Apple Developer account; notarization also needs the user's credentials.
2. The repository has no `.app` bundling step (no Tauri CLI); `desktop:release` produces a bare
   executable.
3. The bundle identifier (A14, `app.dbadmin.desktop` today) fixes the Keychain service and the
   signing identity's app ID, so Keychain ACL results recorded before A14 would need repeating.

User decision 2026-11-16: "Ad-hoc sign for now" (A2 stays open).

Ad-hoc run (2026-11-16, macOS 27 arm64):
- Bundling added: root dev dependency `@tauri-apps/cli` `2.12.4` (exact; 2.12.5 was published the
  same day and skipped), `apps/desktop/src-tauri/tauri.macos-adhoc.conf.json` (bundle `app`,
  `signingIdentity: "-"`, `hardenedRuntime: true`, icon from `icons/icon.png`), script
  `npm run desktop:bundle:adhoc`. The npm install re-hoisted `package-lock.json`; the resolved
  package set is unchanged except the CLI packages and one unreferenced, pruned
  `@codemirror/theme-one-dark` entry.
- `StoneQL.app` (22.49 MiB; binary sha256 prefix `33d5dad01d974871`, CDHash
  `eb83331865bf1cf7da8e8cd042c86e4d4aa6e77e`): `codesign --verify --deep --strict` valid and
  satisfies its designated requirement; `flags=0x10002(adhoc,runtime)`, `TeamIdentifier=not set`,
  no entitlements; identifier `app.dbadmin.desktop`, version 0.1.0. `spctl --assess` → rejected
  (expected: no Developer ID). Notarization skipped by the bundler (no Apple credentials).
- Bundled binary: 0 `sqlite3_` symbols, no SQLite library, no smoke probe.
- Launch of the signed app: window after 1,383 ms, UI rendered in desktop-host mode (native bridge
  bootstrapped under hardened runtime, screenshot checked), three WebKit service processes;
  window close button → process exited with status 0; no alerts left.
- Not run, with reason: Keychain prompts (first save, first use, after upgrade) — an ad-hoc
  designated requirement is the CDHash, so every rebuild is a new identity to the Keychain and the
  prompts would not represent Developer ID behaviour; full native E2E (M002 G3) on the release
  `.app`; notarization and Gatekeeper acceptance. All need a Developer ID Application certificate
  (and notarization credentials) installed by the user, and should follow A14 (bundle identifier).

