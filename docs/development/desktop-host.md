# Desktop development host

## Implementation status

M002 adds a Tauri 2 host around the shared Vue frontend. M003 adds Direct MySQL/MariaDB live profiles (read-only at first); M004 adds Direct PostgreSQL and natively chosen SQLite files (development builds only); M005 adds SSH tunnels and macOS Keychain secret references; M006 adds [read-write SQL execution](./sql-execution.md) (on by default since A15) and live CRUD; M007 adds live SQL/CSV/JSON export and SQL/CSV import. The desktop app is live-only: it has no sample connections or Mock mode, which remain in the browser build. Server vault, updater and HTTP server are not exposed. Browser development remains available through `npm run dev` without native imports.

Use Node 26 and the pinned Rust toolchain. Install the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) first.

```sh
npm ci
npm run desktop:build
npm run desktop:run
npm run desktop:test
npm run desktop:smoke
```

The smoke command opens the actual native window, checks that Vue rendered, round-trips lossless synthetic values, and exits. Its 15-second deadline prevents a stalled WebView being reported as a pass. This debug-only smoke is not a signed distribution test. Build embeds `apps/frontend/dist`; rebuild after frontend changes. Standard OS window decorations retain native dragging. The public beta (v0.1.0-beta.1, a DMG published 2026-11-17; see [Getting started](../getting-started/index.md)) is bundled with an ad-hoc signature (`desktop:bundle:adhoc`, below) and is not notarized; Developer ID signing and notarization remain pending (M009).

## IPC boundary

The native adapter lives in `apps/desktop/src-tauri`. Shared core/contracts contain no Tauri types. Rust generates `contracts/generated/v1.ts`; the browser-safe `nativeHost.ts` bridge uses that contract without importing an SDK. Requests have a version, correlation ID and strict bounded payload. Unknown fields, unsupported versions, forged sessions, untrusted origins and auxiliary windows are rejected.

The application manifest explicitly registers protected commands. Only the main local window receives host-info, synthetic contract-probe, close-resolution and the database command permissions described below. No renderer filesystem, shell, dialog or remote-origin permissions are granted. The dialog plugin is used only by trusted Rust code: the shutdown fallback and the native SQLite and SSH key file pickers. CSP excludes remote scripts, frames and connections.

Native close/quit requests reuse the existing unsaved-change confirmation. Cancel preserves edits; an unresponsive renderer receives a native warning before forced close. The fallback dialog's manual interaction remains unverified. Failed IPC bootstrap disables the bridge (host state `unavailable`); it never implies a real connection.

## Local verification and remaining gates

macOS ARM real native smoke previously completed in 609 ms; latest verification results are recorded in M002's milestone. Four native IPC tests cover ACLs, exact values, protocol rejection and cancellation of close. Two browser-shim tests cover Light/Dark unsaved-close dialogs; they do not replace real WebView/OS testing. The original 17 browser scenarios remain intact.

