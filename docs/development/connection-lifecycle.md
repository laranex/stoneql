# Connection lifecycle evaluation

## Current scope

M003 is in progress. `dbadmin-runtime` implements a host-independent MySQL/MariaDB lifecycle and restricted read-only query adapter. It is not exposed to the frontend or Tauri commands yet. Saved connections and UI operations remain mocks. No credential storage or SSH is implemented.

```sh
cargo test --locked -p dbadmin-runtime
npm run test:lifecycle
```

The second command requires local Docker, OpenSSL and the pinned Rust toolchain. It starts fresh, digest-pinned MySQL 8.4.6 and MariaDB 11.4.8 fixtures on random loopback ports, creates temporary TLS identities/accounts/data, verifies a unique ownership marker, and removes only its own resources. It never uses saved profiles, arbitrary URLs or existing databases. Reports under ignored `artifacts/m003/` contain no credentials. Valid test certificates expire after one day. A separate certificate with fixed validity in January 2000 exercises expiration without changing the system clock. Certificates and keys are deleted with their private temporary directory. CAs explicitly use X.509 v3 extensions rather than platform defaults.

Readiness requires both the unique database marker and the final TCP listener; image entrypoints initially run a socket-only server. During the expired-certificate phase only, MariaDB readiness uses an authenticated Unix socket inside the disposable container without TLS. Application connections always use verified TLS over TCP and must reject the expired certificate. This keeps startup observation independent of the behavior under test.

Docker validation follows CLI precedence: an explicit `DOCKER_CONTEXT` overrides `DOCKER_HOST`. A remote effective context is rejected before fixture creation. Failure-path regressions verify cleanup after certificate setup and Docker shutdown errors in isolated command harnesses; no real Docker operation is used for these negative cases.

## Lifecycle and trust boundaries

Trusted hosts create owner identities. Handles are opaque and checked against their owner on every operation. Capacity is bounded across pending and active connections; failed attempts release permits. Expiry removes/closes pools, disconnect cancels expiry timers, and shutdown rejects late registrations. One pool has at most one physical connection. Credentials are consumed in memory; SQL and driver messages are not logged or returned as diagnostics. This does not establish a secure vault or zeroization guarantee.

TLS always uses `VerifyIdentity`, with a supplied CA or system roots. No insecure fallback is available. Engine identity is checked after TLS/authentication. Server timezone is retained; no UTC conversion is applied to temporal values. Protocol failure never triggers automatic SQL retry.

## Restricted query evaluation

The current subset accepts SELECT projections, simple filters, joins, grouping/order/limits and a small built-in function allowlist. It rejects comments, variables, locking clauses, SELECT INTO, stored/UDF calls, CTEs, multiple statements and ambiguous escaped strings. Unsupported SQL is refused explicitly; it is not rewritten.

This lexer is additional policy, not the security boundary. Each query runs inside a private engine-enforced read-only transaction; disposable tests also verify SELECT-only account privileges. Indirect effects through database-controlled views/functions still require a trust review. Accounts with broad privileges and hostile database definitions are not certified safe.

Connections are discarded after each query, including errors/timeouts/truncated streams, rather than returned with uncertain session state. Client timeout does not confirm server cancellation. User transaction leases/jobs belong to M006.

Typed results preserve signed/unsigned BIGINT strings, DECIMAL scale, binary hex, NULL versus empty and ENUM text. Raw binary temporal decoding preserves zero dates, fractional seconds and TIME values exceeding 24 hours. This avoids SQLx's zero-date-as-NULL behavior. Unknown types remain explicit raw hex. Metadata includes unknown nullability rather than fabricated constraints. See the [MySQL binary protocol](https://dev.mysql.com/doc/dev/mysql-server/8.0.46/page_protocol_binary_resultset.html) and [read-only transaction semantics](https://dev.mysql.com/doc/refman/8.4/en/commit.html).

## Executed evidence and open gates

Nine runtime tests plus the fixture certificate-error classification test passed. Both live engines passed 100 verified-TLS connect/identity/disconnect samples plus ownership, wrong-password/untrusted-CA refusal, engine mismatch, capacity/shutdown, read-only permissions and exact-value/result-limit cases. Local debug lifecycle p95 was 5.604 ms for MySQL and 3.609 ms for MariaDB; these are microbenchmarks, not release budgets. Safe summaries are retained internally under `.ref/planning/m003-evidence/`.

**Allocation-bound gap:** SQLx 0.9's receive loop reads and concatenates full packets before dbAdmin applies its output-byte cap. There is no public receive-size option. Therefore output caps currently do not bound a single incoming row's allocation. An isolated three-file patch evaluation now passes header/continuation and live TLS oversize tests, including hostname and CA rejection. It is not adopted by the root workspace. Reproduce with `npm run test:receive-limit` and `npm run test:receive-limit:live`; source/provenance/limitations live in `experiments/sqlx-receive-limit/README.md`. Production adoption and broader protocol/OS/memory review remain gates before live UI exposure.

Additional live checks now verify hostname mismatch, a valid but untrusted v3 CA, expired server certificates and actual database-session cleanup after a one-second lease. Generic connection failures do not count as certificate rejection: assertions inspect the trusted driver's certificate failure category, and wrong-password checks verify server error 1045. The cleanup observer confirms the physical session disappears and registry capacity can be reused. Failure-path tests also cover initialization/TCP readiness and secret-directory removal when setup or teardown fails.

Earlier CA tests using LibreSSL defaults could reject a version-1 root certificate before issuer validation. Explicit v3 CAs now establish the intended untrusted-issuer test. Historical reports are retained; current hardening evidence is separate under `.ref/planning/m003-lifecycle-hardening/`.

Native UI binding, production packet-allocation limits, full OS/version compatibility, hostile view/function review and representative performance/memory profiles remain pending. M003 and its lifecycle checkpoint are not marked complete.
