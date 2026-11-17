# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

AGENTS.md (imported above) holds the authoritative rules: session start steps, security/data-integrity constraints, required verification and Git policy. This file adds commands and cross-file architecture only.

## Commands

Root npm workspaces: `apps/frontend`, `.website`. Rust is pinned by `rust-toolchain.toml`; Cargo commands use `--locked`.

```sh
npm ci
npm run dev                 # Vite frontend (browser/mock mode)
npm test                    # frontend unit tests + root tests/*.test.mjs
npm run build               # vue-tsc -b && vite build
npm run test:browser        # Playwright; needs `npx playwright install chromium`
npm run docs:build          # VitePress check + build + output check
```

Single tests:

```sh
# frontend unit (node:test with --experimental-strip-types, not Vitest)
cd apps/frontend && node --experimental-strip-types --test tests/connections.test.ts
node --experimental-strip-types --test --test-name-pattern "pattern" tests/data.test.ts
# root regression checks
node --test tests/workspace.test.mjs
# Playwright (starts Vite on :5173 itself)
cd apps/frontend && npx playwright test tests/browser/grid.spec.ts -g "title"
# Rust
cargo test --locked -p stoneql-runtime <test_name>
```

`rust:fmt` only checks; apply with `cargo fmt --all`. `rust:lint` is Clippy with `-D warnings`. Regenerate the TS contract with `npm run contracts:generate`.

Desktop (Tauri 2): `desktop:build` (builds frontend first and embeds `apps/frontend/dist` — rebuild after frontend changes), `desktop:run`, `desktop:test`, `desktop:smoke` (opens a real window, 15 s deadline).

Docker-backed fixtures (each run creates and removes its own containers): `spike:drivers`, `test:native-database`, `test:native-postgres`, `test:lifecycle`, `test:resources`, `test:ssh` (disposable OpenSSH + databases, private agent, per-run Keychain service), `test:execution` (M006 jobs/transactions/cancel), `test:receive-limit[:live]`. `rust:vendor` verifies the patched `vendor/sqlx-mysql`. `test:secret-service` (Linux only, no Docker) runs the Secret Service provider against a throwaway gnome-keyring on a private D-Bus (needs `gnome-keyring`, `libsecret-tools`). `audit:release:linux` audits the shipped `x86_64-unknown-linux-gnu` graph like `audit:release` does for macOS.

## Architecture

**Frontend (`apps/frontend/src`)** — one large `App.vue` shell plus focused components. Connection dialog test/save/connect/close/remove orchestration (mock and live) lives in `composables/useConnectionWorkflow.ts`. State lives in the Pinia store `stores/workspace.ts`, which owns per-connection workspaces and delegates to pure services in `services/` (record mutations, column definitions, validation, indexes, CSV import, export, grid navigation). Mock schemas/rows come from `data/`; `services/mockTransport.ts` simulates connection testing. SQL dialect/autocomplete context is in `sql/`. Unit tests import these services directly (`tests/helpers/vueHost.ts` mounts components without a browser).

**Native bridge** — `services/nativeHost.ts` detects `window.__TAURI__.core.invoke` at runtime (no Tauri SDK import, so the app stays browser-runnable). It bootstraps with `host_info` (fresh nonce per startup, which rotates the session token and closes stale native pools) and a `contract_probe` round-trip; failure falls back to browser/mock mode. `services/liveConnections.ts` wraps the four database commands (`connect_direct`, `read_only_query`, `disconnect_database`, `database_status`); it omits query row/byte limits so the runtime defaults apply. Every request is a versioned JSON envelope with a correlation ID.

**Contracts** — Rust DTOs in `crates/stoneql-contracts/src/v1.rs` are the source of truth; `ts-rs` generates `contracts/generated/v1.ts` (tracked), which the frontend imports via relative path. Changing a DTO means regenerating and passing `contracts:check` (exact-diff + positive/negative TS consumers in `contracts/tests/`).

**Rust crates** (layered, enforced by `tools/check-rust-boundaries.mjs`, which walks the resolved dependency graph):
- `stoneql-core`: zero-dependency engine/transport vocabulary, capabilities (unverified by default), domain errors.
- `stoneql-contracts`: wire DTOs + TS generation; depends only on core. Neither may pull in tauri/axum/sqlx/tokio.
- `stoneql-runtime`: SQLx-based connection lifecycle/pool registry with owner-scoped opaque handles and bounded read-only query execution (`read_only.rs`; `QueryLimits::MAX_ROWS`/`MAX_BYTES` are both the enforced bounds and the defaults).
- `apps/desktop/src-tauri`: Tauri host; `database.rs` adapts IPC commands to the runtime and enforces main-window origin, session identity and concurrency limits.
- `experiments/sqlx-spike`: disposable lab, not in default members.

`[patch.crates-io]` redirects `sqlx-mysql` to `vendor/sqlx-mysql` (receive-limit patch).

**Docs** — `docs/` is canonical Markdown rendered by `.website/` (VitePress). Implementation status per area lives in `docs/development/`; milestone acceptance criteria in `.ref/ms/`.
