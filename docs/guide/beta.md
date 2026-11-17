# What's in the beta

StoneQL **v0.1.0-beta.1** is a public beta, published on 2026-11-17 as a [GitHub pre-release](https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1).

## Platform

| | Status in this beta |
| --- | --- |
| macOS on Apple Silicon (arm64) | Available. Tested on macOS 27.0.1 only; older versions are untested |
| macOS on Intel | Not available |
| Windows, Linux | Not available |
| Signing | Ad-hoc signed, **not notarized** (no Developer ID yet), so the [first launch needs approval](./install.md#approve-the-first-launch) |

## What works

- **Databases:** MySQL, MariaDB and PostgreSQL, live. SQLite is not included in the beta app.
- **Connections:** Direct or through an SSH tunnel (SSH agent, password or Ed25519/ECDSA private key) with explicitly trusted host keys. Verified TLS for other computers; TLS optional for MySQL/MariaDB on this Mac. Passwords can be remembered in the macOS Keychain. See [Connect to a database](./connect.md).
- **Access:** read-write by default (MySQL/MariaDB Direct and SSH, PostgreSQL Direct), with a saved **Read-only connection** option. PostgreSQL over SSH is always read-only. PROD connections ask for confirmation.
- **Browsing and editing:** server-side paging, sort and filter; insert, edit and delete rows in tables with a primary key. See [Browse and edit tables](./browse-and-edit.md).
- **Databases (MySQL/MariaDB):** list every database the account can see, switch between them and create databases.
- **Structure:** add, change, rename and drop columns, and add or drop secondary indexes, with an SQL preview and confirmation.
- **SQL:** single read-only queries, or read-write scripts with transactions, cancellation and honest “unknown” outcomes. See [Run SQL](./run-sql.md).
- **Import and export:** export a table, chosen tables or a database as SQL, CSV (one table) or JSON, optionally GZIP; import CSV into a table, and SQL files into MySQL/MariaDB. See [Import and export](./import-export.md).
- **Interface:** Light and Dark themes, several connections side by side, connection groups and colors.

## Not available yet

These features are hidden in the desktop app rather than simulated:

- the Search tab and bulk Copy of selected rows;
- table Operations (rename, copy, truncate, drop);
- managing views, routines, triggers and events, and exporting them;
- the database-level Structure overview;
- switching to another database on PostgreSQL connections;
- CSV import at database level, and SQL import into PostgreSQL.

Also not supported yet:

- RSA SSH keys, `~/.ssh/config`, jump hosts and two-factor SSH sign-in;
- Teleport and AWS SSM connections (shown as *coming later*);
- SQLite files (development builds only).

## How it was tested

The packaged beta app was checked by hand against **MySQL over a direct connection**. MariaDB, PostgreSQL and SSH tunnels were tested through the app's connection engine against test servers (MySQL 8.4, MariaDB 11.4 and PostgreSQL 17), not by hand in the packaged app. Report anything that does not work as described.

StoneQL is beta software and not a backup tool. Keep backups, use accounts with only the privileges you need, and verify important changes on the server.

## Browser demo

The same interface also runs in a web browser with sample data and simulated operations, for trying the interface and for development. Nothing in the demo touches a real database; never enter real credentials there. See the [browser demo](../development/browser-demo.md) in the Developer Docs.
