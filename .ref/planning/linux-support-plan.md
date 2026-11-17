# Linux desktop support — plan (branch `linux-support`, started 2026-11-17)

**Status: Proposed / in progress. Make no Linux claims in the docs, README or releases until the release gates below pass.** Related: `windows-support-plan.md` (branch `windows-support`), `.ref/ms/M004`, `M005`, `M009`, D005 (secrets).

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
| Saved passwords | `OsSecretStore` returns `Unsupported` (tested). Typed passwords, keys and the agent work | `apps/desktop/src-tauri/src/secrets.rs` |
| Web content crash handler | **Gap.** Tauri 2.12.5's `on_web_content_process_terminate` is macOS/iOS only. On Linux, after `WebKitWebProcess` is killed the window does not reload: the real-window test times out at "reload after the content process terminated". What the window shows was not inspected. The test's process discovery is macOS-specific (`com.apple.WebKit.WebContent`) | `lib.rs` (`#[cfg(target_os = "macos")]`), `tests/real_window_reload.rs` |
| Real window | `desktop:smoke` passes under Xvfb. Startup is 1.2 s with a session bus. Without one it is 31 s: `NO_AT_BRIDGE=1` (accessibility bus) brings it to 15 s, and the remaining wait was not attributed. The real-window navigation-guard part passes | `xvfb-run`, `dbus-run-session` |
| Packaging | None: `bundle.active` is false and only macOS ad-hoc bundle scripts and config exist | `tauri.conf.json`, `package.json` `desktop:*:adhoc` |

## Phases

1. **Build and test facts (no product decision).** Done on the build server; results are above.
2. **Credential store (decision needed).** Options:
   - (a) **Secret Service over D-Bus** (GNOME Keyring, KWallet ≥ 5.97, KeePassXC). This matches the Keychain model: per user, and unlocked with the login session. Provider versions and dependency trees below are from memory and unverified. `dbus =0.9.12` and `libdbus-sys` are already in `Cargo.lock` via `tao` (Linux only), but are not a direct dependency. A binding needs either raw `dbus` calls (rough estimate, about 200–300 lines: open a session with the `plain` or DH algorithm, unlock the collection, create/search/delete items) or a new crate (`secret-service` or `keyring`; neither is in the lockfile, and both pull in `zbus` or extra crypto). Drawbacks: headless or minimal desktops may have no daemon, so the store must fail as `Unsupported`/`Failed` and never fall back silently. Any process of the user on the session bus can read the items, as with a Keychain item that has no ACL prompt.
   - (b) **Encrypted file** in `~/.local/share/me.thixpin.stoneql…`, like the Windows approach. Linux has no DPAPI, so the key has to come from somewhere: a Secret Service item (which needs (a) anyway), a user master password (new UI and an unlock flow), or a key file beside the data (obfuscation only; no protection against other processes or backups). `aes-gcm 0.11.1`, `argon2 0.6.0`, `hkdf` and `zeroize` are already in the lockfile, but only transitively (via `russh`/`ssh-key`), not as direct dependencies.
   - (c) **Unsupported for now** (the status quo): typed passwords and agent/key auth work; saving is refused with `UNSUPPORTED_CAPABILITY`.
   - Recommendation: (c) for the first Linux alpha, then (a) via the existing `dbus` crate, failing closed when no Secret Service is available. Use (b) only with a master password, which is a product decision.
3. **Renderer crash recovery (decision needed).** Handle WebKitGTK's `web-process-terminated` signal through `with_webview` (`webkit2gtk 2.0.2` is already in the lockfile through `wry`) and reuse `renderer_lost`. Or accept a blank window and document "restart the app". Recommendation: implement, because the macOS F-2 guarantee (the session ends and the interrupted job is listed after reload) is otherwise missing. Port the real-window test's process discovery to `WebKitWebProcess` at the same time.
4. **Packaging (decision needed).** Formats (characteristics from memory, unverified):
   - AppImage: one file that runs on most distros and bundles libraries except WebKitGTK/glibc.
   - `.deb`: Debian/Ubuntu, depends on the system `libwebkit2gtk-4.1-0`.
   - `.rpm`: Fedora/openSUSE.

   Recommendation: `.deb` and AppImage first; signing stays deferred, like Developer ID.
5. **WebKitGTK version floor (decision needed).** Tauri 2 needs `webkit2gtk-4.1` (distro availability from memory, unverified: Ubuntu 22.04+, Debian 12+, Fedora 36+). Recommendation: an Ubuntu 22.04 / Debian 12 floor, verified on the oldest supported distro before release. Only Ubuntu 26.04 is verified so far.
6. **Verification standard (decision needed).** Recommendation: the build server runs every check in this file, the Docker fixtures, and the smoke and real-window tests under `dbus-run-session -- xvfb-run`. A real desktop session (GNOME on Wayland, in a VM or on hardware) is checked by hand before release for dialogs, clipboard, HiDPI, the Secret Service and Wayland-specific behaviour (Xvfb is X11 only).
7. **Docs and release** only after 2–6 pass.

## Decisions to ask the user

- Secret store: (a), (b) or (c) above.
- Renderer crash handling on Linux: implement it or document it.
- Package formats and the WebKitGTK/distro floor.
- Verification standard (build server plus a manual desktop session).
