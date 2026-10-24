# M006 — Schema, SQL jobs and integrity

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Schema, SQL jobs and integrity. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M003/M004; D003/D004; D005 if secrets required.

## Technical approach
Implement capability-driven objects, bounded results, transactions, optimistic CRUD and schema generation cache; wire real metadata completion boundary.

## Deliverables
Native object workflows, SQL job lifecycle and authoritative constraint errors.

## Acceptance criteria
Applicable object CRUD verified by engine; transaction leases isolated; stale writes rejected; cancellation/outcome accurate.

## Required tests and verification
Engine object matrix; composite indexes; FK/unique conflicts; stale cache/switching; large result backpressure.

## Risks and limitations
DDL implicit commits, unknown result types, routines with side effects.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
