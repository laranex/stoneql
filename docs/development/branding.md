# StoneQL branding and compatibility

**StoneQL** is the product name; **StoneAge SQL Admin** is its expanded name. The official tagline is **Old habits. New engine.** The former product name was dbAdmin. Use `stoneql` for private machine-readable names and `stoneql-*` for Rust crates.

The app icon (adopted 2026-10-26) shows a faceted stone cradling a database cylinder on a rounded slate-blue tile. It is user-supplied raster artwork. `apps/desktop/src-tauri/icons/icon-source.png` is the 1024 px master, cropped from the supplied image to the tile with a transparent margin on Apple's icon grid; `node tools/prepare-desktop-icon.mjs` regenerates the 512 px desktop `icon.png` and the 256 px `apps/frontend/public/icon.png` used by the frontend header, favicon and documentation site. It replaces the earlier repository-authored cut-stone SVG mark. Bundled macOS `.icns` output is not generated yet because packaging is disabled (M009). UI density and semantic status colors are unchanged.

## Compatibility boundaries

- Private npm workspaces, Rust crate directories/names/imports, fixture prefixes, native events and generated contract provenance use the new name. No SQL behavior, protocol version, milestone ID or approval state changes.
- At startup, valid legacy `dbadmin-*` theme, page-size, connection/group and grid-width preferences migrate to `stoneql-*` only when no new value exists. Connection migration uses the existing non-secret allowlist; secrets and CA contents are not copied. Legacy keys remain for rollback. No origin or native application data location is moved.
- The Tauri bundle identifier remains `app.dbadmin.desktop`. Changing an installed/published identifier can affect identity, upgrades and storage; it requires separate explicit approval. The display name/window title are StoneQL, and internal executable/crates use `stoneql-*`. No package publication or signing is performed.
- Git checkout name, remotes, GitHub repository paths, Pages base and future domain are not renamed. Documentation derives repository links and Pages paths from explicit configuration/GitHub context.
- Historical evidence, source hashes, benchmark/audit snapshots and migration records retain original dbAdmin names and old crate paths as recorded at their commits. They describe earlier source, not the current tree. The original UX report path remains unchanged.
- SQLx vendor source, MIT notices, `DBADMIN.md`, `dbadmin-provenance.json` and the evaluated receive-limit patch remain byte-identical. Their names are provenance, not active application branding. `npm run rust:vendor` continues to verify all 76 source files.

See the internal rebranding audit for verification results. Native release compatibility and the final reverse-DNS identifier still require human review before packaging.
