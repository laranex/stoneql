# Approvals and sign-offs ledger (M002–M007)

Started 2026-11-16 during the "finish M002–M007" run. Each row is a decision or sign-off only the
user can give. Nothing here is approved by inference; status stays **Pending** until the user
answers in chat. "Unblocks" names the acceptance criterion or work item it clears.

User decisions recorded in this run:
- 2026-11-16 — Supported engines for M002–M007 work: MySQL, MariaDB, PostgreSQL. SQLite keeps its
  code and regressions but receives no new work; its criteria are out of scope. Platform stays
  macOS only (D021). No OS change was stated.
- 2026-11-16 — Constraints carried over: no keyboard/window focus use, minimal storage, short tests.
- 2026-11-16 — A6 (4,096 columns, 4 MiB), A8 (2 MiB per PostgreSQL message) and the A20 alpha SQL import scope for MySQL/MariaDB approved; the import must validate the entire file and execute from the same validated snapshot; limits to be benchmarked and sent for sign-off.
- 2026-11-16 — A2 closed by the user with the ad-hoc signed build ("A2 close with ad-hoc signed build.").
- 2026-11-16 — A14 approved: bundle identifier `me.thixpin.stoneql`; use it for the A2 signed-build tests.
- 2026-11-16 — A2 and A4 approved. The SQLite advisory must stay visible in the full audit and
  must not be added to an ignore list.
- 2026-11-16 — A1 and A5 approved. Real windows, keyboard and window focus allowed for the GUI
  tests only; the other constraints stay.

