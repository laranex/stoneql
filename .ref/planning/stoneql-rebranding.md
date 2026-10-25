# StoneQL rebranding audit

## Scope and decision status
User requested repository-wide rebranding to StoneQL / StoneAge SQL Admin / Old habits. New engine. This changes naming and original assets only. Existing implementation authorizations, milestone IDs, technical decisions and incomplete release gates are preserved. No repository/remote rename, publication or deployment authorized.

## Reference classification

| Area | Action | Reason |
| --- | --- | --- |
| Active Vue labels, Help, page metadata, website, README/AGENTS/public guides | Rename | Current product identity |
| Private npm workspace packages and root commands/lock links | Rename to `stoneql` / `@stoneql/*` | Private internal machine names; dependency versions unchanged |
| Rust crates/paths/imports, generated DTO provenance, native executable | Rename to `stoneql-*` | Internal implementation names; no protocol/behavior change |
| Tests, CI commands, fixture prefixes/database environment variables | Rename together | Preserve executable coverage and isolated resource ownership |
| Local-storage preferences | Migrate validated allowlist, retain legacy originals | Preserve user choices without copying credentials or overwriting StoneQL settings |
| Native event names | Rename sender/receiver and tests together | Internal session-only API |
| Tauri reverse-DNS identifier | Preserve `app.dbadmin.desktop` | External/installed identity change not explicitly authorized |
| Git directory/remotes, external repository/domain configuration | Preserve | User explicitly excluded automatic repository/external identifier changes |
| Evidence directories/JSON/checksums, original migration baseline and UX report path | Preserve | Immutable historical evidence, not current branding |
| Vendor provenance, license/source bytes and evaluated patch | Preserve | Auditability and exact approved source; no vendor rebrand |
| Active milestone prose and decision register | Rename product/current crate names | Approval states, IDs and historical facts unchanged |

## Asset approach
Original code-native SVG: layered cut stone forming a database stack. Existing rasterizer derives the native PNG using sandboxed Chromium. No image-generation dependency, copied assets, gradients or screen redesign.

## Verification
Executed locally on macOS ARM with Node 26.11.1 and Rust 1.99.1:

- 154 JavaScript tests passed (137 frontend +17 repository), including secret-free/idempotent migration, malformed/blocked storage and all six existing page-size choices.
- 22 Playwright scenarios passed with sandboxed Chromium. Actual UI legacy-profile/theme migration and official full name/tagline assertions added; Light/Dark branding screenshots visually inspected.
- 30 Rust workspace tests passed; one owning-fixture test intentionally ignored in the default suite was separately executed against both real MySQL/MariaDB TLS fixtures through actual native handlers. Both passed and removed owned resources.
- Two isolated receive-limit socket tests and the boundary/overflow/default unit test passed with the renamed harness.
- Rust formatting, all-target Clippy, generated TypeScript reproducibility, architecture boundaries and SQLx 76-file provenance passed.
- Frontend production, embedded native and VitePress builds passed; documentation navigation/local links/assets verified. Final real StoneQL macOS Wry startup/IPC smoke passed in733ms.
- npm lock records match the former lock after private-name substitution only: no dependency version/integrity changes. Cargo changes rename local workspace packages and regenerate their sorted entries, with no registry upgrades.
- Confirmed no historical evidence/approved patch/vendor byte changes. Local progress files remain ignored and untracked. Git remotes and checkout path untouched.

Existing nonfatal frontend chunk-size advisory remains. Full native GUI/database, Windows/Linux, signed packaging and the two Linux GTK audit findings remain separate M003/M002 gates; no release/security completion is inferred. No new backend functionality, credentials, deployment or external identifier changes.
