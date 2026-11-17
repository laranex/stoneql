# Browser demo

The Vue frontend runs in an ordinary browser without the desktop app. In that mode it uses sample connections and **simulated** operations. It exists for development, tests and trying the interface. **Nothing in the demo touches a real database, file or network service. Never enter real credentials into it.**

Run it with `npm ci` and `npm run dev` (see [Contributor setup](./contributor-setup.md)).

## How the demo differs from the desktop app

| Area | Browser demo | Desktop app |
| --- | --- | --- |
| Connections | Starts with sample profiles in **Mock** mode; MySQL, MariaDB, PostgreSQL and SQLite profiles select engine-specific mock capabilities. An `invalid` host name simulates a connection failure | Every connection is live; no sample profiles or Mock mode (sample/mock profiles saved by earlier builds are removed at startup) |
| Transports | Direct and SSH Tunnel forms are configuration simulations; no network connection, agent or tunnel is used | Real Direct and SSH tunnels |
| Data and schema | Realistic relational mock databases; record and schema changes last only for the session | Real server data |
| SQL | A limited mock `SELECT` subset (column lists, equality filters, ordering, limits); other statements return a simulated error | Real SQL ([user guide](../guide/run-sql.md)) |
| Database objects | Columns, indexes, views, routines, triggers and events can be managed as mock workflows ([details](../database-objects.md)) | Columns and indexes only |
| Import/export | Simulated progress and summaries; no file, dump or compression is produced | Real files |

A success notification in the demo does not mean a database changed or a file was created.

## Simulated export

**Export From** selects the current table or database. Quick mode always uses Structure + Data. Custom mode exposes table selection, Structure/Data choices and engine-specific SQL options. The previous Custom type is kept when toggling modes but does not affect Quick exports. SQL, CSV and JSON have format-specific validation; CSV has no schema representation (use Custom → Data Only when validation requires it). ZIP/GZIP selections affect only the simulated summary. Filename and destination controls describe a future output, not a written file. Progress supports cancellation and simulated errors; success summaries are not backup evidence.

## Simulated import

CSV input is parsed and schema-validated before atomic mock insertion; invalid rows never partially change mock state. SQL import does not execute a script. File selection and progress are prototype interactions. Limits differ from the desktop app (2 MB demo file limit).

## Storage

Names, groups, ordering, colors, tags and allowlisted non-secret settings persist in browser storage. Database passwords, SSH passwords, private keys and passphrases are never persisted in ordinary browser storage.

## SQL completion status

The active editor configures CodeMirror's SQL dialect and schema extension. Additional context and schema-cache helpers exist in `apps/frontend/src/sql/` but are not wired into the active editor. Alias/JOIN resolution, ENUM-context completion and explicit backend metadata refresh are **not verified integrated features**; they need separately scoped implementation and regression tests. Live connections do not yet feed their catalog into completion.

See also the [prototype reference](./prototype-reference.md) and the [database object coverage](../database-objects.md).
