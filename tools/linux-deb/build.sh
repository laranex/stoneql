#!/bin/sh
# Builds the .deb from a clean checkout of one commit. Run in the Dockerfile's image with:
#   /src/repo.bundle  git bundle holding the commit (read-only)
#   /out              where StoneQL_<version>_<sha>_amd64.deb is written
#   /cache            persistent build cache (cargo registry, npm cache, target directory)
# Usage: stoneql-deb-build <full commit SHA> [owner uid:gid for the output]
set -eu
sha="$1"
export CARGO_HOME=/cache/cargo-home CARGO_TARGET_DIR=/cache/target npm_config_cache=/cache/npm
mkdir -p "$CARGO_HOME"
rm -rf /work && git clone --quiet --no-checkout /src/repo.bundle /work && cd /work
git -c advice.detachedHead=false checkout --quiet "$sha"
test "$(git rev-parse HEAD)" = "$sha"
test -z "$(git status --porcelain)"
npm ci --no-audit --no-fund
rm -rf "$CARGO_TARGET_DIR/release/bundle/deb"
npm run desktop:deb
version="$(node -p 'require("./package.json").version')"
deb="$(ls "$CARGO_TARGET_DIR"/release/bundle/deb/StoneQL_"$version"_amd64.deb)"
out="/out/StoneQL_${version}_$(git rev-parse --short=7 HEAD)_amd64.deb"
cp "$deb" "$out"
[ -n "${2:-}" ] && chown "$2" "$out"
sha256sum "$out"
