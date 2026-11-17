# Contributor setup

This page is for people building or changing StoneQL. To use the app, start with the [user guide](../guide/index.md).

Read the root `AGENTS.md` and `CLAUDE.md` before changing anything: they hold the repository rules (security, data integrity, required checks and Git policy).

## Requirements

- Node.js **26** and npm (the root `package.json` declares `engines.node` `26.x`; npm workspaces `apps/frontend` and `.website` share one lockfile).
- For the desktop app and Rust crates: the Rust toolchain pinned by `rust-toolchain.toml` (1.99.1, with rustfmt and Clippy) and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). Cargo commands use `--locked`.
- For database fixtures: a local Docker engine. Each fixture run creates and removes its own containers and never uses existing databases. Docker fixtures run on macOS and Linux; Windows runs the real-database tests through the remote fixture host instead (see [Verification](./verification.md#platform-verification-status-v0-1-0-beta-2)).

### Windows prerequisites

- **Visual Studio Build Tools** (or Visual Studio) with the **Desktop development with C++** workload (MSVC and the Windows SDK).
- **Microsoft Edge WebView2 Runtime**: included in Windows 11 and usually present on Windows 10; otherwise install the Evergreen runtime from Microsoft.
- **Rust 1.99.1** through [rustup](https://rustup.rs/) (the MSVC toolchain, `x86_64-pc-windows-msvc`); `rust-toolchain.toml` pins the version.
- **Node.js 26** and npm.

The package scripts that set `RUSTFLAGS` inline (`desktop:bundle:adhoc`, `desktop:dmg:adhoc`, `desktop:deb`) use POSIX shell syntax and do not run in PowerShell or `cmd`. The Windows installer is built as in CI, from `apps/desktop/src-tauri` after `npm run build`:

```sh
npx tauri build --bundles nsis --features custom-protocol --config tauri.windows-nsis.conf.json -- --locked
```

### Linux prerequisites (Debian and Ubuntu)

```sh
sudo apt install build-essential pkg-config curl file git \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libssl-dev libdbus-1-dev
```

WebKitGTK must be 2.38 or later (on Ubuntu 22.04 from `jammy-updates`). Install Rust 1.99.1 with rustup and Node.js 26 separately. Real-window checks without a desktop run under `dbus-run-session -- xvfb-run` (packages `xvfb` and `dbus`); `npm run test:secret-service` also needs `gnome-keyring` and `libsecret-tools`. Release `.deb` packages are built on Ubuntu 22.04 (glibc 2.35) with `tools/linux-deb/` so the binary runs on Ubuntu 22.04 and Debian 12; a package built on a newer distribution requires a newer glibc.

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

Rebuild after frontend changes. Release packaging: `npm run desktop:dmg:adhoc` builds the ad-hoc signed `.app` and DMG on macOS, `npm run desktop:deb` the `.deb` on Linux, and the NSIS command above the Windows installer. See the [desktop development host](./desktop-host.md).

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
| `npm run test:secret-service` | Linux only: the Secret Service password store against a throwaway keyring on a private D-Bus; needs `gnome-keyring` and `libsecret-tools`, never uses your keyring. See [Secret Service test](./ssh-credentials.md#secret-service-test-npm-run-test-secret-service) |

Single tests:

```sh
cd apps/frontend && node --experimental-strip-types --test tests/connections.test.ts
node --test tests/workspace.test.mjs
cargo test --locked -p stoneql-runtime <test_name>
```

Browser execution needs a host that permits local listeners and sandboxed browser processes; a browser startup failure is not a passing run. Which checks to run for which change, the Docker fixture commands and recorded results are in [Verification](./verification.md).

## Where things live

See the [Developer Docs overview](./index.md#repository-responsibilities) for the repository map, and [Documentation maintenance](./documentation.md) for how this site is built.
