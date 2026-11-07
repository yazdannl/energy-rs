#!/usr/bin/env bash
# Build the add-on image from this repository, the way a local/offline install does (PLAN.md
# section 10): stage a self-contained add-on folder, then build it with that folder as the context.
#
# Usage: scripts/build_addon_local.sh [extra docker build args...]
#
# Overrides: IMAGE=... (default energy-rs-local:<version>), STAGE=...
#
# The image is never pushed here. Publishing is the tag-driven job in
# .github/workflows/release.yml; see docs/RELEASE.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(awk -F'"' '/^version:/ { print $2; exit }' "$ROOT/energy_rs/config.yaml")"
IMAGE="${IMAGE:-energy-rs-local:${VERSION}}"

case "$(uname -m)" in
  x86_64 | amd64) BUILD_ARCH=amd64 ;;
  aarch64 | arm64) BUILD_ARCH=aarch64 ;;
  *)
    printf 'build_addon_local: unsupported machine %s (the add-on ships aarch64 and amd64)\n' \
      "$(uname -m)" >&2
    exit 1
    ;;
esac

if ! docker info >/dev/null 2>&1; then
  printf 'build_addon_local: no Docker daemon. On a machine with Docker:\n' >&2
  printf '  scripts/stage_addon.sh build/energy_rs\n' >&2
  printf '  docker build -f build/energy_rs/Dockerfile -t %s build/energy_rs\n' "$IMAGE" >&2
  exit 2
fi

# rust-embed compiles web/dist into the binary; the checked-in placeholder would ship a "not built
# yet" page, so build the real UI once when it is missing.
if [ ! -f "$ROOT/web/dist/index.html" ]; then
  printf 'build_addon_local: building the UI first (web/dist is embedded)\n'
  npm --prefix "$ROOT/web" ci
  npm --prefix "$ROOT/web" run build
fi

STAGE="${STAGE:-$ROOT/build/energy_rs}"
bash "$ROOT/scripts/stage_addon.sh" "$STAGE" >/dev/null

printf 'build_addon_local: building %s for %s from %s\n' "$IMAGE" "$BUILD_ARCH" "$STAGE"
docker build \
  --build-arg "BUILD_VERSION=$VERSION" \
  --build-arg "BUILD_ARCH=$BUILD_ARCH" \
  -f "$STAGE/Dockerfile" \
  -t "$IMAGE" \
  "$@" \
  "$STAGE"

printf 'build_addon_local: built %s (%s)\n' "$IMAGE" "$VERSION"
printf 'Smoke test: docker run --rm -p 8099:8099 %s && curl -fsS localhost:8099/api/v1/health\n' "$IMAGE"
