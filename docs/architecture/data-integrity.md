# Data integrity

**Status: Proposed production rules / awaiting approval.** Frontend schema validation and mock constraint services already exist; they are UX aids, not database authority.

## Values and validation

Preserve raw BIGINT/DECIMAL and temporal values. Never round, truncate, timezone-convert or silently coerce. Keep NULL, default, empty text and expressions separate throughout editing, transport and adapter layers. Validate type lengths, ranges and supported constraints centrally before mutations, then let the database enforce authoritative constraints and permissions.

Use parameterized values for generated CRUD, engine-aware identifier quoting and schema revisions. Arbitrary user SQL remains SQL: do not apply record-form validation to its text or claim it is safely parameterized by the application.

## Concurrency

Use primary-key identity including all composite parts, plus a reliable row-version or original-value predicate for optimistic updates. An affected-row conflict must not overwrite silently. Tables without stable identity are read-only unless a reviewed explicit mutation strategy is chosen. Handle NULL comparisons and engine affected-row differences deliberately.

Validate an entire mock batch before mutation; production batches should use transactions where supported. Foreign keys, unique indexes, generated columns and defaults are database-enforced. Do not duplicate referential integrity policy in individual buttons. Confirmations and PROD tags do not replace authorization.

## Transactions and uncertain outcomes

Pin one physical connection for a transaction lease. Bound lease lifetime and clean up on cancellation/disconnect. A cancellation acknowledgement is not rollback evidence. Network loss during commit has an unknown outcome; expose that status and require reconciliation rather than retrying.

DDL atomicity varies by engine and operation. In particular, MySQL implicit commits mean a multi-object schema operation cannot be promised atomic. See [MySQL implicit commit documentation](https://dev.mysql.com/doc/refman/8.4/en/implicit-commit.html). Record per-step outcomes and prevent false all-or-nothing success notices.

Proposed execution contract (2026-10-26, awaiting approval): every statement, job and transaction outcome is exactly one of confirmed success, confirmed failure, confirmed cancellation or unknown. Cancellation is best-effort; a cancelled or timed-out write is reported as unknown unless a server-confirmed rollback follows, rollback is never claimed without server confirmation, and writes with uncertain outcomes are never retried automatically.

## Required tests

Use real disposable database integration tests for concurrent edits, FK delete/update actions, composite keys, duplicate batches, precision boundaries, NULL/defaults, rollback, timeout and lost commit responses. Property-based DTO round trips are a candidate, not an installed dependency. Mock unit tests cannot establish real database isolation or DDL behavior.
