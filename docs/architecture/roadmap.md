# Roadmap

**Status: Proposed / awaiting human approval.** Backend implementation has not started. Current work reorganizes the completed mock UI and documents boundaries.

| Milestone | Scope | Prerequisites / gate |
| --- | --- | --- |
| M000 | Monorepo, website, audit and architecture proposals | Preserve source and execute available verification; browser blockers remain explicit |
| M001 | Rust contracts/core foundation and driver spikes | Human approval of core/DTO decisions |
| M002 | Secure Tauri host integration | M001; native capability and packaging review |
| M003 | MySQL/MariaDB direct connectivity | M001/M002; driver/version/TLS decisions |
| M004 | PostgreSQL and SQLite adapters | M003 patterns; engine-specific isolation/file policy |
| M005 | SSH and credential integration | M003/M004; approved vault/host-key policy |
| M006 | Schema objects, SQL jobs, transactions and metadata cache | M003/M004; raw-value/concurrency contract |
| M007 | Streaming SQL/CSV/JSON/XLSX and ZIP/GZIP | M005/M006; library/resource/snapshot decisions |
| M008 | Axum server and authenticated HTTP adapter | M001/M006; OIDC/authorization/limits approval |
| M009 | Native releases, Linux systemd and optional Docker | M002/M005/M007/M008; operational ownership |
| M010 | Hardening, load, restore and compatibility gates | Prior implementation milestones; agreed measurable targets |

The sequence prioritizes a desktop vertical slice, then adapter parity, safe transfers and server hosting. Internal milestone specifications contain deliverables, executable acceptance criteria and risks; they are not public implementation promises. Teleport and AWS SSM remain later investigations, not part of the initial SSH scope.

## Decisions requiring approval

1. Shared-core boundaries and contract generation/versioning approach.
2. Driver selection, supported database/OS versions and dynamic-type/cancellation behavior.
3. Desktop/server credential providers and SSH trust/renewal policy.
4. HTTP/IPC streaming framing, transaction leases and uncertain-outcome UX.
5. Server authentication, ownership authorization and network/file target restrictions.
6. Transfer libraries, native-dump versus logical export, limits and snapshot guarantees.
7. Deployment/signing/update ownership, retention and measurable resource/SLO targets.

Until these gates are approved, documentation recommendations remain proposals. A passing mock test or rendered form does not complete a production milestone.
