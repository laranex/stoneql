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

In the desktop app, Table → Structure shows a live table's columns, indexes and foreign keys exactly as the server's catalog reports them; it is read-only for now. In the browser demo you can also manage columns and indexes, and database-level navigation exposes views, routines, triggers and events; engine capabilities determine available operations. See [database object management](../database-objects.md) for the demo's precise behavior and limitations.

## Not yet available in the desktop app

These features are hidden for live connections rather than simulated:

- the Search tab and bulk Copy of selected rows;
- table Operations (rename, copy, truncate, drop);
- column and index changes in Structure (being worked on next);
- database objects: managing views, routines, triggers and events;
- the database-level Structure overview;
- switching between databases on a server (the Explorer shows the connection's default database);
- CSV import at database level, and SQL import for PostgreSQL;
- exporting views, routines, triggers and events.

## Preferences

The footer toggles Light/Dark immediately. First launch uses Light whatever the OS appearance; a manual selection persists (earlier saved choices, including System, are kept). Settings controls rows per page and keyboard shortcuts. Font sizes and compact layout stay consistent between themes.

[Connections](./connections.md), [SQL editor](./sql-editor.md), and [import/export](./import-export.md) describe the other workflows.
