# M004 — PostgreSQL backend message bound (sqlx-postgres 0.9.0)

Date: 2026-11-16. Status of the fix: **Proposed / awaiting approval** (vendor patch, same class as D002).

## 1. Root cause (code reading, registry sqlx-postgres 0.9.0)
`src/connection/stream.rs` `recv_unchecked`: the 5-byte backend header carries an untrusted u32
length. The driver (a) returns `ControlFlow::Continue(length + 1)`, so `BufferedSocket` reserves and
fills up to ~4 GiB for one message with no cap, and (b) calls `contents.advance(4)` without checking
that the length covers its own 4 bytes.

## 2. Executed against upstream (temporary fake-server probe in stoneql-runtime, not committed)
- Declared length 0: the connect task **panics** (`bytes` "cannot advance past `remaining`: 4 <= 0").
  In StoneQL the connect future is not spawned separately, so the panic unwinds the Tauri command
  future (that attempt never answers; the renderer's timeout applies). Default `panic = unwind`; the
  process survives.
- Declared 1 GiB, server streams 256 MiB and stalls: process RSS 3.1 MiB → 414.5 MiB within 8 s, no
  error. Memory grows with whatever the server sends up to the declared length.
- Threat model: TLS is verified (VerifyFull), so the sender must be the configured server itself
  (hostile or compromised), the same model as the approved MySQL receive limit (D002).

## 3. Prototype (`experiments/sqlx-pg-message-limit/message-limit.patch`, 19 changed lines)
- `PgConnectOptions::max_receive_message_size(NonZeroUsize)`; `None` keeps upstream behavior.
- `recv_unchecked`: length < 4 → `Error::Protocol("invalid backend message length: N")`
  unconditionally; length > limit → `Error::Protocol("receive message exceeds configured byte
  limit")`. Both before any payload is buffered. Connection must be discarded afterwards.
- Executed (`cargo test --manifest-path experiments/sqlx-pg-message-limit/Cargo.toml`): 4/4 pass —
  lengths 0–3 are protocol errors (no panic); 1 GiB header with 2 MiB limit refused within the 3 s
  test deadline while the server streams; exact-limit ErrorResponse (4096 B) parsed as database error
  28P01 and limit + 1 refused; default (no option) still waits for the declared payload.

## 4. Not done / limitations
- Not wired into `stoneql-runtime` (`postgres.rs` would set 2 MiB, mapping the error to
  `PacketTooLarge` like MySQL); no vendor copy under `vendor/`; no provenance tooling.
- No real-PostgreSQL positive control with the patched driver (the fixture runs the unpatched
  registry crate). A real result row > 2 MiB would be refused, as on MySQL today.
- A bound per message is not a process RSS bound; results remain capped at 1 MiB serialized.

## 5. Decision request — Proposed / awaiting approval
Adopt the patch as `vendor/sqlx-postgres` (MIT, provenance like `vendor/sqlx-mysql`) with a 2 MiB
runtime limit, extend `rust:vendor`, map to `PacketTooLarge`, and add a PostgreSQL fixture
regression (oversized value refused, fresh connection afterwards).

## 6. Reproduce
```sh
cargo fetch --locked
node experiments/sqlx-pg-message-limit/prepare.mjs
cargo test --offline --manifest-path experiments/sqlx-pg-message-limit/Cargo.toml --target-dir target/pg-message-limit
```
