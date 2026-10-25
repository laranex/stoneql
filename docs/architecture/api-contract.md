# API contract proposal

**Status: v1 DTOs and scoped Tauri IPC implemented.** The HTTP table remains proposed; no Axum endpoint or implemented OpenAPI specification exists.

## Responsibilities and versioning

The shared Rust core accepts typed application requests and returns typed results. Tauri commands and Axum routes validate envelopes, identity, ownership and limits before invoking it. Neither framework's request/response types enter the core. Frontend transport interfaces select mock, IPC or HTTP at the boundary; Vue views do not branch on host APIs.

Use explicit protocol version `v1`, opaque connection/job handles and request IDs. Additive optional fields can be compatible only with an agreed tolerant reader/negotiation policy; the current M001 query decoder is deliberately closed and rejects unknown fields. Changes to its accepted structure require an explicit version/compatibility decision, not an assumed additive rollout. Reject unsupported versions clearly. Generate TypeScript contracts from Rust-owned wire DTOs in M001; do not substitute handwritten bindings validated only by examples. Keep generated code deterministic and verify regeneration produces zero diff in CI. Wire DTOs remain separate from internal domain models. Generated TypeScript provides compile-time types, not runtime input validation: host/core boundaries still validate untrusted envelopes and ownership.

## Generation and staged delivery

