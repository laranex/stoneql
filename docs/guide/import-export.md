# Import and export

Import and export read and write real files and change real databases. Keep backups before importing into a database that matters.

## Export a table or database

Export works on every MySQL, MariaDB and PostgreSQL connection, including read-only ones.

1. Open a table and choose **Export**, or open the database and choose **Export** to export several tables.
2. Under **Export from:** choose **Current table** or **Database**.
3. Choose the **Export method**:
   - **Quick** uses sensible defaults: structure and data, extended INSERTs, foreign-key checks off for a whole-database export (MySQL/MariaDB), and comma-separated CSV with a header row and `NULL` for NULL. A Quick database export includes every table.
   - **Custom** shows every option below and lets you pick tables (**Select all** / **Deselect all**).
4. Choose the **Format**: **SQL**, **CSV** or **JSON**.
5. Choose **Compression**: **None** or **GZIP** (`.gz`).
6. Choose **Export**, then pick where to save the file in the macOS save dialog.

While it runs you can choose **Cancel export**. StoneQL writes to a temporary file and renames it only when the export is complete, so the destination is either complete or untouched. All rows come from one consistent snapshot of the database.

### Formats

- **SQL:** `INSERT` statements with exact values; binary values as hex. MySQL and MariaDB exports can include table structure. **PostgreSQL exports data only** (table structure needs `pg_dump`). Custom options:
  - **Export type:** Structure and Data, Structure Only or Data Only.
  - **Object creation options** (MySQL/MariaDB): **Add DROP TABLE statement** (optionally **Add IF EXISTS to DROP TABLE**), **Add CREATE TABLE statement**, **Include AUTO_INCREMENT values where applicable**.
  - **INSERT statement style:** one row per statement, extended (500 rows per statement, the default), and on MySQL/MariaDB **INSERT IGNORE** or **REPLACE**.
  - **Enclose data in a transaction**, and on MySQL/MariaDB **Temporarily disable foreign key checks**.
- **CSV** (one table, data only): by default a header row, every value in double quotes, and NULL as the unquoted word `NULL`, so an empty string and the text `"NULL"` stay distinct. MySQL `LOAD DATA … FIELDS ENCLOSED BY '"' ESCAPED BY ''` and PostgreSQL `COPY … (FORMAT csv, HEADER, NULL 'NULL')` read this default form back exactly. Custom options: **Columns separated with**, **Columns enclosed with** (double quote, single quote or none), **Replace NULL with**, and **Put column names in the first row**. Without enclosing, a value that contains the separator or a line break, or equals the NULL text, stops the export and names its column.
- **JSON** (data only): each row is an object keyed by column name. Values are exact strings (numbers too, so large numbers keep every digit), booleans are `true`/`false` and NULL is `null`. One table is an array of rows; several tables are an object keyed by table name.

### Export limits

- Views, routines, triggers and events are not exported yet.
- Binary columns can be exported only as SQL, not CSV or JSON.
- A column type StoneQL cannot represent exactly (for example PostgreSQL `interval`, arrays or `inet`) stops the export and is named; nothing is written.
- A row larger than 2 MiB stops the export; when several tables are exported, the message names the table.
- ZIP and XLSX are not offered.

## Import a CSV file into a table

Needs a read-write connection and a **table with a primary key**.

1. Open the table and choose **Import**.
2. Choose your CSV file (up to 64 MiB). **Format** must be **CSV**.
3. Choose **Import**.

The file must use the default CSV export format above, and its first line must name columns of the table. All rows are inserted in one transaction: if any row is rejected, nothing is imported, and the message names the record range and SQLSTATE. If StoneQL cannot confirm the final commit, it reports the outcome as unknown, never as imported.

CSV import at database level is not available yet.

## Import a SQL file (MySQL and MariaDB)

Needs a read-write MySQL or MariaDB connection.

1. Open the database and choose **Import**. (From a table, choose **Import** and set **Format** to **SQL**; the file runs against the connection's database either way.)
2. Choose **Choose SQL file…** and select the file in the macOS open dialog. A chosen file can be imported once; choose it again to repeat.
3. Choose **Import**. On a PROD connection StoneQL reminds you that the import changes data.

What happens:

1. StoneQL first reads and checks the whole file. It must be UTF-8 text. If any statement is not allowed, nothing runs and the message names the statement number and line.
2. It then runs the statements one at a time, each saved as it completes. It stops at the first failing statement and names it (number, line and SQLSTATE). Statements before it stay saved; statements after it are not run. Statements that change structure (such as `CREATE TABLE`) save immediately, as usual on the server.
3. **Cancel import** stops before the next statement. A statement that was running when you cancelled, or when the connection was lost, has an [unknown outcome](./run-sql.md#unknown-outcomes); it stays listed, also after a restart, until you acknowledge it.

Not allowed in an imported file: transaction statements, autocommit changes, `USE`, `CALL`, `PREPARE`/`EXECUTE`, `LOAD DATA/XML LOCAL`, and compound statements outside stored programs. `DELIMITER` for routine bodies works. Result sets returned by the file are not shown.

Files StoneQL exported import as they are, except exports made with **Enclose data in a transaction** (their `START TRANSACTION`/`COMMIT` lines are refused). Dumps from other tools import only if every statement is allowed; they have not been tested.

### Import limits

- File up to 1024 MiB, each statement up to 16 MiB, up to 1,000,000 statements (limits of v0.1.0-beta.1; later builds may lower the statement limits). The server's `max_allowed_packet` also applies.
- Not available: SQL import into PostgreSQL, compressed (GZIP/ZIP) files and XLSX.

## For developers

The [transfer architecture](../architecture/import-export.md) describes the design and the remaining proposals.