| # | Milestone | Decision or sign-off needed | Unblocks | Status |
|---|---|---|---|---|
| A1 | M002–M006 | Permission to open real StoneQL windows (takes focus) | M002 G3 native E2E, OS fallback close dialog, 30-launch startup profile; M006 physical Cmd+R/context-menu/inspector reload; M005 Keychain prompts and native key-file picker | **Approved 2026-11-16** (user: "You may open real app windows and use keyboard and window focus as needed for those tests"). **Run 2026-11-16**: smoke, 30-launch profile, fallback dialog, reload paths and key picker passed; Keychain prompts and signed-app E2E still need A2 (see Results) |
| A2 | M002 | Signing identity / signed release `.app` (M009 custody) | M002 G3 "E2E on the signed release .app"; M005 Keychain ACL/data-protection checks | **Approved 2026-11-16; closed 2026-11-16 by user decision with the ad-hoc signed build** (user: "A2 close with ad-hoc signed build."). Verified: ad-hoc + hardened-runtime `StoneQL.app` (identifier `me.thixpin.stoneql`), launch, render, clean exit. Not done and accepted by that decision: Developer ID signing, notarization/Gatekeeper acceptance, Keychain prompts on a Developer ID build, full signed-app E2E — see Results |
| A3 | M002–M005 | Approve the Proposed numerical budgets and version matrix: M002 startup/IPC, M003 lifecycle/basic-SQL (MySQL 8.4.6, MariaDB 11.4.8 on macOS arm64), M004 PostgreSQL 17.6 lifecycle/query, M005 SSH transport budget and auth-method matrix | G4/G5 and G3 sign-off for M002–M005 | Pending |
| A4 | M002/M005 | russh RUSTSEC-2026-0495 (agent *server* flaw; StoneQL uses only the agent client, not reachable): approve upgrade `=0.64.1` → 0.65.x (breaking 0.x bump, "russh upgrade policy" from M005 review) or record risk acceptance | Clean macOS dependency audit (M002 alpha blocker) | **Approved 2026-11-16** (user: "Upgrade `russh`, rerun the SSH tests, and audit the release dependency graph separately"). **Done 2026-11-16**: russh 0.65.1; release-graph audit clean (see Results) |
| A5 | M002/M004 | libsqlite3-sys 0.37 RUSTSEC-2026-0459 (bundled SQLite < 3.54, local crafted DB+WAL pair leaks stale memory). sqlx 0.9.0 (latest) requires `=0.37`; no upgrade path. Decide: risk acceptance while SQLite is out of scope, or remove/feature-gate SQLite from the desktop build (conflicts with D022 "keep SQLite regressions") | Clean macOS dependency audit (M002 alpha blocker) | **Approved 2026-11-16** (user: "Exclude SQLite from the release build and verify it is no longer bundled"). **Done 2026-11-16**: not in the release graph or binary; `cargo audit` still reports it from the dev/test lockfile (see Results) |
| A6 | M003/M006/M007 | Adopt the metadata-limit vendor patch (D002 extension, alpha plan e1): 4,096 columns / 4 MiB descriptor budget per result, `experiments/sqlx-metadata-limit/metadata-limit.patch` (re-verified to apply 2026-11-16). Note: the hostile huge-column-count abort was reasoned, not executed. | M003 G4/G5 aggregate memory bound; same driver path for M006 browse/CRUD and M007 export | **Approved 2026-11-16** (user: "I approve A6 (4,096 columns and 4 MiB of metadata)"). **Done 2026-11-16**: patch adopted in `vendor/sqlx-mysql`, real-server regression passed (see Results) |
| A7 | M003 | Accept residual read-only trust limits (hostile UDF/plugin/external effects not certifiable; advisory locks possible via definitions) as documented in `docs/development/read-only-safety.md` | M003 G1 sign-off | Pending |
| A8 | M004/M006/M007 | Adopt the sqlx-postgres message-limit patch as `vendor/sqlx-postgres` (D002-class extension) with a 2 MiB runtime limit mapped to `PacketTooLarge`: fixes a hostile-server panic (length < 4) and unbounded per-message buffering. `.ref/planning/m004-pg-message-limit/findings.md` | M004 G1/G4 message bound; same driver path for PostgreSQL browse (M006) and transfers (M007) | **Approved 2026-11-16** (user: "A8 (2 MiB per PostgreSQL backend message)"). **Done 2026-11-16**: `vendor/sqlx-postgres` adopted, real-server regression passed (see Results) |
| A9 | M004 | Review/approve D011 (PostgreSQL TLS/env refusal), D012 (read-only boundary), D013 (value decoding, interval/arrays/ranges stay unknown) | M004 G1–G3 sign-off | Pending |
| A10 | M004/M009 | PostgreSQL alpha go/no-go (D022: optional, never blocks the alpha). The 2026-11-16 scope instruction covers milestone work; it is not taken as the alpha go decision | Whether PostgreSQL ships enabled in the alpha | Pending |
| A11 | M005 | Approve the alpha-fix mechanisms implemented as Proposed (m005-alpha-fix-plan open questions 1–6): native app-data host-key file (not Keychain); native confirmation dialog on trust; re-enrollment instead of import; fingerprint binding for `ssh_password` only (F-08 JSON scopes not adopted); 60 s native secret deadline / 75 s renderer wait with a saved reference; "Remove anyway" when credential deletion fails; trust shared per SSH endpoint; limits 512 hosts / 8 KiB key / 1 MiB file, 16 pending scans / 5 min | M005 F-01/F-02/F-04/F-10 sign-off (implementation and tests done) | Pending |
| A12 | M005 | RSA policy (stay unsupported recommended / accept RUSTSEC-2023-0071 for client signing / aws-lc-rs) and keyboard-interactive policy (detect-and-refuse recommended / implement) | M005 G3 auth-method matrix | Pending |
| A13 | M005 | Review/approve D005, D006 and D016–D020 (desktop secret provider, trust/agent policy, SSH transport decisions) | M005 sign-off | Pending |
| A14 | M005/M009 | Settle the bundle identifier (`app.dbadmin.desktop` today, D021 open item): it fixes the Keychain service name and the host-key store directory; changing it later de-enrolls every host and orphans saved secrets | M005 alpha blocker note; M009 packaging | **Approved 2026-11-16** (user: "Change the macOS bundle identifier from `app.dbadmin.desktop` to `me.thixpin.stoneql`"). **Done 2026-11-16**: identifier, Keychain service and test prefix changed together; verified in the bundled `.app`; impact documented (see Results) |
| A15 | M006/M007 | Enable unrestricted read-write SQL in builds (`READ_WRITE_SQL_ENABLED`), Direct first; SSH read-write separately | Live CRUD and SQL-tab writes in shipped builds; M007 SQL/CSV import (D022 dependency) | **Approved 2026-11-17** — MySQL/MariaDB Direct + SSH, PostgreSQL Direct; PostgreSQL over SSH still read-only — see Results |
| A16 | M006 | Approve the remaining Proposed M006 decisions and values: D025-11, D025-12, D025-13 (durable journal), `ExecutionLimits` release values, alpha-plan Q1–Q9 | M006 G4/G5 sign-off | Pending |
| A17 | M006 | Approve the live CRUD and metadata design implemented as Proposed: rows addressed by full primary key only (keyless tables read-only), `_utf8mb4 X'…'` / `E'…'` literals, binary columns read-only, 0 affected rows = conflict, one DELETE per selected row stopping at the first failure, catalog refresh on DDL keywords | M006 CRUD sign-off (implemented and fixture-verified) | Pending |
| A18 | M007 | Approve transfer quotas implemented as Proposed defaults: CSV import file ≤ 64 MiB (renderer-held), INSERT batches < 60 KiB, export SQL batches of 500 rows, 8 outstanding export grants / 16 transfer records per session; and the alpha-plan quotas still unimplemented (1 GiB import, free-space floor, +256 MiB RSS for 1M rows) | M007 G4/G5 sign-off | Pending |
| A19 | M007 | Approve the CSV convention (every value quoted, NULL = unquoted `NULL`; binary columns refused in CSV) and PostgreSQL data-only SQL export (structure would need pg_dump) | M007 G2/G3 format sign-off | Pending |
| A20 | M007 | D008 transfer library/format decisions: GZIP export (Proposed keep), ZIP, JSON/XLSX export, GZIP import (all Deferred/not offered); and whether live SQL import (d5) is built natively before the alpha | Remaining M007 formats and d5 | **Alpha SQL import scope for MySQL/MariaDB approved 2026-11-16** (user: "A20’s Alpha SQL import scope for MySQL and MariaDB"; "validate the entire file and execute from the same validated snapshot"). Implemented and verified; **limits awaiting sign-off** from the benchmark (see Results). **JSON and GZIP export approved 2026-11-17** (user: "Approve JSON + GZIP"); ZIP, XLSX and GZIP import still Pending |

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
  no entitlements; identifier `app.dbadmin.desktop` (before A14), version 0.1.0. `spctl --assess` → rejected
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

