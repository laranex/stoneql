# SSH transport and secret references

**Status: M005 local implementation and evidence recorded; not complete.** Desktop live MySQL, MariaDB and PostgreSQL profiles can connect through an SSH tunnel, and their passwords/passphrases can be saved as OS secret references. SSH tunnels were executed on macOS and Linux; Windows has no tunnel endpoint yet. Secret stores: macOS Keychain, Windows DPAPI files, Linux Secret Service or a master-password file (all shipped in v0.1.0-beta.2). The choices below (D016–D020) were implemented under the user's 2026-10-26 continuation instruction and await review; numerical transport budgets, other operating systems and graphical OS interaction remain open gates.

## What is implemented

| Area | Behaviour |
| --- | --- |
| SSH library | `russh` 0.65.1 (upgraded from 0.64.1 for RUSTSEC-2026-0495, A4), pure Rust, `ring` backend, default features off (no RSA, no `aws-lc-rs`) |
| Host keys | A connection requires a host key enrolled in the **native trust store** (`<app data dir>/ssh-host-keys.json`, directory 0700, file 0600, atomic writes; a corrupt file is quarantined and every host becomes unknown). Trust is keyed by SSH host and port and shared by every profile that uses that endpoint, like `known_hosts`. The renderer never supplies or stores a key. Missing = refused before any secret is read, key file is read or network access (`SSH_HOST_KEY_UNKNOWN`); a different key = hard failure (`SSH_HOST_KEY_CHANGED`). Host certificates are refused. No trust-on-first-use |
| Enrollment | `ssh_host_key` reads the presented key, disconnects before authentication and remembers that scan natively (16 entries, 5 minutes, cleared on reload). **Trust this host key** sends only the fingerprint; the host enrolls the key *it* scanned for exactly that host and port, after a **native confirmation dialog** showing host, port, algorithm and fingerprint. A different enrolled key is refused (`SSH_HOST_KEY_CHANGED`): **Forget host key** (in-app confirmation) is the only rotation path. Editing the SSH host or port clears the shown trust and any pending scan |
| Authentication | Agent (socket from `SSH_AUTH_SOCK` captured by the host at startup), password, or a private-key file chosen in the native picker, with optional passphrase. Ed25519 and ECDSA (Ed25519 and P-256 exercised); RSA keys are refused as unsupported |
| Agent policy | The agent signs authentication challenges only and is closed after authentication; at most five identities are tried; agent forwarding, sessions, shells, X11 and remote forwards are never requested |
| Endpoint | One SSH session per logical connection handle. The runtime binds a private Unix socket in a new mode-0700 directory under the temporary directory and forwards each accepted stream through a fresh `direct-tcpip` channel, at most four at a time; a stream beyond that is closed at once. A forward ends as soon as StoneQL closes or discards its side, even while an abandoned statement still runs on the server. No TCP port is opened locally |
| TLS identity | The database adapter dials the socket but keeps `VerifyIdentity`/`VerifyFull` and the **original database hostname** for certificate verification. The SSH server resolves and reaches that hostname |
| Lifecycle | Tunnels belong to the runtime registry entry and close after its pool on disconnect, expiry, renderer reset and shutdown. Setup after secret resolution is bounded (SSH phase within the 3 s deadline, whole setup within 6 s), can be cancelled by attempt id, and partial state is dropped on every failure. Keychain resolution runs inside the registered attempt: **Cancel connecting** and a reload reach it, and it is bounded by a 60 s deadline (Proposed). A blocking Keychain call cannot be aborted, so a prompt may stay visible after a cancel; its result is discarded unused. The renderer waits 75 s (Proposed) when a saved reference is used, otherwise 8 s, cancels the attempt when it gives up and closes a connection that completes later. A tunnel whose SSH server dies or whose path stops answering keepalives (3 × 30 s) releases its handle, slot and endpoint directory automatically |
| Secrets | `store_secret` saves a value in the macOS login Keychain (service `me.thixpin.stoneql.credentials`) under a random UUID reference bound to its target and route; `delete_secret` forgets it. There is no command that returns a secret. Windows uses DPAPI-encrypted files ([Windows credential store](#windows-credential-store)); Linux uses the Secret Service or, without one, a master-password file ([Linux credential stores](#linux-credential-stores)). Other operating systems return `UNSUPPORTED_CAPABILITY` |

SQLite has no network transport: an SSH request for a SQLite profile is refused. Teleport and AWS SSM remain unimplemented extension points.

## Secret references

Profiles persist only opaque references (`passwordRef`, `transport.passwordRef`, `transport.passphraseRef`) and a key-file display name. Host keys live only in the native trust store; profiles saved by earlier builds lose their `hostKey`/`hostKeyFingerprint` on load, so those hosts must be fetched, verified and trusted again. Secrets, key-file grants and the draft-only *remember* toggles are never written to local storage.

Each Keychain item stores `{"v":1,"scope":…,"secret":…}`. The scope binds a database password to engine, account, host, port **and route** (direct, or the exact SSH endpoint); SSH passwords are additionally bound to the enrolled host-key fingerprint (scope v2): if the stored key is swapped or rotated, a saved SSH password no longer resolves and is never sent, and must be entered again. Saving an SSH password requires an enrolled host key. Passphrases are bound to the SSH endpoint and purpose. At connect time the host resolves a reference only when the request's target matches, so a renderer cannot replay a saved password against another server (`SECRET_UNAVAILABLE`). A typed value always wins over a reference; sending both is invalid.

Keychain calls run on a blocking worker. Changing the profile's target requires entering the secret again. Secrets are held in ordinary memory while connecting; memory erasure is not guaranteed.

## Windows credential store

Shipped in v0.1.0-beta.2; verified on GitHub's Windows Server 2025 runners, not on a real Windows 10/11 PC.

- One file per reference at `%APPDATA%\me.thixpin.stoneql.credentials\<reference>.bin`, holding the same `{"v":1,"scope":…,"secret":…}` JSON as on macOS, encrypted with **DPAPI** (`CryptProtectData`, current-user scope, no UI). The service name and reference are passed as entropy, so a file copied to another reference does not decrypt. Writes are atomic (temporary file, then rename).
- Only the default per-user ACL of `%APPDATA%` protects the files. DPAPI has no per-application ACL: **any process running as the same Windows user can decrypt them**, as with Credential Manager (which was not used because it caps blobs at 2,560 bytes, below the 16,384-byte secret limit).
- The DPAPI calls live in `crates/stoneql-dpapi`, compiled only on Windows and the workspace's only exception to `unsafe_code = "forbid"` (approved 2026-11-17).
- In the connection dialog the note reads "New passwords are saved encrypted for your Windows account (DPAPI) in your user profile." and a saved password's placeholder is `Saved (Windows encrypted)`.

## Linux credential stores

Shipped in v0.1.0-beta.2 (`.deb`). Verified on the Ubuntu 26.04 build server, headless (see [Verification](./verification.md#platform-verification-status-v0-1-0-beta-2)); only GNOME Keyring was exercised as a Secret Service.

- **Secret Service** (GNOME Keyring, KWallet, KeePassXC) over the session D-Bus. Items carry a fixed label and only `service` and `account` attributes; the value is the same `{"v":1,"scope":…,"secret":…}` JSON as on macOS. The session uses the `plain` algorithm, so values cross the per-user session bus unencrypted; any process of the same user can read the items through the same API. A locked collection is unlocked only through the service's own prompt. If a Secret Service is present but refuses (no default collection, a dismissed or timed-out prompt, any error), saving and reading fail closed: there is no silent fallback.
- **Encrypted file** (`$XDG_DATA_HOME/me.thixpin.stoneql/credentials/`, default `~/.local/share/…`), used only when no Secret Service is on the session bus. The directory is created 0700 and opened without following symlinks; it and every file are checked by descriptor to be owned by the user with no group or other bits. Each reference is one 0600 file written atomically (temporary file, fsync, rename, directory fsync). A master password (at least 8 characters) is turned into a key with Argon2id (64 MiB, 3 passes, 1 lane, random 16-byte salt, recorded in a header file; about 0.15 s in a release build on the 4-vCPU build server), and each value is sealed with AES-256-GCM under a fresh random nonce, with the format version, service and reference as associated data, so a file copied or renamed to another reference does not open. The header holds a verifier that detects a wrong master password without trying items; a wrong password is refused after a fixed one-second delay, with no lockout and nothing wiped. The key stays in memory for the app session only (zeroized on drop); the master password crosses IPC once, and the host's copies (the request message and the decoded value) are zeroized after derivation; the renderer clears its input fields but cannot guarantee that JavaScript string copies are erased. A file reference whose vault no longer exists (after a reset) reads as missing. **Reset saved passwords** (offered when unlocking, after a confirmation) deletes the header and the item files only.
- **Which store holds a reference** is part of the reference: Secret Service references are version-4 UUIDs, file references version-8 UUIDs, so a reference is never looked up in the other store. A Secret Service that appears later receives new saves; existing file references keep resolving from the file. No migration exists or is needed: Linux could not save passwords before.
- **In the connection dialog**, a saved password's placeholder names the store that holds it, read from its reference (`Saved in keyring` or `Saved in encrypted file`; `Saved in Keychain` on macOS, `Saved (Windows encrypted)` on Windows), and the note under the password names where a new password will go. When a saved credential cannot be removed, the message names the same store: the desktop keyring (Secret Service items with `service` `me.thixpin.stoneql.credentials`, removable by hand there), or StoneQL's encrypted file (removed only by **Reset saved passwords**).

### Secret Service test (`npm run test:secret-service`)

Runs the Linux Secret Service provider against a throwaway keyring; on other operating systems it prints a skip line and exits. It covers a save and read round trip, replacing a value, the fixed item label and the `service`/`account` attributes, deletion, that a present Secret Service is always chosen over the encrypted file, and that a Secret Service which is present but unusable (a second keyring started without a login collection) fails closed with `SECRET_UNAVAILABLE` and creates no encrypted-file directory. It finishes with zero items left under its per-run service name.

Prerequisites (Linux only; no Docker): `dbus-daemon` and `busctl` (installed with D-Bus and systemd), `gnome-keyring` (`gnome-keyring-daemon`), `libsecret-tools` (`secret-tool`, used to count and purge leftover items) and the Rust toolchain. On Debian or Ubuntu: `sudo apt install gnome-keyring libsecret-tools`. A desktop session is not needed; it runs on a headless machine.

```sh
npm run test:secret-service
```

It never touches your keyring. `tools/secret-service-fixture.mjs` starts a private session bus (its own socket, no service directories, so nothing is D-Bus-activated) and a `gnome-keyring-daemon --components=secrets` whose `HOME`, `XDG_DATA_HOME` and `XDG_RUNTIME_DIR` are a new temporary directory. The login keyring there is unlocked with a random password generated for the run, piped on stdin and never printed or stored. The tests use a random `me.thixpin.stoneql.test-…` service name, purge it, stop both daemons and delete the directory, and the run fails if `~/.local/share/keyrings` changed. It does not exercise the unlock prompt of a real keyring, KWallet or KeePassXC.
- **Threat model of the encrypted file.** It protects saved passwords in a copied, synced or backed-up file. It does not protect against code running as the same user while StoneQL is unlocked (that code can read process memory or ask the running app to connect), nor against a weak master password, which can be guessed offline from a copied file at the Argon2id cost above.

Commands: `secret_store_status` (where new saves go: `keychain`, `secret_service`, `encrypted_file` or `unavailable`, plus whether the file exists and is unlocked), `unlock_secret_store` (create or unlock; `MASTER_PASSWORD_REJECTED` on a wrong password) and `reset_secret_store`. A save or connect that needs a locked file answers `SECRET_STORE_LOCKED` before using any secret; the renderer then asks for the master password and retries the same request once.

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
- Job cancellation through the tunnel (2026-10-29, all three engines; in-crate test that marks its own read-only SSH handle read-write; at the time both production gates refused SSH read-write, which A15 has since enabled for MySQL/MariaDB): with a free forward the cancel connection is delivered (`confirmed`, 12–68 ms); with all four forwards busy it is refused at once and the job ends `requested`/`unknown` after the 1.5 s grace period (about 1,502 ms), the lease connection is discarded, and all forwards are free again right after. Mutation: waiting for the server before ending a forward (the previous behaviour) leaves the discarded connection holding a forward and fails the fixture.

SSH overhead was characterized against the Direct loopback baseline on Docker Desktop (debug build, 20 connects and 100 `SELECT 1` each): Direct connect p50/p95 3.3/6.9 ms, SSH 22.3/23.9 ms (host-key check, authentication, channel and TLS); per-query p50/p95 3.6/6.7 ms Direct and 4.7/4.9 ms SSH. These are not approved budgets. Evidence: `.ref/planning/m005-evidence/`.

## Limits and open gates

- **Windows has no SSH tunnel yet.** The endpoint is a Unix socket; on Windows the tunnel is refused as unsupported. Both vendored drivers verify TLS against `host` and dial `socket` or `(host, port)`, so a Windows endpoint needs a vendored-driver patch. Two designs are **Proposed / awaiting approval**: a loopback TCP listener with a separate connect address, or an in-process stream with no listener (recommended, because a loopback port is reachable by every local process). Windows agent support (OpenSSH named pipe, then Pageant) is also pending.
- Linux: the Secret Service unlock prompt and a real desktop session were not exercised (headless build server only).
- The real file picker, Keychain prompts, the native trust dialog and graphical WebView-to-tunnel interaction were not automated (tests enroll through the trusted-caller seam `commit_ssh_host_key`).
- RSA user and host keys are unsupported until the `rsa` crate advisory ([RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)) is resolved or explicitly risk-accepted.
- `~/.ssh/known_hosts`, `~/.ssh/config`, jump hosts, keyboard-interactive/2FA, host certificates and agent confirmation prompts are not supported.
- Key-file grants are session-only: choose the key again after restarting.
- A tunnel whose SSH session dies (keepalive exhausted) releases its handle through the same close path as expiry (F-03; see *Lifecycle* above).
- Same-user reachability: the endpoint's 0700 directory keeps other users out, but any process running as the same user can connect to a live tunnel socket (no peer-process check yet). It still needs database credentials and must pass verified TLS.
- A failed secret deletion is reported: removing a profile keeps it and offers **Remove anyway** (naming the Keychain service); a save warns with the number of old items left. A renderer timeout during `store_secret` can still leave an unreferenced Keychain item.
- RSA agent identities are skipped silently and RSA-only host keys surface as a connection failure rather than an explicit unsupported error.
- 2026-10-26 security review findings, fixes and approval requests: internal `.ref/planning/m005-security-review.md` (F-01 host-key trust anchor held by the renderer; F-02 cancellation/timeout around Keychain resolution; F-03 dead-tunnel slot release; F-04 host/port edits keep the old trusted key).
- Windows compilation, Clippy and the Rust tests run on GitHub's `windows-2025` runners (`.github/workflows/rust.yml`).
- The SSH fixture disables OpenSSH `PerSourcePenalties` because it deliberately aborts many handshakes; real servers may throttle a client that cancels repeatedly.
- G4/G5 transport budgets, cross-OS agent/keyring behaviour and decisions D016–D020 need review before M005 can be accepted.
