# Scoped SQLx receive-limit adoption

User approval: “do it”, responding to the explicit patch-adoption request on 2026-10-25. D002 records the limited mechanism approval; broader capabilities/releases are not approved by inference.

Root Cargo patches SQLx MySQL 0.9.0 to reviewed source with MIT grant/attribution. Rust files match the evaluated source exactly; packaging selects MIT and preserves the original manifest. `npm run rust:vendor` verifies 76 source/license/metadata files and the canonical patch fingerprint. Git byte-preserving attributes prevent line-ending conversion from breaking source fingerprints. Two pre-existing upstream trailing-whitespace locations have narrowly scoped formatting attributes so source stays byte-identical; source integrity tests remain mandatory. Both runtime and separate packet harness use this same source; registry caches are untouched.

Runtime incoming cap: 2 MiB per logical packet. A 3 MiB stored BLOB fails with PacketTooLarge; the connection is discarded and a later small query succeeds on a fresh connection. Failed SQL is not retried. The 1 MiB serialized result cap remains separate. This does not establish total RSS/metadata limits or arbitrary-SQL sandboxing.

Executed locally on macOS ARM: 148 JS tests (131 frontend + 17 root); 27 Rust tests; 31 upstream driver library tests; 2 adversarial socket tests and boundary/overflow cases; 28 original engine assertions; 100 lifecycle samples per MySQL/MariaDB plus TLS negatives, raw values, oversize/recovery and cleanup. Rust build/fmt/Clippy, contract generation/TS checks, shared boundaries and frontend/docs builds passed. No UI source changed; browser/native-window tests were not rerun for this backend-only increment.

Full dependency audit still fails on existing Linux glib/proc-macro-error warnings: RUSTSEC-2024-0429 and RUSTSEC-2024-0370. No ignores or release waiver. The scan uses the local advisory snapshot; its index warning and exact results are retained in audit.txt.

Live UI binding, cross-OS native execution, representative memory/metadata/concurrency profiles, wider codecs and indirect view/function trust remain open. Upgrade the fork only with source/license review, rebased patch, provenance and full affected regression matrix; remove it when upstream supplies a compatible API.
