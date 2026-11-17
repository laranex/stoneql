# Import and export

Mock profiles keep the **simulated** workflow described below: no dump, compression or filesystem backup is created.

## Live tables (desktop)

**Export** (any live MySQL, MariaDB or PostgreSQL session, including read-only): open a table, choose **Export**, pick SQL or CSV and **Choose file and export**. At database level, **Export** writes every base table of the database to one SQL file (MySQL/MariaDB with `CREATE TABLE` and foreign-key checks off while importing; PostgreSQL data only); views, routines, triggers and events are not included yet. The desktop app shows its own save dialog; the window never sees the path. Rows are read in one read-only snapshot (MySQL/MariaDB consistent snapshot, PostgreSQL repeatable read) and streamed into a temporary file beside the destination, which is renamed into place only when complete. Cancelling, a failure, a reload or quitting removes the temporary file, so the destination is either complete or untouched.

- **SQL:** multi-row `INSERT` statements (500 rows each) whose values are exact text literals (`_utf8mb4 X'…'` on MySQL/MariaDB, `E'…'` on PostgreSQL), binary values as hex. MySQL/MariaDB can include `CREATE TABLE` (from `SHOW CREATE TABLE`); PostgreSQL exports data only (table structure needs `pg_dump`).
- **CSV:** a header line, then every value quoted; NULL is the unquoted word `NULL`, so an empty string (`""`) and the text `"NULL"` stay distinct. MySQL `LOAD DATA … FIELDS ENCLOSED BY '"' ESCAPED BY ''` and PostgreSQL `COPY … (FORMAT csv, HEADER, NULL 'NULL')` read it back exactly. Columns with binary data cannot be exported as CSV (use SQL).
- A column whose type StoneQL cannot represent exactly (for example PostgreSQL `interval`, arrays or `inet`) stops the export and is named; nothing is written.
- GZIP, ZIP, JSON and XLSX are not offered for live tables.

**Import** (read-write connections only): **Import** accepts CSV in the format above for a table with a primary key, up to 64 MiB. The first line must name columns of the table. All rows are inserted in one transaction in batches; if any batch is rejected, the whole import is rolled back and the message names the record range and SQLSTATE. A COMMIT whose outcome is uncertain is reported as unknown, never as imported. 

**SQL import** (MySQL and MariaDB read-write sessions): in **Import**, choose the **SQL** format, then **Choose SQL file…** and **Import**. The desktop app shows its own open dialog; the window never sees the path, and a chosen file can be used once.

- The whole file is read once and checked before anything runs: UTF-8 text, statements split with the same rules as the SQL tab (quotes, comments, `DELIMITER` for routine bodies, MySQL `/*! … */` comments), and the same refusals as read-write scripts (transaction statements, autocommit changes, `CALL`, `PREPARE`/`EXECUTE`, compound statements outside routines) plus `USE` and `LOAD DATA/XML LOCAL`. If any statement is refused, nothing runs and the message names its statement number and line.
- The checked file is then run from that same copy, one statement at a time with autocommit, on its own connection. It stops at the first failing statement and names it (number, line, SQLSTATE); statements before it stay committed, statements after it are not run. Data definition statements commit implicitly as usual.
- **Cancel import** stops before the next statement. A statement that was running when you cancelled, or when the connection was lost, has an unknown outcome: the server may or may not have applied it. It is listed under unknown outcomes and survives a restart.
- Result sets returned by statements in the file are not shown.
- Limits (Proposed, benchmarked): file up to 1024 MiB, a statement up to 16 MiB, up to 1,000,000 statements. The server's own `max_allowed_packet` also applies to each statement.
- StoneQL SQL exports import as they are (verified on MySQL 8.4 and MariaDB 11.4). Files from other tools import only when every statement is accepted; a file with `USE`, `START TRANSACTION` or `SET autocommit` is refused before anything runs. Other tools' dumps have not been tested.
- Not offered: SQL import for PostgreSQL, GZIP/ZIP and XLSX import.


## Export

Export From selects the current table or database. Quick mode always uses Structure + Data. Custom mode exposes table selection, Structure/Data choices and advanced engine-specific SQL options. The previous Custom type is retained when toggling modes, but does not affect Quick exports.

SQL, CSV and JSON have format-specific validation. CSV has no schema representation: use Custom → Data Only when required by validation. SQL options include supported DDL, insert style, transaction and object settings according to the engine. ZIP/GZIP selections affect only the simulated summary. Filename and destination controls describe a future local output, not a written file.

Progress supports cancellation and simulated errors. Success summaries report selected tables, records, format and configuration; they are not backup evidence.

## Import

CSV input is parsed and schema-validated before atomic mock insertion. Invalid rows must not partially mutate mock state. SQL import does not execute a real script. File selection and progress are prototype interactions. XLSX streaming, real SQL execution, compression, filesystem access and durable jobs are planned in the [transfer architecture](../architecture/import-export.md).
