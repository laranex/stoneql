# Prototype reference (pre-migration)

Preserved original frontend documentation. Relative source paths below were rooted at the old repository layout; prepend `apps/frontend/` for source/test paths. Current root commands delegate to that workspace. Architecture and verification pages supersede historical planning/results below.


**Database management, made familiar.**

A compact Vue 3 database management UI inspired by classic phpMyAdmin. Browser runnable, with independent connection workspaces and session-only relational mock data. No backend, real database access, credential persistence, SSH tunneling, Rust, or Tauri integration is implemented.

## Getting started

Requires Node.js 26 and npm.

```sh
npm install
npm run dev
```

Open the Vite URL printed in the terminal. The app opens `acme_store.users` with two sample connections. Use the connection manager to configure MySQL, MariaDB, PostgreSQL, and SQLite mock connections. An `invalid` hostname simulates a connection failure.

```sh
npm test                  # service regression tests; Node native TypeScript support
npm run build             # Vue/TypeScript checking and Vite production bundle
npm run preview           # serve dist/ locally
npx playwright install chromium
npm run test:browser      # browser workflow checks and light/dark screenshots
```

## Workflows

- Expand databases and select tables; drag the sidebar divider to resize.
- Browse, sort, filter, paginate, select, copy, delete, and double-click cells to edit. The Insert tab opens record creation.
- The compact breadcrumb is the workspace context: server, database, and table are interactive. Row counts appear on the right; InnoDB/collation metadata appears only for mock MySQL/MariaDB application tables. Long names truncate and metadata wraps without overlapping.
- SQL query previews start expanded. Collapse with the chevron to keep a single truncated line; the preference lasts for the session across tables, screens, and query executions. Edit query remains available when expanded.
- Dismiss query result banners with the × button. New results show a fresh notification; dismissal does not modify results, query history, or records. Success, warning, and error notices share the same accessible behavior.
- Manage columns and named Primary/Unique/Normal/Composite indexes in Table → Structure. MySQL/MariaDB Fulltext/Spatial and PostgreSQL index-method configurations are capability-gated mock workflows. Foreign-key references remain visible.
- Use Insert for new records or the row Edit action for existing records.
- SQL uses CodeMirror 6 highlighting and schema completion. Cmd/Ctrl+Enter executes mock SELECT statements, including column lists, equality WHERE conditions, ORDER BY, and LIMIT. Other SQL produces a clear error. Query tabs and history belong to each connection.
- CSV import strictly validates headers, quotes, and field counts, preserves raw strings, and applies the same atomic schema/UNIQUE/FK checks as record editing. Errors retain CSV record/column context. SQL import is a neutral simulation notice; it does not execute or claim to import records.
- Operations supports table rename, copy, truncate, and drop. Destructive operations require confirmation.
- Settings contains page size, keyboard shortcut preferences, and the shortcut reference. On first launch the app follows the OS theme. The footer sun/moon button switches to the opposite resolved theme and persists a manual Light/Dark choice.

## Connection organization and persistence

The active Connection Manager supports project groups, rename/delete, collapse, filtering, Move to Group selectors, and keyboard-accessible ordering buttons. Deleting a group moves its connections to Ungrouped. Create/Edit Connection supports independent muted colors and environment tags, including custom labels. Production connections show PROD and destructive database operations require an extra acknowledgement.

Only allowlisted connection metadata, group assignments/order, and theme/grid preferences are saved to local storage. Passwords, SSH passwords, private keys, and passphrases remain in memory. Legacy connection arrays migrate without losing connections. Database records and schema changes remain session-only. No real connection is attempted.

## Schema and record editing

Add Column and Change on the actual Structure screen open the updated inline dialog. It separates type parameters, defaults, nullability, attributes, and positioning. The old dialog persisted because its original App.vue handlers had not been connected to the new schema service; those handlers now use the store's schema update method. MySQL/MariaDB support physical positioning; PostgreSQL/SQLite keep existing positions and append new columns. All schema modifications are simulated.

Schema-aware validators are independent of Vue and shared by Insert, Edit, and Browse cell editing. Errors appear inline and prevent saving. NULL and database defaults are explicit choices. BIGINT/DECIMAL input stays as exact text, raw date/time values are not converted to JavaScript dates, and SQL query text is excluded from form validation. Backend/database validation remains authoritative for real integration.

ENUM/SET presentation comes from column metadata. Demo schemas include MySQL/MariaDB ENUM/SET and a registered PostgreSQL native ENUM. Choice editors preserve unknown current values rather than replacing them when a definition changes. Ordinary VARCHAR roles remain plain text.

Browse and SQL results support dragging header boundaries, double-click/Enter auto-fit, keyboard resizing, and a Reset Column Widths context-menu action. Widths are bounded and saved per connection/database/table/column, or per SQL query. Horizontal scrolling stays inside the grid. Header and body separators share 1px geometry; theme tokens slightly strengthen header separators while the resize grips appear only on hover/focus/drag.

## Export prototype

Export supports current-table and database scope. Quick always uses Structure and Data with default output settings, independently of remembered Custom options. Custom shows the Export Type on the right of the Quick/Custom row, table selection, and advanced SQL/CSV/JSON settings. Switching modes preserves the Custom type and table choices. CSV requires Custom mode with Data Only; incompatible Quick CSV exports are blocked. SQL options include DROP/IF EXISTS, CREATE TABLE, engine-specific insert styles, sequence values, transactions, and foreign key handling. Additional object options are capability-gated; sample schemas now include views, routines, triggers and MySQL/MariaDB events. Summaries count configured definitions from the mock snapshot; no definitions are dumped.

