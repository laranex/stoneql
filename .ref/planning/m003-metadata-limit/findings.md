# M003 — SQLx prepared/result metadata memory limits

Status: **investigation complete; vendor change Proposed / awaiting approval.** No `vendor/`, `crates/`, `apps/`, `contracts/` or root manifest change. All numbers are **local debug characterization** (macOS ARM, debug build, standalone client process RSS; Tauri/WebView/server excluded), not release budgets. Date 2026-10-26. Base: approved `vendor/sqlx-mysql` (SQLx 0.9.0 + receive-limit patch), verified 76 files by `tools/check-sqlx-vendor.mjs`. Checksums: `SOURCE_SHA256.json`.

## 1. Root cause (code reading, approved vendor bytes, `vendor/sqlx-mysql/src/`)

| Location | Behaviour |
| --- | --- |
| `connection/executor.rs:430-452` `recv_result_metadata` | `HashMap::with_capacity(num_columns)` (438) and `columns.reserve(num_columns)` (441) reserve from the server-supplied count **before any descriptor arrives**; loop (444-450) receives and retains one `ColumnDefinition` per column. No count or aggregate byte check. |
| `connection/executor.rs:392-428` `recv_next_result_column` | Copies descriptor into owned `Arc<str>`s: name/alias via `UStr::new`, `format!("{schema}.{table}")`, original column name — name held twice when table is non-empty. Packet `Bytes` dropped. |
| `executor.rs:29-74` `prepare_statement` (call at 61) | Count from `PrepareOk::columns`, a u16 (`protocol/statement/prepare_ok.rs:12`) → ≤ 65,535. Parameter descriptors (45-51) dropped, not retained. |
| `executor.rs:147-170` (`run`) | `persistent(false)` with statement cache disabled **re-prepares**: second full metadata read. |
| `executor.rs:216-221` (`run`) | Result-set count is a length-encoded **u64** — no width bound; reaches the reservation **after** the runtime JSON check. Text protocol (`raw_sql`, e.g. `START TRANSACTION READ ONLY`) also uses this path. |
| `connection/stream.rs:199-209` `skip_result_metadata` | Loops over u64 count, retains nothing (time cost only). |

Runtime path at `73d1b64`: `read_only.rs:98` `prepare` = read #1 (kept by statement); `read_only.rs:101-117` `ResultTooWide` JSON check sees only read #1; `read_only.rs:124` non-persistent fetch = read #2 (transient) + read #3 (result set). `lib.rs:91-94` sets `statement_cache_capacity(0)` and the 2 MiB receive cap. So **3 metadata reads per query**, ~3 blocks live at peak.

## 2. Worst case

Measured: `size_of::<MySqlColumn>()` = 96 B; both engines accept prepared expression SELECTs of 65,535 columns.

Reasoned, not executed:
- Per descriptor: 96 B + ~32-40 B hash entry + retained strings ≤ ~2× packet size → up to ~4 MiB per descriptor under the 2 MiB packet cap.
- Prepare reads: 65,535 × ~4 MiB ≈ 256 GiB theoretical; legal responses use hundreds of bytes per descriptor.
- Result-set read: unbounded count; `reserve(n×96)` and an n-sized hash table requested before data → huge n would be a capacity-overflow panic or allocation failure (process abort). Not executed (hostile traffic out of scope).
- Accumulating many descriptors is bounded only by transfer rate × the 3 s deadline (future dropped at deadline). Not measured.

The 2 MiB packet cap bounds each descriptor, not their sum.

## 3. Measured on disposable fixtures

Setup: MySQL 8.4.6 and MariaDB 11.4.8 from the digest-pinned compose file, random project/password, loopback only; runner reuses `fixture-environment.mjs` and `resource-report.mjs`. Client mirrors the runtime sequence/options (read-only transaction → prepare → non-persistent fetch; cache 0; 2 MiB receive cap; TLS `Preferred` without identity verification on the disposable fixture). Each run a separate process under `/usr/bin/time -l`. After any error the connection is dropped and a fresh connection must serve `SELECT 1` — succeeded in every sample. "Limited" = `max_result_metadata(4096, 4 MiB)`.

