# Linux desktop support — plan (branch `linux-support`, started 2026-11-17)

**Status: in progress (decisions 1–5 Approved 2026-11-17; see below). Make no Linux claims in the docs, README or releases until the release gates below pass.** Phase 3 (22.04 `.deb`, distro smoke, remaining fixtures, Linux audit, store-naming UI) is below. Related: `windows-support-plan.md` (branch `windows-support`), `.ref/ms/M004`, `M005`, `M009`, D005 (secrets).

Evidence standard for this branch (user, 2026-11-17): the dedicated build server is the evidence, not GitHub CI (Ubuntu 26.04.1 LTS x86_64, 4 vCPU, 15 GB; Node 26.13.1, Rust 1.99.1, Docker 29.10.0, WebKitGTK 2.52.6). No Linux CI job is added on this branch.

## Current state (phase 1, 2026-11-17)

Unix code paths are already shared with macOS (`cfg(unix)`). Linux builds, and every check in the phase 1 list passes after the test and harness fixes listed below. The list was `npm ci`, `build`, `rust:vendor/fmt/build/lint/test/boundaries`, `contracts:check`, `npm test`, `desktop:test`, `test:browser`, and the `test:native-database`, `test:native-postgres`, `test:execution`, `test:ssh` and `test:lifecycle` fixtures. `test:resources`, `test:receive-limit`, `test:journal-crash`, `docs:build` and the Linux release audit ran in phase 3 (below).

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
   - Consequences: 22.04 needs its updates applied (the versioned `libwebkit2gtk-4.1-0 (>= 2.38)` Depends makes apt pull them; an un-updated 22.04 has 2.36). Only amd64 is built so far. The 26.04-built binary needs `GLIBC_2.39` (`objdump -T`), above both floors: **release gate — build the `.deb` on Ubuntu 22.04** (for example a 22.04 container on the build server) and smoke it on 22.04 and Debian 12. Done in phase 3 (`GLIBC_2.34`, see below).
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

## Phase 3 results (2026-11-17)

**Release `.deb` built on Ubuntu 22.04 (glibc gate closed).** `tools/linux-deb/Dockerfile` (image `stoneql-deb-builder:22.04` on the build server): `ubuntu:22.04`, Tauri build deps from 22.04's apt with WebKitGTK from jammy-updates (`libwebkit2gtk-4.1-dev` 2.50.4-0ubuntu0.22.04.1, `libc6` 2.35-0ubuntu3.18; the build fails if WebKitGTK < 2.38 or glibc ≥ 2.36), Node 26.13.1 (official tarball, SHASUMS256 checked), Rust 1.99.1 (rustup-init, SHA-256 checked). `tools/linux-deb/build.sh` clones a git bundle, checks out one full SHA, asserts a clean tree, runs `npm ci` and the phase 2 `npm run desktop:deb`, and writes `StoneQL_<version>_<sha7>_amd64.deb`. Build cache: Docker volume `stoneql-deb-cache` (cargo registry, npm cache, target dir; the old `bundle/deb` is deleted before each build).

