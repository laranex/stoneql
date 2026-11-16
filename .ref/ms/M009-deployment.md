# M009 — Desktop Release

## Status
Planned / not started. **First release re-scoped to StoneQL v0.1.0 Alpha for macOS (D021, Approved product decision 2026-10-26).** **Feature scope and release direction approved by D022 (2026-10-26):** usable MySQL/MariaDB client (live browsing, CRUD, safe SQL, SSH, basic SQL/CSV import/export), signed and notarized DMG planned, separate desktop/server gates. Implementation plan: `.ref/planning/v0.1.0-alpha-plan.md` workstream (f) (Proposed). The user authorized roadmap continuation and local milestone commits. Prerequisites, unresolved technical decisions and security/release acceptance gates still apply; no push, publication or deployment is authorized.

## Objective and scope
Signed desktop distribution. First release: **StoneQL v0.1.0 Alpha**, signed, notarized and stapled macOS package for Apple Silicon (arm64), with Intel (x86_64) only if separately verified. Windows and Linux packages are Deferred to later desktop releases. This retains the M009 specification path while separating all server packaging into M011.

## Dependencies and prerequisites
For the alpha: M002, M003 and M005 macOS acceptance (M005 including the D022 blockers F-01–F-04 and secret-deletion error handling); the D022 alpha slices of **M006** (safe SQL execution, schema metadata, live browsing, single-row CRUD) and **M007** (SQL/CSV import/export for MySQL/MariaDB). M004 PostgreSQL only if admitted at feature freeze (optional, D022); SQLite Deferred. **M010-D macOS evidence only**; D009-D signing/notarization/update/artifact ownership approved. **No M008, M011, server SLO/vault approval or M010-S completion dependency.**

## Technical approach
Package the shared frontend and Tauri adapter on supported native runners; verify signatures/notarization and restricted updater trust. Keep SQLite/credential/file migrations reversible and browser/mock mode intact.

## Deliverables
Versioned platform artifacts, checksums/SBOM, installation/update/rollback instructions and independent desktop go/no-go record.

## Acceptance criteria
- G1: no bundled secrets; signed/notarized artifacts verified where applicable; unauthorized IPC, tampered/unsigned update and dependency findings handled under approved risk policy.
- G2: upgrade/rollback and uninstall preserve user databases/non-secret profiles/secret references according to explicit policy; no migration data loss or precision change.
- G3: every declared alpha combination (macOS arm64, x86_64 if shipped, minimum macOS version, MySQL/MariaDB fixture versions and any other approved engines) passes native packaging and existing UI regression tests; release notes identify unsupported capabilities and DTO compatibility.
- G4/G5: approved cold-start/lifecycle/performance budgets and failure/restore drills pass using actual release artifacts; M010-D accepted independently of server readiness.
- G6: install/launch/upgrade/rollback/uninstall, signing, checksums and offline required workflows verified for every distributed package; artifact-size/retention policy recorded before sign-off.

## Required tests and verification
Native platform signature/install/update-tamper tests; migration/restore/rollback and offline smoke tests; full supported UI/core/engine suites on release candidates; desktop measurement profile.

