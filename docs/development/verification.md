# Migration verification

## v0.1.0 Alpha (macOS) verification checklist

The macOS-only platform scope of StoneQL v0.1.0 Alpha is an **Approved product decision** (2026-10-26), as is its feature scope (D022): a usable MySQL/MariaDB client with live browsing, CRUD, safe SQL execution, SSH and basic SQL/CSV import/export. This checklist separates what has run from what the alpha still needs. "Executed" rows ran locally on macOS ARM (Apple Silicon) against disposable fixtures with debug builds; none ran against a signed release artifact. Nothing here marks a milestone complete. See the [roadmap](../architecture/roadmap.md#v0-1-0-alpha-macos) and [deployment requirements](../architecture/deployment.md).

### Executed — Implemented and verified (local, fixtures)

| Check | Command or evidence | Limits |
| --- | --- | --- |
| Rust build/fmt/Clippy/tests, contract drift, crate boundaries, vendored SQLx provenance | `npm run rust:build`, `rust:fmt`, `rust:lint`, `rust:test`, `contracts:check`, `rust:boundaries`, `rust:vendor` | Local macOS; CI prepared, not remotely executed |
| Frontend unit, build and browser scenarios (Light/Dark, live-bridge simulations) | `npm test`, `npm run build`, `npm run test:browser` | Browser shims, not the native WebView |
| Native WebView startup smoke | `npm run desktop:smoke` (453 ms latest, debug) | Startup and mount only; unsigned debug build |
| Native IPC ACL, exact values, protocol rejection, close cancellation | `npm run desktop:test` | Tauri test runtime |
| MySQL/MariaDB live read-only handlers with TLS fixtures | `npm run test:native-database`, `test:lifecycle` | MockRuntime command handlers, not GUI |
| Runtime resource profile (100,000-row workload, RSS) | `npm run test:resources` ([profiles](./resource-profiles.md)) | Runtime process only; excludes Tauri/WebView; debug |
| SSH tunnels, host-key policy, 100 mixed cycles, Keychain references | `npm run test:ssh` ([SSH and secrets](./ssh-credentials.md)) | Per-run Keychain service, unsigned; no real prompts or picker |
| M006 execution: BUSY ownership, cancel→terminal samples, transaction outcomes, 100 fault/transaction cycles, session-state isolation | `npm run test:execution` ([SQL execution](./sql-execution.md)) | Uncommitted 2026-10-26 implementation; Direct MySQL/MariaDB/PostgreSQL only — read-write over SSH gated off; lost-COMMIT fault injection not executed; limits Proposed |

### Required — not yet executed (Unresolved release blockers)

| Requirement | What must be shown |
| --- | --- |
| Native packaging | Release `.app`/`.dmg` built with bundling enabled, icons, explicit minimum macOS version; checksums and SBOM |
| Signing and notarization | `codesign --verify --deep --strict`, `spctl --assess`, accepted `notarytool` submission, `stapler validate`, quarantined first launch on a clean machine |
| Native end-to-end | Signed app driving real MySQL/MariaDB Direct and over SSH: enrollment, auth methods, TLS refusal cases, queries with exact values, disconnect, reload, quit with unsaved edits |
| Keychain | First-save, first-use and post-upgrade prompts on the signed build; prompt within the connect timeout; denial handled |
| Install/upgrade/uninstall | DMG install; upgrade from earlier alpha builds preserving profiles and secret references; uninstall and secret cleanup verified |
| Credential cleanup and session lifecycle | Failed secret deletion surfaced; no orphan tunnels/pools/grants after quit, crash relaunch, sleep/wake and network change |
| Startup and resource usage | 30 cold launches and whole-app RSS on arm64 release build against approved budgets (currently Proposed) |
| Intel (x86_64) | Same evidence on Intel hardware, only if x86_64 is included |
| Dependency audit | Audit result applicable to the macOS artifact, with Linux-only advisories excluded by evidence or explicitly risk-accepted |
| SSH review blockers (D022) | Native host-key trust (renderer-supplied keys ignored), Keychain resolution inside cancellable bounded setup, dead-tunnel slot release, host/port edits invalidate trust, failed secret deletion reported |
| Safe SQL execution (D022) | `BUSY` on a busy connection with nothing queued; cancel-to-terminal within the approved bound; unknown outcomes never retried; transaction expiry and cleanup; MySQL write cancellation and SSH cancellation-slot exhaustion resolved before unrestricted read-write SQL |
| Live browsing and CRUD (D022) | Database/table tree, paged browse with server-side sort/filter, single-row insert/update/delete with optimistic conflict detection and database constraint errors, exact values and NULL/default distinctions, on MySQL and MariaDB Direct and over SSH |
| SQL/CSV transfers (D022) | Export and import round trips with independent read-back, quota/cancel/disk-full cleanup, truthful partial outcomes |
| Metadata allocation bound | Decision on the proposed driver-level result-metadata cap, and its fixtures if adopted |

### Deferred (not required for the alpha)

Windows and Linux native runs, credential stores, SSH agents, packaging and remote cross-OS CI remain open for later releases and are not counted as passed. Under D022, SQLite live support, advanced database objects and XLSX import are also Deferred, and PostgreSQL is optional: their existing fixture commands (`npm run test:native-postgres` and the SQLite runtime tests) keep running as regressions but are not alpha release evidence unless PostgreSQL is admitted.

## M001 foundation verification — 2026-10-25

The approved Rust foundation/evaluation adds no production connectivity or frontend integration. Local checks passed: Rust workspace build/fmt/all-target Clippy, 12 Rust tests, generated TypeScript drift/strict compilation, host/driver/runtime boundary checks, 28 live driver assertions, cargo-audit with zero vulnerabilities/warnings after Tokio remediation, 128 frontend tests plus seven root tests, frontend build, 17 browser scenarios (12.9s), documentation build and workflow lint. The 73-file frontend preservation audit still passes. See [driver evaluation](../architecture/driver-evaluation.md) and [Rust development](./rust-foundation.md) for reproducible evidence, tested fixtures and limitations. Native Windows/Linux hosted CI and production TLS/pooling/transaction guarantees remain unverified; M001 completion review and M002 approval are pending. No commit or push was made for M001.

## Latest recheck — 2026-10-25

Approved elevated execution of `npm run test:browser` passed **all 17 scenarios in 13.2 seconds**, covering both themes, column dialogs, grids, database objects, remediation, export, CRUD and connection workspaces. Chromium sandboxing remained enabled. The sandboxed run reached Vite but failed at Chromium macOS Mach-port registration; approved execution resolved that restriction.

Git index-write verification also succeeded with approved elevation. The earlier listener, browser and Git failures below describe historical restricted runs, not the latest outcome. Graphical Mermaid/live documentation checks and external-link verification remain outstanding. This verification does not approve backend implementation.


Verification uses Node 26.11.1 and npm 11.20.0. Results below distinguish executed checks from environment-blocked workflows.

## Executed checks

- Migrated frontend: 128 automated tests passed.
- Migrated frontend production build passed; existing nonfatal large-bundle advisory remains.
- Source, test, asset and application-config SHA-256 preservation is checked separately against the one-time migration baseline. It does not freeze future intentional frontend development.
- Root regression suite: 128 frontend tests plus 6 workspace/documentation tests passed (134 total).
- Separate migration baseline: all 73 original source/test/asset/application-config files match SHA-256 hashes.
- Original frontend dependency lock records are unchanged. Clean `npm ci --offline --ignore-scripts --no-audit --no-fund` succeeded using a temporary cache of genuine npm artifacts; `npm ls --all` passed.
- Documentation production builds and local link/asset checks passed at `/`, `/stoneql/`, and custom-domain `/` with CNAME generation.
- All 17 browser scenarios were collected across seven files. Execution was retried and blocked before any scenario ran.
- Root frontend dev/preview and docs dev/preview commands resolve the correct workspace, then fail at local listener creation with EPERM. Live preview/navigation and screenshots remain unverified.
- Git confirms `.ref/workprogress.md` is ignored and not tracked. No native framework dependency or Rust placeholder was introduced.

## Browser blocker

All 17 existing Playwright scenarios remain required. The post-migration run rejects the Vite listener with `listen EPERM: operation not permitted 0.0.0.0:5173`. Test discovery is not browser verification. Native browser startup also failed in earlier stabilization attempts. No browser sandbox or host security controls are disabled.

Consequently Light/Dark screenshots, interactive object workflows, resize behavior and dialog focus have not been reverified after migration. Run `npx playwright install chromium` and `npm run test:browser` on a supported host. Existing object coverage is documented separately in [database objects](../database-objects.md).

## Documentation checks

Root docs are the canonical source. Source links/navigation and generated page/asset paths are checked by `npm run docs:build`. Mermaid loads from a pinned browser CDN and has a source fallback; graphical rendering requires browser/network verification. External links are not validated by the local checker. No GitHub Pages deployment has occurred.

## Migration-related fixes

The new checks found and fixed three documentation-only issues: VitePress output paths initially resolved outside `.website/`, archived README links needed paths relative to their new location, and base-path validation needed to reject dot segments. Generated-link checks now verify the actual configured output. No Vue source or existing test styling was changed.

The one-time preservation audit can be reproduced with `node --test .ref/planning/preservation.test.mjs`; future intentional source changes should not update this historical migration evidence merely to make a general test suite pass. Root tests enforce ongoing workspace/documentation contracts instead.

## Remaining limitations

No browser, native WebView, screen-reader or graphical Mermaid verification is claimed. External URLs were not exhaustively checked. GitHub repository links require environment configuration locally because no remote is configured; Actions derives the real repository automatically. The frontend retains its existing nonfatal bundle-size advisory. Architecture decisions are proposals, and no backend, real database operation, publication or deployment was performed.

## Commit status

A task-completion commit was requested and attempted. `git add` failed with `Unable to create '.git/index.lock': Operation not permitted` because this execution environment grants read-only access to `.git`. No files were staged and no commit was created. No push or deployment was attempted. Commit the reviewed source/documentation changes from a terminal with authorized Git write access; keep local progress and generated output excluded.
