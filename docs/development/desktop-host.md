# Desktop development host

## Implementation status

M002 adds a Tauri 2 development host around the shared Vue frontend. All database operations remain session mocks. No database driver, SSH, vault, updater or HTTP server is exposed by this host. Browser development remains available through `npm run dev` without native imports.

Use Node 26 and the pinned Rust toolchain. Install the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/) first.

```sh
npm ci
npm run desktop:build
npm run desktop:run
npm run desktop:test
npm run desktop:smoke
```

The smoke command opens the actual native window, checks that Vue rendered, round-trips lossless synthetic values, and exits. Its 15-second deadline prevents a stalled WebView being reported as a pass. This debug-only smoke is not a signed distribution test. Build embeds `apps/frontend/dist`; rebuild after frontend changes. Standard OS window decorations retain native dragging. Packaging/signing remains disabled pending M009.

## IPC boundary

The native adapter lives in `apps/desktop/src-tauri`. Shared core/contracts contain no Tauri types. Rust generates `contracts/generated/v1.ts`; the browser-safe `nativeHost.ts` bridge uses that contract without importing an SDK. Requests have a version, correlation ID and strict bounded payload. Unknown fields, unsupported versions, forged sessions, untrusted origins and auxiliary windows are rejected.

The application manifest explicitly registers protected commands. Only the main local window receives host-info, synthetic contract-probe and close-resolution permissions. No renderer filesystem, shell, dialog or remote-origin permissions are granted. The dialog plugin is used only by trusted Rust shutdown fallback code. CSP excludes remote scripts, frames and connections.

Native close/quit requests reuse the existing unsaved-change confirmation. Cancel preserves edits; an unresponsive renderer receives a native warning before forced close. The fallback dialog's manual interaction remains unverified. Failed IPC disables the bridge while preserving mock mode; it never implies a real connection.

## Local verification and remaining gates

macOS ARM real native smoke previously completed in 609 ms; latest verification results are recorded in M002's milestone. Four native IPC tests cover ACLs, exact values, protocol rejection and cancellation of close. Two browser-shim tests cover Light/Dark unsaved-close dialogs; they do not replace real WebView/OS testing. The original 17 browser scenarios remain intact.

Windows/Linux native execution, signed packaging, representative repeated startup/IPC benchmarks and OS-level fallback-dialog interaction remain pending. CI is prepared, not remotely executed.

A full-lockfile dependency audit currently fails on transitive Linux GTK dependencies: [GLib RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429) and [proc-macro-error RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370). GLib's fixed version is a different major API line; changing the lockfile alone cannot upgrade Tauri's GTK dependency graph. No advisory ignores or security exceptions have been added. M002 security acceptance and Linux distribution remain open pending a compatible upstream fix or an explicitly reviewed dependency patch. This does not affect the independently evaluated M001 core/contracts dependency graph.
