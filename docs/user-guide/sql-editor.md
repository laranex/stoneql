# SQL editor

CodeMirror provides SQL highlighting, dialect-aware built-in keyword/schema completion and multiple query tabs. Use Ctrl+Space for completion and Cmd/Ctrl+Enter to execute the current query in the selected connection mode. History and query tabs belong to the active connection.

```sql
SELECT id, email FROM users WHERE id = 1 ORDER BY id LIMIT 10;
```

The mock executor supports a limited SELECT subset, including column lists, equality filters, ordering and limits. It does not execute arbitrary SQL against a database. Unsupported statements return a simulated error. Preserve SQL keywords, identifiers and raw query results when editing application text.

## Completion status

The active editor configures CodeMirror's SQL dialect and schema extension. Additional context and schema-cache helpers exist in `apps/frontend/src/sql/`, but are not wired into the active editor. Production-grade alias/JOIN resolution, ENUM-context completion and explicit backend metadata refresh are **not verified integrated features**. They require a separately scoped implementation and regression suite.

## Results

Browse-style result grids support selection, resizing and horizontal scrolling. Notifications do not own results or history; dismissing a notification leaves both intact. Editor/result and preview controls retain the existing compact workflow.

## Native read-only results

Explicit desktop Live profiles execute a restricted single SELECT through verified TLS. No mock fallback occurs on native failure. Supported syntax is deliberately narrower than the database grammar; CALL, scripts, mutations, transaction commands and unsupported forms are refused. Schema discovery and live autocomplete metadata are not implemented.

Results retain ordered columns, including duplicate labels, exact integer/decimal strings, raw temporal text, NULL and hexadecimal binary/unknown values. Limits are 1,000 rows, 1 MiB serialized result and 2 MiB per incoming protocol packet; truncation is labelled, while an oversized individual value fails rather than being silently shortened. A three-second native deadline is not confirmation of server cancellation. Errors use safe fixed messages and do not expose raw driver diagnostics. There is no automatic SQL retry.
