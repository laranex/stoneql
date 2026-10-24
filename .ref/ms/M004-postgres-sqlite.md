# M004 — PostgreSQL and SQLite adapters

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Extend the verified lifecycle/basic read-only slice to PostgreSQL and SQLite. Preserve engine-specific capabilities and typed values; advanced schema/object workflows remain M006.

## Dependencies and prerequisites
M003 connection/read-only pattern reviewed and M001 contracts; approved PostgreSQL/SQLite version, schema and file-access policy. No SSH or Axum prerequisite; may run alongside M005.

## Technical approach
Implement PostgreSQL native type/schema identity mapping and SQLite bounded workers/authorized files. Probe missing SQLx capabilities before adopting them for each engine. Read-only opening/enforcement and explicit unsupported behavior reuse core rules.

## Deliverables
Adapters, lossless type fixtures, lifecycle/read-only evidence and capability-gap report; no duplicate host logic.

## Acceptance criteria
- G1: verify PostgreSQL TLS/privilege boundaries and SQLite canonical path/read-only file restrictions; reject unauthorized files and mutation paths.
- G2: preserve PostgreSQL ENUM/unknown types, temporal text and SQLite dynamic/strict values; NULL/default meanings survive round trips; locking failures never imply successful writes.
- G3: approved engine/version/OS matrix passes; SQLite routines/events and other unsupported capabilities are rejected, not presented as functional.
- G4/G5: repeat approved lifecycle/basic-query budgets; SQLite work does not block async execution, busy/timeout behavior is bounded and failed connections/workers release resources.
- Generated v1 contracts and MySQL/MariaDB regression fixtures still pass. Native object DDL/materialized-view behavior is gated in M006.

## Required tests and verification
Disposable PostgreSQL and temporary authorized SQLite files; strict/affinity/ENUM/quoted identifiers/raw precision; TLS, traversal, read-only and busy-lock negatives; lifecycle/query measurement profiles and adapter parity.

## Risks and limitations
Unknown PostgreSQL extensions and SQLite filesystem semantics need explicit support limits. No claim of identical transaction or cancellation behavior across engines.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
