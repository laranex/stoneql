# Linux desktop support — plan (branch `linux-support`, started 2026-11-17)

**Status: in progress (decisions 1–5 Approved 2026-11-17; see below). Make no Linux claims in the docs, README or releases until the release gates below pass.** Related: `windows-support-plan.md` (branch `windows-support`), `.ref/ms/M004`, `M005`, `M009`, D005 (secrets).

Evidence standard for this branch (user, 2026-11-17): the dedicated build server is the evidence, not GitHub CI (Ubuntu 26.04.1 LTS x86_64, 4 vCPU, 15 GB; Node 26.13.1, Rust 1.99.1, Docker 29.10.0, WebKitGTK 2.52.6). No Linux CI job is added on this branch.

## Current state (phase 1, 2026-11-17)

Unix code paths are already shared with macOS (`cfg(unix)`). Linux builds, and every check in the phase 1 list passes after the test and harness fixes listed below. The list was `npm ci`, `build`, `rust:vendor/fmt/build/lint/test/boundaries`, `contracts:check`, `npm test`, `desktop:test`, `test:browser`, and the `test:native-database`, `test:native-postgres`, `test:execution`, `test:ssh` and `test:lifecycle` fixtures. Not yet run on Linux: `test:resources`, `test:receive-limit`, `test:journal-crash`, `docs:build`, `audit:release`.

| Area | Today on Linux | Code |
| --- | --- | --- |
| Build, fmt, Clippy, boundaries, contracts, vendor patch, frontend build and unit tests | Pass unchanged | — |
| Runtime tests | Failed intermittently (5 of 10 runs) with `InUse` in journal tests. Cause: the kill test's spawned child holds copies of other tests' journal-lock descriptors until its `execve`. This is a test-only race. Fixed by serialising the spawn against `Dir` lifetimes (100 of 100 runs pass) | `execution/journal/tests.rs` |
| Outcome journal | Works: same `flock`/`openat` code as macOS. The host opens it once per process, so the spawn window above does not affect the product | `execution/journal.rs`, desktop `lib.rs` `open_journal` |
| SQL editor Ctrl+Enter | Was broken on Linux: CodeMirror's `Mod-Enter` (`insertBlankLine`) ran before the run-query handler. The same happened with Cmd+Enter on macOS (reproduced). Fixed by giving the handler `Prec.highest` | `components/SqlEditor.vue` |
| Playwright | Chromium's sandbox is blocked by Ubuntu ≥ 23.10's AppArmor userns restriction. The build server has a narrow AppArmor profile that grants `userns` to the Playwright browsers only; the sandbox was not disabled | build server `/etc/apparmor.d/playwright-chromium` |
| Docker fixtures | Native Docker keeps host modes. The SSH fixture's 0700 init/TLS mounts were unreadable to the container's database user and are now 0755 inside the private mkdtemp parent, like the other fixtures. The execution harness depended on timing: a fixed 150 ms cancel timer, and the expiry lease's ended record surviving the 100 fault cycles. On this server the statements before the sleep took more than 150 ms, and the fault cycles (74 s) overlapped the 60 s lease expiry. Both checks no longer depend on host speed. The cause of the slower statements was not measured: there are no per-step macOS timings to compare with | `tools/test-ssh-fixture.mjs`, `examples/execution.rs` |
| SSH tunnel / agent | Same Unix-socket endpoint and `SSH_AUTH_SOCK` agent as macOS; `test:ssh` passes on all three engines | `crates/stoneql-runtime/src/ssh.rs` |
| Saved passwords | **Implemented (phase 2).** Secret Service over D-Bus (`dbus` 0.9.12, libdbus): fixed label, `service` + `account` attributes only, `plain` session, unlock only through the service's own prompt. Without a Secret Service on the bus: the master-password encrypted file (Argon2id 64 MiB/t=3/p=1 + AES-256-GCM, 0700 dir, 0600 atomic files, descriptor-validated like the journal). A present Secret Service that refuses fails closed; it never falls back. The provider is the reference's UUID version (v4 Secret Service, v8 file). `test:ssh` now saves and resolves references on all three engines | `secrets.rs`, `secrets/secret_service.rs`, `secrets/vault.rs`, `MasterPasswordDialog.vue` |
| Web content crash handler | **Implemented (phase 2).** WebKitGTK `web-process-terminated` is connected on the main webview through `with_webview` (`webkit2gtk` 2.0.2, no extra features) and runs the macOS handler's body (`renderer_lost`, then reload). The real-window test finds this window's `WebKitWebProcess` by `argv[0]` among the test process's descendants and passes under `dbus-run-session -- xvfb-run` | `lib.rs` `navigation_guard`, `tests/real_window_reload.rs` |
| Real window | `desktop:smoke` passes under Xvfb. Startup is 1.2 s with a session bus. Without one it is 31 s: `NO_AT_BRIDGE=1` (accessibility bus) brings it to 15 s, and the remaining wait was not attributed. The real-window navigation-guard part passes | `xvfb-run`, `dbus-run-session` |
| Packaging | **`.deb` (phase 2).** `npm run desktop:deb` (`tauri.linux-deb.conf.json`) builds `StoneQL_0.1.0_amd64.deb` (8.8 MiB). Depends: `libwebkit2gtk-4.1-0 (>= 2.38), libdbus-1-3`, plus Tauri's own `libwebkit2gtk-4.1-0, libgtk-3-0`. Installed with apt on the server and run under Xvfb: process alive after 12 s with a 1280×860 "StoneQL" window (rendered content not inspected). Unsigned, unpublished. **Built on 26.04 the binary needs `GLIBC_2.39`**, so it does not run on the floor distros: release `.deb`s must be built on Ubuntu 22.04 (or older glibc), see phase 5 | `tauri.linux-deb.conf.json`, `package.json` `desktop:deb` |

