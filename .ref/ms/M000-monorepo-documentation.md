# M000 — Monorepo and documentation

## Status
Implementation finished; automated/build checks passed; all 17 browser scenarios passed with approved elevation on 2026-10-25. Live documentation/Mermaid verification remains outstanding. No backend implementation authorized by this document.

## Objective and scope
Monorepo and documentation. Preserve existing Vue behavior and use existing abstractions; no unrelated redesign.

## Dependencies and prerequisites
Current task; preserve prototype.

## Technical approach
Move existing frontend, retain root commands, configure VitePress root-docs source, write audit and proposed architecture.

## Deliverables
Workspace, website, agent guide, planning register and verification report.

## Acceptance criteria
Original source hashes match; unit/build/docs checks pass; all 17 browser scenarios execute on a supported host or remain explicitly blocked.

## Required tests and verification
npm test; npm run build; npm run docs:build; npm run test:browser; source hash and Git ignore checks.

## Risks and limitations
Browser listeners/native launch restricted; CDN diagrams require browser/network verification.

Public context: `docs/architecture/roadmap.md`. Approval records: `.ref/planning/architecture-decisions.md`. Mark complete only after executing and recording all acceptance checks.
