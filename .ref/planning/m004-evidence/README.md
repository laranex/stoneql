# M004 PostgreSQL and SQLite adapter evidence

Executed locally on 2026-10-26 under the user's continuation instruction. macOS ARM, Node 26.11.1, Rust 1.99.1, local Docker. No secrets, certificates, credentials or user data are retained here.

## Files

- `native-postgresql.json`, `native-cleanup-postgresql.json`: owned PostgreSQL 17.6 TLS fixture through actual Tauri handlers (MockRuntime) plus trusted fixture-only observers; image digest, engine version, 100-cycle lifecycle p50/p95 and cleanup counts.
- `sqlite-lifecycle.json`: 100 SQLite open/close cycles through actual handlers on a private temporary file.
- `regression-*.json`: MySQL 8.4.6 / MariaDB 11.4.8 native IPC, lifecycle and expired-certificate reports re-run after the runtime registry refactor.
- `verification.json`: every check executed for this increment, with counts, and the two mutation checks.
- `source-sha256.json`: hashes of the evaluated source.

## Spikes and findings

1. **PostgreSQL inherited configuration.** `PgConnectOptions::new()` reads `PGSSLROOTCERT`, `PGSSLCERT`, `PGSSLKEY`, `PGOPTIONS` and `.pgpass`; the public API cannot unset them. A host beginning with `/` selects a Unix socket without TLS. StoneQL uses `new_without_pgpass`, refuses those variables and socket hosts (D011).
2. **PostgreSQL certificate failures** surface as rustls `invalid peer certificate` I/O/TLS errors (UnknownIssuer; not valid for name), classified with the same rule as the M003 MySQL lifecycle example. The fixture server is `hostssl`-only; plaintext is refused with 28000.
3. **PostgreSQL types.** SQLx requests binary results for prepared statements. NUMERIC, temporal, jsonb and uuid are decoded from the binary form without floating or time-zone conversion; enum kind comes from SQLx's resolved `PgTypeKind` (D013). INET is deliberately left as explicit unknown hex in the fixture.
4. **PostgreSQL read-only boundary.** Definer function, nested view and sequence-through-view writes succeed for a SELECT-only reader outside a read-only transaction and fail with 25006 inside it. Mutation to `BEGIN` fails the fixture (D012). `SET LOCAL statement_timeout` stopped a server `pg_sleep` after the client deadline.
5. **SQLite file access.** SQLx opens by UTF-8 path with `SQLITE_OPEN_URI` and has no `NOFOLLOW`. Absolute canonical regular files only; re-verified at every open; grants opaque, owner-scoped, capped and revoked on bootstrap (D014). Remaining same-user replacement race documented.
6. **SQLite workers.** SQLx already runs one worker thread per connection; a progress-handler interrupt at the deadline is needed so a timed-out statement does not keep it busy (mutation proves it). Busy timeout 1 s inside the 3 s deadlines (D015).

## Limits

MockRuntime command-handler integration, not graphical WebView or the real OS file picker. One OS (macOS ARM), one PostgreSQL version, bundled SQLite only. Lifecycle timings are debug characterization, not approved budgets; the 100,000-row resource profile was not repeated for the new engines. PostgreSQL/SQLite have no per-message receive cap. Hostile database definitions/operators are not certified safe. M004 is not complete.
