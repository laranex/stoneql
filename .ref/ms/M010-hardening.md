# M010 — Continuous hardening and release evidence

## Status
Planned / not started. The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## Objective and scope
Add adversarial, performance, recovery and compatibility evidence continuously. Do not defer security/integrity tests or couple desktop and server release completion.

## Dependencies and prerequisites
Begin with M001 contracts/fixtures. **M010-D** uses accepted M002–M007 desktop-scope candidates; **M010-S** uses M008 and applicable M005–M007 server candidates. Neither requires a completed M009/M011 release; each sign-off is independent.

## Technical approach
Use G1–G6 in each owning milestone, then aggregate raw samples, negative tests, restore/upgrade drills, support matrix and residual risk per track. Review missing evidence before release, not after packaging is declared complete.

## Deliverables
Separate M010-D desktop and M010-S server evidence reports, with versioned artifact/commit IDs, fixture/hardware/toolchain details, numerical budgets, raw results, owner and go/no-go decision.

## Acceptance criteria
- G1/G2: each track has no unresolved critical/high security finding without explicit owner acceptance, no silent corruption, passing negative authorization and applicable transactional/restore tests; earlier milestones already enforce these rules.
- G3: all supported matrix cells and generated DTO drift/version checks pass; gaps are unsupported or block release, never silently counted as passed.
- G4/G5: approved reference budgets, concurrency/quota/cleanup limits and fault drills pass; actual samples retained; missed or unset targets block the affected sign-off.
- G6: test the actual candidate artifacts and migration/rollback/restore procedure; evidence binds to their checksums/versions.
- M010-D can be accepted while M008/M010-S/M011 are unstarted. Overall M010 remains open for the later track; that does not block M009 after desktop evidence is accepted.

## Required tests and verification
Track-specific full unit/integration/browser/native or HTTP suites, adversarial inputs/ownership, load/RSS/cancel/fault injection, packaging/update and backup/restore drills; preserve reproducible commands and results.

## Risks and limitations
Budgets and compatibility matrix still need approval. A green prototype suite does not establish production capabilities. Release candidates can be built during M009/M011 preparation without those releases being complete.

## Track status
- M010-D: Proposed / no production evidence executed; gates M009 only.
- M010-S: Proposed / no production evidence executed; gates M011 only.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
