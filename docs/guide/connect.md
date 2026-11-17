# Connect to a database

StoneQL connects to **MySQL**, **MariaDB** and **PostgreSQL** servers, either directly or through an SSH tunnel. You need the server's host name and port and a database account (user name and password).

::: tip Start safely
New connections are **read-write**: SQL scripts, row edits and structure changes can change data. When you explore a server that matters, use an account with only the privileges you need, or tick **Read-only connection** (step 6).
:::

## Create your first connection

1. **Open the connection manager.** On the **Welcome to StoneQL** screen choose **Manage connections**. (Later you can also use the **+** next to the connection tabs, or **Manage connections** at the top right.)
2. **Start a new connection.** Choose **New connection** at the bottom of the connection manager.
3. **Name it and pick the engine.** Enter a **Connection name** (for example *Local development*) and choose the **Database engine**: MySQL, MariaDB or PostgreSQL. The port changes to the engine's default (3306 or 5432). SQLite is listed but is not included in the beta app.
4. **Optional: label it.** Choose an **Environment** (Local, Development, Staging, Production or Custom), a **Group** and a **Connection color**. Connections tagged **Production** show a **PROD** badge and ask for extra confirmation; see [Production connections](#production-connections).
5. **Enter the server and account.** Keep **Direct** selected (or see [Connect through an SSH tunnel](#connect-through-an-ssh-tunnel)), then fill in **Host**, **Port**, **Username** and **Password**. These fields are used exactly as typed.
6. **Choose read-write or read-only.** Leave **Read-only connection** unticked for full access, or tick it to allow only reading. The choice is saved with the connection. See [Read-only or read-write](#read-only-or-read-write).
7. **Check encryption.** For a server on another computer, StoneQL always uses verified TLS, so there is nothing to choose. For MySQL or MariaDB on this Mac, see [TLS and certificates](#tls-and-certificates).
8. **Decide whether to remember the password.** **Remember database password in the system keychain** is ticked by default; see [Saved passwords](#saved-passwords). Untick it to type the password each time you connect.
9. **Test and choose a database.** Choose **Test connection**. A successful test shows the databases your account can see in **Databases on this server**: pick one. You can instead type a name in **Default database**. A default database is required to save.
10. **Connect.** Choose **Save & connect**. (**Save** stores the connection without connecting.)

### You are connected when…

- The **Explorer** on the left lists your databases (MySQL/MariaDB) or the tables of your database (PostgreSQL).
- The status bar at the bottom shows **Live read-write · connected** or **Live read-only · connected**.
- Selecting a table opens its rows in **Browse**.

If the connection fails, the dialog shows a message. Look it up in [Troubleshooting](./troubleshooting.md#connection-errors).

Next: [Browse and edit tables](./browse-and-edit.md).

## Read-only or read-write

- **Read-write** (the default) lets you run SQL scripts, use transactions, edit rows, change table structure and import data. Confirmations in StoneQL are safeguards, not permissions: your database account decides what is allowed.
- **Read-only connection** lets you browse, run single `SELECT` queries and export. The database itself enforces it: StoneQL runs each query inside a read-only transaction.
- **PostgreSQL through an SSH tunnel** is always read-only in this beta.

The SQL tab and the status bar show **Read-only** or **Read-write** for the open connection. To change it, edit the connection and connect again.

## Production connections

Set **Environment** to **Production** for servers that matter. These connections show **PROD**, and each time one connects read-write StoneQL asks you to confirm with **PROD — I understand**. Destructive actions on them always ask for confirmation.

## TLS and certificates

- **Another computer:** StoneQL always uses TLS and verifies the server's certificate and host name. It never falls back to an unencrypted connection.
- **MySQL or MariaDB on this Mac** (host `localhost`, `127.0.0.1` or `::1`, Direct): the dialog shows **Use TLS**, off for new connections. Off sends traffic unencrypted and needs no certificate. On verifies the certificate and host name like any other connection. If a local TLS connection fails because the certificate does not match, turn **Use TLS** off and try again.
- **PostgreSQL** and SSH-tunnelled connections always use verified TLS.
- **Your own certificate authority:** if the server's certificate is issued by a private CA, choose its PEM file under **Trusted CA certificate (optional, session only)**. It is kept only until StoneQL quits (16 KiB maximum; private keys are rejected). **Use system trust roots** removes it.

## Saved passwords

With **Remember database password in the system keychain** ticked, StoneQL stores the password in your macOS login Keychain, never in its own files. The saved password works only for the same server, account and route; if you change those, type it again.

The first time a new StoneQL build reads a password saved by an earlier build (for example after an update), macOS asks for your login password, usually twice. Choose **Always Allow**. See [Keychain prompts](./troubleshooting.md#keychain-prompts).

Unticking the option, or removing the connection, deletes the saved password. If the Keychain refuses (for example because it is locked), StoneQL tells you, and removing a connection offers **Remove anyway**.

## Connect through an SSH tunnel

Use this when the database is reachable only from a server you can SSH into.

1. In the connection dialog choose **SSH Tunnel**.
2. Enter **Host** and **Port** of the database **as the SSH server sees it**, plus the database **Username** and **Password**. Connections through a tunnel always use verified TLS, checked against this **Host**, so use a host name that the database server's certificate covers.
3. Under **SSH tunnel configuration**, enter the **SSH host**, **SSH port** and **SSH username**.
4. Choose an **Authentication** method:
   - **SSH agent:** uses the SSH agent available to StoneQL. Agent forwarding is never requested.
   - **Password:** enter the **SSH password**; optionally tick **Remember SSH password in the system keychain**.
   - **Private key:** use **Choose key file…** and, if the key is encrypted, enter the **Key passphrase** (optionally remembered in the Keychain). Ed25519 and ECDSA keys work; RSA keys are not supported yet. Choose the file again after restarting StoneQL.
5. **Trust the host key.** Choose **Fetch host key** (or **Test connection**). StoneQL shows the key type and its SHA256 fingerprint. Compare it with the fingerprint from the server's administrator (for example from `ssh-keygen -lf` on the server). Only if they match, choose **Trust this host key** and confirm. Otherwise choose **Do not trust**.
6. Continue from step 8 of [Create your first connection](#create-your-first-connection).

Trust applies to every connection that uses the same SSH server and port. Changing the SSH host or port shows whether the new server is trusted. Connections saved by earlier StoneQL versions need their host key trusted again. If the server later presents a different key, StoneQL refuses to connect; see [SSH host key changed](./troubleshooting.md#ssh-host-key-unknown-or-changed).

While a test or connection is in progress, **Cancel connecting** stops it and discards the partial tunnel.

## Manage connections

- **Groups:** create, rename and delete collapsible groups in the connection manager, and move a connection with its **Group** selector. Deleting a group keeps its connections under *Ungrouped*.
- **Edit or remove** a connection from the connection manager.
- **Several connections at once:** each open connection has its own tab with its own Explorer, SQL tabs and results.
- **Disconnect** from the SQL tab, or close the connection's tab. Sessions end after one hour, and StoneQL never reconnects by itself: choose **Connect** again.
- After restarting StoneQL, connect again; you are asked for the password unless it is saved in the Keychain.

StoneQL stores connection names, groups, colors, tags and other non-secret settings on your Mac. Passwords, key passphrases and CA certificates are never stored in its own files.

## For developers

The [connection management architecture](../architecture/connection-management.md) and [SSH and credentials](../development/ssh-credentials.md) pages describe the internals.
