# StoneQL SQLx PostgreSQL patch

User-approved on 2026-11-16 (A8) after isolated evaluation (`.ref/planning/m004-pg-message-limit/findings.md`). SQLx 0.9.0 source, selected under its MIT license; upstream authors/copyright remain intact. Full license: LICENSE-MIT.

Only two Rust files differ from the registry archive (`experiments/sqlx-pg-message-limit/message-limit.patch`): `PgConnectOptions::max_receive_message_size` and the backend-message length check in `connection/stream.rs`. A declared length below 4 (which panics upstream) is always a protocol error; a length above the configured limit is refused from the 5-byte header, before any payload is buffered. Packaging selects MIT in Cargo.toml; `Cargo.lock`, `LICENSE-APACHE` and registry bookkeeping files are not copied; no upstream attribution is removed. This is source, not generated build output.

The runtime sets a 2 MiB limit; an oversized message invalidates the physical connection and callers discard it. The option defaults to upstream behavior for other consumers. A per-message bound is not an RSS limit, result limit or SQL sandbox.

Maintenance: review upstream security advisories and every SQLx upgrade; rebase the small patch, retain licenses, regenerate reviewed provenance and rerun the runtime, fixture and workspace tests. Remove this fork when an upstream receive-size API meets the same tests. Never edit Cargo's registry cache. Run npm run rust:vendor to detect unreviewed drift.
