# M002 — Tauri host integration

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Tauri host integration. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M001; approved native permissions/signing scope.

## Technical approach
Bind shared DTOs through restricted IPC; keep Vue browser mode independent; initially use reviewed fixtures.

## Deliverables
Native host adapter and OS-specific development verification.

## Acceptance criteria
Runs on macOS/Windows/Linux; minimum capabilities; no arbitrary shell/filesystem IPC; existing browser UI still passes.

## Required tests and verification
Native smoke tests per OS; unauthorized IPC tests; frontend tests/build/browser suite.

## Risks and limitations
WebView differences, signing and native runner availability.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
