# Windows desktop support — plan (branch `windows-support`, started 2026-11-17)

**Status: Proposed / in progress. No Windows claims in docs, README or releases until the release gates below pass.** Related gates: `.ref/ms/M004`, `M005`, `M009` (Deferred Windows items), D016–D020 (SSH), D005 (secrets).

## Current state (inventory 2026-11-17)

Most Unix-only code is already `cfg`-gated with non-Unix fallbacks, so a Windows build is expected to compile, but it would be badly limited:

| Area | Today on Windows | Code |
| --- | --- | --- |
| SSH tunnel | Refused (`Unsupported`): the local endpoint is a Unix socket and the drivers connect via `socket` | `crates/stoneql-runtime/src/ssh.rs` (`bind`, `#[cfg(not(unix))]`) |
| SSH agent | Unix socket (`SSH_AUTH_SOCK`) only; Windows agents use named pipes | `ssh.rs`, host `agent_socket` |
| Outcome journal | Fail-closed stub (`Unavailable`), so **read-write is refused** | `execution/journal.rs` (`#[cfg(not(unix))] impl Store`) |
| Saved passwords | `OsSecretStore` returns `Unsupported` | `apps/desktop/src-tauri/src/secrets.rs` |
| Host keys / transfer temp files | Fallbacks without owner-only permissions | `host_keys.rs`, `transfer.rs` |
| Web content crash handler | macOS only | `lib.rs` (`on_web_content_process_terminate`) |
| Packaging | ad-hoc macOS scripts use POSIX shell syntax (`$HOME`, inline `RUSTFLAGS=`) | `package.json` `desktop:*:adhoc` |
| Real-server tests | Docker Linux containers; not available on Windows runners | `tools/test-*.mjs` |

## Phases

1. **Build and CI facts (no product decision).** Gate Unix-only tests; non-gating `windows-2025` CI job on this branch whose steps continue after failures; run it by manual dispatch. Outcome: the complete list of compile/test failures.
2. **Credential store.** User preference (2026-11-17): save passwords "in user home dir with encryption". Implemented on the branch (`secrets/dpapi.rs`, unverified until the Windows CI run passes): one file per reference at `%APPDATA%\me.thixpin.stoneql.credentials\<reference>.bin`, encrypted with **DPAPI** (`CryptProtectData`, current-user scope, no UI) with the service and reference as entropy so a file copied to another reference does not decrypt; atomic temp-file + rename; the same scope binding as the Keychain items. Only `%APPDATA%`'s default per-user ACL protects the files. Limitation to document: DPAPI has no per-app ACL — any process running as the user can decrypt (Credential Manager is the same and caps blobs at 2,560 bytes, below `MAX_SECRET_BYTES` 16,384). Dependency: `windows-sys =0.61.2` (already in the lockfile via Tauri/tokio). **Approved 2026-11-17 (user): the DPAPI calls live in a new ~90-line crate `crates/stoneql-dpapi`, the workspace's only exception to `unsafe_code = "forbid"`** (`deny` plus a single `#[allow]` on the FFI function, `undocumented_unsafe_blocks = "deny"`, compiled only on Windows). `tests/workspace.test.mjs` fails if any other member stops inheriting the workspace lints or mentions `unsafe_code`.
3. **Outcome journal on Windows.** Keep the fail-closed stub until a port gives the same guarantees: no reparse-point following, single-process lock, atomic replace (`ReplaceFileW`/`MoveFileExW` with write-through), owner-only location. Then read-write can be enabled on Windows.
4. **SSH tunnel endpoint (decision needed).** Options: (a) loopback TCP listener with per-connection peer-process check — check which name the vendored drivers verify TLS against when connecting by host/port (tunnelled TLS must stay verified against the database host); (b) patch the vendored drivers to accept a custom stream, removing the local listener. Security decision for the user. Agent auth: Windows OpenSSH agent (named pipe) first, Pageant second; OpenSSH-format keys (`.ppk` via PuTTYgen conversion documented). No external PuTTY dependency: the built-in client (russh) is used on all platforms.
5. **Packaging.** Installer format (NSIS vs MSI), WebView2 delivery, unsigned-binary SmartScreen warnings (Authenticode signing deferred like Developer ID), Windows build scripts.
6. **Verification standard (decision needed).** Windows runners cannot run the Docker fixtures; options: native services on the runner (MySQL/PostgreSQL installers), a remote fixture host, or a stated gap.
7. **Docs and release** only after 2–6 pass.

## Decisions to ask the user (batched after phase 1)

- SSH tunnel endpoint design (4a vs 4b) and agent support order.
- Credential store dependency and the DPAPI limitation (confirm the preference above).
- Installer format, WebView2 mode, unsigned-installer stance.
- Windows real-server verification standard.
- Standing permission to push `windows-support` and dispatch CI on it (no automatic triggers).
