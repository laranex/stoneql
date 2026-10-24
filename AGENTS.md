# Repository Guidelines

## Purpose and boundaries

dbAdmin is a compact, phpMyAdmin-inspired database client prototype. Preserve familiarity, readable 14px controls, Light/Dark themes, accessibility, and independent connection workspaces. Current database operations are session mocks. Rust/Tauri/Axum integration is **planned, not implemented or approved**. Do not create backend code until its milestone and design decisions receive explicit human approval.

## Start every session

1. Read this file, `.ref/workprogress.md` if present, and relevant `docs/` pages.
2. Read the current specification in `.ref/ms/` and applicable `.ref/planning/` decisions.
3. Inspect existing implementations, tests, and Git status before editing. Preserve uncommitted work. Do not initialize or reconstruct Git history without instruction.
4. Prefer the smallest complete change. Do not duplicate working screens, validators, documentation, or transport logic.

## Repository map

- `apps/frontend/`: existing Vue 3/TypeScript/Vite/Tailwind/Pinia/Lucide/CodeMirror UI, mock services, models, original assets and tests.
- `docs/`: canonical public Markdown, including implementation status, user guides and proposed architecture.
- `.website/`: VitePress configuration, theme, homepage presentation, assets and documentation checks; reads `docs/` directly.
- `.ref/planning/`: internal audits, proposals, decisions and investigations.
- `.ref/ms/`: milestone scope, dependencies, acceptance criteria, risks and status.
- `.ref/workprogress.md`: ignored, local-only continuity notes. Never stage or commit it.
- `.github/workflows/`: verification and manually gated documentation publication.
- `tests/`: workspace/documentation regression checks.

`apps/desktop`, `apps/server` and `crates` are reserved architectural paths, not current applications. Do not scaffold placeholders.

## Architecture and coding

Use Composition API and `<script setup>`, TypeScript, two-space indentation, camelCase functions and PascalCase components. Follow existing patterns and CSS variables. Keep UI, store, schema validation, mock mutations and transport contracts separate. Vue must remain browser-runnable without Tauri imports. Rich autocomplete helpers are not all integrated; verify actual editor behavior before making claims.

Future Rust should use rustfmt, clippy, explicit errors, snake_case modules and focused crates. Shared domain/application services must not import Tauri or Axum. Tauri IPC and HTTP adapters translate the same versioned contracts; database dialect adapters and connection transports remain distinct. Use capabilities rather than pretending unsupported engine features work. Direct/SSH are intended first; Teleport and SSM remain future proposals.

## Security and data integrity

Never store/log/commit database passwords, private keys, passphrases, access tokens or raw sensitive query data. Current local storage may hold only allowlisted non-secret profile/group/theme/grid preferences. Keep mock records and definitions session-only and simulations clearly labelled. Preserve BIGINT/DECIMAL and raw temporal values. Centralize constraints, validate before mutation, reject invalid batches atomically, and preserve NULL/default distinctions. Confirm destructive actions, retain PROD safeguards and warn before discarding edits. Backend/database rules will remain authoritative. Never weaken host or browser sandbox security to make checks pass.

## Verify and document

Use Node **26** and the root npm workspaces/lockfile. Run `npm test`, `npm run build`, `npm run test:browser` for UI changes, and `npm run docs:build` for documentation/site changes. Root `dev`, `preview`, `docs:dev` and `docs:preview` delegate to the owning workspace. Add regression tests for meaningful defects. Verify both themes, keyboard/dialog behavior and engine capability differences where affected. Browser startup failure or test collection is not a passing browser run. Update public docs when behavior changes; label proposals, simulations, missing features and unverified checks accurately.

## Planning, progress and Git

Update milestone status only against executed acceptance criteria. Keep unresolved choices **Proposed / awaiting approval**, never Approved by inference. Record decisions once and link related plans. Read/update `.ref/workprogress.md` after meaningful progress and before ending: current task, facts completed, pending work, blockers, exact checks and next steps. Do not duplicate milestone specifications or include secrets.

Use concise Conventional Commit titles when commits are requested; no earlier commit convention is established. Include validation and limitations in review descriptions. Do not commit, push, deploy, discard work or change remotes without explicit instruction. Never include local progress files or generated artifacts in a commit.