## v0.1.0 Alpha (macOS) required evidence
None of the following has been executed. Each is an **Unresolved release blocker** unless labelled otherwise.
- Approvals (human): alpha feature scope (now Approved, D022); Apple Developer account, Developer ID identity and notarization credential custody; bundle identifier (**decided 2026-11-16, A14: `me.thixpin.stoneql`**, implemented with the Keychain service constant; see `docs/development/branding.md`); minimum macOS version; x86_64 inclusion and form; artifact format (DMG is a Proposed technical decision); updater stance (none for the alpha is a Proposed technical decision); App Sandbox (not proposed).
- Packaging: release bundling enabled, `.icns` icons, reproducible build from a tagged commit, SHA-256 checksums, SBOM/license report, secret scan of the artifact.
- Signing/notarization: `codesign --verify --deep --strict --verbose=2`, `spctl --assess --type execute`, Hardened Runtime with no exception entitlements unless evidenced, `notarytool` acceptance log, `stapler validate`, quarantined first launch on a clean macOS install with no Gatekeeper override.
- Native E2E on the signed artifact with real MySQL 8.4.6 / MariaDB 11.4.8 fixtures (or approved versions): Direct and SSH (agent, password, key file with passphrase), host-key enrollment/changed-key refusal, TLS verification, bounded read-only queries with exact values, disconnect, reload, quit with unsaved edits.
- Keychain: first-save, first-use and post-upgrade access prompts on the signed build; prompt within the 8 s renderer connect timeout (or timeout adjusted under review); deny/cancel handled without leaking state.
- Install/upgrade/uninstall drills: DMG install; upgrade from each earlier alpha build preserving non-secret profiles and Keychain references; downgrade/rollback stance documented; uninstall plus documented or in-app cleanup leaving no StoneQL Keychain items when requested.
- Credential cleanup and session lifecycle: failed secret deletion surfaced to the user (currently ignored); no orphan tunnels/pools/grants after quit, crash relaunch, sleep/wake and network change.
- Startup/resource profile: 30 cold launches and whole-app RSS (host plus WebView processes) idle and with eight connections on arm64 release build; x86_64 if shipped. Numerical budgets stay Proposed until approved.

