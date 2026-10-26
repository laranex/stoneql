# Architecture review — 2026-10-26

Review before continuing M003. Scope: module boundaries, layering and coupling across `apps/frontend`, `crates/*` and `apps/desktop/src-tauri`. Recommendations are incremental; nothing here approves new features.

## Sound (left unchanged)

- Rust layering matches the docs and is enforced by `tools/check-rust-boundaries.mjs` over resolved dependencies: `stoneql-core` (no deps) → `stoneql-contracts` → `stoneql-runtime` (sole SQLx/Tokio owner) → `apps/desktop/src-tauri` (thin IPC adapter).
- Contract flow: Rust DTOs generate tracked `contracts/generated/v1.ts`; `contracts:check` enforces exact regeneration and TS consumers.
- `services/nativeHost.ts` stays browser-runnable (no Tauri SDK import) and falls back to mock mode on bootstrap failure.
- Git co-change history showed no code-level shotgun surgery; hot files were milestone notes, docs and tooling.

## Findings and status

| # | Finding | Status |
| --- | --- | --- |
| 1 | `App.vue` (2,981 lines) held connection test/save/connect/close/remove orchestration with inline mock/live branching | **Done**: extracted to `apps/frontend/src/composables/useConnectionWorkflow.ts`; template and UI text unchanged; 9 regression tests in `apps/frontend/tests/connectionWorkflow.test.ts` |
| 2 | `stores/workspace.ts` mixes profile persistence, live sessions, mock mutations and preferences | **Deferred by user** — do not refactor yet. Smallest future move: live-session block into its own module |
| 3 | `CapabilitySnapshot` exists in v1 but no command returns it; frontend hardcodes `liveSupported()`; ~64 engine-name checks across 14 files | **Deferred** — no premature capability abstraction. Revisit when a live feature first needs engine support data, or at the first M004 engine addition |
| 4 | Frontend duplicated runtime row/byte limits as literals | **Done**: `LiveQueryRequest.maxRows/maxBytes` optional; `QueryLimits::MAX_ROWS/MAX_BYTES` in runtime are the single bounds + defaults; see `docs/development/read-only-safety.md`. Real MySQL/MariaDB fixtures (`npm run test:native-database`) prove omitted limits return exactly the explicit 1000-row / 1 MiB results |

## Watch

- `stoneql-runtime` returns v1 wire types (`ResultSet`, `CellValue`) directly: avoids copies and preserves raw precision, but contract version changes touch the runtime.
- `decode_query_request` (general, not-yet-bound `QueryRequest`) still repeats 1000 / 1 MiB literals; align with `QueryLimits` when that request is bound to a host.
- Runtime registry is MySQL-specific (`MySqlPool`); introduce an engine abstraction only with the second engine's milestone.

## Leave alone

`vendor/sqlx-mysql` and provenance, Tauri bundle ID `app.dbadmin.desktop`, mock services/sample data, contract generation flow, relative `contracts/generated/v1` import paths.
