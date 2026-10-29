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
Track-specific full unit/integration/browser/native or HTTP suites (desktop: macOS for the alpha), adversarial inputs/ownership, load/RSS/cancel/fault injection, packaging/update and backup/restore drills; preserve reproducible commands and results.

## Risks and limitations
Budgets and compatibility matrix still need approval. A green prototype suite does not establish production capabilities. Release candidates can be built during M009/M011 preparation without those releases being complete.

## M010-D for v0.1.0 Alpha (macOS)
Nothing below is accepted. Labels: Implemented and verified (fixture/local evidence that can feed M010-D) / Unresolved release blocker.
- Implemented and verified (local macOS ARM, debug, fixtures): M003 lifecycle and resource profiles, M005 SSH 100-cycle matrix and Keychain command-handler tests, M004 PostgreSQL/SQLite fixtures. These are inputs, not release evidence: none ran against a signed release artifact.
- Unresolved release blocker: evidence bound to the signed/notarized artifact checksum — native E2E (real MySQL/MariaDB, Direct and SSH), Keychain prompts, install/upgrade/uninstall/credential-cleanup drills, 30-launch startup and whole-app RSS profile on arm64 (x86_64 if shipped).
- Unresolved release blocker: G1 residual-risk record — macOS-applicable dependency audit; explicit owner risk acceptance or exclusion evidence for the Linux GTK advisories; RSA unsupported, memory erasure not guaranteed, documented as limitations.
- Unresolved release blocker: approved numerical budgets for the alpha (currently Proposed in the roadmap).
- Unresolved release blocker: support matrix and omissions for the approved alpha feature scope (**D022**, 2026-10-26: MySQL/MariaDB live browsing, CRUD, safe SQL execution, SSH, SQL/CSV import/export), with unsupported capabilities rejected. PostgreSQL appears only if admitted; SQLite, advanced objects, XLSX import, Windows and Linux are listed as Deferred omissions.
- Unresolved release blocker: D022 safety evidence — M005 F-01–F-04 and secret-deletion fixes; M006 BUSY/no-queue, cancel-to-terminal samples, no automatic retry, transaction expiry/cleanup and 100 fault cycles with zero dirty connections; MySQL write cancellation and SSH cancellation-slot exhaustion evidence before unrestricted read-write SQL; CRUD conflict/constraint fixtures; transfer round trips with read-back and quota/cancel/disk-full cleanup.
- Proposed: evidence workstream and budgets in `.ref/planning/v0.1.0-alpha-plan.md` (g) and §Unresolved numerical limits; metadata-limit vendor decision (e) recorded in the residual-risk record either way.
- Implemented and verified inputs from M004 PostgreSQL/SQLite fixtures count toward M010-D only for engines admitted to the alpha.

## Deferred (not blocking v0.1.0 Alpha)
Windows/Linux M010-D cells (native runs, credential stores, agents, packaging, resource profiles) and executed cross-OS CI. They stay open and block the desktop release that ships those platforms.

## Track status
- M010-D: Proposed / no production evidence executed; gates M009 only. For v0.1.0 Alpha (D021) the M010-D evidence set is macOS-only: arm64 required, x86_64 only if shipped.
- M010-S: Proposed / no production evidence executed; gates M011 only.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
