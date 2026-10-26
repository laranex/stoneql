# M005 desktop secret references and SSH transport evidence

Executed locally on 2026-10-26 under the user's continuation instruction. macOS ARM (Darwin 27), Node 26.11.1, Rust 1.99.1, Docker Desktop (`desktop-linux`). No secrets, keys, certificates, credentials or user data are retained here; reports were scanned for every generated secret (0 found).

## Files

- `ssh-mysql.json`, `ssh-mariadb.json`, `ssh-postgresql.json`: `npm run test:ssh` reports. `runtime` = `crates/stoneql-runtime/tests/ssh_tunnel.rs` (MySQL `full` matrix; MariaDB/PostgreSQL `parity`); `native` = `apps/desktop/src-tauri/tests/live_ssh.rs` through actual Tauri command handlers (MockRuntime) with a per-run Keychain service; `server` = OpenSSH server-side counts from logs/netstat; `keychainItemsRemaining` from `security find-generic-password` on the per-run service only.
- `ssh-cleanup-*.json`: containers/networks remaining, agent stopped, endpoint directories left, Keychain items purged by the runner, temporary directory removed.
- `spike-dependencies.md`: library/provider comparison, audit results and SQLx socket/TLS source findings.
- `mutation-trust-all.txt`: host-key verification mutation result.
- `verification.json`: every check executed for this increment, with counts.
- `source-sha256.json`: hashes of the evaluated source.

## Fixture design

Per engine: digest-pinned database image with a generated CA and a server certificate whose SAN is only `db.stoneql.test` and `127.0.0.1` (Direct baseline); network aliases `db.stoneql.test` and `wrong.stoneql.test`; two `alpine:3.24@sha256:71e9b7…` + `openssh-server 10.3_p1-r1` containers with distinct generated Ed25519 host keys (`ssh` enrolled, `ssh-rotated` for the changed-key case); generated client keys (Ed25519 plain, Ed25519 passphrase-protected, ECDSA P-256, unauthorized); random SSH/DB passwords; a private `ssh-agent -D -a` holding only the plain key. `sshd_config`: `AllowTcpForwarding local`, `PermitOpen` limited to the two aliases, `AllowAgentForwarding no`, `PermitTTY no`, `ForceCommand /bin/false`, `LogLevel DEBUG1`, `PerSourcePenalties no`. Loopback-only published ports; inherited `SSH_*` and database variables filtered; `~/.ssh` and the user's agent/keychain items never read.

## Key results

- Host keys: unknown refused before network; enrolled accepted; rotated server refused (`HostKeyChanged` / `SSH_HOST_KEY_CHANGED`) for all engines; mutation accepting any key fails the fixture.
- Auth: password, Ed25519 key, ECDSA P-256 key, passphrase key (typed and Keychain reference), private agent all succeed; wrong password, missing/wrong passphrase, unauthorized key → `SshAuthentication`; absent/dead agent → `AgentUnavailable`.
- TLS through tunnel: hostname alias outside SAN and untrusted CA refused; original hostname verified.
- Lifecycle (MySQL): explicit cancel (~153 ms incl. 150 ms wait), foreign cancel refused, 20 dropped setups at 0.5–19.5 ms (none completed first), reset and 1 s expiry close tunnels; 100 cycles (20 each connected/wrong password/changed key/TLS mismatch/cancelled) → 0 registered tunnels, 0 pending setups, 0 endpoint directories, descriptors 7 → 7; server-side 0 established connections after each run, 0 `session` channels, 0 `auth-agent-req`.
- Isolation: owner-scoped handles/key grants, separate tunnels per handle, peer disconnect independent; Keychain references refused for another target/route.
- Secrets: 0 occurrences in IPC replies, test stdout/stderr, reports and SSH server logs.
- Overhead (characterization only, debug, Docker Desktop loopback; 20 connects, 100 `SELECT 1`): Direct connect p50/p95 3.299/6.899 ms vs SSH 22.255/23.948 ms; query 3.635/6.733 ms vs 4.685/4.882 ms. Not an approved budget.

## Limits

MockRuntime command handlers and trusted grants, not the graphical WebView, real file picker or Keychain prompts. One OS (macOS ARM); Windows has no Unix-socket endpoint (explicitly unsupported) and Linux was not executed. MariaDB/PostgreSQL ran the parity subset, not the 100-cycle matrix. RSA keys, host certificates, `known_hosts`/`ssh_config`, jump hosts and keyboard-interactive are unsupported. Transport budgets and decisions D016–D020 await review. M005 is not complete.
