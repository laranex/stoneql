# Connection lifecycle evaluation

## Current scope

M003 is in progress. `dbadmin-runtime` implements a host-independent MySQL/MariaDB lifecycle and restricted read-only query adapter. It is not exposed to the frontend or Tauri commands yet. Saved connections and UI operations remain mocks. No credential storage or SSH is implemented.

```sh
cargo test --locked -p dbadmin-runtime
npm run test:lifecycle
```

The second command requires local Docker, OpenSSL and the pinned Rust toolchain. It starts fresh, digest-pinned MySQL 8.4.6 and MariaDB 11.4.8 fixtures on random loopback ports, creates temporary TLS identities/accounts/data, verifies a unique ownership marker, and removes only its own resources. It never uses saved profiles, arbitrary URLs or existing databases. Reports under ignored `artifacts/m003/` contain no credentials. Test certificates expire after one day and are deleted with their private temporary directory.

## Lifecycle and trust boundaries

Trusted hosts create owner identities. Handles are opaque and checked against their owner on every operation. Capacity is bounded across pending and active connections; failed attempts release permits. Expiry removes/closes pools, disconnect cancels expiry timers, and shutdown rejects late registrations. One pool has at most one physical connection. Credentials are consumed in memory; SQL and driver messages are not logged or returned as diagnostics. This does not establish a secure vault or zeroization guarantee.

TLS always uses `VerifyIdentity`, with a supplied CA or system roots. No insecure fallback is available. Engine identity is checked after TLS/authentication. Server timezone is retained; no UTC conversion is applied to temporal values. Protocol failure never triggers automatic SQL retry.

## Restricted query evaluation

The current subset accepts SELECT projections, simple filters, joins, grouping/order/limits and a small built-in function allowlist. It rejects comments, variables, locking clauses, SELECT INTO, stored/UDF calls, CTEs, multiple statements and ambiguous escaped strings. Unsupported SQL is refused explicitly; it is not rewritten.

This lexer is additional policy, not the security boundary. Each query runs inside a private engine-enforced read-only transaction; disposable tests also verify SELECT-only account privileges. Indirect effects through database-controlled views/functions still require a trust review. Accounts with broad privileges and hostile database definitions are not certified safe.

Connections are discarded after each query, including errors/timeouts/truncated streams, rather than returned with uncertain session state. Client timeout does not confirm server cancellation. User transaction leases/jobs belong to M006.

Typed results preserve signed/unsigned BIGINT strings, DECIMAL scale, binary hex, NULL versus empty and ENUM text. Raw binary temporal decoding preserves zero dates, fractional seconds and TIME values exceeding 24 hours. This avoids SQLx's zero-date-as-NULL behavior. Unknown types remain explicit raw hex. Metadata includes unknown nullability rather than fabricated constraints. See the [MySQL binary protocol](https://dev.mysql.com/doc/dev/mysql-server/8.0.46/page_protocol_binary_resultset.html) and [read-only transaction semantics](https://dev.mysql.com/doc/refman/8.4/en/commit.html).

## Executed evidence and open gates

Nine runtime tests passed. Both live engines passed 100 verified-TLS connect/identity/disconnect samples plus ownership, wrong-password/untrusted-CA refusal, engine mismatch, capacity/shutdown, read-only permissions and exact-value/result-limit cases. Local debug lifecycle p95 was 5.604 ms for MySQL and 3.609 ms for MariaDB; these are microbenchmarks, not release budgets. Safe summaries are retained internally under `.ref/planning/m003-evidence/`.

**Allocation-bound gap:** SQLx 0.9's receive loop reads and concatenates full packets before dbAdmin applies its output-byte cap. There is no public receive-size option. Therefore output caps currently do not bound a single incoming row's allocation. A scoped driver-limit evaluation is required before live UI exposure; no production driver approval is inferred.

Native UI binding, packet-allocation limits, hostname/expired-certificate negatives, full OS/version compatibility, hostile view/function review and representative cleanup/performance profiles remain pending. M003 and its lifecycle checkpoint are not marked complete.
