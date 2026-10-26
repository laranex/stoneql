# Connections

Use the top connection-tab **+** or Manage Connections to create a profile. MySQL, MariaDB, PostgreSQL and SQLite profiles select engine-specific mock capabilities. An `invalid` hostname simulates a connection failure in Mock mode.

## Profiles and groups

Create, rename and delete collapsible project groups. Move profiles with the group selector. Deleting a group preserves its connections in Ungrouped. A muted connection color appears as a dot; environment tags remain independent. Production profiles show PROD and require extra confirmation for destructive operations.

Names, groups, ordering, colors, tags and allowlisted non-secret connection settings persist locally. Record/schema changes remain session-only. Database passwords, SSH passwords, private keys and passphrases are not persisted in ordinary browser storage.

## Transport configuration

In Mock mode, Direct and SSH Tunnel forms are configuration simulations. SSH exposes agent, private-key or password authentication and an optional passphrase. No network connection or tunnel is established. Teleport and AWS SSM are future proposals, not active transports.

[Connection management architecture](../architecture/connection-management.md) describes the proposed native lifecycle and credential boundary.

## Desktop live read-only mode

The connection dialog offers an explicit **Live read-only** mode in the Tauri development host for Direct MySQL, MariaDB and PostgreSQL servers and for SQLite files. Browser mode and new profiles default to Mock. SSH remains mock-only; live schema discovery, Browse, mutations and transfers are not exposed.

For a server, choose a configured database and a SELECT-only account, then **Test connection** or **Save & connect**. Testing opens and closes a separate real session. TLS certificate and hostname verification are mandatory for every server engine. System trust roots are used by default; an optional PEM CA certificate (16 KiB maximum) is held for this session only. Private keys are rejected. PostgreSQL connections refuse Unix-socket hosts and will not start while the `PGSSLROOTCERT`, `PGSSLCERT`, `PGSSLKEY` or `PGOPTIONS` environment variables are set, so no hidden trust or client identity is applied.

For SQLite, select **SQLite** and use **Choose file…**. The native file picker grants StoneQL access to that file only; the application never accepts a typed path. The file is opened read-only, a locked database reports an error after about one second, and the grant ends when the app or page restarts — choose the file again to reconnect. Values are shown exactly as SQLite stored them, so a column's declared type does not guarantee every value has that type.

Passwords, CA contents, file grants and native handles are never saved in local storage. Non-secret profiles, the SQLite file name and their explicit mode persist; reopening/reloading requires a manual reconnect (with password entry or a fresh file choice).

Each profile has independent SQL tabs, results and connection state. Disconnect or closing its workspace releases its owned native pool; application shutdown closes all pools. Sessions expire after one hour. The active profile status is checked on tab switches and every 30 seconds; this is registry status, not a network health probe. A fresh renderer bootstrap invalidates the old session token and closes its native pools before live controls are enabled. Pending connection attempts cannot repopulate the old registry generation; a late completion racing bootstrap is closed before its response is returned. No automatic reconnect is attempted.

Live mode makes real connections and executes real restricted SQL. Each engine has its own small SELECT subset: PostgreSQL uses double-quoted identifiers and rejects `::` casts; SQLite accepts `typeof()` for inspecting stored types. It is a development slice, not production connectivity or a backup facility. Use trusted databases and least-privilege accounts; read-only enforcement cannot establish that views or indirect functions are harmless.
