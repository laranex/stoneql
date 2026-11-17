# Import and export

In the desktop app, import and export are live: they read and write real files and databases. The browser demo keeps the **simulated** workflow described [at the end of this page](#browser-demo): no dump, compression or file is created there.

## Live export (desktop)

**Export** works on any live MySQL, MariaDB or PostgreSQL connection, including read-only ones. Open **Export** from a table, or from the database to export it whole.

- **Export from:** the current table, or the database. **Quick** exports every base table of the database; **Custom** lets you choose tables (Select all / Deselect all). Views, routines, triggers and events are not included yet.
- **Quick** uses defaults: structure and data, extended INSERTs, foreign-key checks off for a whole-database export (MySQL/MariaDB), comma-separated CSV with a header row and `NULL` for NULL. **Custom** shows every option below.
- **Export type** (Custom): structure and data, structure only, or data only. PostgreSQL exports data only (table structure needs `pg_dump`); CSV and JSON are always data only.
- **Compression:** none or GZIP (`.gz`), in both modes.

Choose **Choose file and export**. The desktop app shows its own save dialog; the window never sees the path. Rows are read in one read-only snapshot (MySQL/MariaDB consistent snapshot, PostgreSQL repeatable read) and streamed into a temporary file beside the destination, which is renamed into place only when complete. **Cancel export**, a failure, a reload or quitting removes the temporary file, so the destination is either complete or untouched.

**Formats:**

- **SQL:** `INSERT` statements whose values are exact text literals (`_utf8mb4 X'…'` on MySQL/MariaDB, `E'…'` on PostgreSQL), binary values as hex. MySQL/MariaDB structure comes from `SHOW CREATE TABLE`. Custom options:
  - *Object creation* (MySQL/MariaDB): add `DROP TABLE` (optionally `IF EXISTS`), add `CREATE TABLE`, include `AUTO_INCREMENT` values.
  - *INSERT statement style:* one row per `INSERT`, extended (500 rows per statement, the default), and on MySQL/MariaDB `INSERT IGNORE` or `REPLACE`.
  - *Enclose data in a transaction*, and on MySQL/MariaDB *Temporarily disable foreign key checks*. PostgreSQL keeps constraints enabled.
- **CSV** (one table only): by default a header line, then every value enclosed in double quotes, with NULL as the unenclosed word `NULL`, so an empty string (`""`) and the text `"NULL"` stay distinct. MySQL `LOAD DATA … FIELDS ENCLOSED BY '"' ESCAPED BY ''` and PostgreSQL `COPY … (FORMAT csv, HEADER, NULL 'NULL')` read the default form back exactly. Custom options: the separator (one character), the enclosure (double quote, single quote or none), the NULL text and the header row. Without an enclosure, a value containing the separator or a line break, or equal to the NULL text, stops the export and names its column.
- **JSON:** each row is an object keyed by column name. Values are exact strings (numbers too, so BIGINT and DECIMAL keep every digit), booleans are JSON booleans and NULL is `null`. One table is written as an array of rows; several tables as an object whose keys are table names.
- Binary columns cannot be exported as CSV or JSON (use SQL). A column whose type StoneQL cannot represent exactly (for example PostgreSQL `interval`, arrays or `inet`) stops the export and is named; nothing is written.
- A row larger than 2 MiB stops the export; when several tables are exported, the message names its table.
- ZIP and XLSX are not offered for live exports.

## Live import (desktop)

**CSV import** (read-write connections, from a table's **Import**): accepts CSV in the default export format above for a table with a primary key, up to 64 MiB. CSV import at database level is not available yet. The first line must name columns of the table. All rows are inserted in one transaction in batches; if any batch is rejected, the whole import is rolled back and the message names the record range and SQLSTATE. A COMMIT whose outcome is uncertain is reported as unknown, never as imported.

**SQL import** (MySQL and MariaDB read-write sessions): open **Import** from the database, or from a table and choose the **SQL** format (the file runs against the connection's database either way), then **Choose SQL file…** and **Import**. The desktop app shows its own open dialog; the window never sees the path, and a chosen file can be used once.

- The whole file is read once and checked before anything runs: UTF-8 text, statements split with the same rules as the SQL tab (quotes, comments, `DELIMITER` for routine bodies, MySQL `/*! … */` comments), and the same refusals as read-write scripts (transaction statements, autocommit changes, `CALL`, `PREPARE`/`EXECUTE`, compound statements outside routines) plus `USE` and `LOAD DATA/XML LOCAL`. If any statement is refused, nothing runs and the message names its statement number and line.
- The checked file is then run from that same copy, one statement at a time with autocommit, on its own connection. It stops at the first failing statement and names it (number, line, SQLSTATE); statements before it stay committed, statements after it are not run. Data definition statements commit implicitly as usual.
- **Cancel import** stops before the next statement. A statement that was running when you cancelled, or when the connection was lost, has an unknown outcome: the server may or may not have applied it. It is listed under unknown outcomes and survives a restart.
- Result sets returned by statements in the file are not shown.
- Limits (Proposed, benchmarked): file up to 1024 MiB, a statement up to 16 MiB, up to 1,000,000 statements. The server's own `max_allowed_packet` also applies to each statement.
- Uncompressed StoneQL SQL exports import as they are (verified on MySQL 8.4 and MariaDB 11.4), except an export made with *Enclose data in a transaction*: its `START TRANSACTION`/`COMMIT` lines are refused like any transaction statement. Files from other tools import only when every statement is accepted; a file with `USE`, `START TRANSACTION` or `SET autocommit` is refused before anything runs. Other tools' dumps have not been tested.
- Not offered: SQL import for PostgreSQL, GZIP/ZIP and XLSX import.

## Browser demo

The rest of this page describes the simulated workflow in the browser demo.

### Export

Export From selects the current table or database. Quick mode always uses Structure + Data. Custom mode exposes table selection, Structure/Data choices and advanced engine-specific SQL options. The previous Custom type is retained when toggling modes, but does not affect Quick exports.

SQL, CSV and JSON have format-specific validation. CSV has no schema representation: use Custom → Data Only when required by validation. SQL options include supported DDL, insert style, transaction and object settings according to the engine. ZIP/GZIP selections affect only the simulated summary. Filename and destination controls describe a future local output, not a written file.

Progress supports cancellation and simulated errors. Success summaries report selected tables, records, format and configuration; they are not backup evidence.

### Import

CSV input is parsed and schema-validated before atomic mock insertion. Invalid rows must not partially mutate mock state. SQL import does not execute a real script. File selection and progress are prototype interactions. XLSX streaming, real SQL execution, compression, filesystem access and durable jobs are planned in the [transfer architecture](../architecture/import-export.md).