### A14 — bundle identifier `me.thixpin.stoneql` (done 2026-11-16)

- Changed together (D023 recommendation, no migration code): `tauri.conf.json` `identifier`;
  Keychain service `DEFAULT_SERVICE` (desktop) and `CREDENTIAL_SERVICE` (frontend) →
  `me.thixpin.stoneql.credentials`; SSH fixture per-run service prefix and its assertion →
  `me.thixpin.stoneql.test-<random>`; the F-10 frontend test expectation; docs (branding,
  deployment, roadmap, ssh-credentials, sql-execution, user guide), D020/D021/D023, M009 and the
  rebranding record.
- Verified in the bundled ad-hoc `.app` (`npm run desktop:bundle:adhoc`): `CFBundleIdentifier` and
  the code-signature `Identifier` are `me.thixpin.stoneql`; `codesign --verify --deep --strict`
  valid (`adhoc,runtime`); the binary contains `me.thixpin.stoneql.credentials` and no
  `app.dbadmin.desktop` string. Launch: window after 1,630 ms, UI rendered in desktop-host mode,
  close button → exit 0, no alerts. First launch created
  `~/Library/{Application Support,WebKit,Caches}/me.thixpin.stoneql` (Application Support holds
  `outcome-journal`; no host-key file until a host is trusted).
- Tests: `npm test` 220 + 19; `desktop:test` 55; `rust:lint`, `rust:fmt`, `docs:build`;
  `test:ssh` passed with the new Keychain prefix (MySQL 190, MariaDB 41, PostgreSQL 41 channels).
- Impact on data from earlier builds (documented in `docs/development/branding.md`):
  - Keychain: items under `app.dbadmin.desktop.credentials` are no longer read; connecting with a
    profile that referenced one reports "The saved credential is unavailable or belongs to a
    different target. Enter it again." Old items stay until deleted manually.
  - SSH host trust: `…/app.dbadmin.desktop/ssh-host-keys.json` is not read, so every host is
    de-enrolled and must be trusted again explicitly; nothing is trusted automatically.
  - Unknown-outcome journal: records under `…/app.dbadmin.desktop/outcome-journal/` are not listed.
  - Bundled-app WebView storage moves with the identifier (profiles/preferences start empty);
    unbundled development runs key it by executable name and are unaffected.
