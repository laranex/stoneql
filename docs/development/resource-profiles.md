# Runtime resource profiles

M003 has an opt-in, disposable MySQL/MariaDB resource profile. It measures the current runtime; it does not impose a process-memory cap or complete a release gate. The proposed numerical budgets in the [roadmap](../architecture/roadmap.md) still need approval.

```sh
npm run test:resources
```

Use Node 26, the pinned Rust toolchain, local Docker, OpenSSL and `/usr/bin/time` on macOS or Linux. It provisions digest-pinned owned TLS fixtures with random loopback ports and ephemeral credentials. The same Docker-context, ownership/readiness and cleanup boundaries as `test:lifecycle` apply. It never opens saved profiles or existing databases. Windows resource measurement is not implemented. The child workload has a 180-second deadline, shorter than the 240-second outer measurement limit.

## Workload and measurement boundaries

- Create 100,000 rows with a primary key and a 1,024-byte ASCII payload. Verify the row count and that EXPLAIN selects PRIMARY for the reference range query.
- Open eight owned logical connections. Sample connected-idle process RSS, then run 100 primary-key range SELECTs of 200 rows. Each latency sample includes runtime execution, raw decoding and result JSON serialization; the runtime discards its physical query connection as usual. No warmup samples are removed.
- Read empty results with 16, 128, 512 and 2,048 projected columns, measuring serialized metadata. Repeat with a 128-byte result allowance, require ResultTooWide and verify a subsequent small query succeeds.
- Run eight queries concurrently, each requesting 1,000 rows with a 1 MiB serialized-result allowance. Require explicit row-boundary truncation, retain all eight results together, and sample RSS while they are retained. Values are never shortened.
- Observe peak RSS for the entire child process using `/usr/bin/time`. Normalize macOS bytes and Linux KiB, rejecting missing/ambiguous/unsafe measurements. This excludes Docker server, parent harness, Tauri and WebView memory. Record an additional connected-idle snapshot with `/bin/ps`; an approximate difference of these measurements is not a portable hard allocation bound.
- Close the registry and require that only the owning cleanup observer remains before closing it. Remove owned containers/networks and private certificates, including on failure.

Raw 100-sample latency arrays, metadata sizes, process memory, runtime engine version and non-secret CPU/OS/RAM characteristics are written under ignored `artifacts/m003/resource-*.json`. Safe reviewed snapshots are kept in internal planning. Debug instrumentation, Docker scheduling, OS accounting and allocator retention affect results; do not compare them as approved cross-platform release benchmarks.

## Remaining allocation risk

The 2 MiB SQLx limit bounds a logical incoming packet, not the sum of many column-definition packets. In the approved driver's `prepare_statement` and `recv_result_metadata`, multiple descriptors are accumulated before the application checks serialized metadata. The application rejects an oversized output after prepare; this does not establish an upstream aggregate allocation bound. Ordinary wide results and concurrent workloads characterize the tested cases, not a hostile server's maximum allocation.

A 2026-10-26 isolated investigation located the gap in `recv_result_metadata`. It reserves descriptor storage from the server-supplied column count before any descriptor arrives. Each runtime query performs three metadata reads: prepare, a non-persistent re-prepare, and the result set. The result-set count is length-encoded rather than limited to 16 bits. Both pinned engines accepted prepared expression SELECTs of 65,535 columns. On the local debug client, peak RSS was about 37 MiB, compared with about 8 MiB at 4,096 columns; the engines' 4,096-column limit applies to tables, not SELECT lists. An opt-in column-count and descriptor-byte limit was prototyped in `experiments/sqlx-metadata-limit/` and **adopted on 2026-11-16 (A6)**: the vendored driver now refuses more than 4,096 columns or 4 MiB of descriptors per result before reserving or decoding them (see [connection lifecycle](./connection-lifecycle.md)). Expression SELECTs of 4,097–65,535 columns, which worked before, are now refused.

The hostile huge-column-count abort remains reasoned from the code, not reproduced with adversarial traffic. Native IPC/renderer overhead, hostile metadata bounds, real graphical database workflows and Windows/Linux runtime verification remain open M003 gates.

## Local observation — 2026-10-26

On macOS ARM (Apple M5, 10 logical CPUs, 24 GiB RAM), the debug fixture run recorded:

| Engine | Runtime + JSON p95 | Standalone peak RSS |
| --- | ---: | ---: |
| mysql | 14.6 ms | 23.4 MiB |
| mariadb | 14.3 ms | 22.9 MiB |

Both engines passed metadata rejection/recovery and retained eight individually bounded results. The 2,048-column empty result serialized to 109,552 bytes. These are local observations of this workload, excluding native/UI and database-server memory; they are not approved release guarantees or hostile-metadata bounds.
