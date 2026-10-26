# SSH transport and secret references

**Status: M005 local implementation and evidence recorded; not complete.** Desktop live MySQL, MariaDB and PostgreSQL profiles can connect through an SSH tunnel, and their passwords/passphrases can be saved as OS secret references. Only macOS was executed. The choices below (D016–D020) were implemented under the user's 2026-10-26 continuation instruction and await review; numerical transport budgets, other operating systems and graphical OS interaction remain open gates.

## What is implemented

| Area | Behaviour |
| --- | --- |
| SSH library | `russh` 0.64.1, pure Rust, `ring` backend, default features off (no RSA, no `aws-lc-rs`) |
| Host keys | A connection requires an enrolled OpenSSH public key. Missing = refused before any network access (`SSH_HOST_KEY_UNKNOWN`); a different key = hard failure (`SSH_HOST_KEY_CHANGED`). Host certificates are refused. No trust-on-first-use |
| Enrollment | `ssh_host_key` reads the presented key and disconnects before authentication. The dialog shows the algorithm and SHA256 fingerprint; only **Trust this host key** stores it in the profile. **Forget host key** is the only rotation path |
| Authentication | Agent (socket from `SSH_AUTH_SOCK` captured by the host at startup), password, or a private-key file chosen in the native picker, with optional passphrase. Ed25519 and ECDSA (Ed25519 and P-256 exercised); RSA keys are refused as unsupported |
| Agent policy | The agent signs authentication challenges only and is closed after authentication; at most five identities are tried; agent forwarding, sessions, shells, X11 and remote forwards are never requested |
| Endpoint | One SSH session per logical connection handle. The runtime binds a private Unix socket in a new mode-0700 directory under the temporary directory and forwards each accepted stream through a fresh `direct-tcpip` channel. No TCP port is opened locally |
| TLS identity | The database adapter dials the socket but keeps `VerifyIdentity`/`VerifyFull` and the **original database hostname** for certificate verification. The SSH server resolves and reaches that hostname |
| Lifecycle | Tunnels belong to the runtime registry entry and close after its pool on disconnect, expiry, renderer reset and shutdown. Setup is bounded (SSH phase within the 3 s deadline, whole setup within 6 s), can be cancelled by attempt id, and partial state is dropped on every failure |
| Secrets | `store_secret` saves a value in the macOS login Keychain (service `app.dbadmin.desktop.credentials`) under a random UUID reference bound to its target and route; `delete_secret` forgets it. There is no command that returns a secret. Other operating systems return `UNSUPPORTED_CAPABILITY` |

SQLite has no network transport: an SSH request for a SQLite profile is refused. Teleport and AWS SSM remain unimplemented extension points.

## Secret references

Profiles persist only opaque references (`passwordRef`, `transport.passwordRef`, `transport.passphraseRef`), the enrolled public host key, its fingerprint and a key-file display name. Secrets, key-file grants and the draft-only *remember* toggles are never written to local storage.

Each Keychain item stores `{"v":1,"scope":…,"secret":…}`. The scope binds a database password to engine, account, host, port **and route** (direct, or the exact SSH endpoint); SSH passwords and passphrases are bound to the SSH endpoint and purpose. At connect time the host resolves a reference only when the request's target matches, so a renderer cannot replay a saved password against another server (`SECRET_UNAVAILABLE`). A typed value always wins over a reference; sending both is invalid.

Keychain calls run on a blocking worker. Changing the profile's target requires entering the secret again. Secrets are held in ordinary memory while connecting; memory erasure is not guaranteed.

## Native commands

| Command | Purpose |
| --- | --- |
| `ssh_host_key` | Scan the presented host key for explicit verification; never trusts it |
| `pick_ssh_key_file` | Native picker → opaque, owner-scoped key-file grant and file name; key bytes never cross IPC. Grants are typed, so a SQLite grant cannot be read as a key, and are revoked on renderer bootstrap/shutdown |
| `cancel_connect` | Cancel an owned in-flight setup by its renderer-chosen attempt id |
| `store_secret` / `delete_secret` | Write/delete-only OS secret references |

