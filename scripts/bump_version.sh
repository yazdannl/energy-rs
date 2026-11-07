#!/usr/bin/env bash
# Keep the release version in lockstep (PLAN.md section 10).
#
# One version is stamped in four places and they must never drift, because `cargo build --locked`
# fails on a stale Cargo.lock and the Supervisor pulls the add-on image tag `version`:
#
#   Cargo.toml           [workspace.package] version
#   Cargo.lock           version of every internal crate (path packages)
#   energy_rs/config.yaml  version (manifest, add-on store and image tag)
#   the git tag          v<version> (checked by .github/workflows/release.yml)
#
# Usage:
#   scripts/bump_version.sh 0.2.0     # rewrite all three files, then verify
#   scripts/bump_version.sh --check   # CI gate: exit 1 when anything disagrees
#   scripts/bump_version.sh --show    # print the workspace version

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/Cargo.toml"
CARGO_LOCK="$ROOT/Cargo.lock"
MANIFEST="$ROOT/energy_rs/config.yaml"
INTERNAL_CRATES="energy-battery energy-core energy-forecast energy-ha energy-ledger energy-opt energy-price energy-server"

usage() {
  sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

workspace_version() {
  awk '
    /^\[workspace\.package\]/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^version = "/ { version = $0; sub(/^version = "/, "", version); sub(/"$/, "", version); print version; exit }
  ' "$CARGO_TOML"
}

manifest_version() {
  awk -F'"' '/^version:/ { print $2; exit }' "$MANIFEST"
}

# One line per internal crate in Cargo.lock whose recorded version differs from the argument.
lock_mismatches() {
  awk -v want="$1" -v names="$INTERNAL_CRATES" '
    BEGIN {
      count = split(names, list, " ")
      for (i = 1; i <= count; i++) wanted[list[i]] = 1
    }
    /^name = "/ {
      name = $0
      sub(/^name = "/, "", name)
      sub(/"$/, "", name)
      next
    }
    /^version = "/ && (name in wanted) {
      version = $0
      sub(/^version = "/, "", version)
      sub(/"$/, "", version)
      if (version != want) print name " " version
    }
  ' "$CARGO_LOCK"
}

check() {
  local version problems=0
  version="$(workspace_version)"
  [ -n "$version" ] || {
    printf 'bump_version: no [workspace.package] version in Cargo.toml\n' >&2
    return 1
  }
  printf 'Cargo.toml            %s\n' "$version"

  local manifest
  manifest="$(manifest_version)"
  printf 'energy_rs/config.yaml %s\n' "$manifest"
  if [ "$manifest" != "$version" ]; then
    printf 'bump_version: config.yaml says %s, workspace says %s\n' "$manifest" "$version" >&2
    problems=1
  fi

  local mismatches
  mismatches="$(lock_mismatches "$version")"
  if [ -n "$mismatches" ]; then
    printf 'Cargo.lock is stale (packages not at %s):\n%s\n' "$version" "$mismatches" >&2
    problems=1
  else
    printf 'Cargo.lock            %s (all internal crates)\n' "$version"
  fi

  # `cargo build --locked` in the Dockerfile refuses a lock file that does not match the
  # manifests, so prove that here too rather than at image build time. --offline: no network.
  if ! cargo metadata --locked --no-deps --offline --format-version 1 >/dev/null; then
    printf 'bump_version: cargo metadata --locked failed; run `cargo update --workspace`\n' >&2
    problems=1
  fi

  [ "$problems" -eq 0 ] || return 1
  printf 'bump_version: %s is in lockstep\n' "$version"
}

bump() {
  local version="$1" tmp
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
    printf 'bump_version: %s is not MAJOR.MINOR.PATCH\n' "$version" >&2
    return 1
  }

  tmp="$(mktemp)"
  awk -v want="$version" '
    /^\[workspace\.package\]/ { in_package = 1; print; next }
    /^\[/ { in_package = 0 }
    in_package && /^version = "/ { print "version = \"" want "\""; next }
    { print }
  ' "$CARGO_TOML" >"$tmp"
  mv "$tmp" "$CARGO_TOML"

  # Cargo.lock records the version of every path package; patch it in place so bumping the version
  # never needs a network round trip.
  tmp="$(mktemp)"
  awk -v want="$version" -v names="$INTERNAL_CRATES" '
    BEGIN {
      count = split(names, list, " ")
      for (i = 1; i <= count; i++) wanted[list[i]] = 1
    }
    /^name = "/ {
      name = $0
      sub(/^name = "/, "", name)
      sub(/"$/, "", name)
      print
      next
    }
    /^version = "/ && (name in wanted) { print "version = \"" want "\""; next }
    { print }
  ' "$CARGO_LOCK" >"$tmp"
  mv "$tmp" "$CARGO_LOCK"

  tmp="$(mktemp)"
  awk -v want="$version" '
    !done && /^version:/ { print "version: \"" want "\""; done = 1; next }
    { print }
  ' "$MANIFEST" >"$tmp"
  mv "$tmp" "$MANIFEST"

  check
}

case "${1:-}" in
  --check) check ;;
  --show) workspace_version ;;
  "" | -h | --help)
    usage
    exit 2
    ;;
  *) bump "$1" ;;
esac