M001 implements Rust v1 DTOs in `crates/stoneql-contracts/src/v1.rs`, generating `contracts/generated/v1.ts` with [ts-rs](https://docs.rs/ts-rs/latest/ts_rs/) 12.0.1. Its explicit default Config avoids environment-dependent generation. Tests cover serde mappings, literal numeric version 1, NULL/default/empty values, multiple results, raw temporal/JSON text, binary/opaque hex and large integers/decimals as strings. The generator is selected for this foundation, not a general claim that every serde attribute is supported. Unsupported mapping warnings must not be silenced.

`decode_query_request` rejects malformed/duplicate/unknown fields, unsupported versions, invalid opaque IDs and over-budget envelopes (128 KiB), SQL text (64 KiB), row limit (1–1000) and result-byte limit (1–1 MiB). Database context is required as a string or explicit null (at most 1024 bytes). These are initial foundation bounds, not a production quota service. SQL text is preserved and not parsed/executed or record-form-validated. The generic DTO derives do not replace this entry validation. Core errors project only allowlisted codes, static messages and explicit outcomes, with no raw driver/SQL diagnostic payload and no automatic retry permission.

Machine engine IDs are `mysql`, `mariadb`, `postgresql`, `sqlite`. `QueryResult` carries ordered result sets and exact string affected-row counts; unsupported engine types can retain opaque bytes/type names. The M003 runtime evaluation decodes a tested MySQL/MariaDB raw-value subset, including zero dates, with explicit native SQL UI binding. The M002 handshake/probe and M003 database commands use strict versioned envelopes. Browser/default mocks remain independent; job framing is unimplemented.

M003's initial read-only request uses v1 bounded result/error DTOs through a simple core/IPC request-response path. It does not need a query-job protocol, HTTP server or full schema cache. Single-statement/read-only enforcement, explicit row/byte limits and safe deadlines are part of that initial contract. Timeout is not confirmation of server cancellation. M006 introduces job/batch/cancel/transaction and schema-revision DTOs; preserve v1 compatibility or version breaking changes. M008 binds approved contracts to HTTP later. Do not implement the HTTP bindings below to unblock M003.

## Draft operations

| Intent | Proposed HTTP binding | Semantics |
| --- | --- | --- |
| Connect | `POST /v1/connections` | Authorize profile/target; transient secret or credential reference; return owned handle |
| Disconnect | `DELETE /v1/connections/{id}` | Idempotent cleanup; invalidate jobs/leases |
| Schema | `GET /v1/connections/{id}/schema` | Database/schema scope, revision and capability metadata |
| Refresh | `POST /v1/connections/{id}/schema/refresh` | Explicit cache invalidation; stale reply protection |
| Execute | `POST /v1/connections/{id}/queries` | Return accepted job ID; no automatic SQL retry |
| Results | `GET /v1/jobs/{id}/batches?cursor=...` | Bounded batches, opaque cursors, multiple result-set identity |
| Cancel | `DELETE /v1/jobs/{id}` | Request cancellation, not proof of rollback |
| Transaction | `POST /v1/connections/{id}/transactions` | Begin lease; explicit commit/rollback actions on owned transaction |
| Object mutation | `POST /v1/connections/{id}/objects/actions` | Capability, revision and authorization checked |
| Transfers | `POST /v1/connections/{id}/transfers` | Validated import/export job; authorized upload/download tokens |

Every handle, result batch, upload and job must be ownership-checked on every request. Client paths never grant arbitrary server filesystem access. Tauri bindings carry equivalent DTOs, not necessarily identical method names. JSON control messages plus bounded NDJSON batches/SSE progress are candidates; final HTTP framing, IPC channels, replay and disconnect semantics need a spike and approval.

## Lossless values

```json
{
  "version": 1,
  "requestId": "opaque-request-id",
  "payload": {
    "resultSets": [{
      "index": 0,
      "columns": [{ "name": "balance", "databaseType": "DECIMAL", "nullable": true }],
      "rows": [[{ "kind": "decimal", "text": "12345678901234567890.01" }], [{ "kind": "null" }]],
      "affectedRows": "0",
      "truncated": false
    }]
  }
}
```

BIGINT/DECIMAL are strings with type tags. Temporal values retain database text and type/timezone metadata; do not normalize them through JavaScript Date. Binary values use an explicit encoding. Unknown types stay opaque and read-only until supported. NULL, empty string, omitted/default and SQL expression are distinct variants.

## Errors and mutation outcomes

```json
{
  "version": 1,
  "requestId": "opaque-request-id",
  "payload": {
    "code": "CONFLICT",
    "message": "Record changed since it was loaded.",
    "retryable": false,
    "outcome": "rejected"
  }
}
```

Codes should cover validation, unauthorized/forbidden, capability mismatch, stale schema, conflict, timeout, cancellation, connection loss and internal failure. M001 omits database detail payloads. A future reviewed adapter may add allowlisted/redacted details, preserving safe database text and SQL position separately from surrounding UI translations; no such diagnostic forwarding is implemented now. Outcomes distinguish rejected, rolled_back, committed and unknown. Do not infer success from timeout/disconnect or retry SQL based solely on `retryable`. HTTP status mapping and cancellation terminal states require contract tests before native integration.

## Acceptance

Golden DTO round-trips cover raw precision, NULL/default, multiple result sets, binary/unknown values and structured errors. Ownership/resource checks remain future host/core service work; no connection/job service exists in M001. Test dropped clients, stale cursors, out-of-order progress and expired leases. Publish an executable OpenAPI contract only when HTTP scope is approved; no Postman workspace or API has been created by this planning task.

## Implemented desktop database commands

| Command | v1 request → response | Boundary |
| --- | --- | --- |
| `connect_direct` | `DirectConnectionRequest` → `LiveConnection` | MySQL/MariaDB Direct, verified TLS; transient password/optional PEM CA |
| `read_only_query` | `LiveQueryRequest` → `ResultSet` | Owned handle, restricted SELECT, bounded rows/bytes and three-second deadline |
| `disconnect_database` | `ConnectionHandleRequest` → `LiveConnectionStatus` | Remove owned handle and close pool |
| `database_status` | `ConnectionHandleRequest` → `LiveConnectionStatus` | Registry/lease state; no network query |

Every request includes the native session ID inside the strict v1 envelope; command envelopes are bounded to 64 KiB. Connection handles are opaque UUIDs, scoped to the host session. The host admits at most 16 active connect/query requests and eight logical pools, with one-hour leases. Secret-bearing DTOs intentionally do not derive Debug. PEM CA input is bounded to 16 KiB; private-key content is rejected. Renderer secrets are transient, with no credential vault or filesystem permission.

Additional allowlisted errors include DEADLINE_EXCEEDED, PACKET_TOO_LARGE, QUERY_REJECTED, CONNECTION_UNAVAILABLE, CAPACITY and UNSUPPORTED_SQL. Validation/refusal is rejected; uncertain execution failures remain unknown outcomes. The frontend receives fixed safe text, never raw driver errors, and never retries SQL or falls back to mocks. M006 job/transaction contracts and future HTTP authorization remain separate work.