`connect_direct` gains optional `passwordRef`, `ssh` and `attemptId`. All commands are main-window only, session-bound and strictly decoded; a `privateKey`, `path` or `forwardAgent` field is rejected. The renderer still has no filesystem, shell or dialog permission.

## Verification

`npm run test:ssh` (local Docker only) builds a disposable Alpine OpenSSH 10.3 image and, per engine, starts the database plus two SSH servers with distinct generated Ed25519 host keys, generated client keys (one passphrase-protected), random passwords and a private `ssh-agent`. The database certificate names only `db.stoneql.test` (and loopback for the Direct baseline); a second alias `wrong.stoneql.test` reaches the same server. The runner filters `SSH_AUTH_SOCK`/`SSH_*` from the inherited environment, never reads `~/.ssh`, uses a per-run Keychain service and purges it, and verifies removal of containers, networks, agent, endpoint directories and temporary files.

Executed on macOS ARM (2026-10-26):

- MySQL 8.4.6 full matrix through the runtime: unknown key refused without network; enrolled key works; rotated server refused; Ed25519 and ECDSA P-256 key files work; wrong password, missing/wrong passphrase and unauthorized key refused; agent works, absent/dead agent refused; TLS hostname mismatch and untrusted CA refused through the tunnel; owner-scoped handles and key grants; explicit and dropped-future cancellation; reset and expiry; 100 mixed setup/connect/disconnect/failure cycles with zero registered tunnels, pending setups, endpoint directories or descriptor growth.
- MySQL, MariaDB 11.4.8 and PostgreSQL 17.6 parity: password tunnel, three queries (each a fresh forwarded channel), TLS mismatch/untrusted CA, changed/unknown host key and wrong password.
- Actual Tauri command handlers (MockRuntime) for all three engines: host-key scan, Keychain references bound to target and route, refused replay to another target, key-file grant with passphrase reference, agent, failure codes, reload closing a tunnel-backed handle, and every created Keychain item deleted. No secret appeared in any reply, test output, report or SSH server log.
- Server side after each run: zero established SSH connections, zero `session` channels and zero agent-forwarding requests.
- Mutation: accepting any host key makes the fixture fail at the changed-key case.

SSH overhead was characterized against the Direct loopback baseline on Docker Desktop (debug build, 20 connects and 100 `SELECT 1` each): Direct connect p50/p95 3.3/6.9 ms, SSH 22.3/23.9 ms (host-key check, authentication, channel and TLS); per-query p50/p95 3.6/6.7 ms Direct and 4.7/4.9 ms SSH. These are not approved budgets. Evidence: `.ref/planning/m005-evidence/`.

## Limits and open gates

- Windows has no Unix-socket endpoint here and Linux was not executed; both, and Windows/Linux credential stores, are unsupported or unverified.
- The real file picker, Keychain prompts and graphical WebView-to-tunnel interaction were not automated. A Keychain prompt during connect could exceed the renderer's 8 s timeout.
- RSA user and host keys are unsupported until the `rsa` crate advisory ([RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)) is resolved or explicitly risk-accepted.
- `~/.ssh/known_hosts`, `~/.ssh/config`, jump hosts, keyboard-interactive/2FA, host certificates and agent confirmation prompts are not supported.
- Key-file grants are session-only: choose the key again after restarting.
- A tunnel whose SSH session dies (keepalive exhausted) makes the handle report disconnected, but its registry slot is released only on explicit disconnect, reset or the one-hour expiry (as for a dead Direct pool).
- Windows compilation/Clippy of the Unix-only endpoint gating was not executed locally; CI is prepared, not remotely run.
- The SSH fixture disables OpenSSH `PerSourcePenalties` because it deliberately aborts many handshakes; real servers may throttle a client that cancels repeatedly.
- G4/G5 transport budgets, cross-OS agent/keyring behaviour and decisions D016–D020 need review before M005 can be accepted.
