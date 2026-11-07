#!/usr/bin/env bash
# Stage a self-contained add-on folder (PLAN.md section 10).
#
# The Supervisor builds an add-on with the folder that holds config.yaml as the Docker context,
# which is one level below the Rust workspace. This script lays the workspace out around those
# add-on files so that folder builds on its own:
#
#   build/energy_rs/
#     config.yaml  Dockerfile  run.sh  DOCS.md  icon.png  logo.png  LICENSE
#     Cargo.toml   Cargo.lock  rust-toolchain.toml  clippy.toml
#     crates/      web/dist/
#
# Drop the folder into /addons (or use it as a local repository) and the Supervisor can build it,
# or build the image here with scripts/build_addon_local.sh. The tarball attached to a GitHub
# release is exactly this folder.
#
# Usage: scripts/stage_addon.sh [output-dir]
#
# Preferences: scripts/build_addon_local.sh builds the UI when needed; this script only refuses.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/build/energy_rs}"

fail() {
  printf 'stage_addon: %s\n' "$*" >&2
  exit 1
}

[ -f "$ROOT/web/dist/index.html" ] ||
  fail "web/dist/index.html is missing; run: npm --prefix web ci && npm --prefix web run build"

rm -rf "$OUT"
mkdir -p "$OUT"

# Add-on files. README.md ships as DOCS.md: the Supervisor renders that file in the store's
# Documentation tab and it is the same text users need after installing.
cp "$ROOT/energy_rs/Dockerfile" "$ROOT/energy_rs/run.sh" "$OUT/"
cp "$ROOT/energy_rs/icon.png" "$ROOT/energy_rs/logo.png" "$OUT/"
cp "$ROOT/energy_rs/README.md" "$OUT/DOCS.md"
chmod 0755 "$OUT/run.sh"

# Drop `image:`: a staged folder exists to be built on the device, and the Supervisor pulls a
# published tag instead of building whenever that key is present. The image path stays documented
# in energy_rs/config.yaml (repository copy) and docs/RELEASE.md.
grep -v '^image:' "$ROOT/energy_rs/config.yaml" >"$OUT/config.yaml"
if grep -q '^image:' "$OUT/config.yaml"; then
  fail "could not strip the image key from config.yaml"
fi

# Workspace files the build needs. `target/` is build output and would only make
# the context huge; `csrc/` and build.rs files must stay (the C core is compiled, not fetched).
cp "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" "$ROOT/rust-toolchain.toml" "$ROOT/clippy.toml" "$OUT/"
cp "$ROOT/LICENSE" "$OUT/LICENSE"
tar -C "$ROOT" --exclude=target --exclude=__pycache__ -cf - crates | tar -C "$OUT" -xf -
mkdir -p "$OUT/web"
cp -a "$ROOT/web/dist" "$OUT/web/dist"

# A build needs all of these; the folder must stay complete on its own.
missing=()
for path in config.yaml Dockerfile run.sh Cargo.toml Cargo.lock rust-toolchain.toml crates web/dist; do
  [ -e "$OUT/$path" ] || missing+=("$path")
done
[ "${#missing[@]}" -eq 0 ] || fail "staged context is missing: ${missing[*]}"

version="$(awk -F'"' '/^version:/ { print $2; exit }' "$OUT/config.yaml")"
printf 'stage_addon: staged Energy RS %s at %s\n' "$version" "$OUT"