## Decisions (user, 2026-11-17)

1. **Saved passwords — Approved.** Secret Service over D-Bus, failing closed when it is present but unusable. Fallback **Approved**: when no Secret Service is on the session bus, an encrypted file in `$XDG_DATA_HOME/me.thixpin.stoneql/credentials/` keyed from a **master password** (user decided the key source the same day). Implemented as above. **Values approved by the user 2026-11-17:** minimum master-password length 8 characters (user changed it from the proposed 12); Argon2id m=64 MiB, t=3, p=1 (measured 0.154 s release, 2.78 s debug build, on the 4-vCPU server); 1 s fixed delay after a wrong password; prompt wait 50 s for a Secret Service unlock prompt. No migration: Linux could not save passwords before. Not verified: the Secret Service unlock prompt (headless server, no prompter), KWallet and KeePassXC providers.
2. **Renderer crash recovery — Approved.** Implemented; macOS handler unchanged in behaviour (now shares `recover_renderer`).
3. **Packaging — Approved:** `.deb` first, AppImage later. No signing, no publishing.
4. **Oldest supported distros — Approved:** Ubuntu 22.04 / Debian 12. Verified facts (package metadata, fetched 2026-11-17):
   - Ubuntu 22.04: `libwebkit2gtk-4.1-0` 2.50.4-0ubuntu0.22.04.1 in jammy-updates (universe; arm64 is listed there too, the page gives one version for all architectures) and in jammy-security on amd64/i386; the release pocket has **2.36.0-2ubuntu1**, below wry's `v2_38` feature (WebKitGTK ≥ 2.38). glibc 2.35-0ubuntu3.15 (amd64). Sources: https://packages.ubuntu.com/jammy/libwebkit2gtk-4.1-0, https://packages.ubuntu.com/jammy-updates/libwebkit2gtk-4.1-0, https://packages.ubuntu.com/jammy/libc6
   - Debian 12: `libwebkit2gtk-4.1-0` 2.50.6-1~deb12u2 (mipsel 2.42.2), glibc 2.36-9+deb12u14. Sources: https://packages.debian.org/bookworm/libwebkit2gtk-4.1-0, https://packages.debian.org/bookworm/libc6
   - Consequences: 22.04 needs its updates applied (the versioned `libwebkit2gtk-4.1-0 (>= 2.38)` Depends makes apt pull them; an un-updated 22.04 has 2.36). Only amd64 is built so far. The 26.04-built binary needs `GLIBC_2.39` (`objdump -T`), above both floors: **release gate — build the `.deb` on Ubuntu 22.04** (for example a 22.04 container on the build server) and smoke it on 22.04 and Debian 12. Not done yet.
