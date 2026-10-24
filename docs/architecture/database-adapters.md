# Database adapters

**Status: Proposed / awaiting approval.** Existing engine-specific behavior is mocked.

## Adapter contract

An engine adapter should discover server version/capabilities, open connections through a transport endpoint, introspect objects, quote identifiers, execute raw SQL, decode lossless values and report transaction/cancellation support. UI capability flags must come from actual server/driver behavior, not only the selected engine label.

Keep transport establishment separate: an SSH tunnel produces an endpoint; the MySQL adapter still owns MySQL protocol and TLS semantics. SQLite uses an authorized local file handle/path, not a network tunnel.

| Engine | Discovery and capability concerns |
| --- | --- |
| MySQL | Version, storage engine, collation, routines, triggers, events, Fulltext/Spatial support |
| MariaDB | Distinct version/features despite shared protocol; do not infer MySQL parity |
| PostgreSQL | Schemas, native ENUM, functions/procedures, materialized views, extension/index methods and operator classes |
| SQLite | Dynamic affinity, strict tables, file locking, PRAGMA introspection, virtual tables; no stored routines/events |

Indexes retain ordered columns, uniqueness, method, expression/predicate and engine-specific attributes. Unsupported edits are disabled or rejected explicitly. View snapshots in the current UI are not native introspection. Database-object DDL must preserve definitions and use engine-aware quoting; never concatenate user values as SQL literals.

## Driver evaluation

SQLx is a candidate shared async interface; native PostgreSQL/MySQL drivers and rusqlite are alternatives when protocol-specific features are needed. [SQLx connection APIs](https://docs.rs/sqlx/latest/sqlx/trait.Connection.html) expose engine connections and transactions, but that alone does not prove dbAdmin requirements are satisfied. Spike dynamic unknown types, multiple result sets, raw statements, COPY, procedure calls, cancellation, TLS, DECIMAL/BIGINT decoding and pool reset before choosing.

SQLite work must not block the async executor. Evaluate a bounded dedicated worker and transaction-affine connection against driver-provided workers. Avoid pretending concurrent writers or cross-engine DDL transactions have identical semantics.

## Verification gate

Use disposable engine/version fixtures. Test quoted/Unicode identifiers, NULL/defaults, unknown types, composite indexes, DDL failures and privilege-restricted introspection. Unsupported features return structured capability errors. Define minimum supported versions and native dependency/licensing constraints before M003/M004. Integration tests require real disposable databases later; current mock tests are not evidence of adapter support.
