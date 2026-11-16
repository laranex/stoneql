# M000 — Monorepo and documentation

## Status
**Complete (closed 2026-11-16).** Implementation finished; automated/build checks passed; all 17 browser scenarios passed with approved elevation on 2026-10-25. The last open item, live documentation/Mermaid verification, was executed on 2026-11-16 (below). No backend implementation authorized by this document.

## Live documentation verification — 2026-11-16
- Published site https://laranex.github.io/stoneql/ from commit `e51a81a` (docs.yml run 43799234817, build + deploy succeeded, user-approved push and deploy).
- Diagrams: headless Chromium, light and dark color schemes, `.ref/planning/m000-live-docs/diagram-check.mjs`: all 5 diagrams on the 4 diagram pages (architecture/overview, connection-management, roadmap ×2, import-export) rendered as SVG from the pinned Mermaid 11.12.0 loader; no offline fallback shown, no page errors. `development/documentation` mentions Mermaid only in prose (no diagram).
- External links: all 22 distinct external links in `.website/dist` resolve (16 HTTP 200 via curl; the 6 dev.mysql.com pages return 403 to curl bot protection and 200 with their expected titles in headless Chromium).
- Residual, accepted by design: diagrams need network access to the jsDelivr CDN; offline readers see the source fallback.

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
