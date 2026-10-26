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

## M005 desktop secret references and SSH transport — 2026-10-26

Implemented under the user's 2026-10-26 continuation instruction ("continue to the end of the M07. I already approved"; "commit in each ms complete"); awaiting review. These refine D005/D006 for the desktop only; they are reversible implementation choices backed by `m005-evidence/`, not approval of an OS/auth-method matrix, transport budgets, RSA support or a server vault. D005 and D006 themselves remain recommendations until reviewed.

| ID | Choice | Status, evidence and deviation |
| --- | --- | --- |
| D016 | SSH client: `russh` 0.64.1 (Apache-2.0, pure Rust) with `default-features = false, features = ["ring"]` in `stoneql-runtime` only | Implemented; awaiting review. Bounded spike (`m005-evidence/spike-dependencies.md`): ring-only graph 180 crates, `cargo audit` 0 findings; russh defaults add `rsa` (RUSTSEC-2023-0071, no patched release) and the `aws-lc-rs` C build, so they were rejected. `ssh2`/libssh2 rejected (C/OpenSSL toolchain per OS); spawning system `ssh` rejected (askpass plumbing for secrets, child-process orphan risk, argv/config inheritance). 0.65.0 (published 2026-10-25) not adopted: too new to review; 0.64.1 includes the RUSTSEC-2026-0154 fix (>=0.60.3). Consequence: RSA user and host keys are unsupported (explicit `UNSUPPORTED_CAPABILITY`). Lockfile also moved `ctutils` 0.4.5→0.4.6 for the new graph. Boundary check extended to forbid `russh`/`ssh-key`/`security-framework`/`keyring` in core/contracts |
| D017 | Host-key policy: connections require an enrolled OpenSSH public key compared byte-exactly; missing → refused before any network I/O; different → hard failure; host certificates refused; enrollment only via an explicit scan (`ssh_host_key`, disconnects before auth) plus user confirmation of the displayed SHA256 fingerprint; rotation only by explicitly forgetting the old key | Implemented; awaiting review. No TOFU, no trust-all, `known_hosts` not read. Fixture: unknown refused without network, enrolled accepted, rotated server refused (runtime + IPC, all three engines); mutation accepting any key fails the fixture at the changed-key case |
| D018 | Tunnel endpoint: one SSH session per logical handle, a private Unix socket in a new 0700 directory, a fresh `direct-tcpip` channel per accepted stream (≤4 concurrent); SQLx `socket(...)` with the original host retained for `VerifyIdentity`/`VerifyFull` | Implemented; awaiting review. Spike: SQLx MySQL/PostgreSQL use `options.host` for TLS name/verification and still upgrade TLS over a Unix socket; PostgreSQL socket path is `<dir>/.s.PGSQL.<port>`. Fixture: alias outside the SAN and an untrusted CA are refused through the tunnel. Chosen over a loopback TCP port so no other local user/process can reach the endpoint. Windows has no endpoint here (explicit unsupported); Linux not executed. Tunnel is owned by the registry entry and closed after its pool on disconnect/expiry/reset/shutdown; setup is bounded (SSH ≤ deadline, whole ≤ 2×deadline) and cancellable by owner-scoped attempt id |
| D019 | Agent policy: the trusted host captures `SSH_AUTH_SOCK` at startup (tests inject a private agent); the agent is used only to sign authentication and closed afterwards; ≤5 non-RSA identities; no agent forwarding, sessions, X11, remote or streamlocal forwards | Implemented; awaiting review. Fixture server logs show zero `session` channels and zero `auth-agent-req`; absent and dead agents fail with `SSH_AGENT_UNAVAILABLE`. Agent confirmation prompts, hardware keys and Windows/Pageant are unverified |
| D020 | Desktop secret provider: macOS login Keychain generic passwords via `security-framework` 3.7.0 (macOS-only target dependency) behind a small `SecretStore` trait; other OSes return `UNSUPPORTED_CAPABILITY`. Items `{v, scope, secret}` under service `app.dbadmin.desktop.credentials`, random UUID account; scope binds DB passwords to engine/account/host/port/route and SSH secrets to endpoint+purpose; write/delete-only IPC, resolution only inside `connect_direct` | Implemented; awaiting review. Chosen over the `keyring` crate: one verified platform, no unverified Linux/Windows backends presented as working, smaller graph. Fixture uses a per-run service, deletes every item and the runner purges/verifies the service is empty. Bundle identifier unchanged. Keychain prompts after re-signing, memory erasure and Windows/Linux providers are open |