Column ceiling (baseline driver, `SELECT 1 AS cN, …`): 4,096 / 4,097 / 8,192 / 16,384 / 32,768 / 65,535 ok on both. At 65,535: MySQL 37.5 MiB, 333 ms; MariaDB 37.0 MiB, 320 ms. At 65,536: driver `Protocol` error with no server code on both — consistent with u16 `PrepareOk` wrap (hypothesis, not isolated). Wide table with 64-char names: MySQL created a 4,096-column MyISAM table; MariaDB refused 4,096/4,095/4,000/2,048 (HY000), so the table-backed shape is MySQL-only.

Peak RSS MiB (ms):

| Shape | MySQL baseline | MySQL limited | MariaDB baseline | MariaDB limited |
| --- | --- | --- | --- | --- |
| 1 column (floor) | 5.8 | — | 5.7 | — |
| 512 | 6.3 (3.9) | 6.3 (3.4) ok | 6.2 (3.8) | 6.2 (3.4) ok |
| 2,048 | 7.2 (11.7) | 7.1 (11.3) ok | 7.0 (11.8) | 7.0 (12.4) ok |
| 4,096 | 8.1 (21.8) | 8.1 (20.7) ok | 8.0 (22.0) | 8.0 (20.5) ok |
| 4,096 with 64-char aliases | 10.7 | 10.3 ok | 10.6 | 10.1 ok |
| 4,096-column table `SELECT *` | 9.2 (54.0) | 9.2 (35.1) ok | n/a | n/a |
| 4,097 | 8.2 ok | 6.2 (1.7) refused | 8.0 ok | 6.2 (1.5) refused |
| 65,535 | 37.5 ok | 10.8 (23.2) refused | 37.1 ok | 10.7 (16.6) refused |

Every refusal returned the fixed `Protocol("result metadata exceeds configured limit")`, the connection was discarded, and a fresh connection succeeded. Under the caps results are identical to baseline. The 4 MiB byte budget was never reached by a legal shape (descriptors estimated, not instrumented, at ~150-200 B → ~0.6-0.8 MiB per 4,096 columns).

Raw samples: `live-mysql.json`, `live-mariadb.json`; cleanup: `cleanup-mysql.json`, `cleanup-mariadb.json` (0 containers/networks remaining).

## 4. Prototype (`experiments/sqlx-metadata-limit/metadata-limit.patch`)

Five files; 118 lines added (~40 production logic, rest helpers/tests).
- `options/mod.rs`: `MySqlConnectOptions::max_result_metadata(columns: NonZeroUsize, bytes: NonZeroUsize)`, default `None` = upstream behaviour.
- `connection/{stream,tls,mod}.rs`: carry the limit through stream, `boxed_socket` and TLS upgrade like `max_receive_packet_size`.
- `connection/executor.rs`: `check_result_columns` runs **before** `with_capacity`/`reserve`; `check_result_metadata_bytes` sums each received (already capped) packet length **before** decode/retention — overshoot ≤ one packet (≤ 2 MiB). Both return fixed `Error::Protocol("result metadata exceeds configured limit")`.
- Connection discard: like the receive limit, the stream is **not** poisoned by the driver; the caller must discard. The runtime already does (`close_on_drop`, no reuse). In-driver poisoning not prototyped (larger change).
- Unit tests (`connection::executor::result_metadata_limit_tests`): exact count boundary (4,096 ok, 4,097 and `usize::MAX` refused); cumulative bytes (exact, over, single oversize packet, saturating overflow); unset limit = upstream; option default unset and values propagate.
- Result: 35/35 library tests pass. One doctest failure (`options/mod.rs:35`, `sqlx` unresolved) also fails on unmodified vendor via the receive-limit harness — pre-existing.
- "Before reservation/decode" is shown by code order, not by a network-level test.

## 5. Options, ranked

