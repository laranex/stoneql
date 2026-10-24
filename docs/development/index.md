# Development

Read root `AGENTS.md`, relevant public documentation and local `.ref/workprogress.md` before changes. Inspect the active component/store before adding functionality. Preserve compact typography, themes, engine capability gates and working behavior.

## Repository responsibilities

| Path | Responsibility |
| --- | --- |
| `apps/frontend/` | Vue UI, Pinia stores, mock services, schema validators, assets and tests |
| `docs/` | Canonical public Markdown |
| `.website/` | VitePress configuration, theme and link/build checks |
| `.ref/planning/`, `.ref/ms/` | Internal proposals, audit and milestone acceptance criteria |
| `.ref/workprogress.md` | Ignored local continuity notes; never commit |
| `tests/` | Workspace preservation and documentation regression checks |

Desktop, server and Rust crate paths are reserved only. Do not scaffold them before approved milestones. Shared Vue code must remain browser-runnable without Tauri APIs.

## Conventions and validation

Use Vue Composition API, script setup, TypeScript, two-space indentation, PascalCase components and camelCase functions. Reuse CSS variables and schema-aware validation. No secrets in source, logs or ordinary local storage. Keep SQL values lossless and NULL/default distinct.

Run `npm test`, `npm run build`, `npm run test:browser`, and `npm run docs:build` for changes affecting the workspace. Add focused behavioral regression tests for defects. Report blocked checks explicitly. Use concise Conventional Commit messages when a commit is authorized; do not push or deploy without instruction. Update public behavior documentation and local progress notes after meaningful work.

See [verification](./verification.md) and [documentation maintenance](./documentation.md).
