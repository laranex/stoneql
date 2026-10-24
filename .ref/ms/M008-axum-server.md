# M008 — Authenticated Axum server

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Authenticated Axum server. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M001/M006; D007 auth/ownership/network/file approval.

## Technical approach
Bind same core through versioned HTTP contract; OIDC sessions, CSRF, per-resource authorization and quotas.

## Deliverables
Standalone server and authenticated shared frontend host adapter.

## Acceptance criteria
Every handle/job/upload ownership checked; credential isolation; limits and cancellation enforced; no arbitrary filesystem access.

## Required tests and verification
HTTP contract tests; IDOR/CSRF/SSRF negative tests; sessions/revocation; quotas; adapter parity.

## Risks and limitations
Identity provider configuration, multi-user credential threat model and proxy trust.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
