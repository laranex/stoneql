# Rust foundation development

M001 implements shared vocabulary/DTOs and a disposable SQLx lab. This page records that foundation; the desktop host, runtime, SSH and live adapters were added by later milestones (see [desktop host](./desktop-host.md)). No server app or vault exists.

## Tooling and commands

Install Rust using the official rustup installer. `rust-toolchain.toml` selects **1.99.1**, rustfmt and Clippy; do not replace it with a floating stable channel. Use Node 26 and the existing root npm install. Cargo.lock is tracked; normal checks use `--locked`.

```sh
npm ci
npm run rust:build
npm run rust:fmt
npm run rust:lint
npm run rust:test
npm run rust:boundaries
npm run contracts:generate
npm run contracts:check
npm test
npm run build
```

`rust:fmt` checks; apply formatting with `cargo fmt --all`. `contracts:generate` writes `contracts/generated/v1.ts`, intentionally tracked generated source. `contracts:check` compares exact regeneration and type-checks positive/negative TS consumers. No generated files are imported by the Vue app yet. `.ref/workprogress.md`, root `workprogress.md`, `target/` and `artifacts/m001/` stay ignored.

The native validation environment installed Rust into temporary CARGO_HOME/RUSTUP_HOME with `--no-modify-path`; it did not change shell profiles. On a normal rustup installation the commands above work through PATH. Keep both home variables and the cargo bin PATH consistent if using an isolated installation.

## Boundaries

- `crates/stoneql-core`: no dependencies; engine/transport vocabulary, unverified-by-default capabilities and structured domain errors.
- `crates/stoneql-contracts`: Rust v1 wire DTOs, serde validation/golden fixtures, TypeScript generation and a synthetic microbenchmark. Its only local dependency is the core; it has no host/driver/runtime dependency.
- `experiments/sqlx-spike`: unpublished, separately scoped driver executable. It is a workspace member but excluded from default members. It is never a production connection service.
- `contracts/`: generated types and compile-time consumer checks outside Vue. `tools/`: boundary checker and fixture orchestration.

The boundary check traverses resolved dependencies, not just direct Cargo entries. Do not add Tauri/Axum/SQLx/Tokio to shared core/contracts. Future host/adapter implementations belong to their approved milestones.

## Disposable driver evaluation

Start a local Docker daemon, then:

```sh
npm run spike:drivers
```

The runner builds the locked executable, creates dedicated random Compose projects, generates transient credentials/marker SQL, publishes only loopback ports and verifies the marker before any spike DDL. It rejects remote Docker contexts and external/non-fixture database URLs, and removes inherited PostgreSQL/MySQL connection settings and TLS key paths before starting the lab. It never accepts existing database URLs from the user. SQLite is in memory. Container state is tmpfs; normal/failing runs remove their own containers/networks and verify teardown. Never reuse a production/development database for this lab.

Reports in `artifacts/m001/` contain synthetic observations, not backups. A nonzero assertion/fixture/cleanup exit fails evaluation; unsupported and unverified capabilities are explicitly reported. If startup fails, inspect Docker availability; do not disable host/Chromium security, use host networking or change ownership. A forcibly killed runner may need cleanup limited to its `stoneql-m001-*` project; do not remove unrelated resources.

```sh
cargo run --locked -p stoneql-contracts --example benchmark
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --deny warnings
```

The microbenchmark is debug-only, synthetic and not a release benchmark. The audit is a tool install, not a runtime dependency. License metadata and advisory results must be rechecked as dependencies/advisories change; no blanket SQLx approval follows from a clean audit.

## CI and evidence

`.github/workflows/rust.yml` runs when a release is published, or when started manually (`workflow_dispatch`), on a **macOS 15** runner only (macOS on Apple Silicon is the only supported desktop platform): pinned-toolchain frontend build, vendored SQLx provenance, Rust fmt/build/Clippy/tests, crate boundaries, contract drift, workspace tests, `npm test` and the release-graph advisory audit (the full-graph `cargo audit` runs without blocking). Disposable driver spikes and Docker fixtures are not run in CI; Windows and Linux are not in the matrix. Review [evaluation evidence and limitations](../architecture/driver-evaluation.md) before M001 sign-off or any M002 work.
