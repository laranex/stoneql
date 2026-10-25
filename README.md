# StoneQL

**Old habits. New engine.**

**StoneAge SQL Admin** · Formerly dbAdmin.

A compact phpMyAdmin-inspired Vue database client prototype. Browse, edit, SQL, database objects, connections and transfers use realistic session-only mock data. No production database connectivity or Axum backend is implemented. A Tauri development host preserves session mocks and offers explicit Direct MySQL/MariaDB live read-only SQL profiles. The approved M001 Rust foundation and isolated SQLx evaluation do not replace frontend mocks.

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
- `.github/workflows/docs.yml`: prepared manually gated GitHub Pages workflow; not deployed.

The desktop development host is documented in [Desktop development](docs/development/desktop-host.md); the server path remains reserved. Shared Rust foundation crates and an isolated driver lab now exist; see [Rust development](docs/development/rust-foundation.md) and [driver evaluation](docs/architecture/driver-evaluation.md). Read [AGENTS.md](AGENTS.md) before changes. The frontend remains browser-runnable without native APIs.

## Documentation

[Workspace guide](docs/user-guide/index.md) · [Connections](docs/user-guide/connections.md) · [SQL editor](docs/user-guide/sql-editor.md) · [Import/export](docs/user-guide/import-export.md) · [Database objects](docs/database-objects.md) · [Architecture](docs/architecture/overview.md) · [Roadmap](docs/architecture/roadmap.md) · [Development](docs/development/index.md)

The [preserved prototype reference](docs/development/prototype-reference.md) retains the previous detailed workflow documentation. Advanced autocomplete helpers are not all integrated; see the SQL guide rather than assuming helper existence proves support.

The revised roadmap and local milestone continuation are authorized. Detailed production driver/security/release decisions remain gated; no commit certifies unverified capabilities. Documentation publishing requires an explicit manual deployment; no site has been published.
