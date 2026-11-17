# StoneQL

**Old habits. New engine.**

**StoneAge SQL Admin** · Formerly dbAdmin.

A compact, phpMyAdmin-inspired database client. The **desktop app (macOS, public beta)** connects live to MySQL, MariaDB and PostgreSQL servers; the same Vue interface also runs in a browser as a **demo** with sample data and simulated operations.

## Download

**[StoneQL v0.1.0-beta.1](https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1)** (pre-release) · [all releases](https://github.com/stoneAce/stoneql/releases)

- `StoneQL-0.1.0-beta-970ade2.dmg` for **macOS on Apple Silicon (arm64) only**, plus `StoneQL-0.1.0-beta-970ade2.dmg.sha256`. Intel Macs, Windows and Linux builds are not released.
- Verified on macOS 27.0.1 only.
- The app is ad-hoc signed and **not notarized** (no Developer ID yet), so macOS Gatekeeper blocks the first launch of the downloaded copy. Verify the download with `shasum -a 256 -c StoneQL-0.1.0-beta-970ade2.dmg.sha256`, then follow the [install guide](docs/getting-started/index.md) for installing, the first launch, the Keychain prompt, updating and uninstalling.

## What the beta does

- Live MySQL, MariaDB and PostgreSQL over Direct or SSH tunnel, with verified TLS (TLS is optional for MySQL/MariaDB on this computer); passwords can be remembered in the macOS Keychain.
- Read-write by default (MySQL/MariaDB Direct and SSH, PostgreSQL Direct), with a saved **Read-only connection** option; PostgreSQL over SSH is read-only. PROD-tagged connections ask for confirmation.
- Browse with server-side paging, sort and filter; insert, edit and delete rows of tables with a primary key; SQL scripts and transactions.
- MySQL/MariaDB: the Explorer lists every database the account can see, switches between them and creates databases.
- Structure: add, change, rename and drop columns, and add or drop secondary indexes (MySQL, MariaDB, PostgreSQL), with an SQL preview and confirmation.
- Export a table, chosen tables or a whole database as SQL, CSV (one table) or JSON, optionally GZIP; import CSV into a table and SQL files (MySQL/MariaDB).

Not yet available for live connections: Search, table Operations (rename, copy, truncate, drop), bulk Copy, managing views/routines/triggers/events, the database Structure overview, CSV import at database level, PostgreSQL SQL import, switching databases on PostgreSQL and exporting views/routines/triggers/events. See the [workspace guide](docs/user-guide/index.md#not-yet-available-in-the-desktop-app).

## Status

StoneQL is beta software: keep backups and verify important changes on the server. Milestones M002–M007 and M009 remain open with recorded engineering gaps; Developer ID signing and notarization are deferred. Browser-demo operations are simulations and never touch a real database. No Axum server exists yet.

## Run

Requires Node.js **26** and npm. From the repository root:

```sh
npm ci
npm run dev
npm test
npm run build
npx playwright install chromium
npm run test:browser
npm run docs:dev
npm run docs:build
npm run docs:preview
```

`npm run preview` serves the frontend production bundle. See [setup](docs/getting-started/index.md) for details and [verification](docs/development/verification.md) for executed checks and environment blockers.

## Repository

- `apps/frontend/`: shared Vue 3/TypeScript/Vite/Pinia/Tailwind/Lucide/CodeMirror frontend, mocks and tests.
- `docs/`: canonical public guides and architecture proposals.
- `.website/`: VitePress configuration/theme; renders root docs directly.
- `.ref/planning/`, `.ref/ms/`: internal audit, decisions and milestone specifications.
- `.ref/workprogress.md`: ignored local continuity notes; never commit.
- `.github/workflows/docs.yml`: manually gated GitHub Pages workflow for the [documentation site](https://stoneace.github.io/stoneql/).

Building the desktop app from source is documented in [Desktop development](docs/development/desktop-host.md); the server path remains reserved. Shared Rust crates (core, contracts, runtime) and an isolated driver lab exist; see [Rust development](docs/development/rust-foundation.md) and [driver evaluation](docs/architecture/driver-evaluation.md). Read [AGENTS.md](AGENTS.md) before changes. The frontend remains browser-runnable without native APIs.

## Documentation

[Workspace guide](docs/user-guide/index.md) · [Connections](docs/user-guide/connections.md) · [SQL editor](docs/user-guide/sql-editor.md) · [Import/export](docs/user-guide/import-export.md) · [Database objects](docs/database-objects.md) · [Architecture](docs/architecture/overview.md) · [Roadmap](docs/architecture/roadmap.md) · [Development](docs/development/index.md)

The [preserved prototype reference](docs/development/prototype-reference.md) retains the previous detailed workflow documentation. Advanced autocomplete helpers are not all integrated; see the SQL guide rather than assuming helper existence proves support.

Production signing, notarization and server decisions remain gated; no commit certifies unverified capabilities. The documentation site is published manually from `docs/`.