## v0.1.0 Alpha signed and notarized DMG gates (Proposed procedure; none executed)
Each step must run on the actual release candidate and keep its log in the M010-D evidence bundle, bound to the artifact SHA-256 and commit.
1. **Identity and custody (approval):** Apple Developer Program Team; *Developer ID Application* certificate (and *Developer ID Installer* only if a `.pkg` is ever added) held in a release keychain or CI secret store, never in the repository; `notarytool` credentials (App Store Connect API key preferred over an app-specific password) stored with `notarytool store-credentials` or the CI secret store. Who holds them and how they are rotated/revoked must be recorded.
2. **Bundle configuration:** `bundle.active: true`, DMG target, `.icns` icons, explicit `bundle.macOS.minimumSystemVersion`, final identifier per D023, Hardened Runtime enabled, entitlements file with **no exception entitlements** (no `allow-jit`, `allow-unsigned-executable-memory`, `disable-library-validation`, `allow-dyld-environment-variables`); add one only with evidence that the signed app fails without it. App Sandbox not enabled (Proposed).
3. **Build:** clean checkout of a tagged commit; pinned Rust 1.99.1 / Node 26 toolchains; `npm ci`; reproducible release build for `aarch64-apple-darwin` (x86_64/universal only if approved and verified).
4. **Sign:** `codesign --force --options runtime --timestamp --sign "Developer ID Application: …"` on every nested binary inside-out (Tauri's bundler does this when `signingIdentity` is set), then the `.app`, then the DMG.
5. **Verify signature:** `codesign --verify --deep --strict --verbose=2 StoneQL.app`; `codesign -d --entitlements - StoneQL.app` shows only approved entitlements; `codesign -dv` shows the Team ID, Hardened Runtime flag (`flags=0x10000(runtime)`) and secure timestamp.
6. **Notarize:** `xcrun notarytool submit StoneQL_0.1.0_aarch64.dmg --keychain-profile … --wait`; status `Accepted`; retain `notarytool log` output and review its warnings.
7. **Staple and validate:** `xcrun stapler staple` on the DMG (and the `.app` before packaging if the app is distributed separately); `xcrun stapler validate`; `spctl -a -vvv -t exec StoneQL.app` → `source=Notarized Developer ID`; `spctl -a -vvv -t open --context context:primary-signature StoneQL_….dmg`; `hdiutil verify` on the DMG.
8. **Quarantined first launch:** download over HTTP(S) or set `com.apple.quarantine` on a clean macOS install at the minimum supported version and on the current release; open the DMG, drag to Applications, launch with no Gatekeeper override and no "unidentified developer" prompt; repeat offline to confirm the stapled ticket.
9. **Install/upgrade/uninstall drills:** fresh install; upgrade over each earlier alpha build preserving non-secret profiles, Keychain references, enrolled host keys (native store, F-01) and theme/grid preferences; Keychain prompts on first save, first use and after upgrade (same Team ID ⇒ expected silent; record actual behaviour); downgrade stance documented (Proposed: unsupported, profiles forward-compatible only); uninstall by Trash leaves Keychain items and WebView data — documented manual cleanup plus an in-app "remove all saved secrets" action (Proposed) verified to leave zero items under the Keychain service.
10. **Credential cleanup and session lifecycle:** failed secret deletion surfaced (D022 blocker); no orphan tunnels/pools/grants/temp transfer files after quit, crash relaunch, sleep/wake and network change.
11. **Artifact hygiene:** SHA-256 checksums published beside the DMG; SBOM/license report (Rust + npm); secret scan of the `.app` contents; artifact size and retention recorded.

## Separate desktop and server release gates (Approved principle, D009/D022)
| Track | Release | Evidence | Must not depend on |
| --- | --- | --- | --- |
| Desktop | M009 (first: v0.1.0 Alpha, macOS arm64) | M010-D (macOS-only set for the alpha) | M008, M011, M010-S, server identity/vault/SLO decisions |
| Server | M011 (Linux binary/systemd, optional Docker) — **unchanged** | M010-S | M009 distribution, M010-D, desktop signing |
Shared Rust core/contracts stay host- and OS-independent (no macOS API in shared crates; `rust:boundaries`).

## Proposed alpha budgets (recorded for approval; none approved)
Full table with current values and sources: `.ref/planning/v0.1.0-alpha-plan.md` §Unresolved numerical limits. Release-facing subset:
| Budget | Proposed value | Measured on |
| --- | --- | --- |
| Cold launch → interactive | p95 ≤ 3 s over 30 launches | signed arm64 release DMG install |
| Whole-app RSS (host + WebKit processes) | record first; ≤ 300 MiB idle, ≤ 400 MiB with 8 connections | same |
| Basic SQL | p95 ≤ 500 ms; +64 MiB above connected idle | 100,000-row fixture, 200-row pages |
| Cancel → terminal | p95 ≤ 2 s, 100 samples per engine | MySQL/MariaDB, Direct and SSH |
| Transfers | 1M × 1 KiB rows, slow consumer, +256 MiB RSS; temp files removed ≤ 30 s after cancel | CSV and SQL export/import |
| Transport | SSH connect p95 ≤ 6 s; per-query overhead ≤ Direct + 50 ms p95 (loopback reference) | agent/password/key |
| Lifecycle smoke | 100 lifecycle/upgrade cycles, 0 crashes or data-loss incidents | release artifact |

## Deferred (not blocking v0.1.0 Alpha)
Windows and Linux packaging, signing, installers, credential stores, SSH agents, native runners and their CI; Linux GTK advisories. All remain M009 gates for the release that ships those platforms.

## Risks and limitations
Signing custody, artifact formats, OS minima and updater operations need approval. Changing the bundle identifier would move WebView/app-support data of the bundled app; the Keychain service is a separate hard-coded constant (`secrets.rs` `DEFAULT_SERVICE`) and only moves if changed with it (review: `.ref/planning/v0.1.0-alpha-plan.md` §Bundle identifier migration review). Keychain prompts can change when the signing identity changes. Missing server work cannot block desktop sign-off. Graphical docs CDN is not a desktop runtime dependency.


Public context: `docs/architecture/roadmap.md` (G1–G6 and proposed measurement profile). Approval records: `.ref/planning/architecture-decisions.md`. No acceptance criterion is satisfied by this planning edit; record executed evidence before marking complete.
