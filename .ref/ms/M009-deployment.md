# M009 — Desktop Release

## Status
Planned / not started. The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## Objective and scope
Signed desktop distribution for macOS, Windows and Linux. This retains the M009 specification path while separating all server packaging into M011.

## Dependencies and prerequisites
M002–M007 acceptance for the approved desktop engine/feature scope and **M010-D desktop evidence only**; D009-D signing/update/artifact ownership approved. **No M008, M011, server SLO/vault approval or M010-S completion dependency.**

## Technical approach
Package the shared frontend and Tauri adapter on supported native runners; verify signatures/notarization and restricted updater trust. Keep SQLite/credential/file migrations reversible and browser/mock mode intact.

## Deliverables
Versioned platform artifacts, checksums/SBOM, installation/update/rollback instructions and independent desktop go/no-go record.

## Acceptance criteria
- G1: no bundled secrets; signed/notarized artifacts verified where applicable; unauthorized IPC, tampered/unsigned update and dependency findings handled under approved risk policy.
- G2: upgrade/rollback and uninstall preserve user databases/non-secret profiles/secret references according to explicit policy; no migration data loss or precision change.
- G3: every declared OS/architecture/engine combination passes native packaging and existing UI regression tests; release notes identify unsupported capabilities and DTO compatibility.
- G4/G5: approved cold-start/lifecycle/performance budgets and failure/restore drills pass using actual release artifacts; M010-D accepted independently of server readiness.
- G6: install/launch/upgrade/rollback/uninstall, signing, checksums and offline required workflows verified for every distributed package; artifact-size/retention policy recorded before sign-off.

## Required tests and verification
Native platform signature/install/update-tamper tests; migration/restore/rollback and offline smoke tests; full supported UI/core/engine suites on release candidates; desktop measurement profile.

## Risks and limitations
Signing custody, artifact formats, OS minima and updater operations need approval. Missing server work cannot block desktop sign-off. Graphical docs CDN is not a desktop runtime dependency.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