1. **Recommended — count cap + byte budget (this prototype).** Removes the untrusted-count reservation; per read ≈ 0.6 MiB structures + ≤ ~8 MiB strings + one ≤ 2 MiB packet; roughly ≤ 30 MiB per query over 3 reads (reasoned). Opt-in, same shape as the approved receive-limit patch. Cost: a second vendor delta on SQLx upgrades.
2. Count cap only — removes the reservation but 4,096 × ~4 MiB ≈ 16 GiB theoretical remains; saves ~6 lines. Not recommended.
3. Runtime-only (SQL parser column count / JSON pre-check) — cannot see the count before the driver reserves; `SELECT *`, views, joins hide width; non-conforming servers ignore SQL. Convenience only.
4. Upstream SQLx API — right long-term home; upstream issues/PRs not searched.
5. Process isolation with rlimit — strongest, architectural change outside M003.

## 6. Recommended defaults

- **Columns 4,096** — matches engine table limit; all legal table/`SELECT *` shapes fit. Trade-off: newly refuses expression-only SELECTs of 4,097-65,535 columns that work today. The current 1 MiB JSON check would refuse short-named shapes from ~19,600 columns anyway (extrapolated from the 2,048-column → 109,552-byte profile). Alternative **16,384** (baseline RSS 14.1/13.9 MiB measured; limited mode not measured).
- **Bytes 4 MiB** of descriptor payload per block (~5× largest estimated legal shape).

## 7. Decision request — Proposed / awaiting approval

Adoption requires explicit user approval; not implied by this investigation or milestone authorization. Adoption would change:
1. `vendor/sqlx-mysql`: apply patch to `src/options/mod.rs`, `src/connection/{executor,stream,tls,mod}.rs`; regenerate `dbadmin-provenance.json`, update `DBADMIN.md`; extend `tools/check-sqlx-vendor.mjs` / `upstream.json` for the second patch and checksum. MIT notice unchanged.
2. `crates/stoneql-runtime/src/lib.rs`: `.max_result_metadata(4096, 4 MiB)` (or approved values).
3. `read_only.rs` `driver_error`: map the fixed message to existing `ReadError::ResultTooWide`. No contract/UI change; connection already discarded.
4. Tests: 4 driver unit tests + 35-test library suite; fixture regression on both engines (4,096 ok, 4,097 → `ResultTooWide`, fresh query ok); `rust:build`, `rust:fmt`, `rust:lint`, `rust:test`, `rust:boundaries`, `rust:vendor`, `contracts:check`, `npm test`.
5. Docs: `resource-profiles.md`, `connection-lifecycle.md`, M003 status.

Choices requested: column cap (4,096 or 16,384), byte budget (4 MiB), and whether to accept a second vendor delta rather than wait for upstream.

## 8. Not done / limitations

No hostile/adversarial traffic (abort/flood risks in §2 reasoned only); no TLS-verified measurements; no Windows/Linux runs; MariaDB wide-table shape unmeasured; 65,536-column failure cause not isolated; no upstream search.

## 9. Reproduce

```sh
export PATH=/private/tmp/dbadmin-m001-tools/cargo/bin:$PATH CARGO_HOME=/private/tmp/dbadmin-m001-tools/cargo RUSTUP_HOME=/private/tmp/dbadmin-m001-tools/rustup
node experiments/sqlx-metadata-limit/prepare.mjs
cargo test --locked --offline --manifest-path experiments/sqlx-metadata-limit/Cargo.toml --target-dir target/metadata-limit -p sqlx-mysql --lib
cargo clippy --locked --offline --manifest-path experiments/sqlx-metadata-limit/Cargo.toml --target-dir target/metadata-limit --all-targets -- -D warnings
node experiments/sqlx-metadata-limit/run-live.mjs mysql mariadb
```

Machine: darwin arm64 27.0.0, Apple M5, 10 logical CPUs, 24 GiB RAM, Node v26.11.1, Rust 1.99.1, Docker context `desktop-linux`.
