# M005 — SSH and secure credentials

## Status
Local implementation and evidence recorded (2026-10-26, under the user's continuation instruction); not complete. Implemented: desktop SSH tunnels (agent, password, native key-file grant; verified enrolled host keys; private Unix-socket endpoint per handle; original hostname kept for TLS) owned by the runtime registry with cancellable, bounded setup, and macOS Keychain secret references bound to target and route (D016–D020, awaiting review). Executed evidence: `.ref/planning/m005-evidence/` (MySQL 8.4.6 full matrix including 100 mixed cycles with zero orphan tunnels/handles; MariaDB 11.4.8 and PostgreSQL 17.6 parity; actual command handlers with Keychain references; server-side no sessions/agent forwarding; trust-all mutation fails).

Open gates: cross-OS — Windows (no Unix-socket endpoint; explicitly unsupported), Linux execution, and Windows/Linux credential stores and agents (Pageant/OpenSSH for Windows); real native key/file picker, Keychain prompts and graphical WebView-to-tunnel interaction (tests use Tauri MockRuntime and trusted grants); approved G4/G5 transport budget (SSH overhead only characterized: debug, Docker Desktop loopback) and the approved desktop OS/auth-method matrix; RSA keys (blocked by RUSTSEC-2023-0071) and host certificates, `known_hosts`/`ssh_config`/jump hosts/keyboard-interactive; renderer 8 s timeout vs. a Keychain prompt during connect; memory erasure of secrets; dead-tunnel slots released only on disconnect/reset/expiry; Windows compile/Clippy of the Unix-only gating unexecuted (CI prepared, not run); ECDSA P-384/P-521 untested; review of D005/D006 and D016–D020. Teleport/SSM remain unimplemented; no server vault. No push, publication or deployment is authorized.

## Security review — 2026-10-26
Independent review of commit `e908c49`: `.ref/planning/m005-security-review.md`. No Critical findings; the host-key policy, verified TLS through the tunnel, strict IPC decoding and no secret read-back hold. Recommended before the macOS alpha (not yet fixed; planning-only pass): F-01 (High, needs decision) the enrolled host key is stored in and supplied by the renderer — move enrollment native, bind secret scopes to the key fingerprint, or record risk acceptance; F-02 (Medium) Keychain resolution happens before the cancellable, bounded setup and a renderer timeout can leave an invisible handle holding a slot for up to 1 h; F-03 (Medium) dead tunnels keep their slot until disconnect/reset/expiry; F-04 (Medium) editing SSH host/port keeps the old key displayed as trusted. Lower findings F-05–F-19 (algorithm pinning, RSA error mapping, scope canonicalisation, secret-op admission, orphan Keychain items, same-user socket reachability, crash sweep, zeroization) and missing tests (ACL negatives for the five new commands, dead-tunnel, slow/locked Keychain, field rejection) are listed there. Decisions needing approval: F-01 approach, F-02/F-03 as blockers, RSA and keyboard-interactive policy, signing identity/Keychain data-protection, russh upgrade policy, transport budgets. M005 remains not complete.

## Approved review decisions and alpha release blockers — 2026-10-26
**Approved — user, 2026-10-26:**

- Native Rust code owns SSH host-key trust (F-01 option a). Enrollment uses a native store, and the renderer no longer supplies the trusted key.
- F-01, F-02, F-03, F-04 and secret-deletion error handling (the deletion part of F-10) are v0.1.0 Alpha release blockers.
- These controls stay as they are: explicit host-key enrollment, changed-key refusal, TLS hostname verification and credential isolation. No control may be weakened.
- Rust core and contracts gain no macOS host API. The native store lives in the desktop host behind a trait.

**Unresolved alpha release blockers (not implemented, none executed).** Plan: `.ref/planning/m005-alpha-fix-plan.md`.

- **F-01:** native host-key store keyed by `(host, port)`, bound to a native pending scan, with trust/forget/status commands. `hostKey` is removed from the connect request, and saved SSH passwords are bound to the enrolled fingerprint (plan tasks T4/T5).
- **F-02:** secret resolution inside the cancellable, bounded attempt; the renderer cancels on timeout and closes late successes (T1/T2). The proposed 60 s/75 s timeouts are the "reviewed change to the renderer's 8 s connect timeout" named in the D021 list below. They are the same item, not a second gate.
- **F-03:** automatic release of dead tunnels (T3).
- **F-04:** host/port edits invalidate the displayed trust and any pending key (T5).
- **F-10 deletion:** deletion failures are surfaced, never swallowed (T6).

Each blocker closes only against the plan's executed acceptance tests. Start the work after the uncommitted M006 work is committed: about 10 shared files overlap (listed in the plan).

Mechanism choices in the plan stay **Proposed / awaiting approval**:

- an app-data file rather than Keychain for the host-key store
- a native confirmation dialog on trust
- re-enrollment rather than import
- `ssh_password`-only fingerprint binding
- a 60 s native secret deadline and a 75 s renderer timeout when a reference is used
- keeping the profile with a "Remove anyway" choice when a deletion fails
- shared per-endpoint trust

Recommended non-blocking fixes (F-05..F-19) and the deferred and unsupported authentication methods are listed in the plan. RSA and keyboard-interactive policy still need a user decision.

## Platform scope for v0.1.0 Alpha (D021)
Approved product decision (user, 2026-10-26): StoneQL v0.1.0 Alpha is macOS-only — Apple Silicon (arm64) primary, Intel (x86_64) optional pending verification; Windows and Linux desktop deferred; Axum server a separate future release. Windows/Linux-specific implementation and testing must not block the alpha; shared crates gain no macOS dependency.

Required for the macOS alpha (Unresolved release blockers, none executed): the review blockers F-01–F-04 and F-10 deletion above; real macOS Keychain access prompts on the signed release build (first save, first use, after upgrade), including a prompt arriving within — or a reviewed change to — the renderer's 8 s connect timeout, and denial/cancel handling; the real native SSH key-file picker; graphical WebView → SSH tunnel → MySQL/MariaDB end-to-end on the signed artifact (agent, password, key file with passphrase, host-key enrollment and changed-key refusal, TLS refusal through the tunnel); surfaced (not ignored) secret-deletion failures and no orphan tunnels after quit/crash relaunch/sleep-wake; an approved G4/G5 transport budget measured on arm64 (x86_64 if shipped) with the release build; an approved macOS auth-method matrix; review of D005/D006 and D016–D020. Existing macOS ARM fixture evidence (100 mixed cycles, Keychain command-handler tests) is Implemented and verified input, not release evidence.

Deferred (not blocking v0.1.0 Alpha; remain M005 gates for the release that ships each platform): Windows SSH endpoint (no Unix-socket endpoint), Pageant/Windows OpenSSH agent, Windows compile/Clippy of the Unix-only gating, Linux execution and agent behaviour, and Windows/Linux credential stores (currently `UNSUPPORTED_CAPABILITY`). RSA keys, host certificates, known_hosts/ssh_config/jump hosts/keyboard-interactive remain documented unsupported limitations, not alpha blockers unless the alpha feature scope requires them.

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
