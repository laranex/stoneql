# Connections

Use the top connection-tab **+** or Manage Connections to create a profile. MySQL, MariaDB, PostgreSQL and SQLite profiles select engine-specific mock capabilities. An `invalid` hostname simulates a connection failure in Mock mode.

## Profiles and groups

Create, rename and delete collapsible project groups. Move profiles with the group selector. Deleting a group preserves its connections in Ungrouped. A muted connection color appears as a dot; environment tags remain independent. Production profiles show PROD and require extra confirmation for destructive operations.

Names, groups, ordering, colors, tags and allowlisted non-secret connection settings persist locally, including an enrolled SSH host key and opaque references to secrets saved in the system keychain. Record/schema changes remain session-only. Database passwords, SSH passwords, private keys and passphrases are never persisted in ordinary browser storage.

## Transport configuration

In Mock mode, Direct and SSH Tunnel forms are configuration simulations. SSH exposes agent, private-key or password authentication and an optional passphrase. No network connection or tunnel is established. Teleport and AWS SSM are future proposals, not active transports. In desktop Live mode, SSH Tunnel is real; see below.

[Connection management architecture](../architecture/connection-management.md) describes the proposed native lifecycle and credential boundary.

## Desktop live read-only mode

The connection dialog offers an explicit **Live read-only** mode in the Tauri development host for MySQL, MariaDB and PostgreSQL servers (Direct or SSH Tunnel) and for SQLite files. Browser mode and new profiles default to Mock. For **MySQL and MariaDB** live profiles the explorer lists the configured database's tables and views from `information_schema`, and opening one shows a read-only **Browse** (server-side pages of the selected size, filter and sort, with an exact total from `COUNT(*)`) and **Structure** (columns, types, NULL, defaults and auto-increment exactly as the catalog reports them). Every catalog and browse statement is an ordinary read-only SELECT through the same engine-enforced path as the SQL tab. Row editing, inserts, deletes, Search, Export, Import, Operations, and listing indexes or foreign keys are not available for live tables. PostgreSQL and SQLite live profiles remain SQL-only. Filter text containing a backslash is refused by the read-only subset. If `COUNT(*)` does not finish within the native query deadline, the page is still shown and the total is reported as unavailable. This has so far been verified only against a simulated native bridge, not a real MySQL or MariaDB server. Mutations and transfers are not exposed.

For a server, choose a configured database and a SELECT-only account, then **Test connection** or **Save & connect**. Testing opens and closes a separate real session. TLS certificate and hostname verification are mandatory for every server engine. System trust roots are used by default; an optional PEM CA certificate (16 KiB maximum) is held for this session only. Private keys are rejected. PostgreSQL connections refuse Unix-socket hosts and will not start while the `PGSSLROOTCERT`, `PGSSLCERT`, `PGSSLKEY` or `PGOPTIONS` environment variables are set, so no hidden trust or client identity is applied.

For SQLite, select **SQLite** and use **Choose file…**. The native file picker grants StoneQL access to that file only; the application never accepts a typed path. The file is opened read-only, a locked database reports an error after about one second, and the grant ends when the app or page restarts — choose the file again to reconnect. Values are shown exactly as SQLite stored them, so a column's declared type does not guarantee every value has that type.

### SSH tunnel (live)

Choose **SSH Tunnel**, enter the SSH host, port and username, and pick an authentication method:

- **SSH agent** uses the agent StoneQL inherited at startup (`SSH_AUTH_SOCK`). Agent forwarding is never requested.
- **Password** — optionally tick *Remember SSH password in the system keychain*.
- **Private key** — use **Choose key file…**; the key is read natively at connect time and never enters the window. Enter the passphrase if the key is encrypted (optionally remembered in the keychain). Ed25519 and ECDSA keys are accepted (Ed25519 and ECDSA P-256 are tested); RSA keys are not yet. Choose the file again after restarting.

Before the first connection, use **Fetch host key** (or **Test connection**). StoneQL shows the key type and SHA256 fingerprint; compare it with the fingerprint from the server administrator (for example `ssh-keygen -lf` on the server) and only then choose **Trust this host key**. A server that later presents a different key is refused with *SSH host key changed — connection refused*; StoneQL never offers to accept it. If the administrator confirms a legitimate rotation, use **Forget host key**, fetch and verify the new key, and trust it.

The Host and Port fields name the database **as the SSH server sees it**; TLS is still verified against that hostname through the tunnel. While a live test or connect is running, **Cancel connecting** stops the SSH and database setup and discards its partial tunnel. Known limitation: while a saved secret is being read from the Keychain (for example during a Keychain unlock prompt), cancellation is not yet honoured, and a connect that outlasts the app's 8-second wait can leave an invisible connection holding a slot until it expires after one hour; disconnecting all live workspaces or restarting the app releases it. SQLite files have no SSH option.

### Saved credentials

On desktop, *Remember … in the system keychain* stores the database password, SSH password or key passphrase in the macOS Keychain when you save the profile. The profile keeps only an opaque reference, and the saved secret works only for the same server, account and route (direct or the same SSH endpoint); after changing those, enter it again. Unticking the option or removing the profile asks the Keychain to forget the saved secret. Known limitation: if that deletion fails (for example a locked or denied Keychain) the failure is not yet reported, and removing an app does not remove its Keychain items; you can delete leftover `app.dbadmin.desktop.credentials` items in Keychain Access. Other operating systems report that saved credentials are not supported yet.

Passwords, CA contents, file grants and native handles are never saved in local storage. Non-secret profiles, the SQLite file name and their explicit mode persist; reopening/reloading requires a manual reconnect (with password entry unless saved in the keychain, or a fresh file/key choice).

Each profile has independent SQL tabs, results and connection state. Disconnect or closing its workspace releases its owned native pool; application shutdown closes all pools. Sessions expire after one hour. The active profile status is checked on tab switches and every 30 seconds; this is registry status, not a network health probe. A fresh renderer bootstrap invalidates the old session token and closes its native pools before live controls are enabled. Pending connection attempts cannot repopulate the old registry generation; a late completion racing bootstrap is closed before its response is returned. No automatic reconnect is attempted.

Live mode makes real connections and executes real restricted SQL. Each engine has its own small SELECT subset: PostgreSQL uses double-quoted identifiers and rejects `::` casts; SQLite accepts `typeof()` for inspecting stored types. It is a development slice, not production connectivity or a backup facility. Use trusted databases and least-privilege accounts; read-only enforcement cannot establish that views or indirect functions are harmless.
