# M001 — Rust core foundation

## Status
Startup approved by the user on 2026-10-25. Foundation/evaluation locally verified. The user authorized committing M001 and continuing all milestone implementations. M001 foundation scope is accepted for continuation; the documented production-driver and cross-platform limitations remain later evidence gates. No push or deployment is authorized.

## Objective and scope
Create the smallest approved framework-independent core/contracts foundation and capability evidence. Preserve the Vue prototype; no Tauri/Axum application, production backend feature or premature crate scaffold.

## Dependencies and prerequisites
Explicit startup approval received for the roadmap/M001 scope. Evaluation pins: Rust 1.99.1, SQLx 0.9.0, Tokio 1.54.1, ts-rs 12.0.1; MySQL 8.4.6/MariaDB 11.4.8/PostgreSQL 17.6 digest-pinned local fixtures and in-memory SQLite 3.51.3. Reference native runner: macOS ARM, Apple M5/10 logical CPUs/24 GiB. Future production matrix/budgets remain separate gates.

## Technical approach
Keep core/domain/application, engine adapter and transport boundaries independent of host frameworks. Rust-owned versioned wire DTOs generate TypeScript through one reproducible command; evaluate ts-rs first, with serialization fixtures deciding fitness. Separate DTOs from internal domain models. Start with SQLx spikes, not equal-priority driver rewrites.

## Deliverables
Approved minimal Rust workspace after this planning gate; v1 DTO generation/fixtures, error/value/capability contracts, dependency-boundary checks, driver capability matrix and reproducible baseline commands.

## Acceptance criteria
- G1: review new dependencies/licenses; prove secrets/raw query data are absent from diagnostic fixtures; malformed envelopes and unsupported versions are rejected at defined boundaries.
- G2: generated TS and Rust wire JSON preserve BIGINT/DECIMAL, binary/unknown types, temporal text, NULL versus omitted/default; no unsafe numeric conversion.
- G3: core/contracts dependency graph has zero Tauri/Axum imports; generated TS regeneration yields zero diff; selected toolchain and engine versions recorded. Unsupported serde/generator mappings are failures, not ignored warnings.
- D002 spike evidence covers dynamic/unknown types, raw-value precision, multiple result sets, stored procedures and cancellation, plus TLS/session cleanup. Record supported, unsupported and unknown per engine; production scope cannot silently depend on an unverified capability.
- G4/G5: record reference hardware and baseline decode/generation timings, bound fixture sizes, and prove spike connection cleanup on failures. Approve downstream budgets before their implementation; no performance pass inferred from unit tests.

## Required tests and verification
cargo fmt/clippy/test; generated TypeScript type-check and deterministic regeneration; JSON golden round trips; malformed/version mismatch fixtures; disposable SQLx capability probes with captured versions/results. Driver failure can complete an investigation only when the gap and blocked downstream scope are explicitly reviewed; it cannot establish adoption.

## Risks and limitations
ts-rs 12.0.1 generation is implemented and locally verified for current DTOs. SQLx production adoption remains unresolved; positive trusted TLS, exhaustive raw/temporal codecs, cross-user permissions, pool/reset/commit-loss behavior and native Linux/Windows CI results remain outstanding. Native driver exceptions need a separate evidence-based decision. Full production cancellation/jobs/schema cache stay M006; future server/Teleport/SSM APIs are reserved boundaries only.


## Executed evidence and completion gate

- Workspace: core/contracts libraries plus isolated experiment; no desktop/server/SSH/vault implementation. Dependency traversal proves shared crates have no Tauri/Axum/SQLx/Tokio dependency.
- Rust build/tests/format/Clippy and generated TypeScript drift/type checks executed locally. Malformed/duplicate/version/bounds, exact values/defaults, errors, capability guards, target refusal and TLS-negative classifier tests cover the foundation. Final counts/results are recorded in the evaluation report/evidence.
- All 28 live driver assertions passed, with SQLite unsupported routines and unverified TLS/type coverage explicit. Normal and failed spike runs removed their own container/network resources; final evidence has zero leftovers.
- Debug microbenchmark and 100 query/connect-close samples per engine recorded. These do not establish production SLO/memory bounds. Registry license metadata reviewed; cargo-audit 0.22.2 --deny warnings passed after fixing the initial Tokio soundness warning.
- Existing 128 frontend tests plus seven root tests/build passed; all 17 Playwright scenarios passed (12.9s), Chromium sandbox enabled. All 73 original frontend source/config/test/asset hashes still match. No UI/mocks replaced.
- CI is prepared for Linux/macOS/Windows and Linux fixture/audit execution, not run remotely. Review scope/evidence/limits and decide M001 acceptance; do not mark production SQLx supported or begin M002 automatically.

Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. Executed evidence is recorded under `.ref/planning/m001-evidence/`; human completion/adoption review remains pending.
