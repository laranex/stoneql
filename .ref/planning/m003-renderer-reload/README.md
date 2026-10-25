# M003 renderer reload lifecycle evidence

Checks executed locally on 2026-10-25; reviewed after the requested pause on 2026-10-26. Node 26.11.1, Rust 1.99.1, macOS ARM. No dependency, DTO shape, command permission or UI layout changes.

## Regression and fix

The original native regression failed because two distinct renderer bootstrap nonces returned the same host-lifetime token. Reload lost frontend handles while native pools remained until expiry. Bootstrap now invalidates the old token, advances registry generation, closes old pools and returns a fresh renderer token. Repeating the current nonce is idempotent. Pending old-generation registrations are rejected; native post-connect validation closes a late handle if bootstrap changed before its runtime call began. Stale successful query replies are rejected with unknown outcome. No SQL is automatically retried.

## Executed verification

- 155 JavaScript tests (138 frontend + 17 repository),33 default Rust tests and 22 sandboxed Chromium scenarios passed.
- Runtime tests cover pool closure/revoked handles, retained capacity and late registration from a previous generation. Native tests cover token rotation/idempotency and stale probe denial; frontend test covers nonce uniqueness independent of clock resolution.
- Both real owned MySQL/MariaDB TLS fixtures passed actual Tauri handlers under MockRuntime. An observer verifies idle physical-session removal. Old token/handle calls are refused and all eight connection slots can be reused. Cleanup summaries show zero owned containers/networks and removed private directories.
- Frontend/native/docs builds, rustfmt, all-target Clippy, generated contract reproducibility, shared dependency boundaries and 76-file SQLx provenance passed. Real macOS Wry initial startup/handshake smoke passed in 948 ms.
- Existing macOS test-linker __EMBED_INFO_PLIST warning also appears in the preceding trust increment; no linker or security suppressions added. Nonfatal frontend chunk advisory remains. The two known Linux GTK audit findings remain unresolved; no new audit pass is claimed.

## Limits

The fixture exercises real command handlers/database connections, not a graphical live-database reload. Initial Wry smoke does not reload the page. Browser native bridges are simulated. Windows/Linux, actual native GUI-to-database interaction/reload, aggregate metadata/memory bounds and hostile UDF/external effects remain open. Pending attempts may briefly hold permits until bounded cleanup; connection reset is not confirmed server-side cancellation. M003 remains in progress. No publication/deployment or external identifier change.
