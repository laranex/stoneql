# M007 — Streaming transfers and compression

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Streaming transfers and compression. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M005/M006; D008 format/library/snapshot approval.

## Technical approach
Bounded SQL/CSV/JSON/XLSX import/export; ZIP/GZIP; authorized files, quotas, progress and cleanup.

## Deliverables
Streaming services and native UI transfer adapter.

## Acceptance criteria
Measured memory limits; exact values; cancellation and partial commits explicit; no unsafe archive expansion.

## Required tests and verification
Round trips per engine; malformed inputs; disk full; slow consumer; archive bombs; restart cleanup.

## Risks and limitations
Native dump availability, workbook memory and nontransactional DDL.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
