# Security design

**Status: Proposed / awaiting approval.** This is a threat model for future native/server integration, not a certification of the prototype.

## Trust boundaries

Treat the renderer/browser, API clients, SQL text, imported files and database error text as untrusted. Desktop IPC must expose only reviewed commands and minimum capabilities. Shared core operations receive an explicit principal/context and authorized handles. Database credentials do not grant application-level permission to other users' profiles or jobs.

## Credentials

Desktop stores opaque secret references in non-secret profiles and the secrets in an OS-backed credential store: the macOS login Keychain is implemented (M005, awaiting review); other operating systems are explicitly unsupported until verified. Each reference is bound to its connection target and route, and no IPC command returns a secret ([details](../development/ssh-credentials.md)). Server requires a separately approved encrypted vault/envelope-key approach with restricted key access, rotation and revocation. Do not persist secrets in browser storage, URLs, command arguments, logs or source. SSH agent use must not forward agent authority unexpectedly; the desktop client uses the agent only to sign authentication and never requests forwarding. Short-lived secret buffers and redaction reduce exposure but do not guarantee memory erasure in every layer.

## Server identity and authorization

Proposed baseline: OIDC login with secure, HttpOnly, SameSite session cookies, expiry/rotation, logout invalidation and CSRF protection. Scoped API tokens are a separate future use case. Validate issuer/audience/state/nonce and bind sessions to principals. Deny by default and enforce resource ownership for connection, job, transaction, metadata cache, upload and download on every request.

Allowlisted database targets and approved SQLite roots reduce SSRF/local-file risks. Authorize DNS/IP results and redirects/reconnects, not just input host strings. A database administration service intentionally needs private-network access; target policy must be explicit rather than a blanket block or blanket allow. Restrict trusted reverse-proxy headers and CORS origins. Rate-limit login, connection attempts and jobs; apply per-principal pool, time, result-byte, upload and temporary-disk quotas.

## Diagnostics and deployment

Default logs contain request IDs, operation codes, durations and bounded counts, not SQL text, passwords or row content. Redact allowlisted database error details and provide bounded opt-in diagnostics with visible retention. SQL itself may contain secrets. Audit privileged actions with principal/object identifiers while avoiding sensitive payloads. Keep health endpoints free of credential/database details.

Use TLS and verified host keys, signed desktop updates, restrictive CSP and no arbitrary shell/filesystem renderer permissions. [Tauri capabilities](https://v2.tauri.app/security/capabilities/) provide the native boundary mechanism; StoneQL still needs its own validation and authorization policies.

## Verification and open decisions

Threat-review malicious SQL strings in generated CRUD, object ownership/IDOR, CSRF, path traversal, archive bombs, TLS failures, untrusted SSH keys, leaked errors and quota bypass. Test negative authorization cases, credential isolation and cleanup after crashes. Approve authentication provider, credential lifecycle, target policy and diagnostic retention before M005/M008. No security controls may be disabled merely to pass local tests.

Desktop SSH/credential status (2026-10-26 review): host keys require explicit enrollment and exact match, TLS stays verified through tunnels, and secrets are referenced rather than read back. Open hardening before the macOS alpha: the enrolled host key is currently held by the renderer rather than a native store, Keychain resolution is not yet inside the cancellable bounded setup, dead tunnels keep their slot until expiry, same-user processes can reach a live tunnel socket, and secret deletion failures are not surfaced. See [SSH and credentials](../development/ssh-credentials.md).
