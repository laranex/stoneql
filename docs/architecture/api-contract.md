# API contract proposal

**Status: Draft / awaiting approval. No endpoints or IPC commands exist yet.** This is the single planning contract, not an implemented OpenAPI specification.

## Responsibilities and versioning

The shared Rust core accepts typed application requests and returns typed results. Tauri commands and Axum routes validate envelopes, identity, ownership and limits before invoking it. Neither framework's request/response types enter the core. Frontend transport interfaces select mock, IPC or HTTP at the boundary; Vue views do not branch on host APIs.

Use explicit protocol version `v1`, opaque connection/job handles and request IDs. Additive optional fields can be compatible; semantic changes require a new version. Reject unsupported versions clearly. Generate or validate TypeScript bindings against Rust DTO fixtures in M001; no duplicate handwritten value semantics.

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
  "columns": [{ "name": "balance", "type": "DECIMAL", "nullable": true }],
  "rows": [[{ "kind": "decimal", "text": "12345678901234567890.01" }], [{ "kind": "null" }]],
  "nextCursor": null
}
```

BIGINT/DECIMAL are strings with type tags. Temporal values retain database text and type/timezone metadata; do not normalize them through JavaScript Date. Binary values use an explicit encoding. Unknown types stay opaque and read-only until supported. NULL, empty string, omitted/default and SQL expression are distinct variants.

## Errors and mutation outcomes

```json
{
  "version": 1,
  "requestId": "opaque-request-id",
  "error": {
    "code": "CONFLICT",
    "message": "Record changed since it was loaded.",
    "retryable": false,
    "outcome": "rejected",
    "details": { "sqlState": "40001" }
  }
}
```

Codes should cover validation, unauthorized/forbidden, capability mismatch, stale schema, conflict, timeout, cancellation, connection loss and internal failure. Error details are allowlisted/redacted; preserve safe database text and SQL position separately from surrounding UI translations. Outcomes distinguish rejected, rolled_back, committed and unknown. Do not infer success from timeout/disconnect or retry SQL based solely on `retryable`. HTTP status mapping and cancellation terminal states require contract tests before native integration.

## Acceptance

Golden DTO round-trips must cover raw precision, NULL/default, multiple result sets, binary/unknown values, errors and ownership failures. Test dropped clients, stale cursors, out-of-order progress and expired leases. Publish an executable OpenAPI contract only when HTTP scope is approved; no Postman workspace or API has been created by this planning task.
