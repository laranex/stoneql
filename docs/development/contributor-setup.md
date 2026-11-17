# Contributor setup

This page is for people building or changing StoneQL. To use the app, start with the [user guide](../guide/index.md).

Read the root `AGENTS.md` and `CLAUDE.md` before changing anything: they hold the repository rules (security, data integrity, required checks and Git policy).

## Requirements

- Node.js **26** and npm (the root `package.json` declares `engines.node` `26.x`; npm workspaces `apps/frontend` and `.website` share one lockfile).
- For the desktop app and Rust crates: the Rust toolchain pinned by `rust-toolchain.toml` (1.99.1, with rustfmt and Clippy) and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). Cargo commands use `--locked`.
- For database fixtures: a local Docker engine. Each fixture run creates and removes its own containers and never uses existing databases.

## Install and run the browser demo

```sh
npm ci
npm run dev
```

Open the URL printed by Vite. No database server, Rust toolchain or credentials are needed. See [Browser demo](./browser-demo.md) for what the demo simulates.

## Build and run the desktop app

```sh
npm run desktop:build   # builds the frontend first and embeds apps/frontend/dist
npm run desktop:run
```

Rebuild after frontend changes. `npm run desktop:bundle:adhoc` builds the ad-hoc signed `.app` and DMG (the form published as the beta). See the [desktop development host](./desktop-host.md).

## Commands

| Command | Purpose |
| --- | --- |
| `npm run dev` | Run the shared Vue frontend (browser demo) |
| `npm run build` | Type-check and bundle the frontend |
| `npm run preview` | Serve the frontend production bundle |
| `npm test` | Frontend unit tests and root `tests/*.test.mjs` regression checks |
| `npm run test:browser` | Playwright browser scenarios (install Chromium once with `npx playwright install chromium`) |
| `npm run docs:dev` | Run this documentation site |
| `npm run docs:build` | Check links, build the site and check its output |
| `npm run docs:preview` | Preview the built documentation |
| `npm run rust:build`, `rust:fmt`, `rust:lint`, `rust:test` | Rust workspace build, format check, Clippy (`-D warnings`) and tests |
| `npm run rust:boundaries`, `contracts:check`, `rust:vendor` | Crate layering, generated-contract drift and vendored SQLx provenance |
| `npm run desktop:test`, `desktop:smoke` | Native IPC tests and a real-window startup smoke |
| `npm run test:secret-service` | Linux only (in development): the Secret Service password store against a throwaway keyring on a private D-Bus; needs `gnome-keyring` and `libsecret-tools`, never uses your keyring. See [Secret Service test](./ssh-credentials.md#secret-service-test-npm-run-test-secret-service) |

Single tests:

```sh
cd apps/frontend && node --experimental-strip-types --test tests/connections.test.ts
node --test tests/workspace.test.mjs
cargo test --locked -p stoneql-runtime <test_name>
```

Browser execution needs a host that permits local listeners and sandboxed browser processes; a browser startup failure is not a passing run. Which checks to run for which change, the Docker fixture commands and recorded results are in [Verification](./verification.md).

## Where things live

See the [Developer Docs overview](./index.md#repository-responsibilities) for the repository map, and [Documentation maintenance](./documentation.md) for how this site is built.
