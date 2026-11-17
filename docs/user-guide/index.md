# Workspace

Breadcrumbs identify the connection, database and table. Select a table in the resizable Explorer to open Browse. Connection tabs maintain separate navigation, SQL tabs and history. In the Explorer, each database row (arrow, icon and name) is one expand/collapse control; focus it and press Enter or Space.

## Browse and edit

Sort headers, filter, paginate and select rows. Drag a header boundary to resize; double-click it to auto-fit visible content. Widths persist by connection/database/table. Use the grid context menu to reset widths. Double-click a cell to edit; schema-aware validators protect insert, record editing and inline changes. BIGINT, DECIMAL and temporal values retain their raw representation. Grids show what the database stores: an empty string is an empty cell, SQL NULL is a muted italic *NULL* (the text `'NULL'` appears as ordinary text, and the cell tooltip reads “SQL NULL” only for a real NULL), and tabs, line feeds and carriage returns appear as muted ⇥, ↵ and ␍ markers (CRLF as one ␍↵), keeping rows one line high. Markers are display-only; editing, copying, sorting and filtering use the raw value. Copying selected grid text puts the stored characters (a real tab, LF, CR or CRLF) on the clipboard, never the marker glyphs. Screen readers announce SQL NULL as “SQL NULL” and each marker as “tab”, “line feed”, “carriage return” or “CRLF”; a literal `'NULL'` is read as plain text.

Use row Edit, Copy and Delete actions or the Insert tab. In the desktop app these work on read-write connections for tables with a primary key; tables without one, and views, are read-only (see [Connections](./connections.md#desktop-live-connections)). Destructive actions require confirmation; production-tagged connections add a safeguard. Unsaved record changes require confirmation before discarding. SQL result banners can be dismissed independently of data. Collapse the SQL preview to reclaim space; the preference lasts for the session.

### Multi-line values

Editing a cell whose stored value contains a line feed or carriage return opens a floating multi-line editor over the grid, so row heights never change; other values keep the single-line editor, where Enter saves. In the multi-line editor Enter inserts a new line, Ctrl+Enter or Cmd+Enter saves and Escape cancels (asking first if you changed something); focus returns to the cell. The record form uses the same editor for TEXT columns and for any value containing LF or CR.

Browsers show and report every line break in a text box as LF, so StoneQL rebuilds the stored value instead of saving what the text box reports, and a **Line endings** selector next to the text shows what will be written:

- Saving without changing the text writes back the original value exactly, whatever its line endings.
- If the value uses one style (all LF, all CRLF or all CR), edited text — including new lines you type or paste — is saved in that style.
- A value with mixed endings starts on *Mixed (as stored)*: while the number of line breaks is unchanged, each break keeps the ending it had at that position. If you add or remove line breaks, saving is blocked until you choose LF, CRLF or CR, which then applies to every break.
- Choosing LF, CRLF or CR converts every line break on save; this is the only way line endings change.
- New line breaks in a value that had none are saved as LF unless you choose otherwise.

## Structure and database objects

In the desktop app, Table → Structure shows a live table's columns, indexes and foreign keys exactly as the server's catalog reports them. On a read-write MySQL, MariaDB or PostgreSQL connection you can **Add column**, use **Change** (rename or edit) and **Drop** on any column of a base table, and **Add index** (unique or not, columns in the order you pick) or drop a secondary index (a primary key or constraint-backed index is dropped only with its constraint). On MySQL/MariaDB, Change starts from the server's own column definition (`SHOW CREATE TABLE`), so collation, comment, `ON UPDATE` and other attributes are kept unless you edit them; a rename alone uses `RENAME COLUMN`. On PostgreSQL, Change edits the type (with an optional `USING` expression), NULL allowed and the default, and alters only what you changed, so comments, collation, identity and constraints stay as they are. Every change shows the exact SQL before **Apply**; dropping a column or changing a definition asks for confirmation (and PROD connections always confirm). The server enforces privileges: an account without `ALTER`/`INDEX` (MySQL/MariaDB), or that does not own the table (PostgreSQL), is refused and nothing changes. Schema changes are refused while a transaction is open, because the server would commit it. In the browser demo you can also manage columns and indexes, and database-level navigation exposes views, routines, triggers and events; engine capabilities determine available operations. See [database object management](../database-objects.md) for the demo's precise behavior and limitations.

## Databases in the Explorer

For MySQL and MariaDB the Explorer lists every database the account can see; the connection's default database is open first. Opening another database (or one of its tables) switches the session to it on the same connection, so Browse, SQL, edits, import and export use it; unsaved query tabs are kept. A switch is refused while a transaction is open. If the server refuses a database (no access, or it does not exist), StoneQL says so and stays on the current one. On a read-write connection, the **+** next to DATABASES creates a database (name, optional character set and collation), with the SQL shown first.

## Not yet available in the desktop app

These features are hidden for live connections rather than simulated:

- the Search tab and bulk Copy of selected rows;
- table Operations (rename, copy, truncate, drop);
- database objects: managing views, routines, triggers and events;
- the database-level Structure overview;
- CSV import at database level, and SQL import for PostgreSQL;
- exporting views, routines, triggers and events.

## Preferences

The footer toggles Light/Dark immediately. First launch uses Light whatever the OS appearance; a manual selection persists (earlier saved choices, including System, are kept). Settings controls rows per page and keyboard shortcuts. Font sizes and compact layout stay consistent between themes.

[Connections](./connections.md), [SQL editor](./sql-editor.md), and [import/export](./import-export.md) describe the other workflows.
