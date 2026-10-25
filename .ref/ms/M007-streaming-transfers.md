# M007 — Streaming transfers and compression

## Status
Planned / not started. The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## Objective and scope
Stream SQL/CSV/JSON/XLSX and ZIP/GZIP with accurate progress, file authorization and failure outcomes. Reuse format configuration and mock UX.

## Dependencies and prerequisites
M006 execution/schema contracts; approved D008 library, format, snapshot and quota decisions. M005 only for credential-backed/SSH integration; Direct transfer core need not wait for SSH. Host-specific file grants approved before host integration.

## Technical approach
Bound parsers, queues, compressors and sinks; centralize schema validation and exact-value encoding. Desktop file grants and later server upload tokens remain separate host adapters. Choose dump tools versus logical export from recorded evidence.

## Deliverables
Transfer services, native file binding and independently verified engine/format round trips with quota/cleanup reports.

## Acceptance criteria
- G1: reject unauthorized paths/symlinks, malformed archives, expansion bombs and executable spreadsheet formulas; libraries/licenses reviewed; no secrets in summaries/temp names/logs.
- G2: exact values, NULL/empty/default semantics and ordering preserved; committed/rejected/partial/unknown counts truthful; no whole-script rollback promise for nontransactional DDL.
- G3: approved engine/format/compression/OS matrix passes independent round trips; unsupported structure/data combinations rejected; generated DTO compatibility retained.
- G4/G5: meet approved million-row/slow-consumer memory and cleanup profile, enforce queue/disk/byte quotas, and survive disk-full/cancel/restart without orphan resources or hidden partial writes.
- G6: restore/read-back evidence exists for every format advertised as a backup; simulated successes are not accepted as proof.

## Required tests and verification
Engine round trips and independent restore/read-back; precision/encoding fixtures, malformed inputs, quota/archive adversarial cases, slow consumers, cancellation, disk full and restart cleanup; measured RSS/queue/temp usage.

## Risks and limitations
XLSX and ZIP may require bounded temporary storage; SQL export snapshot consistency varies. A library or native dump tool is not approved by this plan.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