Real-window checks on macOS arm64 (A1, 2026-11-16): `npm run desktop:profile` builds the optimized host with the non-shipped `smoke` feature and launches it 30 times (30/30 passed; host start → renderer ready median 607 ms, p95 701 ms, max 715 ms; spawn → exit median 624 ms). The `journal_window` example (development harness, needs the `sqlite` feature) drives the window through System Events: `fallback` freezes the renderer and verifies the native "not responding" dialog (shown by macOS's UserNotificationCenter) — Cancel keeps the app running, Close StoneQL exits with status 0; `reload` verifies Cmd+R, the WebView context-menu Reload and the Web Inspector reload are refused while a lease or job is held; `picker` verifies the native SSH key-file picker (Escape gives no grant, a chosen file gives an opaque grant). `npm run desktop:bundle:adhoc` bundles `StoneQL.app` and a drag-to-Applications DMG with the pinned Tauri CLI (2.12.4) and `tauri.macos-adhoc.conf.json`: an **ad-hoc** signature (`codesign -s -`) with hardened runtime and no entitlements. It verifies with `codesign --verify --deep --strict`, launches, renders through the native bridge and exits 0 from the window close button; Gatekeeper rejects it and it is not notarized. Ad-hoc signing is a development stand-in, not release signing. Windows/Linux native execution, Developer ID signing and notarization, Keychain prompts on a Developer ID build and release-build IPC overhead remain pending (the startup budgets were approved 2026-11-17, A3, and met by the beta DMG). CI is prepared, not remotely executed.

A full-lockfile dependency audit currently fails on transitive Linux GTK dependencies: [GLib RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429) and [proc-macro-error RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370). GLib's fixed version is a different major API line; changing the lockfile alone cannot upgrade Tauri's GTK dependency graph. No advisory ignores or security exceptions have been added. M002 security acceptance and Linux distribution remain open pending a compatible upstream fix or an explicitly reviewed dependency patch. This does not affect the independently evaluated M001 core/contracts dependency graph.

## M003 native binding verification

The active connection dialog and existing SQL workspace use `connect_direct`, `read_only_query`, `disconnect_database` and `database_status`. All enforce the local main-window origin, native session identity and owned opaque handles. Connection/query admission is limited to 16 concurrent requests and eight logical pools. Shutdown rejects new work and awaits pool cleanup before exit. No secret-bearing request derives Debug; ordinary profile persistence excludes passwords, PEM contents and handles.

Run `npm run test:native-database` with local Docker and Rust tooling. It provisions owned TLS fixtures and executes actual Tauri handlers (MockRuntime) against MySQL/MariaDB: exact raw values, duplicate labels, refusal, packet recovery and independent disconnect. This is real IPC-handler/database integration, not native WebView interaction.

The browser suite has 22 scenarios, including two new Light/Dark live-bridge simulations exercising the actual UI, password/CA non-persistence, connection switching, SQL results/errors and narrow dialog actions. The real macOS smoke separately checks Wry startup and the mounted frontend. These three checks do **not** constitute a complete native GUI-to-real-database end-to-end test. Native GUI interaction, Windows/Linux execution, aggregate allocation profiles and indirect view/function trust remain open gates.

## M004 PostgreSQL and SQLite commands

`connect_direct` also accepts `postgresql`. Two further commands are registered for the main window only: `pick_sqlite_file` shows the native OS file picker from trusted Rust code and returns an opaque grant plus file name (or nothing when cancelled), and `connect_sqlite` opens a grant. The renderer still has no filesystem or dialog permission and never sends a path; strict envelopes reject a `path` field. A renderer bootstrap revokes outstanding grants together with pools. The trusted `authorize_sqlite_file` function used by the picker is public only to Rust callers and tests, never IPC. The real picker interaction is not automated; see [PostgreSQL and SQLite adapters](./postgres-sqlite.md).

## Reload cleanup

A new `host_info` bootstrap nonce rotates the renderer token and closes existing pools; retrying the same nonce is idempotent. The existing frontend sends a fresh UUID-based nonce per startup. No command, capability, DTO field or persistence key was added. A native regression first demonstrated that the original code reused a host-lifetime token. Runtime tests now cover pool closure/capacity reuse and stale registration, while real fixtures verify idle database-session removal, stale-token/handle rejection and all eight slots being available after bootstrap. This is command-handler integration; native graphical reload behavior still needs separate OS/WebView testing.

## M005 SSH and secret commands

Five further main-window commands are registered: `ssh_host_key` (scan a presented host key for explicit enrollment; never trusts it), `pick_ssh_key_file` (native picker → opaque, owner-scoped key grant), `cancel_connect` (cancel an owned in-flight setup), and write/delete-only `store_secret`/`delete_secret`. `connect_direct` accepts optional `passwordRef`, `ssh` and `attemptId`. The host owns the agent socket (`SSH_AUTH_SOCK` captured at startup) and the OS secret store; tests inject a private agent and a dedicated Keychain service through `app_builder_with`. The capability description lists the new grants; the renderer still has no filesystem, shell or dialog permission and cannot read a secret back. The trusted `authorize_ssh_key_file` is public only to Rust callers and tests. Run `npm run test:ssh` for the owned SSH/database fixtures; see [SSH transport and secret references](./ssh-credentials.md).
