# Architecture decision register

All entries are **Proposed / awaiting human approval**. No implementation authority is inferred from this plan.

| ID | Decision / recommendation | Alternatives / evidence gate |
| --- | --- | --- |
| D001 | Framework-independent Rust core; separate IPC/HTTP host adapters | Avoid host-specific services; verify dependency graph in M001 |
| D002 | Evaluate SQLx versus native drivers | Dynamic types, multiple results, cancellation, COPY, precision and supported versions must pass spikes |
| D003 | Tagged lossless DTOs and generated/validated TS bindings | JSON numbers are unsafe for large exact values; finalize framing/version policy |
| D004 | Principal-owned pools, leases and schema generations | Shared global pools risk credential/session leakage; test session reset |
| D005 | OS secret references on desktop; approved server vault | Keyring/vault provider and threat model need human review |
| D006 | Direct/SSH first; verified TLS/host keys | Teleport/SSM deferred; never accept insecure fallback |
| D007 | Single Axum process, OIDC sessions and quotas | Token-only auth/distributed services deferred pending requirements |
| D008 | Bounded transfer jobs; explicit partial outcomes | Choose native dump/logical exporter and XLSX/archive libraries after spikes |
| D009 | Desktop signing, native systemd, optional restricted Docker | Operational ownership, update channels and resource/SLO targets unresolved |

Public technical detail lives in `docs/architecture/`; do not duplicate those specifications here. Approvals must name decision ID, reviewer and scope. No approvals recorded.

## Migration decisions implemented

- npm workspaces with frontend under apps/frontend; original source/config/test bytes preserved.
- Root docs are VitePress srcDir; no Markdown mirror.
- VitePress 1.6.4 pinned to a locally available genuine package/lock; frontend dependency versions retained.
- Mermaid uses pinned browser CDN loading and explicit source fallback; graphical/offline verification remains limited by environment.
- No Rust directories/app scaffolds, API implementation or infrastructure deployment.
