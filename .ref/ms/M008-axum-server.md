# M008 — Authenticated Axum server

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Future standalone Axum host and authenticated HTTP binding of the same core. It is not a desktop release dependency.

## Dependencies and prerequisites
M001/M006 contracts; D007 identity/ownership/target/limit decisions and separate server D005 vault policy. Existing M005 transport implementation is reused where selected, but desktop release does not wait for these server decisions.

## Technical approach
Use generated/versioned DTO semantics, authenticate at HTTP boundary and enforce core resource ownership. OIDC sessions/CSRF remain recommended pending approval. Bound per-principal pools, jobs/uploads/temp storage and trusted proxy/network/file targets.

## Deliverables
Server host and shared frontend HTTP adapter with negative security, contract and load evidence. Distribution/systemd/Docker belongs to M011.

## Acceptance criteria
- G1: negative IDOR/CSRF/SSRF, session expiry/revocation, credential isolation and target/path tests pass; deny by default; secure cookies/TLS/proxy trust and redaction verified.
- G2: core lossless/concurrency/transaction fixtures pass unchanged over HTTP; interrupted requests never imply rollback/commit; no cross-principal caches or mutable state.
- G3: supported client/protocol and Linux/engine matrix documented; generated contracts reject unsupported versions; desktop IPC/browser-mock regressions remain passing.
- G4/G5: meet approved server reference workload with enforced per-principal/aggregate quotas, bounded disconnect/shutdown cleanup and no automatic SQL retries.
- G6: development server binary is reproducible; production install/upgrade/restore and deployment sign-off are M011, not M009.

## Required tests and verification
HTTP/IPC fixture parity; ownership/session/target/security negatives, proxy/TLS tests, client disconnect/cursor expiry, slow consumers and multi-principal workload; core/desktop regression suites.

## Risks and limitations
Identity provider, vault, actual traffic/SLO and network policy unresolved. No production server is implemented merely to prepare contracts or ship desktop.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