Output configuration includes filename templates (`@DATABASE@`, `@TABLE@`, `@DATE@`) and None/ZIP/GZIP compression selections. **All export jobs are simulated. No backup, SQL dump, compressed archive, download, or filesystem write is performed.** A completed job displays table, structure, and record counts plus a proposed filename and estimated uncompressed size. The expandable prototype testing control simulates a destination failure. Jobs can be cancelled or retried. Leaving Export cancels any active job. Export preferences remain in Pinia per connection and database during the session.

CSV is data-only; incompatible selections are validated. PostgreSQL hides MySQL insert syntax and disables unsupported events/global foreign key options. SQLite excludes routines/events. Counts come from a snapshot taken when the job starts.

## Architecture

- `src/components/`: reusable CodeMirror editor and dedicated Export workflow.
- `src/stores/workspace.ts`: Pinia connections, independent datasets, table CRUD, query tabs, preferences.
- `src/stores/export.ts`: session export configuration, independent of view lifecycle.
- `src/types/`: connection/transport models, export configuration, reusable engine capability map.
- `src/services/valueValidation.ts`: pure schema-aware record validation and ENUM/SET metadata.
- `src/services/columnDefinition.ts`: engine capabilities, type formatting, schema validation, and atomic mock column updates.
- `src/services/connectionPreferences.ts`: allowlisted connection/group persistence and legacy migration.
- `src/services/gridWidths.ts` and `src/directives/gridResize.ts`: width storage and direct-DOM grid resizing.
- `src/services/mockTransport.ts`: mock connection testing, filters, limited SELECT evaluator.
- `src/services/exportService.ts`: effective configuration, validation, accurate mock summaries, cancellable simulated transport interface.
- `src/data/sample.ts`: sample relational databases with 128 users and hundreds of related records.
- `src/style.css`: compact shared layout, Tailwind import, CSS-variable themes.
- `public/icon.svg`: original dbAdmin database/checkmark app icon, also used as the logo.
- `tests/`: deterministic service regression tests and Playwright UI workflows.

Connection transports are discriminated types: Direct and SSH have configuration UI; Teleport and AWS SSM are future types only. Replace service interfaces with native commands for future Tauri integration without moving transport behavior into components.

## Appearance and verification

The interface uses native system fonts, centralized 14px body/control/grid typography, 13px semibold grid and section headings, 20px page titles, and a 12px minimum for secondary/status text. Light and Dark palettes use neutral surfaces and muted blue interaction colors. Status colors are reserved for success, warning, and destructive states. Active connections and navigation use restrained blue selection surfaces; sidebar and toolbar surfaces remain subtly distinct. SQL highlighting uses the same shared palette, with contrast-tested keyword, string, numeric, type, and comment colors in both themes. Help, Settings, and the one-click Light/Dark toggle remain in the fixed bottom bar while workspace content scrolls. Dedicated footer surface, text, and border tokens distinguish the status bar without changing its height, spacing, or control positions. The footer occupies its own layout space and does not cover data rows.

The production build passes on Node 26; Vite reports a non-blocking bundle-size advisory. Service, actual-screen component, and contrast/typography regression tests pass with `npm test`. Browser tests cover all screens, connection setup, theme changes, and footer access, and capture Light/Dark screenshots in `test-results/`. Browser verification and before/after screenshot comparison could not run in this sandbox: preview server binding is denied and Chromium is blocked by macOS process permissions.

No native Tauri configuration, custom title bar, or window-drag hooks exist in this browser prototype. Removing the global header does not remove any native hooks; native window dragging must be verified when Tauri integration is implemented.

## UI review remediation

See [the findings and verification ledger](../ux-review-remediation.md). Record changes use one pure mutation boundary for outgoing constraints and incoming foreign-key RESTRICT checks. Mock schemas currently represent single-column keys and references, not database-complete constraint semantics.

Browse and SQL results have bounded scroll regions with sticky headers. Browse cells support arrow/Home/End/Tab traversal, Enter/F2 editing and Space selection; inline editors are bounded popovers. Dialogs share focus trapping/restoration and unconditional Escape dismissal. Unsaved edits use the in-app confirmation.

Server/database breadcrumbs open compact overviews. A database provides contextual SQL, whole-database Export, and table-targeted Import; table actions open the existing workflows. Empty databases offer Structure, Export and supported database object tabs; the limited SELECT/CSV importer still requires an existing target. Query results, errors, tree/filter expansion, page sizes and pagination remain independent per connection; each query tab owns its result. SQL execution still supports the documented limited SELECT subset only. Editing a generated Browse preview retains its mock query plan, including filters, offset and engine-specific quoting.

The default rows-per-page preference is persisted separately from each workspace's live page size. Saving a connection without connecting is supported; saved metadata remains allowlisted and secrets stay in memory. This follows the later explicit persistence requirements, which supersede the original memory-only connection guidance in AGENTS.md (that file is preserved).

## Database objects

The [database object audit and before/after coverage matrix](../database-objects.md) records verified workflows, engine compatibility, test results, and remaining backend work. Database Structure now lists tables/views and opens minimal table creation or view definitions. Procedures/functions live under database Routines; Triggers and MySQL/MariaDB Events have their own database tabs. PostgreSQL materialized views have explicit mock refresh and Structure indexes. Existing Browse/column editing is reused; views are read-only snapshots.

These are frontend mock management workflows. Routine calls validate inputs only; SQL bodies are not executed, triggers never fire, events are never scheduled and indexes are never built. Definitions are stored verbatim for future backend validation. Composite uniqueness is enforced by the shared mock mutation/validation boundary; arbitrary SQL/extension/operator-class support is not implied.
