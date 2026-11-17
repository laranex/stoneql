# Release v0.1.0-beta.2 — plan (Proposed steps; decisions Approved 2026-11-17)

User request (2026-11-17): after all Windows and Linux tests pass, publish one GitHub **pre-release** with a Windows x64 installer, a Linux x64 `.deb` and an Intel Mac `.dmg`, all built from the same release commit, each verified, with SHA-256 checksums, platform install notes, known limitations and pending real-hardware checks labelled. Ask testers to try install, launch, database connection and saved-password reconnect, and to report their OS version and asset.

## Decisions (user, 2026-11-17)
- Tag `v0.1.0-beta.2` (SemVer, matches `v0.1.0-beta.1`); app versions bumped to `0.1.0-beta.2`.
- Release commit on a new branch `release/v0.1.0-beta.2` from `main`, merging `windows-support` and `linux-support`; `main` unchanged until the user decides.
- Windows: NSIS `.exe` (per-user, WebView2 download bootstrapper), unsigned (SmartScreen warning documented).
- macOS: Intel `x86_64` **and** Apple Silicon `.dmg`, ad-hoc signed like beta.1 (first-launch approval documented).

## Steps
1. Gates: Windows CI run 44121992103 (remote real-database step) and the Linux 22.04 `.deb` work pass.
2. Release branch: merge both branches, resolve the secrets conflicts, restore `macos-15` in `rust.yml`, bump versions, regenerate contracts if needed.
3. Verify the release commit: macOS locally (all required checks + fixtures), Linux on the test server, Windows by one CI run (including the remote real-database step).
4. Build from that commit: macOS x86_64 + aarch64 `.dmg` (Mac), Linux `.deb` (Ubuntu 22.04 container on the server), Windows NSIS (GitHub Actions job on the tag commit, artifact downloaded).
5. Verify each artifact: architecture, version, checksum; `.deb` install + Xvfb launch on 22.04 and Debian 12; NSIS silent install + launch on the Windows runner; `.dmg` mount, `lipo`/`file`, launch (x86_64 under Rosetta).
6. Publish the pre-release with notes and `SHA256SUMS`.

## Known facts for the notes (verify again at release)
- Windows: read-write is available (journal ported; documented gaps: no ACL check, no directory fsync). **SSH tunnels are not available on Windows yet** (endpoint design pending). Passwords: DPAPI files in `%APPDATA%`.
- Linux: passwords in the Secret Service; without one, a master-password encrypted file (minimum 8 characters).
- Pending on real hardware: Windows install/launch on a real PC, GNOME keyring prompt and master-password flow in a real session, Intel Mac on real Intel hardware.
