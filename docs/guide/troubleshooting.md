# Troubleshooting

## First launch is blocked

**“StoneQL” Not Opened** (“Apple could not verify “StoneQL” is free of malware…”), with only **Move to Trash** and **Done**.

This is expected: the beta is not notarized by Apple. If the [checksum matched](./download.md#check-the-download):

1. Choose **Done** (not Move to Trash).
2. Open **System Settings → Privacy & Security** and scroll to the message about StoneQL.
3. Choose **Open Anyway** and confirm (macOS may ask for your login password).

If you chose **Move to Trash** by mistake, install again from the DMG. A newly downloaded build may need this approval again after an update.

## Keychain prompts

When StoneQL reads a saved password for the first time after an update, macOS asks for your **login** password **twice** for the same saved item:

1. “…wants to use your confidential information…”
2. “…wants to access key…”

Both refer to the same saved password. Enter your login password and choose **Always Allow** each time.

- **Allow** permits only this one read, so you will be asked again next time.
- **Deny** refuses it. StoneQL then reports “The saved credential is unavailable or belongs to a different target. Enter it again.” and does not retry.

Every beta build counts as a new app for macOS, so expect these prompts once after each update.

**If the prompts keep coming back** (Always Allow does not always stick for this beta), save the password again from the installed build:

1. Open **Manage connections** and edit the connection.
2. Type the database password (or SSH password or key passphrase) again, with **Remember … in the system keychain** ticked.
3. Save. The saved item now belongs to the installed build, and later connections do not prompt.

While a Keychain prompt is open, StoneQL waits up to 75 seconds. **Cancel connecting** stops waiting, but the macOS prompt may stay on screen; its answer is then ignored.

## Connection errors

Messages appear in the connection dialog or as a notice. Find the message below.

### “Connection failed. Check authentication, TLS and availability.”

StoneQL could not open the connection. It deliberately does not show the server's own error text. Check, in this order:

1. **Server running and reachable:** the host name and port are correct, the server is running and no firewall blocks it.
2. **Account:** the username and password are correct, and the account may connect from your Mac.
3. **TLS on a remote server:** StoneQL verifies the server certificate and that it names the host you typed. Use the host name in the certificate (not an IP address, unless the certificate includes it). If the certificate comes from a private certificate authority, add the CA's PEM file under **Trusted CA certificate (optional, session only)**. StoneQL never connects without TLS to another computer.
4. **SSH tunnel:** TLS is verified against the database **Host** you entered (as the SSH server sees it), so that name must be covered by the database server's certificate.
5. **TLS on this Mac (MySQL/MariaDB):** if the message adds “If this local server has no certificate trusted for this host name, turn off Use TLS and try again.”, edit the connection, untick **Use TLS** and connect again. Local development servers often have a self-signed certificate.
6. **PostgreSQL:** StoneQL refuses to start a PostgreSQL connection while the `PGSSLROOTCERT`, `PGSSLCERT`, `PGSSLKEY` or `PGOPTIONS` environment variables are set, and it does not accept Unix-socket hosts. Use a TCP host name and port.

### “The server requires an encrypted sign-in for this account…”

Full message: “The server requires an encrypted sign-in for this account. Turn on Use TLS with a trusted certificate, or (MySQL caching_sha2_password) sign in once with the mysql client since the server last started, then try again. Nothing was retried without TLS.”

You connected to MySQL or MariaDB on this Mac with **Use TLS** off, and the server needs an encrypted sign-in. This happens with MySQL 8 accounts using `caching_sha2_password` (the default), or when the server requires secure connections. Either:

- turn **Use TLS** on (the server needs a certificate trusted for the host name), or
- sign in once with the `mysql` command-line client after the server starts, then connect again from StoneQL.

### SSH host key unknown or changed

- “The SSH host key is not enrolled. Fetch it, verify the fingerprint and trust it explicitly.”: choose **Fetch host key**, compare the fingerprint with the one from the server's administrator, and choose **Trust this host key** only if they match. See [Connect through an SSH tunnel](./connect.md#connect-through-an-ssh-tunnel).
- “SSH host key changed — connection refused. Verify the new key with the server administrator; forget the stored key only after verification.”: the server presented a different key than the one you trusted. This can mean the server was reinstalled, or that someone is intercepting the connection. Ask the administrator. Only if they confirm the change: edit the connection, choose **Forget host key**, fetch and verify the new key, and trust it. Saved SSH passwords for that server must then be entered again.

### “SSH authentication failed. Check the SSH username, password, key file or passphrase.”

Check the **SSH username** and the chosen method. RSA keys are not supported yet: use an Ed25519 or ECDSA key. After restarting StoneQL, choose the key file again with **Choose key file…**.

### “The SSH agent is unavailable. Start an agent with your key loaded, or choose another method.”

StoneQL found no usable SSH agent. StoneQL uses the agent available when it started: make sure an agent is running and holds your key (add it with `ssh-add`), then quit and reopen StoneQL. Or choose **Private key** or **Password** instead.

### “The saved credential is unavailable or belongs to a different target. Enter it again.”

The saved password could not be used. Causes:

- you chose **Deny** at a [Keychain prompt](#keychain-prompts), or the Keychain is locked;
- you changed the server, account or route (direct or SSH) after saving the password; a saved password works only for the target it was saved for.

Edit the connection, type the password again with **Remember … in the system keychain** ticked, and save.

### “Choose or enter the default database. Test connection lists the databases you can use.”

A connection needs a default database before it can be saved. Choose **Test connection** and pick one from **Databases on this server**, or type its name.

### “The server denied this account access; nothing was changed.” / “That database does not exist on the server.”

The account lacks the privilege for that database or action, or the database name is wrong. Ask the database administrator for access, or choose another database. When switching databases, StoneQL stays on the current one.

### “This connection is busy with another statement or an open transaction. Nothing was queued or sent.”

The connection is still running a statement, or a transaction is open where the action is not allowed. Wait for the statement to finish or choose **Cancel**, and **Commit** or **Rollback** an open transaction before switching databases or changing structure.

### “Operation timed out; server-side cancellation is unconfirmed.”

The server did not answer in time. The server may still be working on, or may have completed, the request. Check the network and the server's load, verify on the server whether the change happened, and try again.

### “Connection limit reached. Disconnect another live workspace.”

Too many live connections are open. Disconnect or close a connection tab you are not using.

### “Connection is unavailable to this session. Reconnect before running queries.”

The connection is no longer available to this window, for example because its session ended (sessions last one hour). Choose **Connect** again; StoneQL never reconnects by itself.

### “Native host unavailable · Restart StoneQL to connect”

Shown in the status bar when the app's connection engine did not start. Quit StoneQL and open it again from Applications.

## Unknown outcomes

If a red **Outcome unknown** panel appears, StoneQL could not confirm whether the server applied some work. See [Unknown outcomes](./run-sql.md#unknown-outcomes).

## Still stuck?

Check [What's in the beta](./beta.md) to see whether the feature is available yet. Report problems in the [StoneQL repository on GitHub](https://github.com/stoneAce/stoneql). Never include passwords, keys or private data in a report.
