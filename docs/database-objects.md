# Database object management

> **Scope:** this page describes the **browser demo** (sample data, simulated operations). In the desktop app, views can be browsed and columns and indexes of base tables can be changed on read-write connections (see [Change table structure](./guide/browse-and-edit.md#change-table-structure)), but managing views, routines, triggers and events is not available for live connections yet; see [not available yet](./guide/beta.md#not-available-yet). The [browser demo](./development/browser-demo.md) page summarises all demo differences.

Repository layout note: frontend source and tests now live under `apps/frontend/`. Root npm commands delegate to that workspace. The [migration verification report](./development/verification.md) distinguishes current executed checks from blocked browser workflows.

## Before/after coverage matrix

Checked the active App routes/handlers, DatabaseOverview, workspace store, sample schemas, column/mutation/validation/export services, and tests on Node 26.11.1. Baseline: 88 tests pass. No Git metadata exists in this workspace. The browser suite was previously blocked by sandbox port/process restrictions; this audit does not claim pixel verification.

“Working” below means frontend mock behavior, never database-server support.

| Feature | Before: verified status | Gap / required change | Priority | After |
| --- | --- | --- | --- | --- |
| Table Browse/Structure/CRUD | Working mock, regression-tested | Preserve; guard other object kinds | P0 | Working frontend mock; existing routes preserved; view writes now guarded |
| Table creation | Missing from database Structure | Add a minimal initial-table path; reuse existing Add Column editor | P1 | Simulated Create table with generated integer id; existing column workflow reused |
| Database Structure | Partial: table list and actions | Object type, view actions and creation | P1 | Working mock table/view list, object types, definitions and creation actions |
| Views | Type-only placeholder; inaccessible | Definitions, create/edit/drop, browsing and read-only safeguards | P1 | Simulated create/edit/rename/drop, SQL definition, read-only Browse and explicit snapshot refresh |
| Routines | Missing; export options only | Database Routines for procedures/functions, parameters, definitions, simulated calls and deletion | P1 | Simulated procedure/function CRUD with ordered parameters; validated mock calls, no body execution/results |
| Triggers | Missing; export options only | Database listing, table/event/timing definition, create/edit/drop; no automatic execution claim | P1 | Simulated database-level create/edit/drop, target/timing/event checks; never fire |
| MySQL/MariaDB Events | Missing; export options only | Schedule/status/body management; no real scheduler | P2 | Simulated create/edit/drop and enabled metadata with recurring schedules; never scheduled |
| PostgreSQL materialized views | Missing | Definition, browsing, explicit simulated refresh, create/edit/drop | P2 | Simulated create/edit/drop, read-only snapshots, explicit refresh and indexes; PG-only |
| Primary/Unique/Normal indexes | Partial: column flags and read-only listing; creation through column dialog only | Named table index model, create/edit/drop and constraint consistency | P0 | Working mock named-index CRUD inside Table → Structure; dependency guards |
| Composite indexes | Missing; column dialog explicitly limits PK to one column | Ordered key parts, ASC/DESC and composite constraint checks | P1 | Working ordered key parts/directions and atomic tuple uniqueness, integrated with record/column editing |
| Fulltext/Spatial indexes | Missing | MySQL/MariaDB capability/type validation | P2 | MySQL/MariaDB mock configuration; type/nullability checks; no index construction/search acceleration |
| PostgreSQL index methods | Missing; UI labels everything BTREE | B-tree, Hash, GiST, SP-GiST, GIN, BRIN with restrictions | P2 | Mock B-tree/Hash/GiST/SP-GiST/GIN/BRIN definitions with restricted default-operator-class checks |
| Export object counts | UI-only; summaries hardcode zero | Count configured mock objects accurately; retain simulation boundary | P2 | Simulated summary now counts configured views, triggers, routines and events; no SQL dumping |

## Audit evidence before changes

- `App.vue` exposes database Structure/SQL/Export/Import only. Index rows derive from `primary/unique/indexed` column flags and have no action handlers.
- `Database` stores only tables; `Table.kind` allows a view but every existing action treats it as a writable table. Sample data contains no views, routines, triggers or scheduled events.
- `columnDefinition.ts` supports single-column key flags; `valueValidation.ts` enforces only individual keys. No ordered index collection exists.
- `exportService.ts` reports zero object counts regardless of options. Existing tests cover export capability flags, not object management.
- Existing table workflows and the shared focus/confirmation directive are reusable. No duplicate Browse, SQL editor, or column editor is needed.

## Implementation and verification

The audit was recorded before implementation. Components were traced through their actual App handlers; unused type placeholders and export flags were not counted as working workflows. Existing table views, column dialogs, CRUD validators, confirmation shell, and SQL editor were retained. The derived read-only index listing was replaced in-place by the active IndexManager.

- `DatabaseObjects.vue`: one database-level list/dialog host for Routines, Triggers, Events and view/table definitions. Procedures/functions share Routines. It uses the existing focus/unsaved confirmation behavior; destructive requests use the existing PROD acknowledgement.
- `IndexManager.vue`: only inside Table → Structure (also available for PostgreSQL materialized views). Supports named key parts, reordering, ASC/DESC where applicable and inline validation.
- `indexes.ts`: legacy flag migration, index CRUD, tuple constraints and dependency checks. Column renames update index key parts atomically; incompatible type changes reject without mutation. Composite uniqueness never implies uniqueness of each individual column. Single-column references retain their required unique key. Generated-column index requirements distinguish engines.
- `databaseObjects.ts`: definition validation, mock schema changes, limited snapshot evaluation and input-only routine-call simulation. SQL bodies/definitions remain raw text.
- `objectCapabilities.ts` and `types/objects.ts`: reusable engine and metadata contracts. `Database.views/routines/triggers/events` and `Table.indexes` are optional for backward compatibility. Views use the existing Browse/SQL paths and schema adapter; every record mutation rejects view writes at the service boundary.
- `data/objects.ts`: relationally relevant example definitions, including active customers, a PostgreSQL materialized snapshot, routines, a validation trigger, and a disabled MySQL/MariaDB cleanup event. Products include geometry or PostgreSQL search/JSON metadata for index demonstrations.

## Engine compatibility

This matrix describes **available frontend mock workflows**, not real server execution.

| Workflow | MySQL / MariaDB | PostgreSQL | SQLite |
| --- | --- | --- | --- |
| Tables and ordinary views | Yes; views read-only here | Yes; views read-only here | Yes; views read-only here |
| Procedures/functions under Routines | SQL definitions; input-only function parameters | SQL/PLpgSQL definitions and parameter metadata | Not offered; native application UDF registration is outside scope |
| Triggers | BEFORE/AFTER on tables | BEFORE/AFTER on tables; INSTEAD OF on views; function clause stored | BEFORE/AFTER on tables; INSTEAD OF on views |
| Scheduled Events | Recurring EVERY integer unit; enabled/disabled metadata | Not offered | Not offered |
| Materialized views | Not offered | Definition/snapshot/refresh/index configuration | Not offered |
| Primary/Unique/Normal/Composite | B-tree, ASC/DESC | B-tree, ASC/DESC | B-tree; physical PK changes would need backend table rebuild |
| FULLTEXT / SPATIAL index kinds | Text columns / one NOT NULL geometry column | Use GIN/GiST-style methods where the mock supports a default operator class | Not offered as ordinary index kinds; FTS5/R-tree virtual tables are outside scope |
| Other index methods | InnoDB-oriented mock; no MEMORY hash workflow | Hash, GiST, SP-GiST, GIN, BRIN; no DESC control for non-B-tree | No alternate ordinary-index methods |

Capabilities are version/storage-engine approximations for the sample connections, not server discovery. Reference terminology was checked against [MySQL indexes](https://dev.mysql.com/doc/refman/8.0/en/create-index.html), [MySQL events](https://dev.mysql.com/doc/refman/8.0/en/create-event.html), [MariaDB indexes](https://mariadb.com/docs/server/reference/sql-statements/data-definition/create/create-index), [PostgreSQL 16 index methods](https://www.postgresql.org/docs/16/indexes-types.html), [PostgreSQL materialized views](https://www.postgresql.org/docs/16/sql-creatematerializedview.html), [PostgreSQL triggers](https://www.postgresql.org/docs/16/sql-createtrigger.html), and [SQLite SQL](https://www.sqlite.org/lang.html). No third-party source code or translation catalogs were incorporated.

## Reproducible workflows

1. Click the database breadcrumb → Structure. Create table opens a minimal generated-id schema; continue with the existing Add Column dialog. Create view stores a SELECT definition. Open a view → Browse → Refresh to evaluate the supported mock SELECT subset; Structure shows its formatted definition and edit/drop actions.
2. Database → Routines → Add routine. Choose Procedure or Function, add/reorder parameter names/types/modes, provide a return type for functions and a body. Save/Edit/Drop update the active mock database. Execute (mock) validates supported primitive input types and NULL choices, then explicitly states that no body, return or OUT values were executed/computed.
3. Database → Triggers or Events. Create/Edit/Drop metadata. Invalid target/timing or event schedules remain in the dialog as errors. Enabled event metadata never starts a scheduler.
4. Table → Structure → Indexes → Add index. Name/type/method plus ordered key parts; use arrows to reorder. Editing restores the full definition. Drop requires confirmation. Existing duplicates, nullable PKs, duplicate columns, invalid engine/type/method combinations, FK dependencies and generated-key conflicts prevent saving.
5. PostgreSQL connection → database Structure → customer_snapshot. Browse/Structure shows a read-only materialized snapshot; Refresh recomputes it from the limited evaluator. New materialized views are available in Create view. Indexes remain in Structure.

## Verification results

- Node 26.11.1; baseline 88 tests passed before edits.
- The first expanded run passed 96 of 97 tests and exposed a legacy-index deletion identity bug. Matching names fixed it; subsequent full runs passed 102, 106, 107, 108, 110, 112, 114 and 115 tests. Existing database-tab expectations were extended because the new object tabs are now genuinely reachable.
- Previous implementation checkpoint: 115 tests passed with `npm test`; the production build passed. The current pre-Rust stabilization results are recorded below.
- Pure tests cover all engines, composite tuples/NULL/precision, atomic rejection, named-index CRUD/order, FK/generated-key dependencies, method/type restrictions, view lifecycle/read-only behavior, routine parameters and calls, triggers/events and object export counts.
- Actual App host-renderer tests cover both themes for index CRUD/reordering, function CRUD/invalid and valid simulated calls, trigger/event CRUD, view creation/definition editing/rename/Browse, PostgreSQL materialized refresh/method controls, SQLite hidden tabs and table creation → existing column dialog. These test active handlers and reactive state, not browser pixel geometry.
- `npm run test:browser` was attempted; Vite cannot bind `0.0.0.0:5173` (`EPERM`) in this environment. `playwright test --list` successfully discovers 17 scenarios across seven files. New `apps/frontend/tests/browser/objects.spec.ts` scenarios and screenshots are authored but **not executed**. No browser, screenshot, native-webview or screen-reader pass is claimed.

## Pre-Rust stabilization — 2026-10-24

**Browser sign-off is blocked: 0 of 17 browser scenarios executed.** All 128 non-browser automated tests and the production build pass on Node 26.11.1. This is not a visual or browser interaction pass, and the prototype has not received full browser sign-off for backend integration.

### Browser blocker investigation

Environment: macOS 27.0.1 / Darwin 27 arm64, Node 26.11.1, installed Playwright 1.64.1. Chromium revision 1248 (both full Chrome for Testing and headless shell) and WebKit revision 2370 are installed. Firefox is not installed. The installed Playwright platform mapper maps this host to mac26-arm64; compatibility of the newer host cannot be fully ruled out for the browser aborts.

| Check actually executed | Result / conclusion |
| --- | --- |
| `npm run test:browser` | Exits 1 during web-server setup: `listen EPERM: operation not permitted 0.0.0.0:5173`. No page or test worker reaches an application workflow. |
| Bare Node TCP server, repository and OS temporary directory | `listen` returns `EPERM` on `127.0.0.1:5173`, `127.0.0.1:0`, `::1:0` and `0.0.0.0:0` in both directories. Port 0 requests a free ephemeral port. |
| Chromium headless shell, sandbox explicitly enabled | Aborts before a page can be created: `EXC_BREAKPOINT / SIGTRAP`. No StoneQL content was loaded. |
| Full Chromium, `channel: 'chromium'`, sandbox enabled | Aborts with `SIGABRT`. Its matching macOS crash report has `_RegisterApplication` / `___RegisterApplication_block_invoke` / `TransformProcessType` on the faulting startup stack. |
| Installed WebKit | Aborts with exit 134 / `SIGABRT`; the matching crash report likewise terminates during macOS application registration, before a page exists. |
| Docker fallback, read-only `docker version` probe | Permission denied connecting to the existing Docker Unix socket. No permission or daemon configuration was changed. |
| `playwright test --list` | Successfully discovers the same 17 scenarios across seven files. Collection is not execution. |

The **root of the EPERM server failure is the execution context's socket-listening restriction**, not a Vite-specific error, unavailable fixed port, missing browser download, public-interface binding alone, or repository path. The bare Node reproductions establish this independently of StoneQL and Playwright. Switching to loopback or another port does not fix it here.

There is a separate browser-startup blocker: installed browser processes cannot establish a usable session. Full Chromium and WebKit crash during native macOS application registration; the headless shell also aborts. These observations establish the failure phase, but do not prove the precise private macOS service/policy responsible. No application defect is inferred from these startup crashes. No accessible alternate approved browser runner is available in this session.

No sandbox was disabled, host security setting changed, Docker permission loosened, or privileged browser launched. `playwright.config.ts` now explicitly requires `chromiumSandbox: true` because Playwright's installed launch-option defaults disable it. A regression test guards that requirement and rejects explicit security-bypass arguments. The configuration change does **not** claim to resolve the host restrictions.

To reproduce the bind check independently:

```sh
node --input-type=module -e "import net from 'node:net'; const server=net.createServer(); server.on('error',e=>{console.error(e.code,e.syscall);process.exitCode=1}); server.listen(0,'127.0.0.1',()=>server.close());"
```

To run browser verification, use an approved desktop/CI runner that permits ordinary local TCP listening and sandboxed Chromium startup:

```sh
# Node 26; install browser/dependencies using the approved host's normal setup.
npm ci
npx playwright install chromium
npm test
npm run build
npm run test:browser
```

A runner that cannot support the browser sandbox must fail; do not add `--no-sandbox`, disable web security, alter macOS security controls, or loosen the Docker socket. The current Vite server command still requires port 5173. Browser download/reinstallation would not resolve the independently reproduced listening denial.

### Verified defects fixed

Each defect was reproduced with the actual Vue App handlers before its source fix. Existing interfaces, typography, colors and layouts were preserved.

| Defect | Verified cause | Targeted fix / regression coverage |
| --- | --- | --- |
| Invalid routine retry displayed stale success | `DatabaseObjects.call()` set an error without clearing the previous simulation notice. | Clear previous call feedback before validating a new attempt. Light/Dark active-component regressions verify success → invalid input shows only the current error. |
| Non-ordering index types retained an invalid disabled DESC value | Type/method changes disabled the direction control but retained its previous B-tree direction; validation then rejected an uneditable value. | Reset draft directions to ASC when selecting a non-ordering kind/method. Light/Dark handler tests cover Fulltext and PostgreSQL Hash; stored rows are untouched. The existing object browser scenarios include this transition. |
| Dropping the last table stranded table context | The store always selected Browse after drop, even when no table remained. | Select database Structure when the last table is removed; preserve Browse when another table remains. Light/Dark actual Drop confirmation tests verify Create table stays accessible. The two remediation browser scenarios now exercise the analytics database's last-table drop. |
| Table copy bypassed relation/index namespaces | The Operations handler cloned/pushed a table directly, permitting collisions with views and duplicate PostgreSQL/SQLite schema-scoped index names. | Move the existing Copy operation behind a store/service boundary. Validate against tables and views, allocate non-colliding schema-scoped index names, validate copied indexes before insertion and preserve raw values/deep copies. Pure and active-component tests verify all four engines, structure-only copies, exact BIGINT/DECIMAL strings and atomic name-conflict rejection. |

### Executed functional verification

- Baseline: 115 tests passed. Targeted Light/Dark regressions reproduced the first three defects before their fixes; the table-copy lifecycle checks exposed the namespace regression. An initial test selector used “Drop the table” instead of the rendered “Drop table”; it was corrected before confirming the actual state failure. SQLite's TEXT column correctly has no Length input, and its lifecycle test now explicitly verifies that capability instead of assuming VARCHAR controls.
- Final: **128 tests passed, zero failed**, via `npm test`. `npm run build` passes; Vite retains its existing non-blocking >500 kB bundle advisory. No runtime dependency was added.
- The expanded actual-App lifecycle matrix executes MySQL, MariaDB, PostgreSQL and SQLite in **both Light and Dark state**. It covers table creation/Add Column, record Insert/Edit/Browse, table rename/copy/truncate/drop, view definition creation/edit/refresh/drop, named indexes, routines/validated simulated calls, trigger CRUD, MySQL/MariaDB event CRUD/enabled metadata, and PostgreSQL materialized-view creation/edit/refresh/index/drop. Unsupported tabs and SQLite type-parameter differences are asserted.
- Existing automated checks for constraints, exact values, ENUM/SET, export simulation, connection workspaces, themes, contrast tokens, grid navigation, notifications, column dialogs and focus/keyboard directive behavior still pass.
- These tests use Vue's host renderer or pure services. They do **not** exercise a browser DOM, native select behavior, actual focus/pointer dispatch, CodeMirror's browser rendering, layout measurement, horizontal scrolling or screenshots.

### Exact browser scenarios still unverified

All rows below are blocked before application execution; none passed, and discovery is not counted as a pass. Existing object/remediation scenarios were extended with the new regression paths without increasing the scenario count.

| File | Scenario(s), all unexecuted |
| --- | --- |
| `appearance.spec.ts` | Light; Dark — readable typography, compact workspace, persistent footer and dialogs. |
| `column-dialog.spec.ts` | Light; Dark — active Add/Edit Column definitions and physical positioning. |
| `grid.spec.ts` | Light; Dark — aligned borders, hover grip, drag resize, auto-fit, reset, persistence and horizontal scrolling. |
| `objects.spec.ts` | Light; Dark — object navigation/definitions, routine-call retry, view Browse, composite/Fulltext indexes, copy-name conflicts and PostgreSQL index-name isolation. |
| `remediation.spec.ts` | Light; Dark — RESTRICT confirmation, paging/workspace restoration, bounded scrolling, Quick CSV recovery and last-table drop. |
| `workflows.spec.ts` | Quick export/persistence; Custom database/format validation; export cancellation/failure/retry; theme toggle/PostgreSQL capabilities; CRUD/independent connections. |
| `workspace.spec.ts` | Light; Dark — compact workspace, independent notices, preview preference and fixed-footer behavior at smaller widths. |

Also still unverified in a real browser: comprehensive MariaDB/SQLite object dialogs, complete procedure/function/trigger/event/materialized-view CRUD and engine-specific index controls beyond the existing browser smoke scenarios. The expanded host-renderer matrix covers their handlers only. Actual focus restoration/trapping, Escape, keyboard/pointer resizing, native form validation, long-identifier/text overflow, responsive pixel geometry, Light/Dark screenshots and screen readers require a functioning browser environment. No Rust/backend integration or real database connection was started.

## Known limits and backend integration

- All metadata and records are session mocks. No database connection, DDL/DML, real index construction, routine execution, trigger firing, scheduled event, file dump or backup occurs.
- Views are explicit read-only snapshots, even when an engine could provide updatable views. Snapshot refresh uses the existing simple SELECT evaluator: no JOIN/CTE/aggregation, nested-view evaluation or arbitrary SQL. Unsupported SQL reports an error and leaves the previous snapshot intact. Definitions may still be stored verbatim for future backend validation. Changing a definition clears the snapshot; materialized-view edits also clear indexes because a backend would need recreation.
- Indexes support ordinary columns only: no expression/partial/covering indexes, prefix lengths, included columns, collations/operator-class configuration, NULLS NOT DISTINCT, partitioned/concurrent builds, PostGIS extension discovery or vector indexes. PG default-operator-class checks are a restricted allowlist, not complete compatibility validation. Geometric values themselves are not parsed. SQL/full-text/spatial query execution is not added.
- Composite keys enforce complete tuples; foreign keys remain single-column. Collation, engine coercion, deferred constraints and server-specific NULL/rowid nuances are not fully emulated. SQLite PK edits are mock schema updates, not a real rebuild. The column dialog keeps its single-column key shortcut; composite/specialized index membership is managed in Indexes.
- Routine execution is input-validation-only. Unknown/opaque parameter types cannot be simulated. Return and OUT values, overload resolution, definer/security/privilege options, language extensions and server SQL syntax checks are not implemented. Object names use the existing simple ASCII identifier convention and are database-unique in the mock; PostgreSQL/SQLite named-index collisions are checked at schema/database scope while MySQL/MariaDB index names remain table-local and primary keys use the fixed PRIMARY name; schema-qualified names and per-table trigger namespaces are not modeled.
- Trigger bodies are not parsed or dependency-resolved; PostgreSQL stores a function invocation clause. Events support recurring EVERY schedules, not AT/STARTS/ENDS, timezone or scheduler status discovery.
- Table renames update explicit FK/trigger target metadata and view renames update explicit trigger targets, not arbitrary SQL body text. Dependencies embedded in SQL definitions are not rewritten or cascaded; a later snapshot refresh can fail clearly. Table/view drop removes associated trigger metadata, but arbitrary routine/view dependency graphs are not modeled.
- A real Rust transport must discover server/version/storage/extension capabilities and metadata, preserve stable object identities, validate/quote DDL, handle permissions/dependencies/transactions/rebuilds, execute real routines, return results/errors and authoritative schemas, and invalidate caches. Vue components must remain consumers of that interface; frontend checks remain UX only.
