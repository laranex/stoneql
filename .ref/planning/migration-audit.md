# Repository migration audit

Status: migration implemented; final verification recorded in docs/development/verification.md. Scope is file organization, documentation and proposed architecture only.

## Observed baseline

- Root Vite/Vue app, one large active App.vue, 13 reusable components, Pinia workspace/export stores; all mutations are mocked. Existing validated object/index workflows must remain intact.
- SQL editor uses CodeMirror's dialect/schema SQL completion. `sql/context.ts` and `sql/schema.ts` contain additional context/cache scaffolding but are not wired into the active editor. No production-quality advanced autocomplete or its promised documentation/tests may be assumed.
- Connection grouping/profile metadata persistence is allowlisted; secrets remain memory-only. Database mock data is session-only. Export is a cancellable simulated job; import is raw CSV into mock state or non-executing SQL simulation.
- Database objects have typed mock definitions, engine capabilities, read-only snapshots, named/composite index checks and routine-call input simulation; no stored SQL executes.
- Baseline frontend: 128 Node tests, 17 discoverable browser scenarios. Browser execution has been blocked by process/socket restrictions. At audit start no Rust, Tauri, server, CI, planning, website or Git metadata existed. An empty Git repository subsequently became available; no remote or existing commits were found.
- Vite has no custom aliases. TypeScript includes source and local Vite config. Tailwind is loaded by the Vite plugin and CSS import. Assets and HTML use normal Vite public/source roots.

## Affected paths

Move source, tests, public assets, HTML, Vite/TypeScript/Playwright configs into `apps/frontend/` together. Their existing relative imports and URL-based test fixtures should remain byte-identical. Keep one root lockfile and hoisted npm workspace dependency resolution. Root scripts delegate with npm workspace selection, preserving application working directory. Retain generated output under the application, ignored.

Public docs stay at root. Existing audit reports are preserved and current source path references updated. VitePress uses `.website` as project/config root and an explicit root `docs` source directory. No Markdown mirrors or backend placeholder applications.

## Preservation evidence

A SHA-256 manifest and complete file backup were captured before moving files at `/private/tmp/dbadmin-monorepo-before`. `frontend-preservation.json` stores only source/config/test/asset hashes and the mapping, not duplicate source. No existing work was discarded. The subsequently available empty Git repository permits index/untracked and ignore checks. Git was not initialized or history reconstructed by this migration. User subsequently authorized task-completion commits; no push or deployment is authorized.

## Decisions / pending gates

- Adopt npm workspaces, not a task-runner framework: two current workspaces, minimal root delegates.
- Use VitePress 1.6.4, a cached stable release; frontend dependency versions must remain locked.
- Native crates, persistence format, SQL drivers, protocol framing, auth and tunnel libraries remain proposals. See architecture decision register.
- GitHub repository URL cannot be inferred from a absent remote; website/CI must accept configuration rather than invent an owner.
- Do not publish. The Pages workflow must default to build-only and require an explicit manual deploy request.

## Verification and retained limitations

All 73 original frontend source/config/test/asset hashes match. All original frontend dependency records remain unchanged. Root tests cover ongoing workspace/lock/navigation/base/credential-free repository-link contracts; the historical hash baseline is a separate one-time audit. Frontend tests/build and docs root/subpath/custom-domain builds pass. Browser execution and live previews remain blocked by listener EPERM; the 17 collected scenarios and Light/Dark screenshots are unexecuted. Full results: docs/development/verification.md.

VitePress output/cache paths use explicit absolute paths after a build exposed relative-path resolution. Browser Mermaid uses a pinned CDN and readable source fallback because its package was unavailable locally; graphical rendering is not verified. No external registry fetch, security bypass, Git initialization, push or deployment was performed.