5. **Verification standard — Approved.** The build server runs every check, real-window tests under `dbus-run-session -- xvfb-run`. **Pending (user's step, before release):** a manual check in a real GNOME session (dialogs, clipboard, HiDPI, Wayland, GNOME Keyring unlock prompt, crash recovery).

## Phase 2 results (2026-11-17)

Build server (Ubuntu 26.04.1), clean checkout of `5155a3c`, then `b12c40b` for the re-runs:

| Check | Result |
| --- | --- |
| `rust:fmt`, `rust:lint`, `rust:boundaries`, `contracts:check` | pass |
| `rust:test` | 181 passed, 0 failed, 10 ignored (`b12c40b`; at `5155a3c` the IPC command-count assertion still said 32) |
| `desktop:test` (no session bus) | 71 passed, 0 failed, 6 ignored (`b12c40b`) |
| `npm test` | 239 + 22 passed |
| `npm run build`, `docs:build` | pass |
| `test:browser` | 73 passed, including the new master-password spec in both themes |
| `test:secret-service` (new; disposable bus + gnome-keyring 50.0 unlocked with a generated password piped on stdin) | pass: round trip, replace, label, attributes, delete, a present service never selects the vault, and a second keyring started without a login keyring (present but unusable) fails closed with `SECRET_UNAVAILABLE` and no vault directory; 0 items left; `~/.local/share/keyrings` absent before and after |
| `real_window_reload` under `dbus-run-session -- xvfb-run` | pass, including the F-2 `WebKitWebProcess` kill and reload |
| `desktop:smoke` under the same | pass, `startupMs` 1690 |
| `test:ssh` | pass on MySQL, MariaDB, PostgreSQL with saved references (3 created and deleted per engine, 0 left) |
| `desktop:deb` | `StoneQL_0.1.0_amd64.deb`, 9.3 MB; installed with apt, alive after 12 s with a "StoneQL" 1280×860 window under Xvfb; then removed |

New direct dependencies (Linux only unless noted; all already in `Cargo.lock`, licences from `cargo metadata`): `dbus` 0.9.12 (Apache-2.0/MIT), `webkit2gtk` 2.0.2 (MIT), `aes-gcm` 0.11.1 (Apache-2.0 OR MIT), `argon2` 0.6.0 (MIT OR Apache-2.0), `getrandom` 0.4.3 (MIT OR Apache-2.0), `nix` 0.31.3 (MIT), and `zeroize` 1.9.1 (Apache-2.0 OR MIT, all platforms). `unsafe_code` stays forbidden.

Argon2id cost (m=64 MiB, t=3, p=1): 0.154 s release, 2.78 s debug.

Server changes this phase: installed `gnome-keyring`, `libsecret-tools` (test fixture) and `x11-utils` (`xwininfo` for the `.deb` smoke). The AppArmor profile, SSH and firewall are unchanged; no containers left.

Mac (macOS, shared code): `rust:fmt`, `rust:lint`, `contracts:check`, `rust:boundaries` pass; `desktop:test` 62 passed, 0 failed; `npm test` 239 + 22; `test:browser` 73 passed. `real_window_reload` was not re-run on the Mac (needs the desktop session); the macOS handler now calls the shared `recover_renderer` with the same body.

Not verified: the Secret Service unlock prompt and its dismissal (no prompter on the headless server); KWallet/KeePassXC; the master-password flow in the real app window (covered by unit, IPC-double and browser tests, not end to end in WebKitGTK).

## Remaining before any Linux release

- Build the `.deb` on Ubuntu 22.04 and smoke it on 22.04 and Debian 12 (glibc gate above).
- AppImage (approved for later).
- The manual GNOME session check (user).
- Not yet run on Linux: `test:resources`, `test:receive-limit`, `test:journal-crash`, `audit:release`.
- Docs, README and release notes stay free of Linux claims until then; `docs/development/ssh-credentials.md` describes the Linux stores as in development and unreleased.
- Known UI gaps: removal-failure messages still name the Keychain; the "Saved in…" placeholder shows the store for new saves, not the store of that reference. `test:secret-service` and its prerequisites (`dbus-daemon`, `gnome-keyring`, `libsecret-tools`) are not yet in the contributor/verification docs.
