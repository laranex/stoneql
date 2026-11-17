# StoneQL branding and compatibility

**StoneQL** is the product name; **StoneAge SQL Admin** is its expanded name. The official tagline is **Old habits. New engine.** The former product name was dbAdmin. Use `stoneql` for private machine-readable names and `stoneql-*` for Rust crates.

The app icon (adopted 2026-10-26) shows a faceted stone cradling a database cylinder on a rounded slate-blue tile. It is user-supplied raster artwork. `apps/desktop/src-tauri/icons/icon-source.png` is the 1024 px master, cropped from the supplied image to the tile with a transparent margin on Apple's icon grid; `node tools/prepare-desktop-icon.mjs` regenerates the 512 px desktop `icon.png` and the 256 px `apps/frontend/public/icon.png` used by the frontend header, favicon and documentation site. It replaces the earlier repository-authored cut-stone SVG mark. The base Tauri configuration keeps bundling off; the ad-hoc bundle configuration (`tauri.macos-adhoc.conf.json`, used for the published beta) uses `icons/icon.png` as the bundle icon. UI density and semantic status colors are unchanged.

## Compatibility boundaries

- Private npm workspaces, Rust crate directories/names/imports, fixture prefixes, native events and generated contract provenance use the new name. No SQL behavior, protocol version, milestone ID or approval state changes.
- At startup, valid legacy `dbadmin-*` theme, page-size, connection/group and grid-width preferences migrate to `stoneql-*` only when no new value exists. Connection migration uses the existing non-secret allowlist; secrets and CA contents are not copied. Legacy keys remain for rollback. No origin or native application data location is moved.
- The bundle identifier is `me.thixpin.stoneql` (approved 2026-11-16, A14/D023), replacing `app.dbadmin.desktop` before any signed or published build. The Keychain service follows it (`me.thixpin.stoneql.credentials`). See *Bundle identifier change* below for what the change does to existing data. Any later change needs its own migration plan and approval. The display name/window title are StoneQL, and internal executable/crates use `stoneql-*`. The public beta v0.1.0-beta.1 is published with this identifier, ad-hoc signed and not notarized.
- Git checkout name, remotes, GitHub repository paths, Pages base and future domain are not renamed. Documentation derives repository links and Pages paths from explicit configuration/GitHub context.
- Historical evidence, source hashes, benchmark/audit snapshots and migration records retain original dbAdmin names and old crate paths as recorded at their commits. They describe earlier source, not the current tree. The original UX report path remains unchanged.
- SQLx vendor source, MIT notices, `DBADMIN.md`, `dbadmin-provenance.json` and the evaluated receive-limit patch remain byte-identical. Their names are provenance, not active application branding. `npm run rust:vendor` continues to verify all 76 source files.

See the internal rebranding audit for verification results. The reverse-DNS identifier was approved on 2026-11-16 (A14) and is used by the published beta; changing it later would move user data (see below).

## Bundle identifier change (2026-11-16)

`app.dbadmin.desktop` → `me.thixpin.stoneql`, in one change with the Keychain service constant
(`DEFAULT_SERVICE` in the desktop host, `CREDENTIAL_SERVICE` in the frontend) and the SSH fixture's
per-run test service prefix. No migration code: only developer machines hold data from earlier
builds, which were never signed with a Developer ID or published.

| Data | Where (old → new) | Effect for data saved by earlier builds |
|---|---|---|
| Saved passwords and passphrases (Keychain) | service `app.dbadmin.desktop.credentials` → `me.thixpin.stoneql.credentials` | Not read any more. Connecting with a profile that referenced one fails with "The saved credential is unavailable or belongs to a different target. Enter it again."; re-enter it and save. The old items stay in the login Keychain until deleted |
| Trusted SSH host keys | `~/Library/Application Support/app.dbadmin.desktop/ssh-host-keys.json` → `…/me.thixpin.stoneql/ssh-host-keys.json` | Every host is de-enrolled: the next connection shows the host key for explicit trust again. Nothing is trusted automatically |
| Unknown-outcome journal | `…/app.dbadmin.desktop/outcome-journal/` → `…/me.thixpin.stoneql/outcome-journal/` | Outcomes recorded by earlier builds are no longer listed; inspect them in the old directory if needed |
| Profiles, theme, grid preferences (WebView storage) | bundled app: `~/Library/WebKit/<identifier>` and `~/Library/Caches/<identifier>`; unbundled development runs: keyed by the executable name (`stoneql-desktop`) | The bundled app starts with no profiles. Unbundled development runs are unaffected |

Developer reset (manual, optional; nothing is deleted automatically):

```sh
security find-generic-password -s app.dbadmin.desktop.credentials     # repeat delete until this fails
security delete-generic-password -s app.dbadmin.desktop.credentials
rm -r ~/Library/Application\ Support/app.dbadmin.desktop ~/Library/WebKit/app.dbadmin.desktop ~/Library/Caches/app.dbadmin.desktop
```

Keychain access for items created by the new build is bound to its code signature: ad-hoc builds
bind to the binary hash (every rebuild is a new identity and prompts), Developer ID builds to the
identifier and Team ID.

