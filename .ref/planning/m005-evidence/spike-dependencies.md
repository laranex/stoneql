# M005 bounded spikes (2026-10-26)

Scratch crates outside the repository, pinned Rust 1.99.1, `cargo-audit` with RustSec advisory DB revision `115e418ee9a2` (2026-10-26). No code from the spikes was committed.

## SSH client library (D016)

| Option | Result |
| --- | --- |
| `russh 0.64.1`, `default-features = false, features = ["ring"]` (+ `security-framework 3.7.0`) | Builds on 1.99.1 in ~13 s; lock 180 crates; `cargo audit`: 0 vulnerabilities, 0 warnings. Supports Ed25519/ECDSA keys, OpenSSH/PKCS#8 (encrypted) key files, agent client, `direct-tcpip`. Apache-2.0 |
| `russh 0.64.1` default features | Lock 184 crates incl. `rsa` → **RUSTSEC-2023-0071** (Marvin timing side channel, `patched = []`), plus `aws-lc-rs` (C/CMake build). Rejected under the no-advisory-waiver policy |
| `russh 0.65.0` | Published 2026-10-25 (one day old); not reviewed; 0.64.1 already contains the RUSTSEC-2026-0154 fix (patched `>=0.60.3`) |
| `ssh2` (libssh2) | Requires libssh2 + OpenSSL C toolchains on each OS; rejected as larger native surface |
| System `ssh -L` child process | Passwords/passphrases need askpass helpers; inherits user `ssh_config`/`known_hosts`/agent policy; child-process orphan risk on crash; rejected |

Root workspace after integration: `cargo audit` scans 575 crates and reports only the two pre-existing Linux GTK warnings (RUSTSEC-2024-0370 proc-macro-error, RUSTSEC-2024-0429 glib); no new advisory, no ignores added.

## OS secret provider (D020)

| Option | Result |
| --- | --- |
| `security-framework 3.7.0` directly (macOS target dependency) | `set/get/delete_generic_password`; no new advisories (RUSTSEC-2017-0003 affects <0.1.12 only). One verified platform; other OSes compile to an explicit `Unsupported` provider |
| `keyring 3.6.3` (apple/windows/secret-service features) | Small graph (34 crates) and 0 advisories, but enables Windows and Linux backends that cannot be verified here; would present unverified providers as working. Kept as the likely future option once those OSes are tested (reversible behind the `SecretStore` trait) |

## Endpoint / TLS identity (D018)

Source review of SQLx 0.9.0 (`sqlx-mysql` vendored copy, unmodified; `sqlx-postgres` registry copy):

- MySQL `establish`: `options.socket` selects `connect_uds`, while `tls::maybe_upgrade` uses `hostname: &options.host` and the same `ssl_mode` → TLS with original-host verification over a Unix socket.
- PostgreSQL `PgStream::connect`: `fetch_socket()` = `<socket>/.s.PGSQL.<port>`; `MaybeUpgradeTls` runs for both TCP and Unix streams with `hostname: &options.host`. (D011's refusal of `/`-prefixed hosts is unchanged; the tunnel sets `socket(...)` explicitly.)
- Confirmed by fixture: `wrong.stoneql.test` (same server, outside SAN) and an untrusted CA are refused through the tunnel for all three engines.
- russh `Handle` drop closes the session loop (`receiver.recv()` → `None` → disconnect); verified server-side as zero established connections after every run.

## OpenSSH fixture note

OpenSSH 10 `PerSourcePenalties` throttled the fixture after deliberate pre-auth aborts (cancellation/failure cycles); it is disabled in the fixture `sshd_config` only.
