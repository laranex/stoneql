# Import and export

Mock profiles keep the **simulated** workflow described below: no dump, compression or filesystem backup is created.

## Live tables (desktop)

**Export** (any live MySQL, MariaDB or PostgreSQL session, including read-only): open a table, choose **Export**, pick SQL or CSV and **Choose file and export**. The desktop app shows its own save dialog; the window never sees the path. Rows are read in one read-only snapshot (MySQL/MariaDB consistent snapshot, PostgreSQL repeatable read) and streamed into a temporary file beside the destination, which is renamed into place only when complete. Cancelling, a failure, a reload or quitting removes the temporary file, so the destination is either complete or untouched.

- **SQL:** multi-row `INSERT` statements (500 rows each) whose values are exact text literals (`_utf8mb4 X'…'` on MySQL/MariaDB, `E'…'` on PostgreSQL), binary values as hex. MySQL/MariaDB can include `CREATE TABLE` (from `SHOW CREATE TABLE`); PostgreSQL exports data only (table structure needs `pg_dump`).
- **CSV:** a header line, then every value quoted; NULL is the unquoted word `NULL`, so an empty string (`""`) and the text `"NULL"` stay distinct. MySQL `LOAD DATA … FIELDS ENCLOSED BY '"' ESCAPED BY ''` and PostgreSQL `COPY … (FORMAT csv, HEADER, NULL 'NULL')` read it back exactly. Columns with binary data cannot be exported as CSV (use SQL).
- A column whose type StoneQL cannot represent exactly (for example PostgreSQL `interval`, arrays or `inet`) stops the export and is named; nothing is written.
- GZIP, ZIP, JSON and XLSX are not offered for live tables.

**Import** (read-write sessions only; the read-write gate is off in current builds): **Import** accepts CSV in the format above for a table with a primary key, up to 64 MiB. The first line must name columns of the table. All rows are inserted in one transaction in batches; if any batch is rejected, the whole import is rolled back and the message names the record range and SQLSTATE. A COMMIT whose outcome is uncertain is reported as unknown, never as imported. Live SQL import is not available yet (run statements from the SQL tab).


## Export

Export From selects the current table or database. Quick mode always uses Structure + Data. Custom mode exposes table selection, Structure/Data choices and advanced engine-specific SQL options. The previous Custom type is retained when toggling modes, but does not affect Quick exports.

SQL, CSV and JSON have format-specific validation. CSV has no schema representation: use Custom → Data Only when required by validation. SQL options include supported DDL, insert style, transaction and object settings according to the engine. ZIP/GZIP selections affect only the simulated summary. Filename and destination controls describe a future local output, not a written file.

Progress supports cancellation and simulated errors. Success summaries report selected tables, records, format and configuration; they are not backup evidence.

## Import

CSV input is parsed and schema-validated before atomic mock insertion. Invalid rows must not partially mutate mock state. SQL import does not execute a real script. File selection and progress are prototype interactions. XLSX streaming, real SQL execution, compression, filesystem access and durable jobs are planned in the [transfer architecture](../architecture/import-export.md).
