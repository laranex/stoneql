# SQLx receive-limit evaluation

Status: evaluated patch approved and adopted in the main runtime on 2026-10-25. This separate test harness now shares the reviewed source under `vendor/sqlx-mysql`; native live UI integration remains pending.

## Reproduce

Requires the pinned Rust toolchain and Node 26.

```sh
npm run test:receive-limit
npm run test:receive-limit:live
```

The live command additionally requires OpenSSL and a local Docker daemon. It reuses the owning M003 fixture runner, random loopback ports/identities and automatic cleanup. No saved profiles, arbitrary database URLs or nonlocal Docker contexts are used. The old generated `driver/` copy is ignored and unused; binaries are ignored under root `target/receive-limit/`. The independent Cargo.lock is tracked.

`tools/prepare-receive-limit.mjs` retains the original command entry point and verifies the approved vendored source through `tools/check-sqlx-vendor.mjs`. Fresh checkouts no longer require generated driver copies, tar or patch utilities. The source is tracked with package/patch/license fingerprints; Cargo's registry cache is never modified. See `docs/development/connection-lifecycle.md` and `vendor/sqlx-mysql/STONEQL.md` for current adoption and maintenance rules.

## Change under evaluation

Only three upstream source files change:

- Options add an optional `NonZeroUsize` logical receive-packet limit. The upstream default remains unchanged.
- Stream reads check the header's length plus accumulated continuation bytes before reading the payload. Arithmetic overflow is refused. Initial continuation-buffer capacity avoids an unconditional 32 MiB reservation.
- Socket boxing and TLS upgrade preserve the limit.

Oversize is a fixed protocol error. The connection must be discarded; consuming an untrusted remainder to reuse it would undermine the bound. TLS record/socket buffers, allocator capacity, metadata, decoded rows and serialization remain additional bounded-work considerations: this is a logical packet limit, not a process RSS limit. Generated queries must still use bound values. This patch does not make arbitrary SELECTs a general SQL sandbox.

## Executed verification

On macOS ARM, two adversarial socket integration tests passed: an oversized first header without payload, and a continuation header exceeding the aggregate limit without its payload. All 31 upstream-copy library tests passed, including the added exact-boundary, aggregate, integer-overflow and legacy-default test. The evaluation crate passes all-target Clippy with warnings denied. The isolated 134-package advisory scan has no findings (it reports an unavailable local crates.io index; no advisory exceptions). Both MySQL 8.4.6 and MariaDB 11.4.8 TLS fixtures passed a 65,536-byte limit with an oversized row, hostname/CA rejection and a clean fresh connection afterward. Reports are in ignored `artifacts/m003/packet-*.json`, with safe copies in `.ref/planning/m003-packet-evidence/`.

The runtime now enables a 2 MiB logical packet cap and tests oversized responses plus recovery on both engines. Expired-certificate checks are covered by the lifecycle runner. Cross-OS execution, aggregate memory/latency profiles, broader codecs and native live UI binding remain open. M003 is not complete.

## Provenance and licensing

Base: [SQLx v0.9.0](https://github.com/launchbadge/sqlx/tree/v0.9.0/sqlx-mysql), exact registry archive checksum in `upstream.json`. The patch and StoneQL harness are original modifications. SQLx is dual licensed MIT OR Apache-2.0; this evaluation selects MIT and includes the complete upstream grant in [SQLX-LICENSE-MIT](./SQLX-LICENSE-MIT), Copyright SQLx Contributors / portions LaunchBadge, LLC. The registry subcrate contains relative license placeholders; the preparer supplies the complete MIT license in the generated copy. No GPL translation/theme assets are incorporated.
