# Developer Docs

These pages are for people building, testing or reviewing StoneQL. To download and use the app, see the [user guide](../guide/index.md).

Read root `AGENTS.md`, the relevant public documentation and local `.ref/workprogress.md` before changes. Inspect the active component/store before adding functionality. Preserve compact typography, themes, engine capability gates and working behavior.

## Start here

- [Contributor setup](./contributor-setup.md): Node 26/npm, `npm ci`, the browser demo, building the desktop app and the command list.
- [Browser demo](./browser-demo.md): what the browser build simulates and how it differs from the desktop app.
- [Verification](./verification.md): which checks to run, test commands, Docker fixtures and recorded results.
- [Documentation maintenance](./documentation.md): how this site is built and published.

## Architecture

The architecture pages include diagrams (rendered from Mermaid): [overview](../architecture/overview.md), [connection management](../architecture/connection-management.md), [import/export](../architecture/import-export.md) and the [roadmap](../architecture/roadmap.md). Also: [database adapters](../architecture/database-adapters.md), [API contract](../architecture/api-contract.md), [data integrity](../architecture/data-integrity.md), [security](../architecture/security.md), [deployment](../architecture/deployment.md) and [driver evaluation](../architecture/driver-evaluation.md). Sections not yet implemented are labelled Proposed.

## Implementation areas

[Rust foundation](./rust-foundation.md) · [Desktop host](./desktop-host.md) · [Connection lifecycle](./connection-lifecycle.md) · [Read-only SQL safety](./read-only-safety.md) · [Read-write SQL execution](./sql-execution.md) · [PostgreSQL and SQLite](./postgres-sqlite.md) · [SSH and credentials](./ssh-credentials.md) · [Resource profiles](./resource-profiles.md) · [Branding](./branding.md) · [Prototype reference](./prototype-reference.md) · [Demo database objects](../database-objects.md)

## Milestones and verification

Milestone status and release gates are tracked in the [roadmap](../architecture/roadmap.md) and the [verification](./verification.md) checklist; the [UX review ledger](../ux-review-remediation.md) records UI findings. The public beta v0.1.0-beta.1 does not complete milestones M002–M007 or M009, which remain open with recorded engineering gaps.

## Repository responsibilities

| Path | Responsibility |
| --- | --- |
| `apps/frontend/` | Vue UI, Pinia stores, mock services, schema validators, assets and tests |
| `apps/desktop/src-tauri/` | Tauri 2 desktop host and native IPC commands |
| `crates/` | Shared core, contracts and the host-independent runtime |
| `contracts/` | Generated v1 TypeScript contract and consumer checks |
| `docs/` | Canonical public Markdown |
| `.website/` | VitePress configuration, theme and link/build checks |
| `.ref/planning/`, `.ref/ms/` | Internal proposals, audit and milestone acceptance criteria (not published) |
| `.ref/workprogress.md` | Ignored local continuity notes; never commit |
| `tests/` | Workspace preservation and documentation regression checks |

The [Tauri desktop host](./desktop-host.md) connects live to MySQL, MariaDB and PostgreSQL (Direct and SSH), with [read-write SQL execution](./sql-execution.md), live CRUD and live import/export; the server implementation remains future work. Shared Vue code must remain browser-runnable without Tauri APIs.

## Conventions and validation

Use Vue Composition API, script setup, TypeScript, two-space indentation, PascalCase components and camelCase functions. Reuse CSS variables and schema-aware validation. No secrets in source, logs or ordinary local storage. Keep SQL values lossless and NULL/default distinct.

Run `npm test`, `npm run build`, `npm run test:browser` and `npm run docs:build` for changes affecting the workspace, and the Rust checks listed in [verification](./verification.md) for Rust changes. Add focused behavioral regression tests for defects. Report blocked checks explicitly. Use concise Conventional Commit messages when a commit is authorized; do not push or deploy without instruction. Update public behavior documentation and local progress notes after meaningful work.

## Continuous integration

GitHub Actions runs `.github/workflows/rust.yml` ("Rust foundation") when a release is published, or when started manually, on **macOS 15** and **Windows Server 2025** (`windows-2025`) runners: frontend build, vendored SQLx provenance, Rust format/build/Clippy/tests, crate boundaries, contract drift, workspace tests, `npm test` and the release-graph advisory audit (`audit:release`; the full-graph `cargo audit` is informational). It does not run Docker fixtures, Playwright browser scenarios, the native smoke or bundling. On Windows it also runs the real-database tests through the [remote fixture host](./verification.md#windows-remote-fixture-host) and, on `release/*` branches, builds and smoke-tests the NSIS installer. Linux is not in the matrix; its evidence comes from a dedicated build server. `.github/workflows/docs.yml` builds this site on documentation pull requests and publishes it to GitHub Pages only on a manual dispatch with deployment selected.
