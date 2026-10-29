# Database adapters

**Status: scoped M003/M004 read-only adapters implemented locally; broader capabilities remain gated.** For v0.1.0 Alpha (D022, approved 2026-10-26) MySQL and MariaDB are the primary supported engines; PostgreSQL is optional and SQLite is deferred — their implemented adapters stay and keep their tests, but are outside alpha support claims unless admitted. M001 isolated SQLx experiments, the [M003 MySQL/MariaDB runtime](../development/connection-lifecycle.md) and the [M004 PostgreSQL/SQLite adapters](../development/postgres-sqlite.md) are implemented; [evidence](./driver-evaluation.md) is separate from mock UI behavior and production capability adoption.

## Adapter contract

An engine adapter should discover server version/capabilities, open connections through a transport endpoint, introspect objects, quote identifiers, execute raw SQL, decode lossless values and report transaction/cancellation support. UI capability flags must come from actual server/driver behavior, not only the selected engine label.

Keep transport establishment separate: an SSH tunnel produces an endpoint; the MySQL adapter still owns MySQL protocol and TLS semantics. SQLite uses an authorized local file handle/path, not a network tunnel.

| Engine | Discovery and capability concerns | v0.1.0 Alpha |
| --- | --- | --- |
| MySQL | Version, storage engine, collation, routines, triggers, events, Fulltext/Spatial support | Primary (Approved). Alpha discovery: databases, tables/views, columns, primary/unique keys; routines/triggers/events Deferred |
| MariaDB | Distinct version/features despite shared protocol; do not infer MySQL parity | Primary (Approved); same alpha discovery slice, verified separately |
| PostgreSQL | Schemas, native ENUM, functions/procedures, materialized views, extension/index methods and operator classes | Optional; read-only adapter implemented, browse/CRUD not planned for the alpha unless admitted |
| SQLite | Dynamic affinity, strict tables, file locking, PRAGMA introspection, virtual tables; no stored routines/events | Deferred; read-only adapter implemented, not claimed |

Alpha CRUD (Proposed slice) edits one row at a time through parameterized statements keyed by the primary key with an optimistic check on the previous values; zero affected rows is a conflict, never a silent overwrite, and constraint errors come from the database. Tables without a primary key and views are browse-only. Advanced objects are Deferred.

Indexes retain ordered columns, uniqueness, method, expression/predicate and engine-specific attributes. Unsupported edits are disabled or rejected explicitly. View snapshots in the current UI are not native introspection. Database-object DDL must preserve definitions and use engine-aware quoting; never concatenate user values as SQL literals.

## Driver evaluation

SQLx 0.9.0 with the user-approved receive-limit patch is adopted for the tested M003 MySQL/MariaDB runtime subset. M004 uses the same SQLx release for the tested PostgreSQL 17.6 and bundled SQLite 3.51.3 read-only subset; the receive-limit patch is MySQL-only, so PostgreSQL and SQLite rows have no equivalent per-message allocation cap. Broader engine/type/transaction support and release readiness remain gated. Native PostgreSQL/MySQL drivers and rusqlite are fallback alternatives only after an evidenced capability gap and explicit exception decision. [SQLx connection APIs](https://docs.rs/sqlx/latest/sqlx/trait.Connection.html) expose engine connections and transactions, but that alone does not prove StoneQL requirements are satisfied. Spike dynamic unknown types, multiple result sets, raw statements, COPY, procedure calls, cancellation, TLS, DECIMAL/BIGINT decoding and pool reset before approving the affected production capability. M001 records the evidence matrix; M003 adopts only the basic read-only subset, while M006 gates multiple result sets, routines, advanced transactions and cancellation. A documented API is not proof that our versions, raw-value fidelity and cleanup requirements work.

[SQLx raw SQL documentation](https://docs.rs/sqlx/latest/sqlx/fn.raw_sql.html) distinguishes raw statements from prepared parameterized queries; generated CRUD must retain bound values, and arbitrary script/routine behavior needs separate probes. Never infer safe concatenation or cross-engine atomicity from a generic execution API.

SQLite work must not block the async executor. M004 uses SQLx's dedicated per-connection worker thread (one connection per pool, at most eight pools) with a busy timeout and a deadline interrupt; no second StoneQL worker pool was needed. Avoid pretending concurrent writers or cross-engine DDL transactions have identical semantics.

## Implemented runtime layout

`crates/stoneql-runtime` keeps one registry (`lib.rs`: ownership, capacity, expiry, reset/shutdown, SQLite file grants) and one bounded executor (`read_only.rs`: limits, the dialect-aware SELECT lexer and row/byte streaming). Engine modules own only what differs: `mysql.rs` (options, identity, MySQL binary values), `postgres.rs` (VerifyFull TLS, read-only transaction and statement timeout, binary NUMERIC/temporal decoding) and `sqlite.rs` (file authorization, read-only open, worker interrupt, storage-class values). Unsupported engine features (SQLite routines/events, PostgreSQL `CALL`, multiple statements, casts, dollar quoting) are refused, not emulated.

## Verification gate

Use disposable engine/version fixtures. Test quoted/Unicode identifiers, NULL/defaults, unknown types, composite indexes, DDL failures and privilege-restricted introspection. Unsupported features return structured capability errors. Define minimum supported versions and native dependency/licensing constraints before M003/M004. Integration tests require real disposable databases later; current mock tests are not evidence of adapter support.