- Package: `~/artifacts/StoneQL_0.1.0_4d24a99_amd64.deb` on the server, built from `4d24a994fd8031d3a614634138c1b605fd2e10fa` (the last code commit; a first build from `34edcd4` passed the same checks and was deleted), SHA-256 `f29410637575bf51c11b57d223b8d0943f399306c929c98f704a45e0760c4cb1`, 9,263,746 bytes (builds are not byte-reproducible: the two packages' hashes differ); package name `stone-ql`, one ELF (`/usr/bin/stoneql-desktop`). Not committed, unsigned, unpublished.
- Highest required glibc symbol version: **`GLIBC_2.34`** (`objdump -T` and `readelf --version-info`, every ELF file in the extracted package), within 2.35 (22.04) and 2.36 (Debian 12).
- Declared `Depends: libwebkit2gtk-4.1-0 (>= 2.38), libdbus-1-3, libwebkit2gtk-4.1-0, libgtk-3-0`: the WebKitGTK floor is 2.38 as planned (the unversioned duplicate is Tauri's own).
- Install and launch (`tools/linux-deb/smoke.sh`, fresh `ubuntu:22.04` and `debian:12` containers, no extra privileges or security options; the package mounted read-only and its SHA-256 re-checked inside; `apt-get install ./…deb` resolved the Depends; run as an unprivileged user under `dbus-run-session -- xvfb-run`, fixed 30 s wait):

| Distro | Resolved | Result |
| --- | --- | --- |
| Ubuntu 22.04.5 | libc6 2.35-0ubuntu3.15, libwebkit2gtk-4.1-0 2.50.4-0ubuntu0.22.04.1, libgtk-3-0 3.24.33-1ubuntu2.2 | alive after 30 s; `xdotool`/`xwininfo` find one 1280×860 window titled "StoneQL"; screenshot inspected: the welcome screen (Connections bar, "Welcome to StoneQL", Manage connections, status bar) rendered |
| Debian 12 | libc6 2.36-9+deb12u14, libwebkit2gtk-4.1-0 2.50.6-1~deb12u2, libgtk-3-0 3.24.38-2~deb12u3 | same: alive, one "StoneQL" 1280×860 window, the same welcome screen rendered |

  Results are the same for both packages. Output on both: one `dbind-WARNING` (no AT-SPI accessibility bus in the container); no panic or other error. Not verified: interaction in the window, WebKit's bubblewrap sandbox state inside the container (bubblewrap was installed as a WebKitGTK dependency; not inspected), arm64.

**UI gaps closed.** A failed credential removal now names the store holding each reference (`the Keychain (service …)` on macOS; `the desktop keyring (Secret Service, service …)` or `StoneQL’s encrypted file` on Linux, from the reference's UUID version; `the system credential store` when unknown, e.g. Windows), with a matching hint (encrypted file: "until you reset saved passwords"). Password placeholders name the store of that reference (`Saved in Keychain`/`keyring`/`encrypted file`, `Saved password` when unknown), and the note now reads "New passwords are saved in …" for where a new password goes. `test:secret-service` and its prerequisites are documented in `docs/development/ssh-credentials.md` and `contributor-setup.md`.

**Release dependency audit.** `npm run audit:release:linux` (`tools/audit-release.mjs --linux`) audits the shipped `x86_64-unknown-linux-gnu` graph of `stoneql-desktop --features custom-protocol` (normal+build) the same way as the macOS mode. Server run at `4d24a99`: 399 of 575 locked packages, **0 vulnerabilities**, 2 informational warnings (RUSTSEC-2024-0370, RUSTSEC-2024-0429; see below), exit 0, nothing ignored. The macOS mode on the server: 364 packages, 0 vulnerabilities, 0 warnings. (The Mac resolves 404 Linux packages because build and proc-macro dependencies resolve for the host; the server figure is the Linux-host one.)

Build server checks (Ubuntu 26.04.1, `34edcd4`; `npm test`, `test:resources` and the audits again at `4d24a99`, whose only change is the resource harness):

| Check | Result |
| --- | --- |
| `rust:fmt`, `rust:lint`, `rust:boundaries`, `contracts:check`, `docs:build` | pass |
| `rust:test` | 181 passed, 0 failed, 11 ignored |
| `npm test` | 245 + 22 passed (both SHAs) |
| `test:browser` | 75 passed (includes the new `secret-store-location` spec in both themes) |
| `desktop:test` (no session bus) | 71 passed, 0 failed, 7 ignored |
| `test:ssh` | pass: MySQL 195, MariaDB 46, PostgreSQL 41 forwarded channels |
| `test:secret-service` | pass; 0 items left; user keyrings absent |
| `test:resources` | **failed first** (MySQL fixture OOM-killed: docker `oom` event; the server has no swap, Docker Desktop's swap hid the 1 GiB limit on tmpfs data) and needed GNU `time` (`/usr/bin/time`, now installed on the server). Fixed in `4d24a99` (2 GiB, no swap, resource profile only), then pass: MySQL p50/p95/max 110.8/112.1/113.2 ms, peak RSS 27.6 MiB, idle 14.5 MiB; MariaDB 109.7/111.1/111.5 ms, 28.1 MiB; PostgreSQL p95 164.3 ms, 27.6 MiB. p95 is about 7.7× the macOS record (14.6 ms); the uniform ~110 ms looks like a fixed per-query delay and is **not attributed** (compare the phase 1 slow-statement note) |
| `test:receive-limit` | pass (2 + 1 tests); `test:receive-limit:live`: MySQL and MariaDB TLS packet-bound and certificate checks pass |
| `test:journal-crash` | pass: 40 SIGKILL restarts, 845 unknown outcomes, 885 acknowledged, 22 open transactions at kill, 0 lost/resurrected/missing; second process refused (`in_use`) |

Mac (shared code, `7324b40`+): `npm test` 245 + 22, `test:browser` 75 passed, `desktop:test` 62 passed, `npm run build`, `docs:build` pass.

Server changes this phase: installed `time` (GNU time) and `cargo-audit` 0.22.1 (`~/.cargo/bin`); image `stoneql-deb-builder:22.04`, volume `stoneql-deb-cache`, pulled `ubuntu:22.04` and `debian:12` kept as build cache. No containers left; SSH, firewall and AppArmor unchanged.

## Linux release audit exceptions: Approved (user, 2026-11-17; confirmed directly 2026-11-18)

A **documented exception** for exactly the two informational advisories below, re-checked at every Tauri upgrade. `tools/audit-exceptions.mjs` holds the two IDs with reason and dependency path; `audit:release:linux` fails on any other advisory or warning and on a stale exception; `tests/audit-exceptions.test.mjs` (4 tests); `verification.md` documents it. The macOS release audit has no exceptions.

- RUSTSEC-2024-0429, `glib` 0.18.5, unsound `VariantStrIter` iterators. Path: stoneql-desktop → tauri 2.12.5 → muda / tao / wry → gtk 0.18.2 → glib 0.18.5 (in the runtime graph). Relayed source search: no use of `VariantStrIter` in tauri, wry, tao, muda, gtk, webkit2gtk, gio, soup3, atk, gdk or StoneQL.
- RUSTSEC-2024-0370, `proc-macro-error` 1.0.4, unmaintained. Path: … glib 0.18.5 → glib-macros 0.18.5 (proc-macro) → proc-macro-error. Verified compile-time only: `cargo tree --target x86_64-unknown-linux-gnu -e normal,no-proc-macro` lists glib but neither glib-macros nor proc-macro-error, while `-e normal,build` lists proc-macro-error.

## Remaining before any Linux release

- AppImage (approved for later).
- **Pending manual QA (user, real GNOME session; not simulated):** the real GNOME Keyring unlock prompt (and its dismissal), and the master-password flow in the real app window; plus the rest of decision 5's list (dialogs, clipboard, HiDPI, Wayland, crash recovery).
- Docs, README and release notes stay free of Linux claims until then; `docs/development/ssh-credentials.md` describes the Linux stores as in development and unreleased.
