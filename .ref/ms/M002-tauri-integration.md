# M002 — Tauri host integration

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

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
