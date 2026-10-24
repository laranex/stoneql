# M001 driver evaluation

**Status: evaluation implemented; completion review pending. SQLx is not production-approved.** User approval on 2026-10-25 authorizes foundation and isolated spikes only. No frontend mock, production connection service, Tauri, Axum, SSH or credential store is replaced or implemented.

## Reproducible environment and evidence

- Rust **1.99.1**, edition 2024; Cargo.lock locks the evaluation. SQLx **0.9.0**, Tokio **1.54.1**, ts-rs **12.0.1**. Direct dependencies are exact-pinned in Cargo.toml.
- Host: Apple M5, 10 logical CPUs, 24 GiB RAM, darwin/arm64, kernel `27.0.0`. Docker 29.8.2; database containers are Linux images, not native macOS servers.
- MySQL 8.4.6, MariaDB 11.4.8 and PostgreSQL 17.6 images are digest-pinned in the fixture Compose file. SQLite 3.51.3 is bundled through the locked driver. Fixture versions describe the tested scope, not an approved production support/security matrix.
- Every run creates its own project/network, random loopback ports, ephemeral passwords, marker database and tmpfs state. Remote Docker daemons and non-fixture URLs are refused. Existing database containers are never used. Inherited database targets/passwords and PostgreSQL TLS key paths are removed from the lab environment. The project bridge permits egress; it is not an air-gapped network. No host networking, Docker socket mount or persistent database volume is used.
- Internal raw evidence: `.ref/planning/m001-evidence/` includes environment/lock hash, per-engine assertions/timings, contract samples, dependency metadata/audit and SHA256.json. Generated local reruns go to ignored `artifacts/m001/`. Reports contain no passwords, URLs, row data or SQL text.

## Observed capability coverage

All **28 live assertions passed** (seven per engine). The four additional unsupported/unverified observations are explicit limits, not supported features. Safety-boundary tests and DTO tests are separate Rust unit tests.

| Feature | MySQL / MariaDB | PostgreSQL | SQLite |
| --- | --- | --- | --- |
| Runtime result metadata | Named/type metadata and changing result shapes | INT8/NUMERIC/BYTEA metadata; custom ENUM name | Runtime storage classes and result shapes |
| NULL versus empty text | Distinct | Distinct | Distinct |
| BIGINT | Signed minimum and unsigned maximum exact | Signed minimum exact | Signed minimum exact; out-of-range integer CAST clamps, explicitly observed |
| DECIMAL | DECIMAL(38,4) fixture with exact value and trailing scale | NUMERIC decoded without float conversion, trailing scale retained | Exact only in tested TEXT representation; NUMERIC affinity converted the fractional fixture to REAL |
| Binary | NUL/high-byte values exact | BYTEA exact | BLOB exact |
| ENUM / opaque type | Schema ENUM label preserved | Custom ENUM metadata and raw wire bytes preserved | No native ENUM claim |
| Multiple results | Two statements and two-result procedure, completion boundaries drained | Two statements; INOUT procedure result | Two statements; CALL rejected, no stored procedures |
| Cancellation | Control connection KILL QUERY; interruption observed; SELECT after cancellation succeeds | pg_cancel_backend, SQLSTATE 57014; SELECT after cancellation succeeds | Safe SQLx progress-handler interruption; handler removed; connection reusable |
| TLS failure behavior | VerifyIdentity rejects untrusted fixture certificate | VerifyFull rejects fixture without trusted TLS | Not applicable |

SQLx has not been accepted as an all-engine abstraction. Cancellation methods differ; these experiments do not implement a common production cancel API. The cancellation probe's elapsed time includes connection/setup/observation/reuse, not just cancel acknowledgement. MySQL standalone SLEEP may return a successful value on interruption, so the probe uses SLEEP within a table query. Prepared MySQL queries can appear as `Execute` rather than `Query` in PROCESSLIST; the readiness check handles both. These harness corrections are not SQLx bugs.

Positive trusted-TLS handshakes, temporal/JSON/extension-type codecs, exhaustive opaque MySQL values, cross-user permissions, transaction/commit-loss semantics, session/pool reset and arbitrary routine behavior remain unverified. The fixtures use privileged **disposable** accounts to inspect/cancel their own sessions; this is not an acceptable production credential policy.

## Measurements

