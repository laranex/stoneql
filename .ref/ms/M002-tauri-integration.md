# M002 — Tauri host integration

## Status
**In progress — not closable (reviewed 2026-11-17; record "Acceptance record — 2026-11-17" at the end).** Met: G1, G2, G6, OS fallback close dialog. Not met, engineering only (no approval needed): G3 native GUI end-to-end click-through of the packaged ad-hoc `.app` against real MySQL/MariaDB (Direct and SSH) — unblocked since A1 was run and A2 was closed with the ad-hoc build; `audit:release` re-run after `flate2` entered the shipped graph (2026-11-17); per-call IPC overhead in a release build. Blocked: G4/G5 budget sign-off (A3). Developer ID signing/notarization and Keychain prompts on a Developer ID build are M009 items by the A2 closure, not M002 criteria. The 2026-11-16 line "Blocked — all unblocked work done" is superseded. Earlier status text follows.

Authorized by user instruction to continue all milestones. Implemented development host; local verification in progress. Not complete. **Re-scoped for v0.1.0 Alpha (D021, Approved product decision 2026-10-26):** only macOS gates block the alpha (arm64 required, x86_64 if shipped). Open for the alpha: native macOS E2E, release-build startup/IPC profile, OS fallback-dialog interaction, macOS-applicable audit result. Windows/Linux execution and the Linux GTK advisories are Deferred (see below), not passed. D022 (Approved 2026-10-26) adds alpha IPC surface for jobs, transactions, metadata, browse/CRUD and native transfer file grants; each new command needs the same G1 ACL negatives, strict decoding and session checks before the alpha, and the shared crates stay free of macOS host APIs.

## Objective and scope
Bind the shared core through a minimal Tauri host and existing Vue UI; retain browser/mock mode. No server implementation.

## Dependencies and prerequisites
M001 versioned/generated contracts and approved D001/D003 boundaries; approved native capability and OS test matrix (for the alpha: macOS arm64, plus x86_64 if shipped). Production signing custody is a later M009 gate, not required for fixture-based development.

## Technical approach
Use reviewed IPC DTOs and host-local identity/handle authorization; no Tauri types in shared crates. Start with fixture requests and integrate M003 only when ready.

## Deliverables
Native IPC adapter, capability policy, generated-contract integration and cross-OS smoke-test evidence.

## Acceptance criteria
- G1: deny unauthorized window/command access; no arbitrary renderer shell/filesystem API or secrets in renderer storage/logs.
- G2: IPC round trips preserve tagged raw values/errors without conversion; reconnect/disconnect cannot target another workspace.
- G3: macOS arm64 (and x86_64 if shipped) native smoke and E2E pass for the alpha; Windows/Linux runners are Deferred to their releases; browser frontend remains independent of native imports; reject unsupported DTO versions.
- G4/G5: record startup/IPC overhead against the approved reference profile, bounded errors/timeouts and clean shutdown; failed IPC must not freeze the editor.
- G6: reproducible development bundle documented; production signing/install/upgrade acceptance remains M009.

## Required tests and verification
Native macOS smoke and E2E tests (three-OS smoke deferred for Windows/Linux); negative capability/handle tests; generated-contract fixtures; existing frontend automated/browser suites and build; measure IPC/startup samples.

## v0.1.0 Alpha (macOS) required evidence
*2026-11-17 note: this list predates the A1 run, the A2 closure (ad-hoc signed build) and the A4/A5 audit work; current status is in "Acceptance record — 2026-11-17". Kept as history.*

Labels: Implemented and verified / Unresolved release blocker / Proposed technical decision. Nothing here is complete.
- Implemented and verified: debug macOS ARM WebView smoke (453 ms latest), four native IPC ACL/value/protocol/close tests, browser-shim close dialogs, generated-contract and boundary checks.
- Unresolved release blocker: native GUI end-to-end on the signed release `.app` (macOS arm64) driving real MySQL/MariaDB fixtures Direct and over SSH — connect, query, browse, CRUD, export/import, cancel, disconnect, reload, quit with unsaved edits or an open transaction; current tests use MockRuntime or browser shims.
- Unresolved release blocker: OS fallback close dialog on an unresponsive renderer, exercised manually or automated on macOS.
- Unresolved release blocker: 30-launch cold-start and IPC overhead profile of the release build on arm64 (x86_64 if shipped); budgets remain Proposed technical decisions.
- Unresolved release blocker: dependency audit applicable to the macOS artifact (Linux GTK findings either shown absent from the macOS link graph or explicitly risk-accepted; no ignore added).
- Proposed technical decision: release bundling configuration (icons, minimum macOS version, Hardened Runtime) belongs to M009 but must not weaken the M002 capability/CSP boundary.

