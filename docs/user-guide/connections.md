# Connections

Use the top connection-tab **+** or Manage Connections to create a profile. MySQL, MariaDB, PostgreSQL and SQLite profiles select engine-specific mock capabilities. An `invalid` hostname simulates a connection failure in Mock mode.

## Profiles and groups

Create, rename and delete collapsible project groups. Move profiles with the group selector. Deleting a group preserves its connections in Ungrouped. A muted connection color appears as a dot; environment tags remain independent. Production profiles show PROD and require extra confirmation for destructive operations.

Names, groups, ordering, colors, tags and allowlisted non-secret connection settings persist locally. Record/schema changes remain session-only. Database passwords, SSH passwords, private keys and passphrases are not persisted in ordinary browser storage.

## Transport configuration

In Mock mode, Direct and SSH Tunnel forms are configuration simulations. SSH exposes agent, private-key or password authentication and an optional passphrase. No network connection or tunnel is established. Teleport and AWS SSM are future proposals, not active transports.

[Connection management architecture](../architecture/connection-management.md) describes the proposed native lifecycle and credential boundary.

## Desktop live read-only mode

The connection dialog offers an explicit **Live read-only** mode in the Tauri development host for Direct MySQL/MariaDB. Browser mode and new profiles default to Mock. PostgreSQL, SQLite and SSH remain mock-only; live schema discovery, Browse, mutations and transfers are not exposed.

Choose a configured database and a SELECT-only account, then **Test connection** or **Save & connect**. Testing opens and closes a separate real session. TLS certificate and hostname verification are mandatory. System trust roots are used by default; an optional PEM CA certificate (16 KiB maximum) is held for this session only. Private keys are rejected. Passwords, CA contents and native handles are never saved in local storage. Non-secret profiles and their explicit mode persist; reopening/reloading requires manual reconnect and password entry.

Each profile has independent SQL tabs, results and connection state. Disconnect or closing its workspace releases its owned native pool; application shutdown closes all pools. Sessions expire after one hour. The active profile status is checked on tab switches and every 30 seconds; this is registry status, not a network health probe. A fresh renderer bootstrap invalidates the old session token and closes its native pools before live controls are enabled. Pending connection attempts cannot repopulate the old registry generation; a late completion racing bootstrap is closed before its response is returned. No automatic reconnect is attempted.

Live mode makes real network connections and executes real restricted SQL. It is a development slice, not production connectivity or a backup facility. Use trusted databases and least-privilege accounts; read-only enforcement cannot establish that views or indirect functions are harmless.
