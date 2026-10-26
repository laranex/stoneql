# Architecture decision register

## Status and authority

Revision 2026-10-25. The user approved the revised roadmap/M001 and subsequently instructed “commit and continue all ms,” authorizing local milestone implementation and commits. M001 is committed; M002's macOS development host is verified with Linux security/cross-OS gates open; M003 runtime evaluation is in progress. Continuation does not waive acceptance criteria, approve unverified driver capabilities, provide signing/server identity configuration, or authorize push/publication/deployment. Later technical mechanisms and release budgets remain review gates. Historical M001 evidence below describes its original dependency graph, not the subsequent Tauri dependencies.

| ID | Decision / recommendation | Status and evidence gate |
| --- | --- | --- |
| D001 | Framework-independent core/contracts; separate IPC/HTTP adapters and engine/transport boundaries | Approved M001 boundary; core/contracts implemented and transitive host/driver/runtime checks passed; no Tauri/Axum types in shared crates |
| D002 | SQLx 0.9.0 with the scoped MySQL receive-limit patch | User approved adoption (“do it”, 2026-10-25) after reviewing the tested patch. Root runtime enables a 2 MiB logical packet cap; vendored provenance/license and CI drift checks retained. This approves the patch mechanism, not unverified engines/types, server security, OS compatibility or release readiness |
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

## D002 scoped receive-limit adoption approval

- Approver: user in this conversation, “do it”, responding to the explicit request to adopt the tested receive-limit patch in the main runtime before live UI integration.
- Scope: root `[patch.crates-io]` for SQLx MySQL 0.9.0; reviewed source under `vendor/sqlx-mysql`, MIT selection with complete upstream grant/attribution; runtime logical-packet cap 2 MiB. Other SQLx consumers retain the upstream default unless opting into the limit.
- Mechanism: incoming headers/continuations checked before payload reads; oversized connections discarded, clear runtime error, fresh physical connection on subsequent work. No automatic retry of failed SQL.
- Maintenance: provenance checked by `npm run rust:vendor` and CI; advisory review and complete affected test matrix on upgrades; remove fork when a compatible upstream API meets the same tests. Shared Rust core/contracts remain host/driver independent.
- Not approved by this decision: production release, signing/deployment, complete RSS/metadata bounds, arbitrary SQL sandboxing, all codecs/engines/platforms, or waivers for the existing Linux GTK advisories. M003 live binding and other gates remain separate work.

## M004 PostgreSQL/SQLite adapter choices — 2026-10-26

Implemented under the user's 2026-10-26 continuation instruction ("continue to the end of the M07. I already approved"; "commit in each ms complete"); awaiting review. These are reversible implementation choices backed by the evidence in `m004-evidence/`, not approvals of a production engine/version/OS matrix, release budgets or hostile-database safety.

| ID | Choice | Status, evidence and deviation |
| --- | --- | --- |
| D011 | PostgreSQL uses SQLx 0.9.0 `PgSslMode::VerifyFull` (system roots or a session PEM CA), `new_without_pgpass`, no statement cache, and refuses Unix-socket hosts plus inherited `PGSSLROOTCERT`/`PGSSLCERT`/`PGSSLKEY`/`PGOPTIONS` | Implemented under the 2026-10-26 continuation instruction; awaiting review. Spike finding: `PgConnectOptions::new()` silently applies libpq environment trust/client-identity/session settings and `.pgpass`, and a leading `/` host bypasses TLS; neither can be unset through the public API, so StoneQL refuses instead. Fixture: untrusted CA and hostname mismatch rejected as certificate failures, server `hostnossl reject`, wrong password 28P01 |
| D012 | PostgreSQL read-only boundary is `START TRANSACTION READ ONLY` plus `SET LOCAL statement_timeout` equal to the client deadline; per-query physical connection discard as in M003 | Implemented; awaiting review. Definer function/nested view/sequence positive controls write outside the transaction and fail with 25006 inside it; mutation omitting the read-only transaction fails the fixture. Statement timeout stopped a server `pg_sleep` after the client deadline (observer). Advisory locks remain possible and are released on discard |
| D013 | PostgreSQL values: exact binary decoders for bool/int2/int4/int8/oid/float4/float8/numeric/text-like/json/jsonb/bytea/uuid/date/time/timetz/timestamp/timestamptz, enum labels and domains over those bases; everything else (arrays, ranges, composites, interval, inet, extension types) is explicit `unknown` raw hex | Implemented; awaiting review. NUMERIC text is rebuilt from base-10000 digits with display scale (NaN/±Infinity kept); temporal text follows PostgreSQL output including BC and ±infinity; `timestamptz` is rendered in UTC with `+00` (the stored instant, no session/local zone). Interval stays unknown rather than inventing a text form |
| D014 | SQLite files are reachable only through native, owner-scoped opaque grants issued after the OS file picker; the runtime accepts only absolute canonical regular files and re-verifies canonical identity at every open | Implemented; awaiting review. Spike: SQLx opens by path with `SQLITE_OPEN_URI` and exposes no `NOFOLLOW`; canonical re-verification narrows but does not remove a same-user replace race. Picker symlink selections resolve once to their target. Grants are capped (32), revoked on renderer bootstrap/shutdown and never persisted |
| D015 | SQLite opens `SQLITE_OPEN_READONLY` with `query_only=ON`, `trusted_schema=OFF`, `cell_size_check=ON`, `mmap_size=0`, 1 s busy timeout, one connection per pool on SQLx's dedicated worker thread, and a progress-handler interrupt at the query deadline | Implemented; awaiting review. Busy lock fails within the timeout and later succeeds; deadline interrupt releases the worker (mutation without the interrupt fails). Uses SQLx's driver worker rather than a second StoneQL thread pool: no evidenced gap justified another worker layer. WAL files without writable `-shm` access, network filesystems and Windows verbatim paths are unverified |
