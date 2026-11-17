# M005 — SSH and secure credentials

## Status
**Open — approvals recorded 2026-11-17; remaining engineering gaps:** (1) native SSH trust confirmation dialog not exercised in a real window (G1); (2) A12 policy approved but not implemented in `970ade2` — no keyboard-interactive detect-and-refuse; RSA agent identities skipped silently and RSA host keys reported as connection failures rather than refused explicitly (G3); (3) SSH overhead not measured in a release build (G4/G5; the debug measurement is within the approved budget); (4) graphical WebView → SSH → database end-to-end on the packaged app not run (the user's QA of `970ade2` covers MySQL Direct only); (5) no orphan checks after quit, crash-relaunch or sleep/wake; (6) PostgreSQL read-write over SSH approved but not implemented (still refused before network access). Met: G2, 100-cycle/0-orphan part of G4/G5, Teleport/SSM boundary, secret-deletion failures surfaced, dependency audit at `970ade2`. A11, A12, A13 and A3 approved 2026-11-17; Keychain prompts and native E2E on a Developer ID build are M009 items, deferred by the user (A2). See "Acceptance record — 2026-11-17" (updated after the blanket approval). Earlier status text follows.

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

*2026-11-17 note: the five blockers below were implemented and verified on 2026-11-16 ("Acceptance record — 2026-11-16"); current status is in "Acceptance record — 2026-11-17". Text kept as history.*

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

*2026-11-17 note: several items in the next paragraph have since been executed or re-scoped (F-01–F-04/F-10 done; key-file picker run; Keychain prompts on a Developer ID build moved to M009 by the A2 closure); see "Acceptance record — 2026-11-17".*

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

## Acceptance record — 2026-11-16 (macOS arm64; MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6)
*Statuses in this table are superseded by "Acceptance record — 2026-11-17" at the end of this file; kept as history.*

Alpha fix plan tasks (`.ref/planning/m005-alpha-fix-plan.md`) — all executed:
| Task | Commits | Executed evidence |
|---|---|---|
| T0 tests-only hardening | `8295836`, `2c920f7` | Every IPC command (24, list parsed from `generate_handler!`) refuses auxiliary windows, remote origins, version 2 and unknown envelope fields; the five SSH/secret commands refuse forged sessions; 5/5 unknown credential fields rejected at wire and IPC level (< 1 s, unroutable target). |
| T1 F-02 native | `0e73b8d` | `Connections::begin_attempt`/`Attempt`/`connect_attempt`; `connect_direct` races resolution against cancel and `secret_deadline` (60 s, Proposed). IPC: cancel during a blocked Keychain read → `CANCELLED` < 1 s, 20 cycles, attempt unregistered; deadline 300 ms → `DEADLINE_EXCEEDED`; locked → `SECRET_UNAVAILABLE`. Runtime: reset cancels a held attempt, drop unregisters. Mutation (remove cancel arm) caught. |
| T2 F-02 renderer | `48b496f` | Timed-out connect cancels its attempt once and closes a late handle; 75 s wait with saved references (Proposed); fake-timer tests. |
| T3 F-03 dead tunnels | `404fb61`, `b0e5786` | Session-end signal from the russh handler drop; registry releases through the single close path; `serve()` stops. Fixture (`test:ssh`, dedicated `ssh-dead` service): killed server released in 101–102 ms (bound 5 s) on all three engines; paused path released by keepalive in 119,979 ms (bound 130 s, MySQL); 8 slots reusable, 0 endpoint directories, descriptors 7 → 7. |
| T4 F-01 native store | `a8ded89` | `SshTunnelRequest.host_key` removed (contracts regenerated, `contracts:check`); `host_keys.rs` file store (0700/0600, atomic, quarantine, limits) with unit tests incl. failed-write; pending native scans (16 / 5 min, cleared on reset/shutdown); `ssh_host_key_status` / `trust_ssh_host_key` (native dialog) / `forget_ssh_host_key`; lookup before secrets/key file/network; SSH password scope v2 bound to fingerprint. IPC + live fixture (all engines): wrong fingerprint / other endpoint refused, rotated commit `SSH_HOST_KEY_CHANGED`, swapped stored key → saved SSH password `SECRET_UNAVAILABLE`. |
| T5 F-01/F-04/F-05 renderer | `4420483` | Trust read natively; host/port edits clear pending key and trust and re-query (F-04); legacy `hostKey`/`hostKeyFingerprint` dropped on load/save; payload never carries `hostKey`; browser Light/Dark. |
| T6 F-10 deletion | `0020a35` | Removal keeps the profile and offers **Remove anyway**; save warns with a count; rollback failure reported; source check; native `SECRET_UNAVAILABLE` with item kept. |
| T7 docs and verification | `6103228`, `b0e5786` | `docs/development/ssh-credentials.md`, `docs/user-guide/connections.md`. 2026-11-16: `npm test` 208 + 19, build, `test:browser` 52/52, `docs:build`, `rust:fmt`, `rust:lint`, `rust:test` 157 (7 ignored), `rust:boundaries`, `contracts:check`, `rust:vendor`, `desktop:test` 34 (3 ignored), `test:ssh` all engines (190/41/41 forwarded channels, 0 session channels, 0 agent-forwarding requests, 0 secrets in logs/output, 0 leftovers). Evidence: `.ref/planning/m005-evidence/alpha-fixes-2026-11-16/`. |

| Criterion | Status |
|---|---|
| G1 host keys, certificates, no secrets in profile/logs, no agent forwarding | Verified (fixture + IPC). Native SSH key-file picker verified in a real window 2026-11-16 (A1, `journal_window picker`: cancel → no grant, chosen file → opaque grant). Native trust dialog and Keychain prompts need a Developer ID-signed build: the ad-hoc build of 2026-11-16 binds Keychain access to its CDHash, so its prompts would not represent release behaviour (not run) |
| G2 identity, credential isolation, interrupted setup leaves no handle | Verified (T1/T2/T4 tests, fixture) |
| G3 approved OS/auth-method matrix | Blocked (A3, A12): MySQL/MariaDB/PostgreSQL over agent/password/key file executed on macOS arm64 only |
| G4/G5 100 cycles, 0 orphans, transport budget | 100 mixed cycles 0 orphans (MySQL full matrix, re-run 2026-11-16); dead-tunnel release bounded; budget approval A3 |
| Dependency audit 0 vulnerabilities (plan T7) | Release graph verified 2026-11-16: russh upgraded to 0.65.1 (A4); `npm run audit:release` → 0 vulnerabilities, 0 warnings (366 packages, arm64+x86_64). Full `cargo audit` still reports RUSTSEC-2026-0459 (SQLite, test-only graph, deliberately not ignored) (see `.ref/planning/approvals-ledger.md` Results) | Verified for the release graph |

- 2026-11-16 re-run: `npm run test:ssh` passed on `ecb28b5` (includes `b0e5786` F-4/session-end changes): MySQL full matrix 190 forwarded channels, MariaDB/PostgreSQL parity 41 each, dead-tunnel cases passed again.

## Acceptance record — 2026-11-17 (supersedes the 2026-11-16 statuses above; macOS arm64; MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6)
Evidence base: `npm run test:ssh` 2026-11-17 on the working tree (`b0bfea3` and its predecessors): MySQL full matrix 195 forwarded channels and 0 session channels, MariaDB 46, PostgreSQL 41; `secretsInOutput` 0, `reportSecrets` 0 and Keychain items remaining 0 for all three engines (`artifacts/m005/ssh-{mysql,mariadb}.json` 12:42, `ssh-postgresql.json` 11:07, untracked); ledger A15 extension.

**Updated 2026-11-17 after the user's blanket approval and QA sign-off of `970ade2`** (ledger Results "2026-11-17 — blanket approval of outstanding release decisions"). `test:ssh` re-ran at `970ade2` and passed for MySQL 8.4.6, MariaDB 11.4.8 and PostgreSQL 17.6 (`artifacts/m005/ssh-*.json`, 14:32–14:33; `reportSecrets` 0 for all three); suites as in M002's updated record.

| Criterion | Status | Evidence or gap |
|---|---|---|
| G1 host keys, certificates, no secrets in profile/logs, no agent forwarding | Not met (engineering); A11 and A13 approved 2026-11-17 | Mechanisms (A11 = D024) and D005/D006/D016–D020 (A13) approved. Remaining: the native trust confirmation dialog has not been exercised in a real window. Earlier text: Fixture/IPC evidence 2026-11-16 re-run 2026-11-17; verified TLS required through the tunnel (D026 keeps TLS mandatory for SSH routes); key-file picker in a real window (A1, 2026-11-16). Mechanisms (app-data host-key file, trust dialog, 60 s / 75 s, "Remove anyway", per-endpoint trust, limits) need A11; D005/D006/D016–D020 need A13. Engineering gap: the native trust confirmation dialog has not been exercised in a real window. Keychain prompts on a Developer ID build: not performed; open for M009 by the A2 closure (2026-11-16). |
| G2 identity unchanged, credential isolation, interrupted setup leaves no handle | Met | Re-run in `test:ssh` at `970ade2`. Earlier text: T1/T2/T4 tests and fixtures (2026-11-16). 2026-11-17 host-path SSH read-write check (`live_ssh.rs`): `stoneql_writer` connects read-write through the tunnel, DDL/DML jobs succeed, a transaction rolls back (`native.sshReadWrite` in the MySQL and MariaDB reports). |
| G3 approved OS/auth-method matrix; unsupported pairs rejected | Not met (engineering); A3, A12, A13 approved 2026-11-17 | Approved matrix: macOS arm64; agent/password/key file; MySQL 8.4.6, MariaDB 11.4.8, PostgreSQL 17.6 — all passed `test:ssh` at `970ade2`. A12 (RSA unsupported; keyboard-interactive detect-and-refuse) is not implemented: there is no keyboard-interactive detection, and only RSA user key files are refused explicitly. Earlier text: Agent/password/key file on macOS arm64 for all three engines. Unsupported pairs rejected: PostgreSQL read-write over SSH refused before network access (runtime test above; `tests/ssh_read_write.rs`); RSA and keyboard-interactive unsupported pending A12. |
| G4/G5 100 cycles, 0 orphans; SSH overhead measured; transport budget approved | Part Met; budget approved 2026-11-17 (A3); release-build measurement Not met (engineering) | At `970ade2`: 100 mixed cycles (20 each outcome), `registeredTunnelsAfter` 0, endpoint directories 0, descriptors 7 → 7 (`artifacts/m005/ssh-mysql.json`). Against the approved budget (SSH connect p95 ≤ 6 s; per-query ≤ Direct + 50 ms p95, loopback): MySQL debug build connect p95 109.0 ms; `SELECT 1` p95 54.2 ms vs Direct 41.8 ms (+12.4 ms). Not measured in a release build or per auth method. Earlier text: 100 mixed cycles (20 each connected/cancelled/changed host key/TLS mismatch/wrong password), `registeredTunnelsAfter` 0, endpoint directories 0, descriptors 7 → 7 (`artifacts/m005/ssh-mysql.json` `runtime.full.cycles`, 2026-11-17); dead-tunnel release bounded (2026-11-16). Overhead characterized in debug only; a release-build measurement is engineering work. |
| Teleport/SSM unimplemented; owned endpoint/expiry interfaces | Met | No Teleport/SSM code; tunnels are owned by the runtime registry with expiry and a single close path (D018–D020, review A13). |
| Alpha: graphical WebView → SSH tunnel → MySQL/MariaDB end-to-end | Not met (engineering) | **2026-11-17:** the user's QA of `970ade2` covered MySQL Direct only, not SSH. Earlier text: Not run on the ad-hoc app (agent, password, key file with passphrase, enrollment and changed-key refusal, TLS refusal through the tunnel). Developer ID-signed run is M009 (A2 closure). |
| Alpha: no orphan tunnels after quit / crash relaunch / sleep-wake | Not met (engineering) | Dead-tunnel release verified; quit, crash-relaunch and sleep-wake cases not recorded. |
| Alpha: secret-deletion failures surfaced | Met | T6 (`0020a35`), 2026-11-16. |
| Alpha: dependency audit | Met (`970ade2`) | `audit:release` at `970ade2` passed: 366 release crates, no advisories (M009 RC evidence). Earlier text: Release graph clean 2026-11-16 (A4, russh 0.65.1); `flate2` added to the shipped binary 2026-11-17 without a recorded `audit:release` re-run. |
| Windows/Linux, Pageant, other credential stores | Deferred (D021) | Not run. |
