# PostgreSQL and SQLite adapters

**Status: M004 local implementation and evidence recorded; acceptance gates remain open.** Desktop live profiles support Direct PostgreSQL and natively chosen SQLite files (development builds only; release builds exclude SQLite) alongside MySQL/MariaDB. Read-only queries reuse the M003 registry, limits, error mapping and per-query connection discard. This page records the M004 read-only adapters; later milestones added PostgreSQL schema discovery and Browse, SSH tunnels ([M005](./ssh-credentials.md)), read-write jobs, transactions and cancellation over Direct ([M006](./sql-execution.md)) and data-only export ([M007](../guide/import-export.md)). SQLite live profiles remain SQL-only.

## PostgreSQL

- **TLS:** always `VerifyFull` (certificate chain and hostname) against system roots or a session-only PEM CA. No plaintext, `prefer` or trust-all fallback. A host beginning with `/` (Unix socket, which bypasses TLS) is refused.
- **No inherited configuration:** SQLx would otherwise apply libpq environment variables. StoneQL ignores `.pgpass` and refuses to connect when `PGSSLROOTCERT`, `PGSSLCERT`, `PGSSLKEY` or `PGOPTIONS` are set, rather than silently using unreviewed trust, client identity or session options.
- **Identity:** `version()` must report PostgreSQL; protocol-compatible servers are refused as an engine mismatch. The displayed version comes from `server_version`.
- **Read-only boundary:** each statement runs inside `START TRANSACTION READ ONLY` with `SET LOCAL statement_timeout` equal to the three-second client deadline, on a physical connection that is discarded afterwards. The engine rejects writes through `SECURITY DEFINER` functions, nested views and sequences (SQLSTATE 25006). The statement timeout stops abandoned server work; a client-side `DEADLINE_EXCEEDED` still does not claim confirmed cancellation.
- **Lexer dialect:** double-quoted identifiers are accepted; backticks, `::` casts, `$` parameters/dollar quoting, `E''` escapes (backslashes), `CALL`, `INTO`, locking clauses and non-allowlisted functions such as `pg_sleep` or `nextval` are refused before execution.

### Value mapping

| PostgreSQL type | Result value |
| --- | --- |
| `int2`, `int4`, `int8`, `oid` | Exact integer text |
| `numeric` | Exact decimal text with the column's display scale; `NaN`, `Infinity`, `-Infinity` preserved |
| `float4`, `float8` | Shortest round-trip text; `NaN`/`Infinity` spelled as PostgreSQL does |
| `date`, `time`, `timetz`, `timestamp`, `timestamptz` | PostgreSQL-style temporal text, including `BC`, `24:00:00` and `±infinity`; `timestamptz` is the stored instant in UTC with `+00` |
| Native `ENUM` | Label text, column type is the enum name |
| `text`, `varchar`, `bpchar`, `name`, `uuid` | Text |
| `json`, `jsonb` | JSON text (jsonb in PostgreSQL's normalized form) |
| `bytea` | Binary hex |
| Domains | Decoded as their base type |
| Arrays, ranges, composites, `interval`, `inet`, `money`, extension types | Explicit `unknown` with raw wire hex; never coerced |

## SQLite

**Release builds exclude SQLite (A5, approved 2026-11-16).** `libsqlite3-sys` 0.37 (RUSTSEC-2026-0459) is required exactly by SQLx 0.9.0, so SQLite sits behind the `stoneql-runtime` `sqlite` feature (default for the runtime's own tests) and the desktop `sqlite` feature (off by default). `npm run desktop:release` builds without it: `cargo tree -i libsqlite3-sys` is empty for the arm64 and x86_64 macOS release graphs, and the release binary contains no `sqlite3_` symbol and no SQLite library. `HostInfo.sqlite` tells the renderer, which then disables **Choose file…**; the three SQLite commands stay registered and answer `UNSUPPORTED_CAPABILITY` after the window/session checks, before any dialog. `npm run desktop:test` runs the SQLite regressions with the feature and the IPC suite without it. `cargo audit` still lists the advisory because it reads the whole lockfile (development and test graph).

- **File access:** the renderer never sends a path. **Choose file…** asks the native host to show the OS file picker; the host resolves the selection to its canonical path once and returns an opaque grant plus the file name for display. Only absolute, canonical, regular files are granted. Grants are scoped to the native session, capped at 32, revoked by renderer reload/bootstrap and shutdown, and never persisted. Reopening after restart requires choosing the file again.
- **Re-verification:** each open (every query opens a fresh connection) re-checks that the granted path is still the same canonical regular file, so a later symlink or directory swap is refused. SQLite opens by path, so a same-user replacement race between that check and open remains a documented limit.
- **Read-only open:** `SQLITE_OPEN_READONLY`, `query_only=ON`, `trusted_schema=OFF`, `cell_size_check=ON`, `mmap_size=0`, no create-if-missing and no extension loading. Writes fail in the engine (`SQLITE_READONLY`) even without the lexer.
- **Workers and locks:** SQLx runs each SQLite connection on its own worker thread, so file I/O never blocks the async executor. A pool holds one connection; at most eight pools exist. Lock waits use a one-second busy timeout and fail as `QUERY_REJECTED` with unknown outcome, never as success. A progress-handler interrupt stops the statement at the query deadline and releases the worker.
- **Values:** decoded by each value's runtime storage class (INTEGER, REAL, TEXT, BLOB, NULL). Dynamic affinity is reported as stored — an `INTEGER` column holding `'yes'` returns text, and a large decimal literal converted to REAL by NUMERIC affinity is reported as a float, not an exact decimal. STRICT tables, `ANY` columns, empty text, NULL and column defaults survive unchanged. The column type is the declared type when SQLite reports one.
- **Unsupported:** SQLite has no routines or events; `CALL`, `PRAGMA`, `ATTACH`, CTEs, multiple statements and functions outside a small allowlist (`typeof`, `sqlite_version`, aggregates, `substr`, …) are refused.

## Verification

```sh
cargo test --locked -p stoneql-runtime
npm run desktop:test
npm run test:native-postgres
npm run test:native-database
```

Runtime tests use private temporary SQLite files: canonical/symlink/traversal/directory/missing-file and foreign-grant refusal, grant revocation and cap, symlink swap after grant, dynamic/strict values, engine write refusal, busy lock, and deadline interrupt. A desktop MockRuntime test drives the actual commands with a trusted grant (the picker itself is not automated) and records 100 open/close cycles.

`npm run test:native-postgres` provisions an owned, digest-pinned PostgreSQL 17.6 container with a generated one-day v3 CA, random credentials and port, `hostssl`-only authentication and a SELECT-only reader. It checks certificate-classified untrusted CA and hostname failures, wrong password, server plaintext refusal, privilege denial, definer positive controls and 25006 refusals, exact values, 100 IPC lifecycle cycles, default limits, statement-timeout server stop, advisory-lock release, lease expiry, renderer reload cleanup and capacity reuse, then removes every owned resource. Docker must use a local context.

These are command-handler/database integrations under Tauri MockRuntime, not graphical WebView tests. Cross-OS execution (Windows paths, Linux), the real native file dialog, other PostgreSQL versions/extensions, SQLite WAL files on read-only directories, network filesystems, hostile database definitions and approved performance/memory budgets remain open. PostgreSQL and SQLite have no per-message receive cap comparable to the MySQL patch.