## Deferred (not blocking v0.1.0 Alpha)
- Windows and Linux native execution, WebView differences and OS fallback dialogs.
- Linux GTK advisories RUSTSEC-2024-0429 / RUSTSEC-2024-0370: remain open for Linux distribution.
- Remote execution of the prepared three-OS CI matrix (Windows/Linux jobs).

## Risks and limitations
WebView/platform differences and native runners can block a target. A development bundle does not establish a signed release. Desktop progress must not depend on Axum.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.

## Implementation evidence — 2026-10-25
- Tauri 2.12.5 / tauri-build 2.7.4; three allowlisted IPC commands, local main-window origin/session guards, strict bounded v1 contracts; no database integration.
- Four native IPC regression tests passed, including denied remote/auxiliary/shell/filesystem operations and cancelled close. Renderer dialog permission regression passed.
- Frontend/root automated tests passed; 19 browser scenarios passed (original 17 plus Light/Dark native-close shim tests), 24.5 seconds. Frontend build passed with existing bundle-size advisory.
- Latest actual macOS WebView smoke passed in 453 ms. Workspace Rust tests (16), all-target Clippy, generated-contract checks, transitive boundary checks and documentation build passed.
- `cargo audit --deny warnings` failed: RUSTSEC-2024-0429 (glib 0.18.5, Linux GTK graph) and RUSTSEC-2024-0370 (proc-macro-error 1.0.4). No ignores. Not a clean security gate.
- Windows/Linux native runtime, OS fallback dialog, repeated benchmark profile and production packaging not verified. CI preparation is not execution evidence.
- See `docs/development/desktop-host.md`. Preserve these open gates while proceeding with independent M003-L lifecycle work.

## Acceptance record — 2026-11-16 (scope: macOS, MySQL/MariaDB/PostgreSQL)
*Statuses in this table are superseded by "Acceptance record — 2026-11-17" at the end of this file; kept as history.*

| Criterion | Evidence (executed) | Status |
|---|---|---|
| G1 window/command ACL, no renderer shell/fs, no secrets in storage | `tests/ipc.rs::every_command_refuses_foreign_windows_origins_versions_and_unknown_fields` (all 21 registered commands, list parsed from `generate_handler!` so new commands cannot skip it): auxiliary window and `https://untrusted.invalid` refused. `ssh_and_secret_commands_refuse_forged_sessions` closes M005 review "missing tests" 1 for the five SSH/secret commands. Existing shell/fs/dialog plugin denials. Mutation note: removing the in-command `guard` label check is masked by the Tauri capability layer (still refused), so the test proves denial, not the second layer in isolation. | Verified |
| G2 raw values/errors over IPC, no cross-workspace targeting | Existing `actual_ipc_preserves_values_and_rejects_foreign_sessions`, renderer bootstrap rotation, handle ownership tests (desktop:test 2026-11-16: 24 passed, 3 opt-in ignored). | Verified (MockRuntime) |
| G3 macOS native smoke + E2E; reject unsupported DTO versions; browser independent | Version 2 → `UNSUPPORTED_VERSION` and unknown envelope field → `INVALID_REQUEST` for all 21 commands. Browser suite runs without Tauri (52/52 on 2026-11-16). Real-window native checks on arm64 (A1, 2026-11-16): `desktop:smoke` passed (2,783 ms debug); fallback dialog, physical reload paths and SSH key picker passed (see `.ref/planning/approvals-ledger.md` Results). Ad-hoc signed `.app` (A2 interim, 2026-11-16): signature valid with hardened runtime; launched, rendered through the native bridge, closed with exit 0. Full native E2E on a Developer ID-signed, notarized `.app` not run: no signing identity on this Mac (see `.ref/planning/approvals-ledger.md` Results). | Blocked (Developer ID certificate) |
| G4/G5 startup/IPC overhead, bounded errors, clean shutdown | `npm run desktop:profile` 2026-11-16, arm64, optimized build + non-shipped `smoke` feature: 30/30 launches; host start → renderer ready (includes the `host_info` + 2 `contract_probe` IPC round trips) min 562 / median 607 / p95 701 / max 715 ms. Per-call IPC overhead not isolated in release. Clean exit on every launch. Budgets need A3; x86_64 not measured (not shipped). | Measured; sign-off blocked (A3) |
| G6 reproducible development bundle documented | `docs/development/desktop-host.md` (`npm run desktop:build`). | Verified |
| Alpha: dependency audit applicable to the macOS artifact | cargo-audit 0.22.1 (2026-11-16, advisory DB of that date). `cargo tree -p stoneql-desktop --target {aarch64,x86_64}-apple-darwin`: glib (RUSTSEC-2024-0429), proc-macro-error (RUSTSEC-2024-0370) and rsa are **absent** from both macOS graphs (present only for x86_64-unknown-linux-gnu) → Linux-only, Deferred as recorded. **New:** RUSTSEC-2026-0495 russh 0.64.1 (agent server; StoneQL uses only `AgentClient`, `ssh.rs:256`, not reachable) and RUSTSEC-2026-0459 libsqlite3-sys 0.37.0 (bundled SQLite; sqlx 0.9.0 pins it; no upgrade path) are in the macOS graph. **A5 done 2026-11-16:** SQLite excluded from the release build — `libsqlite3-sys`/`sqlx-sqlite` absent from both macOS release graphs, 0 `sqlite3_` symbols in the release binary; `cargo audit` still lists it from the dev/test lockfile. **A4 done 2026-11-16:** russh 0.65.1; `npm run audit:release` (release graph only, both macOS targets) → 0 vulnerabilities, 0 warnings; positive control with `sqlite` enabled reports RUSTSEC-2026-0459. | Verified (release graph) |
| Alpha: OS fallback close dialog on unresponsive renderer | `journal_window fallback` 2026-11-16 (arm64, debug): frozen renderer → native alert; Cancel keeps the app running; Close StoneQL exits 0 (see `.ref/planning/approvals-ledger.md` Results). | Verified (macOS arm64) |

