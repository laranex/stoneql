# dbAdmin

**Database management, made familiar.**

A compact phpMyAdmin-inspired Vue database client prototype. Browse, edit, SQL, database objects, connections and transfers use realistic session-only mock data. No real connectivity, Rust, Tauri or Axum backend is implemented.

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

Desktop/server/crates paths are reserved, not scaffolded. Read [AGENTS.md](AGENTS.md) before changes. The frontend remains browser-runnable without native APIs.

## Documentation

[Workspace guide](docs/user-guide/index.md) · [Connections](docs/user-guide/connections.md) · [SQL editor](docs/user-guide/sql-editor.md) · [Import/export](docs/user-guide/import-export.md) · [Database objects](docs/database-objects.md) · [Architecture](docs/architecture/overview.md) · [Roadmap](docs/architecture/roadmap.md) · [Development](docs/development/index.md)

The [preserved prototype reference](docs/development/prototype-reference.md) retains the previous detailed workflow documentation. Advanced autocomplete helpers are not all integrated; see the SQL guide rather than assuming helper existence proves support.

Architecture is **proposed, awaiting approval**. No commit implies backend design approval. Documentation publishing requires an explicit manual deployment; no site has been published.
