# Deployment proposal

**Status: Proposed / awaiting approval.** The ad-hoc signed public beta **v0.1.0-beta.1** (macOS arm64 DMG) was published as a GitHub pre-release on 2026-11-17 ([release](https://github.com/stoneAce/stoneql/releases/tag/v0.1.0-beta.1)); no Developer ID signing, notarization, server infrastructure or server deployment exists. The platform scope of the first release, StoneQL v0.1.0 Alpha for macOS, is an **Approved product decision** (2026-10-26), and so is its feature scope and release direction: a usable MySQL/MariaDB client distributed as a **signed and notarized DMG**, with separate desktop and server release gates and a Rust core independent of macOS host APIs (D022, 2026-10-26). Every packaging, signing and lifecycle mechanism below is a **Proposed technical decision** or **Unresolved release blocker**, except items labelled **Implemented and verified**, which describe only the current unsigned development host and fixture-level behavior.

## Independent release gates

Separate desktop and server release gates are an **Approved** principle (D009, reaffirmed by D022). M009 is Desktop Release; M011 is Server Release. M010-D and M010-S are separately accepted evidence sets, not a requirement to finish both before either release. Desktop distribution has no Axum, server-vault, server-SLO or server-release prerequisite. Core services/contracts remain shared and framework-independent; server support is retained without scaffolding it early.

Use the [roadmap G1–G6 gates and measurement profile](./roadmap.md#gates-applied-throughout) for security, exact values, compatibility, latency/memory, lifecycle reliability and packaging in every owning milestone. Numerical targets are proposed, not measured or approved. Missing scoped budgets, support matrix or release evidence block the affected track. Scope reduction requires explicit approval and accurate unsupported-feature labels, not silent test omissions.

## Desktop

Tauri 2 packages the shared frontend and IPC adapter. The architecture stays cross-platform; the first release is macOS only. Browser mode remains independent and uses mocks. Offline desktop operation must not depend on the documentation site's Mermaid CDN.

### Current host configuration — Implemented and verified

Observed in `apps/desktop/src-tauri/tauri.conf.json` and `Cargo.toml`: product `StoneQL`, version `0.1.0`, identifier `me.thixpin.stoneql`, `bundle.active: false` and an empty `bundle.icon` list (only a PNG source icon exists); `tauri.macos-adhoc.conf.json` turns bundling on for an ad-hoc signed `.app` (`npm run desktop:bundle:adhoc`). No Developer ID identity, entitlements file, minimum macOS version or updater plugin is configured. The CSP limits renderer connections to IPC; all database and SSH networking happens in the Rust host. `security-framework` is a macOS-only target dependency of the desktop host, not of shared crates. Debug development builds, a startup smoke and the ad-hoc signed bundle and DMG (the published beta) have run.

### Current public beta build

The public beta is **v0.1.0-beta.1** (tag → `970ade2`), published 2026-11-17 as a GitHub pre-release: `StoneQL-0.1.0-beta-970ade2.dmg` (arm64 only) with a `.sha256` file, built by `npm run desktop:bundle:adhoc` (the `.app` above inside a DMG with a drag-to-Applications layout). It is ad-hoc signed with Hardened Runtime and **not notarized**: the first launch of a downloaded copy is blocked (“StoneQL” Not Opened, with only Move to Trash and Done), and users choose **Open Anyway** in System Settings → Privacy & Security ([install guide](../getting-started/index.md#first-launch-of-the-downloaded-beta)). `Info.plist` declares macOS 10.13 as minimum (Tauri default; not decided), but the build was verified on macOS 27.0.1 only. SQLite is excluded from release builds (A5). Release-candidate checks on the DMG (checksum, `hdiutil verify`, `codesign --verify`, content scan, install/upgrade over the previous build, 30 cold launches, uninstall/reinstall, quarantined first launch) passed; Open Anyway was not exercised. This beta does not meet the signed-and-notarized release gates below, which remain open.

### v0.1.0 Alpha (macOS) packaging and lifecycle requirements

| Area | Requirement or proposal | Label |
| --- | --- | --- |
| Targets | macOS arm64 primary; x86_64 only after its own verification (separate build or universal binary is part of that decision) | Approved product decision; x86_64 inclusion and packaging form unresolved |
| Artifacts | Enable Tauri bundling for a release `.app` inside a `.dmg`, with `.icns` icons, SHA-256 checksums and an SBOM/license report bound to the commit | Proposed technical decision; DMG vs other format needs approval |
| Minimum macOS version | Set `bundle.macOS.minimumSystemVersion` explicitly and test on that version; do not rely on an unrecorded default | Unresolved release blocker; version needs approval |
| Signing | Developer ID Application certificate; sign every nested binary with Hardened Runtime and a secure timestamp; verify with `codesign --verify --deep --strict` and `spctl --assess` | Unresolved release blocker; Apple Developer account and identity custody need approval |
| Notarization | Submit with `notarytool` using credentials held outside the repository (CI secret store or release workstation keychain), staple the ticket to the DMG/app and verify with `stapler validate` and a quarantined first launch on a clean machine | Unresolved release blocker; credential custody needs approval |
| Hardened Runtime entitlements | Start with no exception entitlements (no JIT, unsigned executable memory, library-validation or dyld-environment exceptions); add one only with evidence that the signed app fails without it | Proposed technical decision |
| App Sandbox | Not proposed for Developer ID distribution in the alpha. If adopted, it would need at least network client, user-selected read-only file access and a review of SSH-agent socket and temporary Unix-socket tunnel paths, which a sandbox container may block | Proposed technical decision; sandbox yes/no needs approval |
| Keychain | Generic-password items in the login Keychain under service `me.thixpin.stoneql.credentials`; item access control follows the code signature, so the signed build must be tested for first-use and post-upgrade prompts, including the prompt racing the renderer's 8 s connect timeout | Implemented and verified for unsigned fixture runs; on the ad-hoc beta, a new build reading an item saved by an earlier build triggers two login-password prompts for that item; Always Allow did not persist for a new ad-hoc identity, while re-saving the password from the installed build made later reads silent (observed 2026-11-17). Developer ID-signed behavior is an Unresolved release blocker |
| Bundle identifier | `me.thixpin.stoneql` (approved 2026-11-16, A14), changed from `app.dbadmin.desktop` before the first Developer ID build. Observed: the bundled app keys its WebView store by the identifier (`~/Library/WebKit/<id>`, `~/Library/Caches/<id>`); the app data directory (`~/Library/Application Support/<id>`: SSH host keys, outcome journal) uses the identifier for bundled and unbundled runs; unbundled runs key WebView data by executable name. The Keychain service `me.thixpin.stoneql.credentials` moved with it. Impact on earlier developer data: [branding](../development/branding.md#bundle-identifier-change-2026-11-16). Freeze at the first public alpha | Approved; implemented |
| Updater | No auto-updater in v0.1.0 Alpha; users install a newer notarized DMG manually. A later updater requires a signed update feed, pinned public key and tamper/downgrade tests | Proposed technical decision |
| Install | Drag-to-Applications from the DMG; first launch passes Gatekeeper without user overrides | Drag-to-Applications executed on the ad-hoc beta DMG (2026-11-17); passing Gatekeeper without overrides needs notarization and remains an Unresolved release blocker |
| Upgrade | Replacing the app preserves non-secret profiles and Keychain references, keeps the same identifier and signing team, and is tested from each prior alpha build; schema changes to stored profiles need a reversible migration | Replacing the previous ad-hoc build kept profiles and saved-password references (2026-11-17); each ad-hoc build is a new code identity, so macOS asks again for Keychain access ([user steps](../getting-started/index.md#saved-passwords-and-the-keychain-prompt)). Signed-build behaviour and the migration policy remain Unresolved release blockers |
| Uninstall and credential cleanup | Dragging the app to the Trash leaves Keychain items and WebView data behind. Proposed: an in-app "remove all saved secrets" action, documented manual cleanup of remaining data, and surfacing (not ignoring) failed `delete_secret` calls on profile removal | Uninstall/reinstall drill executed on the beta DMG; manual cleanup is [documented](../getting-started/index.md#uninstall) and failed deletions are reported (**Remove anyway**). The in-app "remove all saved secrets" action is not implemented: Unresolved release blocker |
| Session lifecycle | Pools, tunnels, key-file/SQLite grants close on disconnect, expiry, renderer reload and shutdown | Implemented and verified with command-handler fixtures; graphical and signed-artifact verification unresolved |

### Signed and notarized DMG release gates (v0.1.0 Alpha)

All steps are **Unresolved release blockers** until executed on the actual release candidate; logs are kept with the desktop evidence set and bound to the artifact checksum.

1. Approved custody of the Apple Developer Team, *Developer ID Application* certificate and `notarytool` credentials, outside the repository.
2. Bundling enabled with DMG target, `.icns` icons, explicit minimum macOS version, final identifier, Hardened Runtime and an entitlements file without exception entitlements.
3. Clean tagged build with pinned toolchains for `aarch64-apple-darwin` (Intel only if approved and verified).
4. Sign nested code, app and DMG with `codesign --options runtime --timestamp`.
5. Verify with `codesign --verify --deep --strict --verbose=2`, inspect entitlements and the Hardened Runtime flag.
6. Notarize with `xcrun notarytool submit --wait`; keep the log.
7. Staple and validate: `xcrun stapler staple` / `stapler validate`, `spctl -a -vvv -t exec` on the app, `spctl -a -t open --context context:primary-signature` on the DMG, `hdiutil verify`.
8. Quarantined first launch on a clean Mac (minimum and current macOS), online and offline, with no Gatekeeper override.
9. Install, upgrade-from-each-prior-alpha, downgrade stance and uninstall drills, including Keychain prompts and saved-secret cleanup.
10. Credential cleanup and session lifecycle: failed secret deletion reported; no orphan tunnels, pools, grants or temporary transfer files after quit, crash relaunch, sleep/wake or network change.
11. Checksums, SBOM/license report and an artifact secret scan.

### Alpha budgets (approved 2026-11-17)

| Budget | Proposed value |
| --- | --- |
| Cold launch to interactive | p95 at most 3 s over 30 launches of the signed arm64 build |
| Whole-app memory (host plus WebKit processes) | Measure first; proposed at most 300 MiB idle and 400 MiB with eight connections |
| Basic SQL | p95 at most 500 ms; at most 64 MiB above connected idle |
| Cancellation | Accepted cancel to terminal state p95 at most 2 s |
| Transfers | One million 1 KiB rows with a slow consumer within 256 MiB additional memory; temporary files removed within 30 s of cancellation |
| SSH transport | Connect p95 at most 6 s; per-query overhead at most 50 ms above Direct on the loopback reference |

The user approved these values on 2026-11-17 (A3, blanket approval); they are release targets, not measured guarantees. The beta DMG met the cold-launch budget (p95 2.79 s) and idle memory (191 MiB); the eight-connection memory, transfer and release-build SSH measurements are not done yet; the [roadmap measurement profile](./roadmap.md#gates-applied-throughout) remains the shared definition.

### Deferred desktop platforms

Windows and Linux desktop packaging, signing (Authenticode, distribution package signing), credential stores, SSH agents and native runners are **Deferred** and must not block v0.1.0 Alpha. Their open findings stay open for later releases: Linux GTK dependency advisories (RUSTSEC-2024-0429, RUSTSEC-2024-0370), no Windows SSH tunnel endpoint, and unexecuted Windows/Linux CI. No Windows/Linux-specific code is removed; shared crates gain no macOS dependency.

## Future Linux server

Start with one standalone Axum binary serving the built Vue frontend behind a TLS reverse proxy. Use systemd, a dedicated service identity, scoped state/runtime directories and read-only application files. Configure resource ceilings, graceful shutdown, restart backoff and health checks. Where compatible, use systemd hardening such as ProtectSystem and NoNewPrivileges; verify required database networking and SQLite file access rather than copying an untested unit.

Secrets should arrive through the approved vault or restricted credential mechanism, never checked-in unit Environment values. Back up non-secret profiles and encrypted vault metadata separately from keys. Define supported upgrade/rollback and state migrations before release. No HA, distributed queue or Kubernetes is justified by current requirements; revisit only after concrete availability/load targets.

## Optional Docker

Package the same server binary with a non-root identity, minimal runtime, read-only root filesystem, dropped capabilities and explicit state/temp mounts. Do not mount the Docker socket or host credential stores. Networking and file policies still apply. Containerization does not replace authentication or secret isolation. Native systemd deployment remains supported.

## Documentation hosting

The VitePress workflow builds pull requests and deploys only on explicit manual dispatch with deployment selected. Repository subpaths and custom domains are configurable. The site is published at <https://stoneace.github.io/stoneql/>. See [website maintenance](../development/documentation.md).

## Release gates and unknowns

Before M009's v0.1.0 Alpha sign-off (feature scope approved by D022): approve signing/notarization custody, the bundle identifier, minimum macOS version, x86_64 inclusion, artifact format and startup/install/upgrade budgets, then run signed native smoke and end-to-end tests on macOS using the actual release artifact. Windows/Linux desktop gates move to their later releases. Define server OIDC/vault/TLS ownership, Linux compatibility, resource/availability/SLO and RPO/RTO targets before M011; do not require server decisions for desktop distribution. None of these production checks are part of the browser prototype's passing unit suite.
