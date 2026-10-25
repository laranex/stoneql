# Architecture decision register

## Status and authority

Revision 2026-10-25. The user approved the revised roadmap/M001 and subsequently instructed “commit and continue all ms,” authorizing local milestone implementation and commits. M001 is committed; M002's macOS development host is verified with Linux security/cross-OS gates open; M003 runtime evaluation is in progress. Continuation does not waive acceptance criteria, approve unverified driver capabilities, provide signing/server identity configuration, or authorize push/publication/deployment. Later technical mechanisms and release budgets remain review gates. Historical M001 evidence below describes its original dependency graph, not the subsequent Tauri dependencies.

| ID | Decision / recommendation | Status and evidence gate |
| --- | --- | --- |
| D001 | Framework-independent core/contracts; separate IPC/HTTP adapters and engine/transport boundaries | Approved M001 boundary; core/contracts implemented and transitive host/driver/runtime checks passed; no Tauri/Axum types in shared crates |
| D002 | **SQLx remains proposed**; native drivers are exceptions, not equal default choices | Evaluation authorized only: SQLx 0.9.0 has 28 passing live assertions and explicit limits. Production adoption remains unapproved; trusted TLS, exhaustive raw types, pool/transaction/permission semantics need later evidence |
| D003 | Rust-owned v1 wire DTOs generate TypeScript; deterministic regeneration and runtime validation at boundaries | Approved direction implemented with ts-rs 12.0.1 and strict v1 serde/golden/TS fixtures. Foundation tool choice verified locally; evolving compatibility/host validation remains reviewable |
| D004 | Principal-owned pools, leases and schema generations | Recommendation; M003-L lifecycle proof before M005, M006 advanced jobs/transactions/cache; no cross-principal/session leakage |
| D005 | OS secret references on desktop; separately approved server vault | Recommendation; desktop provider gates M005, server vault gates M008/M011 and never desktop progress |
| D006 | Direct/SSH first with verified TLS/host keys; Teleport/SSM preserved as future extension points | Required scope ordering; host-key enrollment/agent/renewal policy unresolved; M005 may start after M003-L without M004 |
| D007 | Future single Axum process, OIDC sessions and quotas | Recommendation pending server threat model; no server app required for desktop release |
| D008 | Bounded transfers, precise/partial outcomes and capability-driven formats | Recommendation; library/native-dump/snapshot/resource decisions gate M007 |
| D009 | Independent Desktop and Server Release approval | User-required planning constraint; former combined M009 scope separated, retaining IDs/paths below |
| D009-D | M009 signed native desktop distribution | Recommendation pending signing/artifact/updater ownership, OS/engine matrix and M010-D evidence; no server prerequisite |
| D009-S | M011 Linux/systemd, optional restricted Docker | Recommendation pending server operating owner/SLO/RPO/RTO, vault, compatibility and M010-S evidence |
| D010 | G1–G6 incremental evidence and proposed measurable profiles | Gate structure required by user; numerical budgets/reference runners need approval before affected implementation/sign-off, not blanket approval of future SLOs |

Public technical detail lives in `docs/architecture/`; the roadmap is the canonical dependency and metric table. Preserve this register's IDs in approval records; name reviewer, date, scope/capability, evidence and any risk acceptance. The approval record below authorizes M001 startup only. Passing prototype tests or committing documentation does not approve production decisions.

## Approval and implementation evidence — 2026-10-25

- Approver: user in this conversation. Instruction: “Approved: revised dbAdmin roadmap and M001 startup.” Scope explicitly includes core/workspace, generated versioned contracts, errors/capability interfaces, pinned Rust/fixtures, SQLx evaluation, formatting/lint/tests/CI and benchmark documentation.
- Selected evaluation tooling: Rust 1.99.1; SQLx 0.9.0; ts-rs 12.0.1; Tokio 1.54.1 after RUSTSEC-2026-0386 remediation. Exact pins/Cargo.lock, not floating releases.
- Fixture choices: digest-pinned disposable MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6 and bundled SQLite 3.51.3; native execution on macOS ARM. These are evaluated versions, not a production support matrix.
- Evidence: `m001-evidence/` and public `docs/architecture/driver-evaluation.md`. Foundation implementation is ready for completion review; no completion/adoption or M002 approval recorded.
- No dependency/advisory warning is waived. Final audit reports zero vulnerabilities/warnings; native cross-OS hosted CI and positive trusted TLS remain unexecuted, explicitly limited.

## M001 startup checklist and later questions

Startup scope is approved and implemented. Review the evaluated fixtures/tooling and recorded acceptance results before M001 sign-off; do not infer a production support or SQLx adoption decision. Generated TS compilation plus runtime golden serde fixtures verify the current model; future attributes/types need their own evidence. Lifecycle/basic-query and release performance gates are not satisfied by the SELECT 1/debug microbenchmarks.

Unresolved later gates are intentionally separate: desktop OS secret provider/SSH trust (M005), advanced job/transaction/cache semantics (M006), transfer libraries/snapshots (M007), server identity/vault/target policy (M008), desktop signing (M009), and Linux operation/recovery objectives (M011). No later server choice is an M001 or desktop-release dependency.

## Revision rationale

- Former M009 required M008 and coupled desktop distribution to Axum; M009 now covers desktop and M011 server, with independent M010-D/M010-S sign-offs to avoid an indirect dependency cycle.
- M003 now has usable bounded read-only SQL; advanced cancellation/jobs/transactions/full schema metadata remain M006. M003-L isolates lifecycle evidence so SSH is not delayed by UI completion or new engines.
- Every relevant milestone owns security, exact-value/data integrity and compatibility acceptance criteria; M010 aggregates evidence rather than postponing these obligations.
- Proposed reference workloads and budgets are explicit review inputs, not assumed performance or approved product requirements.

## Migration decisions implemented

- npm workspaces with frontend under apps/frontend; original source/config/test bytes preserved.
- Root docs are VitePress srcDir; no Markdown mirror.
- VitePress 1.6.4 pinned to a locally available genuine package/lock; frontend dependency versions retained.
- Mermaid uses pinned browser CDN loading and explicit source fallback; graphical/offline verification remains limited by environment.
- No Rust directories/app scaffolds, API implementation or infrastructure deployment.

## Continuation authorization

The user instructed “commit and continue all ms” after receiving the M001 results. This authorizes local commits and implementation of the revised roadmap, including M002 onward. It does not waive unexecuted acceptance tests, automatically certify SQLx for untested capabilities, provide signing/server identity configuration, or authorize pushing, publishing or deployment. Use the documented recommendations for reversible implementation choices and record evidence/deviations. External release prerequisites remain explicit gates.
