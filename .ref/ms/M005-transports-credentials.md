# M005 — SSH and secure credentials

## Status
Local implementation and evidence recorded (2026-10-26, under the user's continuation instruction); not complete. Implemented: desktop SSH tunnels (agent, password, native key-file grant; verified enrolled host keys; private Unix-socket endpoint per handle; original hostname kept for TLS) owned by the runtime registry with cancellable, bounded setup, and macOS Keychain secret references bound to target and route (D016–D020, awaiting review). Executed evidence: `.ref/planning/m005-evidence/` (MySQL 8.4.6 full matrix including 100 mixed cycles with zero orphan tunnels/handles; MariaDB 11.4.8 and PostgreSQL 17.6 parity; actual command handlers with Keychain references; server-side no sessions/agent forwarding; trust-all mutation fails).

Open gates: cross-OS — Windows (no Unix-socket endpoint; explicitly unsupported), Linux execution, and Windows/Linux credential stores and agents (Pageant/OpenSSH for Windows); real native key/file picker, Keychain prompts and graphical WebView-to-tunnel interaction (tests use Tauri MockRuntime and trusted grants); approved G4/G5 transport budget (SSH overhead only characterized: debug, Docker Desktop loopback) and the approved desktop OS/auth-method matrix; RSA keys (blocked by RUSTSEC-2023-0071) and host certificates, `known_hosts`/`ssh_config`/jump hosts/keyboard-interactive; renderer 8 s timeout vs. a Keychain prompt during connect; memory erasure of secrets; dead-tunnel slots released only on disconnect/reset/expiry; Windows compile/Clippy of the Unix-only gating unexecuted (CI prepared, not run); ECDSA P-384/P-521 untested; review of D005/D006 and D016–D020. Teleport/SSM remain unimplemented; no server vault. No push, publication or deployment is authorized.

## Objective and scope
Desktop secret references and Direct/SSH transport integration; allow work as soon as connection ownership/cleanup is stable. Do not implement Teleport, SSM or a server vault here.

## Dependencies and prerequisites
Accepted M003-L lifecycle checkpoint and M001 contracts; approve D005 desktop provider and D006 trust/agent policy. **M004, full M003 SQL/UI completion, M008 and server-vault selection are not prerequisites.**

## Technical approach
Compose verified SSH agent/key/password transports with the existing database lifecycle. Preserve original database identity for TLS through forwarding. Define secret provider and endpoint/cleanup boundaries without host-framework imports in shared crates.

## Deliverables
Secure desktop provider binding, transport lifecycle and MySQL/MariaDB-over-SSH evidence; later engine parity added when those adapters are ready.

## Acceptance criteria
- G1: unknown/changed host keys fail or require explicit verified enrollment; certificates remain verified; no secrets in profile/browser/logs/arguments or unintended agent forwarding. Private keys/passphrases stay within approved host/secret boundaries.
- G2: forwarded endpoint never changes database/profile identity or raw values; credential/session isolation persists; interrupted setup never leaves a usable misleading connection handle.
- G3: approved desktop OS/auth-method matrix passes with MySQL/MariaDB; unsupported engine/transport pairs are rejected. PostgreSQL support is a later parity test, not a start gate; SQLite has no implied network tunnel.
- G4/G5: 100 setup/connect/disconnect/failure cycles with zero orphan tunnels/handles within the approved cleanup bound; measure SSH overhead separately from the Direct baseline and approve a transport budget before completion.
- Future Teleport/SSM variants remain unimplemented, with owned endpoint/expiry interfaces available for later review.

## Required tests and verification
Host-key rejection/rotation; failed auth, agent unavailable, passphrase expiry, TLS mismatch through tunnel, cancellation of setup, crash/disconnect cleanup and credential isolation; lifecycle measurements per supported OS/auth path.

## Risks and limitations
Agent/keyring differences and host-key enrollment require human policy decisions. Server credential isolation is owned by M008/M011; it must not delay desktop SSH.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
