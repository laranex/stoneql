# M011 — Server Release

## Status
Proposed / awaiting approval. No backend implementation authorized by this document.

## Objective and scope
Separate future Linux server distribution: standalone binary/systemd first, optional restricted Docker. No dependency on completing desktop distribution.

## Dependencies and prerequisites
M008 and applicable accepted M005–M007 server scope; **M010-S** evidence; D009-S deployment/recovery ownership and server compatibility, availability/SLO, RPO/RTO and resource targets explicitly approved. M009/M010-D are not prerequisites.

## Technical approach
Package the same Axum/core binary with a dedicated identity, scoped state/runtime/credential access and reviewed systemd hardening. Optional non-root/read-only container reuses the binary; no Docker socket or broad host mounts.

## Deliverables
Versioned Linux packages, reviewed service/container configuration, SBOM/checksums, configuration/migration/backup/restore runbooks and separate server release decision.

## Acceptance criteria
- G1: TLS/proxy/session/vault/authorization and network/file target policy validated in packaged deployment; no secrets in image/unit/artifact/logs; container/service least privilege verified.
- G2: upgrade/rollback/restore preserve profiles and vault references; separate key recovery policy tested; clients receive accurate failed/partial/unknown job outcomes across restart.
- G3: approved Linux architecture, engine, proxy/identity-provider and protocol matrix passes; optional Docker support is advertised only after its own tests.
- G4/G5: meet approved server workload/quotas, restart/shutdown and recovery objectives; no cross-principal resource leakage; measured restore/rollback meets agreed RPO/RTO.
- G6: actual binary/systemd install/upgrade/rollback/uninstall verified; optional container image pinned/checksummed and restricted; M010-S approved without waiting for desktop packaging.

## Required tests and verification
Systemd and optional container integration/least-privilege tests; multi-principal/credential-isolation security suite; load/backpressure/restart/disk-full tests; state/key recovery and upgrade/rollback drills against release artifacts.

## Risks and limitations
Server operating owner, SLO/RPO/RTO, identity provider and vault choice are unresolved. No production topology, Docker requirement, deployment or public service is approved by this document.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
