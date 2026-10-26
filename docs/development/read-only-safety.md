# Read-only SQL safety verification

M003's restricted SELECT lexer is an additional refusal policy. The engine read-only transaction and database privileges enforce distinct boundaries. A SELECT-only account can still invoke a view whose definer can write: MySQL documents that view/routine execution may use the [definer's privileges](https://dev.mysql.com/doc/refman/8.4/en/stored-objects-security.html), and MariaDB documents the same [routine security context](https://mariadb.com/docs/server/server-usage/stored-routines/stored-functions/stored-routine-privileges). Therefore neither a SELECT prefix nor the reader's direct table grants establish that an indirect operation is harmless.

## Executable fixture cases

`npm run test:native-database` provisions owned MySQL 8.4.6 and MariaDB 11.4.8 TLS fixtures. Their initialization creates private counters, definer functions, views and SELECT-only/writer accounts. It uses no existing profiles, user databases, native UDF libraries, filesystem access or external services.

| Case | Expected evidence |
| --- | --- |
| Definer view writes an InnoDB counter | Positive control changes it outside a read-only transaction; engine guard rejects it with1792 |
| Definer view writes a MyISAM counter | Positive control changes it;1792 refusal and unchanged counter prove safety is not merely rollback |
| Nested view invokes the writing function | Same refusal through an additional view layer |
| Actual native `read_only_query` command on all three views | Fixed QUERY_REJECTED error with unknown outcome; no raw driver detail forwarded |
| Ordinary invoker view after each rejected query | Subsequent request succeeds on a fresh physical connection |
| Definer view invokes advisory lock acquisition | Result proves indirect lock acquisition is possible; independent observer proves the query connection's close released it |
| Counter inspection after native requests | Both persistent counters remain zero after positive controls were reset |

Positive controls prevent a false pass caused by missing routine privileges, invalid view definitions or a function that never runs. Trusted fixture-only SQL inspects the server's exact error code separately from native UI messages. The native adapter correctly keeps an uncertain error outcome conservative; these fixture observations do not change the public error contract.

The native tests use Tauri MockRuntime and real SQLx/database connections. They verify command-handler/database integration, not graphical WebView-to-database interaction. Cleanup reports and safe results are retained under `.ref/planning/m003-read-only-trust/` in the repository; no secrets, certificate material or user query data are retained.

## Limits and deployment implications

Result limits have one authority. `QueryLimits::MAX_ROWS` (1000) and `QueryLimits::MAX_BYTES` (1 MiB serialized) in `crates/stoneql-runtime/src/read_only.rs` are both the enforced upper bounds and the defaults. `LiveQueryRequest.maxRows`/`maxBytes` are optional: the renderer omits them, `apps/desktop/src-tauri/src/database.rs` substitutes the runtime maximums, the strict envelope decoder rejects `null`, and `Connections::read_only` rejects any explicit value outside 1–max; both surface as `INVALID_REQUEST`. The native query deadline is fixed at three seconds by the desktop adapter. The 2 MiB incoming packet cap is a separate driver receive limit, not a result limit.

The function allowlist is **not transitive** through database-controlled views. Read-only transactions reject the tested persistent writes; they do not make arbitrary stored code, UDFs, plugins or external effects safe. Advisory locks are session-scoped effects that can occur before connection cleanup. Client timeouts remain distinct from confirmed server cancellation.

Use trusted database definitions and least-privilege accounts, preferably grants on reviewed base tables rather than blanket access to unreviewed definer views. Do not describe M003 as safe for hostile database definitions, broad administrative accounts or arbitrary SQL. No UDF library is installed or executed by the tests. The test account's ability to invoke fixture views must not be treated as recommended production grants.

Native GUI interaction, Windows/Linux compatibility, aggregate prepared metadata/process-memory limits remain open. M006 still owns full schema metadata, routines, advanced jobs, transactions and execution cancellation. This evidence narrows one M003 trust question; it does not complete M003 or approve a production release.
