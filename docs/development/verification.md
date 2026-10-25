# Migration verification

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
