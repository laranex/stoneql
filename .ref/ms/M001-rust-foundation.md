# M001 — Rust core foundation

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Rust core foundation. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
Human approval D001–D004; M000 verification reviewed.

## Technical approach
Spike SQLx/native drivers; define framework-independent value, error, capability and adapter contracts; generate/validate TS DTO fixtures.

## Deliverables
Minimal approved Rust workspace, contract fixtures and dependency boundaries.

## Acceptance criteria
No Tauri/Axum core imports; lossless values and structured outcomes round-trip; driver spike records supported versions and gaps.

## Required tests and verification
cargo fmt/clippy/test; DTO round trips; cancellation/dynamic-type driver spikes on disposable databases.

## Risks and limitations
Driver coverage, native dependencies and version policy unresolved.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
