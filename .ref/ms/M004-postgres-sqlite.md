# M004 — PostgreSQL and SQLite adapters

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
PostgreSQL and SQLite adapters. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M003 patterns reviewed; approved schema/file policy.

## Technical approach
Add PostgreSQL schema/native type mapping and SQLite bounded worker/file access with capability parity.

## Deliverables
Adapters, type fixtures and unsupported-operation reporting.

## Acceptance criteria
Raw temporal/ENUM types preserved; SQLite does not block executor; unsupported routines/events rejected.

## Required tests and verification
Disposable PostgreSQL; temporary SQLite files; locking, strict/affinity, quoted identifiers and materialized-view tests.

## Risks and limitations
Extension types, busy locks, native versions and platform paths.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
