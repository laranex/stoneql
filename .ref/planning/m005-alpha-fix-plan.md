# M005 v0.1.0 Alpha fix plan — SSH host-key trust, setup cancellation, dead tunnels, secret deletion

**Status: implementation plan. Nothing here is implemented or verified.** Prepared 2026-10-26 from the independent review `m005-security-review.md` (reviewed commit `e908c49`). Planning-only pass: no application, test, vendor, Cargo or package files were changed and no git writes were made.

## Decisions this plan builds on

**Approved — user, 2026-10-26:**

1. Native Rust code owns SSH host-key trust (review F-01 option a: a native enrollment store). The renderer no longer supplies the trusted key.
2. F-01, F-02, F-03, F-04 and the deletion part of F-10 (secret-deletion error handling) are **v0.1.0 Alpha release blockers**.
3. These controls stay as they are: explicit host-key enrollment, changed-key rejection, TLS hostname verification and credential isolation. No control may be weakened.
4. Rust core and contracts stay free of macOS host APIs. The native store lives in the desktop host, or in a host-side module behind a trait.

**Proposed / awaiting approval** (choices made by this plan; see [Open questions](#open-questions-requiring-a-user-decision)): app-data file rather than Keychain for the store, a native confirmation dialog on trust, re-enrollment instead of import, fingerprint-bound SSH password scopes, the timeout values, and how a failed deletion behaves.

## Baseline and working-tree overlap

This plan was written against the committed M005 code at `e908c49`. An M006 implementation agent has **uncommitted** edits in the working tree (`git diff --stat`: 37 files). The planned M005 changes overlap these M006-edited files:

| File | M006 working-tree change | Planned M005 change |
|---|---|---|
| `crates/stoneql-runtime/src/lib.rs` | `connect_with` → `connect_with_access`, `Access`, job/lease cancellation in `Entry::close`/register, `register_or_close(…, access)` | F-02 attempt API; F-03 third expiry-task arm |
| `apps/desktop/src-tauri/src/database.rs` | `access()`; `connect_direct` calls `connect_with_access`; `connected()` reports `access` | F-02 resolution inside the attempt; F-01 host key from the store |
| `apps/desktop/src-tauri/src/lib.rs` | `mod execution`; five job/transaction commands in `invoke_handler` | three new host-key commands; `HostServices`/`NativeSession` fields |
| `apps/desktop/src-tauri/build.rs`, `capabilities/main-window.json` | five new commands/permissions | three new commands/permissions |
| `crates/stoneql-contracts/src/v1.rs`, `contracts/generated/v1.ts` | `AccessModeDto`, job/transaction DTOs, `DirectConnectionRequest.access` | drop `SshTunnelRequest.host_key`; new host-key DTOs |
| `apps/frontend/src/services/nativeHost.ts` | `BUSY`/`CONFLICT` messages, `hostMessages`, union extended | union + interactive list; timeout cancel/late close |
| `apps/frontend/src/services/liveConnections.ts` | `openLive(…, access)`, job helpers | `sshTunnel` without `hostKey`; host-key commands; cancel on timeout |
| `apps/frontend/src/composables/useConnectionWorkflow.ts` | `confirmReadWrite`, `access` in draft/save | trust/forget/status, F-04 invalidation, F-10 deletion handling |
| `apps/frontend/src/App.vue`, `src/types/index.ts` | read-write UI, types | host-key row; drop `hostKey`/`hostKeyFingerprint` |
| `docs/development/ssh-credentials.md`, `docs/user-guide/connections.md` | small edits | behaviour updates |

M006 has not touched these files: `crates/stoneql-runtime/src/ssh.rs`, `apps/desktop/src-tauri/src/{secrets.rs,ssh_transport.rs}`, `apps/frontend/src/services/connectionPreferences.ts`, `apps/desktop/src-tauri/tests/{ipc.rs,live_ssh.rs}`, `crates/stoneql-runtime/tests/ssh_tunnel.rs`.

**Rule:** start implementation only after the M006 work is committed or explicitly handed over. Rebase every task onto that tree, not onto `e908c49`. Function names below refer to `e908c49` (for example `connect_with`); apply them to whatever M006 left (`connect_with_access`). Never discard or overwrite M006 edits.

## Task order

```
T0 tests-only hardening ──┬─> T1 F-02 native ─> T2 F-02 renderer ──┐
                          ├─> T3 F-03 dead-tunnel release ──────────┤
                          └─> T4 F-01 native store + IPC ─> T5 F-01/F-04/F-05 UI + migration ─┤
                              T6 F-10 deletion surfacing (independent) ────────────────────────┤
                                                                     T7 docs, evidence, full re-run
```

- T1 and T4 both edit the resolution block in `connect_direct`, so do T1 first and build T4 on top of it.
- T3 and T6 are independent and can run in parallel with T1, T2, T4 and T5.

| Task | Blocker | Size |
|---|---|---|
| T0 Tests-only hardening (no behaviour change) | supports all | S |
| T1 F-02 native: register the attempt before resolving secrets; bound and cancel resolution | F-02 | M |
| T2 F-02 renderer: cancel on timeout; close late successes | F-02 | S |
| T3 F-03 dead-tunnel release | F-03 | M |
| T4 F-01 native host-key store, pending scan, trust/forget/status IPC, fingerprint-bound scope | F-01 | L |
| T5 F-01 renderer, F-04 invalidation, F-05 removal, migration | F-01, F-04 | M |
| T6 F-10 surface secret-deletion failures | F-10 (deletion) | S |
| T7 Docs, decision corrections, evidence, full verification and mutation re-run | all | S |

---

## T0 — Tests-only hardening (S)

These tests lock in current behaviour so later tasks can be checked against it. They do not change behaviour.

- `apps/desktop/src-tauri/tests/ipc.rs`, in `database_commands_enforce_session_acl_engine_and_secret_boundaries_before_network`:
  - Extend the command loop to `ssh_host_key`, `pick_ssh_key_file`, `cancel_connect`, `store_secret` and `delete_secret`.
  - For each command, assert rejection from three callers: the `auxiliary` window, the origin `https://untrusted.invalid`, and a stale `sessionId` (expect `FORBIDDEN`).
  - T4 adds its three new commands to the same loop.
  - The M006 commands (`submit_job` and the others) belong in this loop too. That is M006's work, but flag it if they are missing.
- `ipc.rs`, new test `ssh_requests_reject_unknown_credential_fields`: `connect_direct` with `ssh.privateKey`, `ssh.path`, `ssh.forwardAgent`, `ssh.auth.privateKey` or `ssh.auth.path` → `INVALID_REQUEST`. Check that no network I/O happens: use the unroutable host `192.0.2.1:9` and expect the call to return in under 1 s.
- `crates/stoneql-contracts/tests/wire.rs`: the same unknown-field rejection at `decode_envelope` level.

**Acceptance:** 8 command × 3 caller negatives fail closed (24/24 assertions). 5/5 unknown-field payloads are rejected. `rust:test` and `desktop:test` pass.

---

## T1 — F-02 native: resolve secrets inside the cancellable, bounded attempt (M)

**Problem (from e908c49).**

- `database.rs::connect_direct` resolves references in `blocking()` before `Connections::connect_with` calls `begin_attempt`. While a Keychain prompt is showing, `cancel_connect` finds no attempt and returns `false`.
- Resolution has no deadline.

**Chosen mechanism.** The host holds an attempt registered by the runtime. (Rejected alternative: a host-only cancel flag. It would duplicate the runtime's owner-scoped attempt registry.)

1. **`crates/stoneql-runtime/src/lib.rs`**
   - Add `pub struct Attempt { _guard: AttemptGuard, cancelled: oneshot::Receiver<()> }`.
   - Add `pub fn Connections::begin_attempt(&self, owner, attempt: Option<&str>) -> Result<Attempt, LifecycleError>`, built from the existing private `begin_attempt`.
   - Give `Attempt` an `async fn cancelled(&mut self)` that resolves when cancel or reset fires.
   - Change `connect_with` (M006: `connect_with_access`) to take `attempt: Option<Attempt>` instead of `Option<&str>`. When `None`, it creates an anonymous attempt as it does today. Inside, it races setup against the passed attempt's receiver.
   - Keep `connect()` unchanged for callers without an attempt id.
   - The runtime stays host-independent: it only gets a public type plus one method.
2. **`apps/desktop/src-tauri/src/database.rs::connect_direct`**
   - Validate the input, then call `session.connections.begin_attempt(owner, input.attempt_id)`. A duplicate attempt id gives `INVALID_REQUEST`, as today.
   - Then race the existing `blocking(resolve…)` future against `attempt.cancelled()` and `tokio::time::sleep(session.secret_deadline)`:
     - cancel → `CANCELLED`
     - deadline → `DEADLINE_EXCEEDED`
     - resolver error → its code (`SECRET_UNAVAILABLE` and so on)
   - Only after resolution, call `connect_with(…, Some(attempt))`. The capacity permit is still taken inside `connect_with`, after resolution, so a Keychain prompt never holds one of the 8 slots.
   - `spawn_blocking` cannot be aborted. A cancelled or timed-out resolution keeps running on its worker until the Keychain call returns. Its result is then dropped unused: no handle, no tunnel, no network I/O. The macOS prompt can stay on screen after the cancel; document this.
3. **`apps/desktop/src-tauri/src/lib.rs`**: add a `secret_deadline: Duration` field to `HostServices`. `system()` sets it to **60 s** (proposed). Tests pass e.g. 300 ms. `NativeSession::new` copies it.
4. **Not planned:** the review's native attempt→handle map. The renderer-side late close in T2 is smaller and covers the same race (the renderer still holds the invoke promise). Renderer reload is already covered by bootstrap `reset()`. Revisit only if T2's tests show a gap.

**Tests**

All of these use `ipc.rs` and MockRuntime with no network: the target `192.0.2.1` is never dialled because resolution blocks first.

- `SlowStore`: a `SecretStore` whose `get()` blocks on a `std::sync::Barrier` or `Condvar` until the test releases it. Variants: `LockedStore` returns `SecretError::Failed`; `NeverStore` sleeps longer than the deadline.
  - Cancel during resolution: start `connect_direct` with a `passwordRef` and `attemptId`, wait until `get()` is entered, then call `cancel_connect`.
    - Expected: `cancelled: true`; `connect_direct` → `CANCELLED` within 1 s of the cancel; `pending_setups() == 0`.
    - Then release the store and wait 2 s. Expected: 0 registered handles (the runtime exposes `open_tunnels`; add a test-visible `handles()` count if needed) and no reply carrying a handle.
  - Deadline: `NeverStore` with a 300 ms deadline → `DEADLINE_EXCEEDED` within 300 ms + 500 ms; pending 0; handles 0.
  - Locked: `LockedStore` → `SECRET_UNAVAILABLE`; pending 0; handles 0. This covers missing test 5 with a mock. The real locked keychain stays a manual alpha check.
  - Capacity: 20 cancel-during-resolution cycles, then 8 `register` calls succeed, so no slot leaked.
- Runtime unit test: `begin_attempt` + reset cancels a held `Attempt`; dropping an `Attempt` removes the pending entry.

**Acceptance:** all of the above pass, with 0 handles, 0 pending setups and 0 leaked permits after every case. `test:ssh` full matrix is unchanged: 100 mixed cycles with 0 orphans.

---

## T2 — F-02 renderer: cancel on timeout and close late successes (S)

**Files:** `apps/frontend/src/services/nativeHost.ts`, `apps/frontend/src/services/liveConnections.ts`. The workflow clears `attemptId` in `finally` and stays as it is.

1. **`nativeHost.ts::call`.** Keep the original `invoker.invoke` promise. Add an optional `onLate?: (payload: T) => void`. When the timeout wins the race and the invoke later resolves, `onLate` runs with the late payload.
2. **`nativeHost.ts::nativeDatabase`.** For `connect_direct` and `connect_sqlite`, pass `onLate` that calls `disconnect_database` with the late `handle`, swallowing only that cleanup call's own errors (log nothing).
   - Use the interactive timeout (proposed **75 s**, above the 60 s native deadline) for `connect_direct` when the payload carries `passwordRef`, `ssh.auth.passwordRef` or `ssh.auth.passphraseRef`. Otherwise keep 8 s.
   - Add `trust_ssh_host_key` (T4) to the interactive list, because it shows a native dialog.
3. **`liveConnections.ts::openLive`.** On a `DEADLINE_EXCEEDED` error when an `attemptId` was given, call `cancelConnect(attemptId)` once (best effort) before rethrowing. `store.connectLiveProfile` goes through `openLive`, so both the test path and the connect path are covered.

**Tests** (`apps/frontend/tests/nativeHost.test.ts`, `liveConnections.test.ts`, fake timers):

- An invoker that never resolves → after 8 s `DEADLINE_EXCEEDED`, with exactly one `cancel_connect` call carrying the same `attemptId`.
- An invoker that resolves with `{handle:"h1"}` at 8.5 s → exactly one `disconnect_database {handle:"h1"}`.
- A payload with `passwordRef` uses the 75 s timeout. Without references, it uses 8 s.

**Acceptance:** 100% of these pass. `npm test` and `npm run test:browser` pass.

---

## T3 — F-03 release dead tunnels automatically (M)

**Problem.**

- `Entry::usable()` hides a closed tunnel, but the entry, `_permit`, pool and endpoint directory stay until disconnect, reset or the 1 h expiry.
- `serve()` never exits on a closed session.
- russh 0.64.1 `client/mod.rs:1300-1335` runs `stream_write.shutdown().await?` *before* `Handler::disconnected`. On a dead TCP path that shutdown can fail and skip the callback. So the release signal must **not** rely on `disconnected` alone.

**Files and functions**

1. **`crates/stoneql-runtime/src/ssh.rs`**
   - `Verifier` gains `_alive: Option<tokio::sync::watch::Sender<()>>`. The handler is owned by the russh session task and is dropped when that task ends, on every path: remote disconnect, keepalive timeout, I/O error or our own disconnect. Dropping the sender completes every receiver's `changed()` with `Err`.
   - Both construction sites gain the field: `handshake` (called only from `open()`) creates the channel, and `scan`'s own `Verifier` literal sets `None`.
   - `Tunnel` stores the `watch::Receiver<()>` and exposes `pub(crate) fn closed(&self) -> impl Future<Output = ()> + Send + 'static`, which clones the receiver and awaits `changed()`.
   - `serve()` adds a `select!` arm on `closed` and `break`s. This also covers the review's request to exit when `session.is_closed()`.
2. **`crates/stoneql-runtime/src/lib.rs::register`**
   - Before `tunnel.take()`, capture `let tunnel_closed = tunnel.as_ref().map(Tunnel::closed)`.
   - In the spawned expiry task, add a third arm `_ = async { match tunnel_closed { Some(f) => f.await, None => std::future::pending().await } } => {}`. It falls through to the existing remove-and-`Entry::close()` path, which is the single close path. In M006's tree that path also cancels jobs and rolls back leases before the pool and tunnel close. Reuse it; do not add a second path.
   - `_expiry_cancel` drop (explicit disconnect or reset) still returns early, so nothing is closed twice.
3. No contract or renderer change. `database_status` already reports `false`. After removal the handle is `FORBIDDEN`, which the renderer already maps to "Reconnect".

**Tests** (`crates/stoneql-runtime/tests/ssh_tunnel.rs` fixture, run by `npm run test:ssh`). Keep each case separate so one failure does not hide the rest (addresses missing test 10 for these cases).

- **Server killed** (`docker kill` on the SSH container while the handle is open). Within **5 s**:
  - `is_connected == false`
  - `open_tunnels() == 0`
  - capacity fully available (8 `connect`s or `register`s admitted)
  - no `stoneql-ssh-*` directory left for this run
  - process fd count back to the pre-connect baseline ±0
- **Path dead, keepalive** (`docker pause`). The same assertions within **30 s × (3 + 1) + 10 s = 130 s**. Then `docker unpause` and clean up. This case adds about 2 minutes, so run it once per engine family (MySQL only); MariaDB and PostgreSQL get the kill case.
- Unit test (plumbing only): a `closed()` future built over a `watch` receiver resolves once its sender is dropped. `Tunnel`/`Session` cannot be built without a real handshake, so `serve()` exiting is proven by the fixture cases above.

**Acceptance:** both fixture cases pass with the bounds above. The 100-cycle matrix still shows 0 orphans. The user-guide sentence about dead tunnels is updated (T7).

---

## T4 — F-01 native host-key trust store and IPC (L)

### Data model and location

**Store: an app-data JSON file (recommended)** rather than a Keychain item.

| Criterion | App-data file (`<app_data_dir>/ssh-host-keys.json`) | Keychain generic password per `(host, port)` |
|---|---|---|
| Confidentiality needed? | No. Host keys are public. | Adds nothing. |
| Integrity against the renderer | Yes. The renderer has no fs/shell/dialog permission, so only native code writes it. | Yes. |
| Integrity against same-user processes | No (same as OpenSSH `known_hosts`). Mitigated by fingerprint-bound SSH password scopes (below): a tampered key cannot unlock saved SSH passwords. | Partial: ACL prompt for other apps. Unsigned or ad-hoc builds re-prompt on every rebuild (D020). |
| Cost per connect | One small file read. No prompt. | A Keychain read before setup. Can raise a prompt, which feeds F-02 latency. |
| Portability (shared later by Windows/Linux) | Plain `std::fs` | macOS-only. Needs another backend per OS. |
| Testability | Temp dir or in-memory trait impl | Per-run Keychain service with purge (as in D020 tests) |

**File format.** Versioned, strict, bounded:

```json
{"v":1,"hosts":[{"host":"bastion.example","port":22,"key":"ssh-ed25519 AAAA…"}]}
```

- **Key:** `(host.to_ascii_lowercase(), port)`. Exactly one key per endpoint.
- **Shared trust:** trust is shared by every profile that uses the same SSH endpoint, as in `known_hosts`. This is a behaviour change from per-profile keys, so document it.
- **No stored fingerprint or algorithm.** Both are derived from `key` on every load with the runtime parser (this resolves F-05).
- **Limits:** 512 entries; 8 KiB per key; 1 MiB file.
- **Location and writes.**
  - The path comes natively from `app.path().app_data_dir()` in `.setup()`. Until it is resolved, the store refuses (fail closed).
  - The directory is created with mode 0700 and the file is written with mode 0600.
  - Writes are atomic: temp file in the same directory, `sync_all`, `rename`. Writes are serialized by a `Mutex`.
- **Corruption** (parse error, wrong `v`, out-of-bounds or unparsable key): rename the file to `ssh-host-keys.json.corrupt-<unix-ts>` and start empty. Every host then becomes *unknown*, which is refused, so this fails closed. Re-enrollment needs the explicit flow. Never "repair" by guessing.
- **Bundle-identifier risk:** `app_data_dir` is derived from the bundle identifier (`app.dbadmin.desktop`; a change is still an open D021 item). Changing the identifier silently de-enrolls every host, which is safe but confusing. Settle the identifier before the alpha, or ship a native-only migration that copies the file from the old directory without the renderer.

**Placement (boundary rule).** New `apps/desktop/src-tauri/src/host_keys.rs`:

```rust
pub trait HostKeyStore: Send + Sync {
    fn get(&self, host: &str, port: u16) -> Result<Option<String>, HostKeyStoreError>;
    fn trust(&self, host: &str, port: u16, key: &str) -> Result<(), HostKeyStoreError>;
    fn forget(&self, host: &str, port: u16) -> Result<(), HostKeyStoreError>;
}
```

- Implementations: `FileHostKeyStore` (production) and `MemoryHostKeyStore` (tests). Both are injected through `HostServices { host_keys: Arc<dyn HostKeyStore>, … }`.
- `stoneql-core` and `stoneql-contracts` gain nothing.
- `stoneql-runtime` gains one public helper, `pub fn describe_host_key(openssh: &str) -> Result<HostKey, LifecycleError>` (wrapping `parse_host_key` + `describe`). It is used to derive fingerprints and to validate keys on load.
- `SshConfig.host_key: Option<String>` stays. It is now filled **only** from the native store.

### Pending scans (binds trust to what native code actually saw)

- `NativeSession` gains `pending_host_keys: Mutex<HashMap<(String, u16), (HostKey, Instant)>>`. It holds at most 16 entries (oldest evicted). Entries expire after **5 min** and are cleared on renderer bootstrap `reset` and on shutdown.
- `ssh_transport::ssh_host_key` (scan) records the scanned `HostKey` there, then returns it as today.

### IPC and contract changes (`crates/stoneql-contracts/src/v1.rs`, regenerate `contracts/generated/v1.ts`)

- **Remove `SshTunnelRequest.host_key`.** Strict round-trip decoding (`decode_envelope`) then rejects any `hostKey` field with `INVALID_REQUEST`. Nothing else changes in decoding.
- **New DTOs.**
  - `SshHostKeyTrustRequest { sessionId, host, port, fingerprint }`
  - `SshTrustedHostKey { algorithm, fingerprint }`
  - `SshHostKeyStatus { trusted: Option<SshTrustedHostKey> }`
  - Forget and status reuse `SshHostKeyRequest { sessionId, host, port }`.
- **New commands** in `ssh_transport.rs`. All of them call `authorize()` and `admit()` and are strictly decoded.

| Command | Behaviour |
|---|---|
| `ssh_host_key_status` | No network. Returns the enrolled algorithm and fingerprint for `(host, port)`, or none. |
| `trust_ssh_host_key` | Requires a non-expired pending scan for exactly `(host, port)` whose fingerprint equals the request's. Otherwise `INVALID_REQUEST`, and the store is unchanged. If a **different** key is already enrolled → `SSH_HOST_KEY_CHANGED`: rotation still requires an explicit forget first. If the same key is enrolled → idempotent success. Then shows the **native confirmation dialog** (below). Declined → `CANCELLED`, store unchanged. Approved → `HostKeyStore::trust`; returns `SshHostKeyStatus`. Store I/O failure → `INTERNAL`. |
| `forget_ssh_host_key` | Removes the entry. Idempotent. Returns `SshHostKeyStatus { trusted: None }`. Fail-closed (it can only cause refusals), so no native dialog. The renderer shows its existing in-app confirm (T5). |

- **Native confirmation dialog.** `tauri_plugin_dialog` `message(…).buttons(OkCancelCustom("Trust host key", "Cancel"))`, parented to the main window. This is the same plugin and pattern as the existing close fallback in `lib.rs`. Text:

  > Trust the SSH host key for `{host}:{port}`? `{algorithm}` `{fingerprint}`. Compare this fingerprint with one obtained from the server administrator. Connections will be refused if the key later changes.

  `host` has already passed `validate_endpoint`, so it contains no whitespace or control characters.

  **Why it is needed:** without it, a compromised renderer could still run forget → scan → trust against an attacker's server for the same `host:port`, given DNS or network control. Native storage alone does not close F-01. Mark it **Proposed** (see open questions). If it is declined, record the residual risk.
- **Trusted-caller seam for tests.** `pub fn commit_ssh_host_key<R>(app: &AppHandle<R>, host, port, fingerprint) -> Result<SshHostKeyStatus, ErrorDto>` runs the pending-scan match and the store write without the dialog. It mirrors `authorize_ssh_key_file`: "trusted native callers only; never an IPC command". The command calls it after approval. The real dialog stays a manual GUI check, like the real file picker.
- **Registration:** add the three commands to `lib.rs` `invoke_handler`, `build.rs` `commands(&[…])`, and `capabilities/main-window.json` (`allow-ssh-host-key-status`, `allow-trust-ssh-host-key`, `allow-forget-ssh-host-key`; update the description). No new plugin permission for the renderer: the dialog is opened natively.

### Connect path

- `ssh_transport::tunnel_config` takes `host_keys: &dyn HostKeyStore`. It sets `host_key = host_keys.get(&request.host, request.port)?` and maps a store error to `INTERNAL`, failing closed.
- The lookup runs before secret resolution and before reading the key file. Missing → `SSH_HOST_KEY_UNKNOWN` before any secret is resolved, any key file is read or any network I/O happens. This also fixes F-16.
- The runtime `Verifier` comparison is unchanged: byte-exact `key_data()`, certificate refused, no TOFU.

### Fingerprint-bound SSH secret scopes ("swapped key cannot unlock secrets")

- In `secrets.rs`, `ssh_password_scope(endpoint, fingerprint)` becomes `v2|ssh_password|user@host:port|<SHA256 fingerprint>`.
- `store_secret` for `SshPassword` looks up the enrolled fingerprint natively for `target.ssh`. If none is enrolled → `SSH_HOST_KEY_UNKNOWN`, so a password cannot be saved for an unenrolled host.
- The resolver uses the fingerprint of the key the native store will hand to the verifier for this connect.
- **Effect:** if the stored key is swapped (same-user file tampering, or forget plus re-enrolling a different key), a saved SSH password reference no longer resolves (`SECRET_UNAVAILABLE`) and is never sent.
- **Consequence:** a legitimate key rotation invalidates saved SSH passwords, and users must enter them again. The user guide must say so.
- **Not bound (rationale):**
  - `ssh_passphrase`: the passphrase only decrypts a local key file and is never sent to the server.
  - `database_password` via SSH: still protected by verified database TLS through the tunnel.
  - Binding these would only add re-entry on rotation. Open question 4 lets the user extend the binding.
- **Option (not required):** because the scope format changes anyway, F-08 (JSON or length-prefixed scope canonicalisation) can be done in the same change at no extra migration cost. If adopted, it invalidates every existing reference once, including direct database passwords. Before the alpha that is acceptable.

### Migration of renderer-stored keys — re-enrollment required (recommended)

- Any import path would take trust material from the renderer, which is exactly what the approved decision removes. A "re-confirm" import is no cheaper for the user than Fetch → compare → Trust.
- No public release has shipped, so only development profiles are affected.
- Mechanics (T5):
  - `sanitizeConnection` drops `hostKey`/`hostKeyFingerprint` on load and save, so the next save removes them from localStorage.
  - Existing saved SSH password references (v1 scope) stop resolving and show `SECRET_UNAVAILABLE` → "Enter it again". When a new password is saved, or remember is unticked, the old reference becomes obsolete and is deleted with T6's surfaced handling.

### Tests (T4)

- **Unit, `host_keys.rs`:**
  - round-trip
  - mode 0600 and directory 0700
  - atomic write: no temp file is left behind, including when `rename` fails
  - corrupt, oversized or bad-version files are quarantined and the store comes back empty
  - entry cap enforced
  - hosts keyed case-insensitively
  - fingerprint derived from the key (a forged fingerprint field is ignored and rejected by the strict schema)
- **IPC (`ipc.rs`, `MemoryHostKeyStore`, no network):**
  - `connect_direct` with `ssh.hostKey` → `INVALID_REQUEST`.
  - Unknown host → `SSH_HOST_KEY_UNKNOWN`. The `SlowStore` from T1 shows `get()` is **never called** and the target `192.0.2.1` is never dialled.
  - `commit_ssh_host_key` without a pending scan, with a fingerprint mismatch, with an expired scan, after a bootstrap reset, or over a different enrolled key → each rejected, store unchanged.
  - Status, forget and idempotent trust.
  - `store_secret(SshPassword)` for an unenrolled host → `SSH_HOST_KEY_UNKNOWN`.
  - The three new commands are added to T0's ACL loop.
- **Swapped key cannot unlock secrets (missing test 8).**
  - Unit: a reference stored under fingerprint A does not resolve under B.
  - Fixture (`live_ssh.rs`, actual command handlers): enroll server A, save an SSH password reference, then replace the stored key with server B's (direct store write as a same-user tamper, and forget plus commit of B).
  - Connect to B with the reference → `SECRET_UNAVAILABLE`. B's sshd log shows **0** password authentication attempts.
  - Typed passwords still follow the normal changed-key refusal: connecting to the rotated server with A enrolled → `SSH_HOST_KEY_CHANGED`.
- **Existing fixture matrix (`test:ssh`), all three engines:** unknown refused without network; enrolled accepted; rotated refused; TLS mismatch and untrusted CA refused through the tunnel; 100 mixed cycles with 0 orphans.
- **Mutation:** re-run the trust-all mutation (`Verifier` accepts any key). The fixture must still fail at the changed-key case. Add a second mutation: `tunnel_config` ignoring the store and using `None`/any key. The unknown and changed cases must fail.

**Acceptance (T4)**

- 0 code paths take a host key from IPC: `SshTunnelRequest` has no `host_key` field, and neither the renderer transport type nor `sshTunnel()` has a `hostKey` property. Other DTOs legitimately keep `SshHostKey*` names.
- All T4 tests pass.
- `contracts:check` regenerates deterministically.
- `rust:boundaries` shows that core and contracts gained no host or macOS dependency.

---

## T5 — F-01 renderer, F-04 invalidation, F-05 removal, migration (M)

**Files:** `apps/frontend/src/services/connectionPreferences.ts`, `src/types/index.ts`, `src/services/liveConnections.ts`, `src/composables/useConnectionWorkflow.ts`, `src/App.vue` (the host-key row near `e908c49` line 2494), `src/services/nativeHost.ts` (union).

- **Types and preferences.**
  - Delete `hostKey`/`hostKeyFingerprint` from the `ssh` transport type.
  - In `sanitizeConnection`, remove the `hostKey` helper and both fields, so they are dropped on load and never serialized (this resolves F-05).
- **Services.**
  - `sshTunnel()` no longer sends `hostKey`.
  - Add `hostKeyStatus(host, port)`, `trustHostKey(host, port, fingerprint)` and `forgetHostKey(host, port)`.
- **Workflow.**
  - New `hostKeyTrust = ref<SshTrustedHostKey | null | undefined>()`, where `undefined` means unknown or loading.
  - `newConnection` fetches the status for SSH live drafts.
  - A `watch` on `[transport.host, transport.port]` (F-04) immediately clears `pendingHostKey` and `hostKeyTrust`, then refreshes the status with a 300 ms debounce. This is a native lookup with no network I/O. `changeTransport` keeps resetting.
  - A pending scan for an old endpoint can never be trusted for the new one. Native code also rejects it, because the pending scan is keyed by `(host, port)`.
  - `trustHostKey` calls the native command; the native dialog appears. On success it sets `hostKeyTrust` and clears `pendingHostKey`. `CANCELLED` → keep the notice and say nothing more.
  - `forgetHostKey` uses the existing `confirm()`: "Forget the trusted host key for host:port? All profiles using this SSH server will be refused until it is verified again. Saved SSH passwords for it must be re-entered."
  - `testConnection` and `saveLiveConnection` replace `!ssh.hostKey` with `!hostKeyTrust.value`. The native refusal stays authoritative.
- **UI, the existing compact row only.** Same buttons and layout. The label reads from `hostKeyTrust` ("Trusted · SHA256:…" or "Not enrolled · …"). The pending notice is unchanged. No new dialog in the renderer.

**Tests** (`apps/frontend/tests/connectionWorkflow.test.ts`, `connections.test.ts`, `liveConnections.test.ts`; `tests/browser/live.spec.ts` with the native mock):

- Editing the SSH host, and separately the port, after Fetch clears the pending key and the Trust button. The status re-queries for the new endpoint (missing test 6).
- A legacy profile with `hostKey`/`hostKeyFingerprint` loads without them and saves without them.
- `connect_direct` payload never contains `hostKey`.
- Trust → status shown. Cancelled → no change. Forget asks for confirmation.
- Both themes and keyboard focus are correct for the row (browser test).

**Acceptance:** all pass. `npm test`, `npm run build` and `npm run test:browser` pass. Manual (signed-build GUI gate, already listed in the milestone): the native trust dialog shows host, port, algorithm and fingerprint; Cancel leaves the host unenrolled.

---

## T6 — F-10 surface secret-deletion failures (S)

**Native side:** already correct. `delete_secret` returns `SECRET_UNAVAILABLE` on `Failed` and success on `NotFound`. Add the missing `ipc.rs` test: a `FailingStore` whose `delete` returns `Failed` → `SECRET_UNAVAILABLE`, and the item is still present.

**Renderer:** `useConnectionWorkflow.ts`, three `.catch(() => undefined)` sites plus the desktop gate. Define the behaviour per site:

| Site (e908c49) | New behaviour |
|---|---|
| `persistSecrets` rollback (`:179`), after a failed store | Try every created reference. If any deletion fails, append "N newly saved credential(s) could not be removed from the Keychain (service `app.dbadmin.desktop.credentials`)." to the thrown error. |
| Obsolete references after a successful save (`:258`) | The save stands. Show a warning notification: "Saved, but N old credential(s) could not be removed from the Keychain…". |
| Profile removal (`:304`) | Delete all references first. On any failure, **keep the profile** and open the existing `confirm()`: "Remove anyway" (the profile is removed; the message names the Keychain service so the user can delete the item manually) or Cancel. |
| Host not `desktop` (unavailable bootstrap) and the profile holds references | Removal shows the same "could not be removed" choice. It never silently skips. Browser-mode profiles cannot hold references (`sanitizeConnection` keeps references only for live mode, and live mode needs desktop), so this case has no special path. |

- A renderer timeout on `delete_secret` (120 s) counts as a failure (outcome unknown), so it is surfaced, never treated as success.
- A retry queue or "purge unreferenced" maintenance command is *not* in alpha scope (backlog).

**Tests** (`connectionWorkflow.test.ts`, mocked `deleteSecret` rejecting):

- removal keeps the profile and shows the choice; "Remove anyway" removes it
- a save with failing obsolete deletion warns with the count
- rollback failure is reported
- a source check (test or lint): 0 occurrences of `deleteSecret(...).catch(() => undefined)`

**Acceptance:** all pass. The user guide states what happens when the Keychain is locked or denied.

---

## T7 — Docs, decision corrections, evidence, full verification (S)

- **Docs.**
  - `docs/development/ssh-credentials.md`: enrollment, native store location, shared per-endpoint trust, the native dialog, rotation invalidating saved SSH passwords, re-enrollment after upgrade, secret deadline and cancel, dead-tunnel release, deletion failures.
  - `docs/user-guide/connections.md`: correct "Cancel connecting…" and "removing the profile forgets the saved secret".
  - `docs/architecture/security.md`: as needed.
  - Fix the `postgres.rs:28` comment (review doc item 7) only if that file is touched anyway.
- **Evidence:** new `.ref/planning/m005-evidence/` reports for the T1–T4 fixture runs and the mutation re-runs.
- **Full verification** (AGENTS.md):
  - `npm test`, `npm run build`, `npm run test:browser`, `npm run docs:build`
  - `npm run rust:build`, `rust:fmt`, `rust:lint`, `rust:test`, `rust:boundaries`, `contracts:check`, `rust:vendor`
  - `npm run desktop:test`, `npm run test:ssh`
  - `cargo audit` with a fresh database: 0 vulnerabilities
- **Release acceptance for these blockers:** every task's acceptance passed and recorded, and the milestone and register updated against executed evidence only. The signed-build GUI items (real Keychain prompt, native trust dialog, real picker) remain the separate alpha gates already listed in M005.

---

## Risks

| Risk | Mitigation |
|---|---|
| Rebase conflicts with uncommitted M006 work in 10+ shared files | Start after M006 commits. T3/T6 and the clean files first. Never overwrite M006 edits. |
| Native dialog adds a second confirmation | Keep the renderer notice informational; the native dialog is the only "Trust" commit. Users enroll a host once. |
| Shared per-endpoint trust surprises users with several profiles on one bastion | Forget confirmation names the endpoint and "all profiles". Documented. |
| Scope v2 invalidates saved SSH passwords on upgrade and on every rotation | Pre-alpha. Clear `SECRET_UNAVAILABLE` text. Documented. |
| Bundle-identifier change moves `app_data_dir` and de-enrolls every host | Decide the identifier before the alpha (open D021 item), or migrate natively. |
| Same-user process edits the store file | Cannot unlock saved SSH passwords (fingerprint scope). Typed passwords are exposed, the same exposure as OpenSSH. D017 wording must not claim more. |
| `spawn_blocking` Keychain call outlives a cancel; the prompt stays visible | Result dropped; documented. A real-prompt GUI check stays an alpha gate. |
| Keepalive fixture case is slow (~130 s) | Run once (MySQL); kill case for the other engines. |
| A `Verifier` drop signal does not fire on some russh path | The fixture kill and pause cases are the proof. If either fails, add a periodic `is_closed()` poll (5 s) in the expiry task as a fallback. |
| 60 s/75 s timeouts are unapproved limit values | Recorded as Proposed (open question 2). Constants live in one place each. |

---

## Recommended but non-blocking backlog (F-05..F-19 and remaining review tests)

| ID | Item | Suggested priority | Size | Note |
|---|---|---|---|---|
| F-05 | Fingerprint persisted separately | **Closed by T4/T5** | — | Derived natively; renderer stores none |
| F-16 | Key file read before host-key check | **Closed by T4** | — | Store lookup precedes the key read |
| F-06 | Pin `Preferred.key` to the enrolled algorithm; scan with non-RSA list; "no common host-key algorithm" → `Unsupported` | **Alpha-recommended** | S | Prevents false "changed" alarms that train users to forget keys, now that forgetting also invalidates saved passwords |
| F-07 | RSA host key or failed host-key negotiation → explicit `UNSUPPORTED_CAPABILITY`; fixture with RSA-only sshd | **Alpha-recommended** (pairs with F-06) | S | Required for the explicit message in the deferred table below |
| F-12 | Startup sweep of same-UID `stoneql-ssh-*` dirs with dead sockets (keep `create()` no-follow semantics) | **Alpha-recommended** — effectively required by the existing D021 milestone item "no orphan tunnels after quit/crash relaunch" | S | |
| F-09 | `admit()` and a cap for `store_secret`/`delete_secret` | Alpha-recommended | XS | |
| F-14 | Distinguish agent I/O errors from session errors | Alpha-recommended | XS | |
| F-17 | Read key file through `take(MAX+1)`; optionally warn on group/world-readable keys | Alpha-nice | XS | |
| F-08 | JSON or length-prefixed scope; reject `@ \| /` in hosts | Alpha if bundled with T4 scope v2, else later | S | Free to bundle now; later it costs a second invalidation |
| — | Keyboard-interactive-only / partial-success detection → `UNSUPPORTED_CAPABILITY` (see deferred section) | Alpha-recommended | S | Uses russh `AuthResult::Failure { remaining_methods, partial_success }`; no prompt UX |
| F-10 (rest) | Orphan from `store_secret` completing after renderer timeout; "purge unreferenced" native command; retry queue | Beta | M | Must never enumerate values |
| F-11 | Peer-PID check on the Unix socket (`LOCAL_PEERPID`/`SO_PEERCRED`) | Beta | S | D018 wording already corrected |
| F-13 | `$TMPDIR` too long → fallback dir or document | Later (doc now) | XS | Fails closed today |
| F-15 | `zeroize`/`secrecy` for host-owned secret copies | Beta | M | Documented limitation |
| F-18 | Track `ssh-key 0.7.0-rc.11`; re-review when russh moves to stable | Policy (with russh upgrade policy) | — | |
| F-19 | Rate-limit `ssh_host_key` scans | Later | XS | Trust now requires a scan, but scan exposure equals Direct connect |
| Test 9 | `MAX_FORWARDS` behaviour; same-user foreign peer (after F-11) | Beta | S | |
| Test 10 | Split monolithic fixture tests or record per-case results | Alpha-nice (done for T3 cases) | S | |
| Test 11 | Windows/Linux compile gating in CI | With the Windows/Linux release (D021) | — | |

---

## Deferred features and unsupported authentication methods

**"Explicit"** means the user sees a specific `UNSUPPORTED_CAPABILITY`-class message naming the feature. **"Documented"** means the behaviour is listed under limits in `docs/development/ssh-credentials.md` and the user guide, and the runtime fails closed with a generic code. Nothing below may be presented as working.

| Feature | Current behaviour (e908c49) | Required surfacing for alpha | User decision needed? |
|---|---|---|---|
| RSA user key files | `UNSUPPORTED_CAPABILITY` | Explicit (already). Message should name RSA. | **Yes** — RSA policy: (a) stay unsupported (recommended for alpha); (b) risk-accept RUSTSEC-2023-0071 for client signing only; (c) `aws-lc-rs` backend |
| RSA agent identities | Silently skipped; may end as `SSH_AUTHENTICATION_FAILED` | Explicit when the agent offered only RSA identities ("agent keys are RSA; RSA is not supported") | Same RSA decision |
| RSA host keys | `Unavailable`/connection failed | Explicit via F-06/F-07 | Same RSA decision |
| Keyboard-interactive / 2FA / `AuthenticationMethods` multi-factor | `SSH_AUTHENTICATION_FAILED` | Explicit: when `AuthResult::Failure` reports `partial_success` or `remaining_methods` containing only `keyboard-interactive`, return `UNSUPPORTED_CAPABILITY` ("server requires keyboard-interactive / multi-factor authentication"). Detection needs no prompt UX. | **Yes** — support it (native prompt UX plus timeout design, beta) or stay unsupported with detection (recommended for alpha) |
| Host certificates (CA-signed host keys) | Scan → `UNSUPPORTED_CAPABILITY`; connect → `SSH_HOST_KEY_UNKNOWN` | Explicit at scan (already). Documented. | No (later CA-trust policy) |
| `known_hosts` / `ssh_config` import | Not read | Documented. Any future import must go through native enrollment with explicit confirmation, never implicit trust. | No |
| Jump hosts / ProxyJump | Single hop only; no UI field | Documented | No |
| Agent confirmation prompts / hardware-backed agents | Unverified; may exceed the 3 s SSH deadline → `DEADLINE_EXCEEDED` | Documented as a known limitation (D019 note) | No |
| ECDSA P-384 / P-521 user and host keys | Compiled in; untested (only Ed25519 and P-256 exercised) | Documented as **untested**. Do not claim support until fixture-tested. | No |
| Windows endpoint / Pageant / Windows OpenSSH agent | `Unsupported` (no Unix-socket endpoint) | Explicit (already); deferred by D021 | No (D021) |
| Linux Secret Service / Windows Credential Manager | `UNSUPPORTED_CAPABILITY` | Explicit (already); deferred by D021 | No (D021) |

---

## Open questions requiring a user decision

1. **Native confirmation dialog on trust** (recommended). It is what actually stops a compromised renderer from enrolling an attacker key. Without it, the residual risk is: XSS plus network control → silent enrollment → typed SSH passwords exposed. Saved ones stay protected by the fingerprint scope.
2. **Timeout values:** native secret-resolution deadline 60 s; renderer `connect_direct` timeout 75 s when a secret reference is present, otherwise 8 s.
3. **Migration:** re-enrollment required (recommended), rather than a one-time import with re-confirmation.
4. **Fingerprint binding scope:** `ssh_password` only (recommended), or also `ssh_passphrase`/database-via-SSH. Also: adopt F-08 JSON scopes in the same one-time invalidation?
5. **Failed deletion on profile removal:** keep the profile and offer "Remove anyway" (recommended), or remove and warn.
6. **Shared per-endpoint host-key trust** across profiles (recommended, `known_hosts` semantics), rather than per profile.
7. **RSA policy** (a/b/c above) and **keyboard-interactive** (detect-and-refuse for alpha vs. implement).
8. **Bundle identifier** must be settled before the alpha, because it fixes both the Keychain service and the host-key store location.

Related: `m005-security-review.md`, `.ref/ms/M005-transports-credentials.md`, `architecture-decisions.md` (D016–D021).
