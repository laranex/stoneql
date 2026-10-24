# M003 — MySQL and MariaDB direct connectivity

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
MySQL and MariaDB direct connectivity. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M001/M002; D002/D004 driver, version and TLS approval.

## Technical approach
Implement direct adapter vertical slice, introspection, lossless queries, bounded pools and cancellation handling.

## Deliverables
Real MySQL/MariaDB connection lifecycle and integration fixtures.

## Acceptance criteria
Separate engine/version capabilities; verified TLS; exact DECIMAL/BIGINT; no unsafe automatic SQL retry.

## Required tests and verification
Disposable MySQL/MariaDB matrix; pool isolation, timeout, disconnect and uncertain commit tests.

## Risks and limitations
MariaDB divergence, protocol cancellation and session reset.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