Debug builds, 100 sequential `SELECT 1` samples and 100 connect/close samples per engine, loopback Docker transport (SQLite in memory). Times include client/transport overhead and local scheduling. These are microbenchmarks, **not** the M003 200-row/100,000-row-fixture workload, production SLOs, memory bounds or release acceptance.

| Engine | Reported version | Query p95 (µs) | Connect + close p95 (µs) |
| --- | --- | ---: | ---: |
| mysql | 8.4.6 | 566 | 6784 |
| mariadb | 11.4.8-MariaDB-ubu2404 | 257 | 6557 |
| postgresql | 17.6 (Debian 17.6-2.pgdg13+1) | 320 | 21099 |
| sqlite | 3.51.3 | 21 | 113 |

The contract debug microbenchmark ran 10 warmups and 1000 samples over a 647-byte synthetic value fixture. Decode p95: **45167 ns**; complete TypeScript declaration generation p95: **74709 ns**. Raw samples are retained; no constant-time, memory/SLO or production throughput claim follows. Peak Rust process RSS and Windows/Linux native build performance have not been measured.

Normal and assertion-failure runs tear down only their unique fixture projects. Final evidence records zero remaining containers/networks for all three server engines. A forcibly killed runner can require manual cleanup; this is a laboratory harness, not a durable job manager. SQLx pool/reset leak guarantees remain M003/M006 work.

## Dependency and security review

The initial Tokio 1.48.0 evaluation selection triggered [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html) (AsyncFd I/O soundness). It was replaced by non-yanked 1.54.1; no advisory was ignored. Final cargo-audit **0.22.2** with `--deny warnings` reports **0 vulnerabilities and 0 warnings**, using advisory database commit `21d909e4c8bd4e14c7aea58bf980c881a0053ab1`. This is date-specific evidence, not a lasting security guarantee.

Registry license metadata is recorded for all resolved packages (including target-specific packages). Permissive alternatives are selected where offered: for example r-efi's MIT option rather than LGPL; webpki-roots certificate data is CDLA-Permissive-2.0. MIT/Apache, Unicode, ISC, BSD, Zlib/BSL and LLVM-exception notices need retention as applicable. This metadata review is not a final distribution/SBOM legal audit; bundled native/data notices must be checked at release.

Statement logging is disabled in network driver options. Diagnostic reports retain only case names, codes, methods and timings. Generated numeric wire values remain strings; no Rust core crate depends on SQLx/Tokio/Tauri/Axum. The pinned SQLx 0.9 API requires an explicit AssertSqlSafe for dynamic SQL; the spike uses it only for a validated server-returned numeric KILL identifier, never to concatenate record values or claim arbitrary SQL is sanitized.

## Local verification

Rust workspace build, rustfmt, all-target Clippy with warnings denied and **12 Rust tests** passed. Shared dependency checks find one package for the dependency-free core and 22 for contracts, with no Tauri/Axum/SQLx/Tokio in either closure. Exact TypeScript regeneration and strict positive/negative consumer compilation passed. The generation drift check caught an unregenerated engine-ID change during development; the checked-in generated source now matches Rust.

Existing **128 frontend tests + seven workspace tests**, frontend production build and **17 Playwright scenarios (12.9s)** passed. All 73 pre-migration frontend source/config/test/asset hashes still match. The existing bundle-size advisory remains. Chromium sandboxing was preserved; approved elevated execution was used for browser startup and final Rust helper checks where required by the local sandbox. Documentation build and local source/generated-link checks passed. actionlint 1.7.7 validated both workflows. None of this claims remote CI or cross-OS native execution.

## Reproduce and next decisions

See [Rust development](../development/rust-foundation.md) for commands and safety constraints. Hosted CI is configured for Linux/macOS/Windows foundation checks and Linux Docker spikes, but has **not been executed remotely**. Local macOS ARM evidence does not establish the entire native support matrix.

Recommendation for review: retain SQLx 0.9.0 for the next bounded read-only evaluation, contingent on trusted-TLS and codec-policy evidence. No tested driver gap currently justifies adding a parallel native driver. SQLite affinity limits are engine semantics, and cancellation still needs engine-specific control; neither should be hidden behind a falsely uniform capability. This recommendation is not production adoption or permission to start another milestone.

M001 stops at review: decide whether this evidence is sufficient to accept the foundation, whether SQLx merits the M003 read-only slice, and what additional TLS/raw-type/cancellation/compatibility probes are required. M002 and production driver adoption still require explicit approval. Native-driver exceptions are not selected automatically.