- This Mac: the old identifier's data (`Application Support/app.dbadmin.desktop/outcome-journal`,
  `WebKit/` and `Caches/app.dbadmin.desktop`) was left untouched; no old Keychain items and no
  host-key file existed. Manual reset commands are in the branding doc.
- A2 signed-build checks now use the new identifier (ad-hoc interim above, rerun under
  `me.thixpin.stoneql`); Developer ID signing remains blocked on a certificate.

### A2 closure (2026-11-16)

Closed by user decision with the ad-hoc signed build. Evidence is the ad-hoc runs above (signature
valid with hardened runtime, identifier `me.thixpin.stoneql`, launch/render/close). Not performed
and not claimed as verified: Developer ID signing, notarization and Gatekeeper acceptance, Keychain
access prompts on a Developer ID build (first save, first use, after upgrade), and the full native
E2E on a Developer ID-signed `.app`. These remain open release items for M009 distribution: a
public build that Gatekeeper accepts still needs a Developer ID certificate.

### A6 — MySQL/MariaDB result-metadata limit (done 2026-11-16)

- `experiments/sqlx-metadata-limit/metadata-limit.patch` applied to `vendor/sqlx-mysql` as its second
  reviewed delta; `dbadmin-provenance.json` regenerated (5 source files + `DBADMIN.md`) with
  `metadataLimitPatchSha256`; `tools/check-sqlx-vendor.mjs` verifies it; `tests/sqlx-vendor.test.mjs`
  covers source, provenance, receive-limit and metadata-limit patch drift.
- Runtime: `max_result_metadata(4096, 4 MiB)` in `mysql.rs`; the fixed driver error maps to
  `ResultTooWide` (IPC `PACKET_TOO_LARGE`).