## Acceptance record — 2026-11-17 (supersedes the 2026-11-16 statuses above; macOS arm64)
Evidence base: working tree on `b0bfea3` plus uncommitted JSON/GZIP export changes. Suite counts as reported by the orchestrator's 2026-11-17 run (not in a tracked file): `desktop:test` 60 passed, `rust:test` 171, `npm test` 229 + 22, `test:browser` 66 (both themes), fmt/lint/boundaries/vendor/`contracts:check` pass. Real-server reports are under `artifacts/` (untracked).

| Criterion | Status | Evidence or gap |
|---|---|---|
| G1 window/command ACL, no renderer shell/fs, no secrets | Met | `tests/ipc.rs` sweep parses `generate_handler!` (31 registered commands on 2026-11-17); `desktop:test` 60 passed. The read-write gate is now on (A15, `lib.rs:50`); refusal with the gate off is covered by the injected-services `ipc.rs` gate test. |
| G2 raw values/errors over IPC, no cross-workspace targeting | Met | As 2026-11-16, re-run in `desktop:test` 2026-11-17; real-server host tests `test:native-database` (`artifacts/m003/native-{mysql,mariadb}.json`, 2026-11-17 12:38) and `test:native-postgres` (`artifacts/m004/native-postgresql.json`, 12:39). |
| G3 macOS native smoke + E2E; unsupported versions rejected; browser independent | Not met (engineering) | Met parts: version 2 / unknown fields refused for every command; browser suite 66 without Tauri; real-window smoke, fallback dialog, reload and key picker (A1, 2026-11-16); ad-hoc `StoneQL.app` launch/render/exit (A2/A14). Missing: graphical click-through of the packaged `.app` against real MySQL/MariaDB Direct and SSH (connect, query, browse, CRUD, export/import, cancel, disconnect, reload, quit with unsaved edits / open transaction). Browser specs use a stubbed host; host IPC tests use Tauri MockRuntime. No approval blocks this run. |
| G4/G5 startup/IPC overhead, bounded errors, clean shutdown | Blocked on A3 (evidence executed) | 30-launch profile 2026-11-16 (median 607 / p95 701 ms, clean exits). Per-call IPC overhead in a release build not isolated (engineering). x86_64 not shipped. |
| G6 reproducible development bundle documented | Met | `docs/development/desktop-host.md`; `npm run desktop:bundle:adhoc` produced the RC `StoneQL.app` for `b0bfea3`. |
| Alpha: dependency audit of the macOS artifact | Not met (engineering) | Met on 2026-11-16 (`npm run audit:release`, 0 vulnerabilities). On 2026-11-17 `flate2` 1.1.10 (`rust_backend`) became a direct shipped dependency for GZIP export (uncommitted); no `audit:release` re-run is recorded since. |
| Alpha: OS fallback close dialog | Met | `journal_window fallback`, 2026-11-16 (ledger A1 Results). |
| Windows/Linux, Linux GTK advisories | Deferred (D021) | Not run. |
