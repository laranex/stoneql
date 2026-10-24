# M009 — Production packaging and deployment

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Production packaging and deployment. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
M002/M005/M007/M008; D009 operational approval.

## Technical approach
Signed desktop artifacts; one Linux binary with reviewed systemd hardening; optional non-root restricted container.

## Deliverables
Release packaging, upgrade/rollback and operating runbooks.

## Acceptance criteria
Native three-OS smoke tests; systemd and optional Docker operate without privileged host access; secrets outside artifacts.

## Required tests and verification
Signing/update verification; restart/shutdown; state migration/rollback; Linux file/network limits.

## Risks and limitations
Signing credentials, platform distribution, backup ownership and incident response.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
