# M006 — Advanced SQL, schema metadata and integrity

## Status
Planned / not started. The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## Objective and scope
Own advanced query jobs, streaming/batches, multiple result sets, transaction leases, cancellation, object/schema management and introspection/cache lifecycle. Extend M003 read-only results rather than replacing its working path.

## Dependencies and prerequisites
M003/M004 stable adapters; approved D003/D004 and per-engine SQLx capability spikes. M005 is required only for SSH/secret-backed integration coverage, not Direct core implementation. Approve job/resource/transaction/cancellation limits before implementation.

## Technical approach
Use framework-independent bounded job services, physical-connection transaction affinity, typed errors/outcomes and centralized schema/mutation rules. Add metadata revisions/refresh/invalidation and autocomplete discovery through generated DTOs. Keep IPC/HTTP framing at host boundaries.

## Deliverables
Native object/CRUD workflows, advanced execution lifecycle, metadata cache and authoritative constraint/error integration, with engine-specific capability report.

## Acceptance criteria
- G1: every handle/job/transaction/cache entry is owner-scoped; unauthorized actions and capabilities fail; diagnostics exclude SQL/row secrets by default. Destructive UI safeguards do not substitute for authorization.
- G2: FK/unique/composite constraints and optimistic conflicts are authoritative; batches have documented atomicity; raw values/defaults unchanged; cancellation, DDL implicit commits and lost commits report partial/unknown outcomes accurately.
- G3: applicable object/routine/index/materialized-view workflows verified on approved engine versions; multiple results and unknown types preserved; contract generation and unsupported-version tests pass.
- G4/G5: meet approved bounded-result/cancel profile; no dirty connection returned to pools; best-effort server cancellation is distinguished from confirmed termination; stale metadata and workspace replies cannot leak/cross generations.
- Basic M003 read-only queries still work through the same core contracts. No Tauri/Axum dependency enters shared services.

## Required tests and verification
Engine object matrix, FK/unique/conflict/NULL fixtures, multiple result sets and stored procedures, transaction/cancel/connection loss, stale cache/schema switching, bounded backpressure and owner isolation; repeat M003/M004 regressions.

## Risks and limitations
Raw SQL can mutate session state; pool reset/discard must be proved. Engine cancellation/DDL semantics differ. An M001 spike is capability evidence, not production feature completion.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
