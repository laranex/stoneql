# M002 — Tauri host integration

## Status
Authorized by user instruction to continue all milestones. Implemented development host; local verification in progress. Not complete: cross-OS execution, performance profile and transitive Linux security findings remain open.

## Objective and scope
Bind the shared core through a minimal Tauri host and existing Vue UI; retain browser/mock mode. No server implementation.

## Dependencies and prerequisites
M001 versioned/generated contracts and approved D001/D003 boundaries; approved native capability and OS test matrix. Production signing custody is a later M009 gate, not required for fixture-based development.

## Technical approach
Use reviewed IPC DTOs and host-local identity/handle authorization; no Tauri types in shared crates. Start with fixture requests and integrate M003 only when ready.

## Deliverables
Native IPC adapter, capability policy, generated-contract integration and cross-OS smoke-test evidence.

## Acceptance criteria
- G1: deny unauthorized window/command access; no arbitrary renderer shell/filesystem API or secrets in renderer storage/logs.
- G2: IPC round trips preserve tagged raw values/errors without conversion; reconnect/disconnect cannot target another workspace.
- G3: approved macOS/Windows/Linux development runners pass native smoke tests; browser frontend remains independent of native imports; reject unsupported DTO versions.
- G4/G5: record startup/IPC overhead against the approved reference profile, bounded errors/timeouts and clean shutdown; failed IPC must not freeze the editor.
- G6: reproducible development bundle documented; production signing/install/upgrade acceptance remains M009.

## Required tests and verification
Native three-OS smoke tests; negative capability/handle tests; generated-contract fixtures; existing frontend automated/browser suites and build; measure IPC/startup samples.

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