- Tests: vendored driver library suite 35/35 (includes the patch's limit tests); runtime unit test
  for the mapping; `npm run test:native-database` (MySQL 8.4.6, MariaDB 11.4.8, verified TLS):
  4,096 columns returned, 4,097 refused, the next query works. Before the patch both servers
  returned up to 65,535 columns, so the 4,097 assertion only passes with the patch.
- Remaining gap: the hostile huge-column-count process abort is still reasoned, not reproduced.

### A8 — PostgreSQL backend-message limit (done 2026-11-16)

- `vendor/sqlx-postgres` = registry sqlx-postgres 0.9.0 (crate sha256 `87a2bdd6…`, matches
  `Cargo.lock`) + `experiments/sqlx-pg-message-limit/message-limit.patch`; MIT selected, full MIT
  text kept (the registry's `LICENSE-MIT` symlink replaced by the identical text), `Cargo.lock`,
  `LICENSE-APACHE` and registry bookkeeping files not copied; `DBADMIN.md`, `dbadmin-provenance.json`
  and `experiments/sqlx-pg-message-limit/upstream.json` added; root `[patch.crates-io]` entry added;
  `rust:vendor` verifies both drivers (76 + 117 files).
- Runtime: `max_receive_message_size(2 MiB)` in `postgres.rs`; the error (which carries the
  driver's source location) maps to `PacketTooLarge` by prefix.
- Tests: driver library suite 163/163; runtime unit test; `npm run test:native-postgres`
  (PostgreSQL 17.6, verified TLS): a 3 MiB value is refused with `PACKET_TOO_LARGE` and the next
  query works; the full PostgreSQL fixture, export restore and execution suites pass.
- Audit coverage: `cargo audit` (full and `audit:release`) matches advisories against registry
  packages only, so the vendored `sqlx-mysql` and now `sqlx-postgres` path crates are outside it;
  their upstream advisories must be reviewed manually on every SQLx upgrade (DBADMIN.md
  maintenance rule). Full audit and release-graph audit results are otherwise unchanged.
- Note: the vendored crate shows one upstream `dead_code` warning (non-member path dependency;
  `rust:lint` passes). The adversarial length-0..3 and 1 GiB-header cases are covered by the
  prototype's 4/4 tests, not re-run against the vendored copy.

### A20 — alpha SQL import for MySQL/MariaDB (implemented 2026-11-16; limits awaiting sign-off)

Design (as approved): the host reads the granted file once; the runtime validates the entire file
(UTF-8, size, statement count, per-statement size, the read-write script policy plus `USE` and
`LOAD DATA/XML LOCAL`) and, only if every statement passes, runs that same in-memory snapshot (same
buffer, same statement ranges; the file is never read again) one statement at a time with
autocommit, stopping at the first failure. Each statement is journaled before it is sent;
cancel, release, reload protection and unknown-outcome recording are the job path's.

Verification: runtime unit tests (validation, refusals with statement number and line, limits,
BOM/CRLF/`DELIMITER`/executable comments), SQLite runner tests (success with discarded result
rows, stop at first failure, refusal sends nothing, cancel → unknown + journal, busy handle);
desktop IPC/host tests (native-only grants, refusals before reading, symlink swap, oversize from
size; 31 commands in the ACL sweep); real servers via `npm run test:execution` on MySQL 8.4.6 and
MariaDB 11.4.8: a StoneQL export with `CREATE TABLE` round-trips 5 edge-value rows exactly, a
duplicate key stops at statement 3 / line 3 (SQLSTATE 23000) with statements 1–2 committed and 4
not run, a refused statement runs nothing; browser spec in both themes.

Benchmark (`npm run bench:sql-import`, release build, Apple M5 10-core / 24 GiB, macOS 27.0.1,
Docker MySQL 8.4.6 and MariaDB 11.4.8 with a 1 GiB memory cap each, verified TLS, durable journal
attached as in the app; StoneQL-export-style multi-row INSERTs; one process per case):

| Case | Statements | Read | Validate | Execute MySQL / MariaDB | Throughput MySQL / MariaDB | Client peak RSS |
|---|---|---|---|---|---|---|
| 4 MiB, 1 KiB statements | 4,310 | 0–1 ms | 15–19 ms | 36.6 s / 37.6 s | 118 / 115 statements/s | 12 MiB |
| same, journal off (diagnostic) | 4,310 | — | — | 3.2 s / 4.2 s | 1,341 / 1,017 statements/s | 12 MiB |
| 64 MiB, 64 KiB statements | 1,029 | 11–12 ms | 93–106 ms | 9.7 s / 10.2 s | 6.6 / 6.3 MiB/s | 73 MiB |
| 256 MiB, 1 MiB statements | 259 | 55 ms | 347–423 ms | 8.8 s / 8.7 s | 29.2 / 29.5 MiB/s | 272 MiB |
| statements just under 4 / 8 / 15 / 16 MiB | 3 each | ≤ 2 ms | ≤ 34 ms | all succeeded on both | 17–32 MiB/s | 27–90 MiB |
| 1 GiB, ~1.3 KiB statements (validation only) | 813,051 | 176–181 ms | 1.64–1.72 s | — | — | 1,045 MiB |
| 1 GiB, 4 MiB statements | 259 | 160–251 ms | 1.58–1.68 s | 37.5 s / 29.4 s | 27.3 / 34.9 MiB/s | 1,052 MiB |

Findings for the sign-off:
1. Memory: the client holds the file once (peak RSS ≈ file size + ~30 MiB). A first run showed
   2× (the validated text was copied into an `Arc<str>`); fixed and re-measured.
2. Per-statement journaling dominates small statements: ~7.7 ms of the ~8.5 ms per statement
   (journal on vs off). At the proposed 1,000,000-statement limit a file of small statements would
   take ~2.4 h and hit the 2 h import deadline.
3. Server limits: `max_allowed_packet` is 64 MiB (MySQL 8.4) and 16 MiB (MariaDB 11.4). Statements
   just under 16 MiB succeeded on both. A statement of exactly the 16 MiB StoneQL limit would exceed
   MariaDB's default by the 1-byte command header (protocol reasoning; not executed).
4. Fixture capacity: in one full-matrix run the MySQL container was OOM-killed (its 1 GiB memory
   cap) during the 1 GiB import after ~1.3 GiB of earlier inserts; the same case passed in a fresh
   container. A server-side limit, not a StoneQL client failure; how StoneQL reported that lost
   connection was not captured by the benchmark.
5. Validation is fast: ~1.7 s per GiB including ~0.8 M statements; reading ~0.2 s per GiB.


### A15 — read-write SQL for Direct connections (approved 2026-11-17)

- User: "approved to safety, no need the read only connection anymore"; then chose **Direct only** (SSH stays read-only, separate approval) and **default read-write with an opt-in read-only** setting.
- `READ_WRITE_SQL_ENABLED = true`; host still refuses SSH read-write (`ssh_read_write.rs`) and refuses read-write when a host is built with the gate off (`ipc.rs` gate test, injected services).
- Renderer: "Session access" select and "Read-write…" button replaced by a saved per-profile **Read-only connection** checkbox (`readOnly: true` persisted only when ticked); locked on for SSH and when the host reports the gate off. PROD acknowledgement kept for every read-write connect.
- Real servers (2026-11-17, gate on): `test:execution` MySQL 8.4.6 / MariaDB 11.4.8 / PostgreSQL 17.6 read-write execution checks passed (cancel p95 132.3 / 63.1 / 50.5 ms); `test:native-database` MySQL and MariaDB real database IPC checks passed (includes SQL import). desktop:test 60 passed; rust fmt/lint/boundaries, contracts:check pass; npm test 228+22; browser 64+.
- Not yet: graphical row editing against a real server in the packaged app (browser specs use the stubbed host); SSH read-write.

#### A15 extension — MySQL/MariaDB read-write over SSH (approved 2026-11-17)

- User: "I approve A15 read-write for MySQL/MariaDB over both Direct and SSH." Runtime and host gates now allow SSH read-write for MySQL/MariaDB only; PostgreSQL over SSH is refused before Keychain/network (`ssh_read_write.rs`, runtime `postgres_read_write_over_ssh_is_refused_before_network_access`).
- Real servers (2026-11-17): `test:ssh` passed for MySQL (195 forwarded channels, full matrix), MariaDB (46) and PostgreSQL (41), including the tunnelled-cancellation fixture and a new host-path check (`live_ssh.rs`): a `stoneql_writer` account connects read-write through the tunnel, DDL/DML jobs succeed, a transaction rolls back (report `sshReadWrite: {jobs, transactionRollback}` for MySQL and MariaDB).
- `test:execution` passed on MySQL/MariaDB/PostgreSQL (cancel p95 138.7 / 26.4 / 69.6 ms) including the whole-database export round trip (export → drop → re-import, NULL/empty/binary exact; 3 MiB row names its table; CSV refused). A failed export now ends its snapshot and closes its connection before reporting (found by this run).
- `test:native-database` MySQL/MariaDB passed; rust:test 167, desktop:test 60, lint/fmt/boundaries/vendor/contracts pass; npm test 229+22; browser 66; docs:build.

### A20 — JSON and GZIP export (approved 2026-11-17)

- User chose "Approve JSON + GZIP" for live export in this release. Implemented with the phpMyAdmin-style custom export: table/database scope, table subsets, structure/data/both, SQL options (DROP TABLE [IF EXISTS], CREATE TABLE, AUTO_INCREMENT values, one-row/extended INSERT, INSERT IGNORE/REPLACE, transaction, FK checks off), CSV options (separator, enclosure incl. none with ambiguity refusal, NULL text, header), JSON (values as exact strings, NULL as null, binary refused), GZIP (flate2 1.1.10 `rust_backend`, already in the lockfile, new to the shipped binary; compressed into a second private temp file, then renamed).
- Real servers 2026-11-17: `test:execution` MySQL/MariaDB/PostgreSQL passed (cancel p95 113.6 / 121.0 / 59.0 ms) incl. whole-database export round trip with DROP TABLE IF EXISTS, JSON of chosen tables, JSON binary refusal, oversized-row table naming, whole-database CSV refusal, and no export snapshot left open; `test:native-database`, `test:native-postgres`, `test:ssh` passed. Runtime unit tests cover CSV/JSON/SQL writers and file wrapping. GZIP verified by host unit path only (no real-server gzip round trip yet).
- Known limit: an export with "Enclose data in a transaction" cannot be re-imported by StoneQL's SQL import (transaction statements refused); the UI says so.
