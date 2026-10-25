# dbAdmin SQLx MySQL patch

User-approved on 2026-10-25 after isolated evaluation. SQLx 0.9.0 source, selected under its MIT license; upstream authors/copyright remain intact. Full license: LICENSE-MIT.

Only three Rust files differ from the registry archive: receive-limit options, stream length checking and TLS limit propagation. The reviewed patch and package/patch checksums live in experiments/sqlx-receive-limit/. Packaging selects MIT in Cargo.toml; no upstream attribution is removed. This is source, not generated build output.

The runtime explicitly enables a 2 MiB logical receive-packet limit. Oversize invalidates the physical connection; callers must discard it. The option defaults to upstream behavior for other consumers. A packet bound is not an RSS limit, result limit or SQL sandbox. Existing OS/security/type/release gates remain open.

Maintenance: review upstream security advisories and every SQLx upgrade; rebase the small patch, retain licenses, regenerate reviewed provenance and rerun runtime, adversarial packet, TLS/engine, unknown-type and existing workspace tests. Remove this fork when an upstream receive-size API meets the same tests. Never edit Cargo's registry cache. Run npm run rust:vendor to detect unreviewed drift.
