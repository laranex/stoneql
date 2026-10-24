# Workspace

Breadcrumbs identify the connection, database and table. Select a table in the resizable Explorer to open Browse. Connection tabs maintain separate navigation, SQL tabs and history.

## Browse and edit

Sort headers, filter, paginate and select rows. Drag a header boundary to resize; double-click it to auto-fit visible content. Widths persist by connection/database/table. Use the grid context menu to reset widths. Double-click a cell to edit; schema-aware validators protect insert, record editing and inline changes. BIGINT, DECIMAL and temporal values retain their raw representation.

Use row Edit, Copy and Delete actions or the Insert tab. Destructive actions require confirmation; production-tagged connections add a safeguard. Unsaved record changes require confirmation before discarding. SQL result banners can be dismissed independently of data. Collapse the SQL preview to reclaim space; the preference lasts for the session.

## Structure and database objects

Manage columns and indexes under Table → Structure. Database-level navigation exposes supported views, routines, triggers and events. Engine capabilities determine available operations. See [verified object coverage](../database-objects.md) for precise mock behavior and limitations.

## Preferences

The footer toggles Light/Dark immediately. First launch follows the OS; a manual selection persists. Settings controls rows per page and keyboard shortcuts. Font sizes and compact layout stay consistent between themes.

[Connections](./connections.md), [SQL editor](./sql-editor.md), and [import/export](./import-export.md) describe the other workflows.
