# Energy RS add-on image (PLAN.md section 10).
#
# One Dockerfile, two build contexts:
#
#   * repository root        docker build -f energy_rs/Dockerfile -t energy-rs:0.1.0 .
#   * staged add-on folder   scripts/build_addon_local.sh (scripts/stage_addon.sh lays the workspace
#                            out around energy_rs/ so the Supervisor can build it from source)
#
# The Supervisor builds an add-on with the folder that holds `config.yaml` as the context, which is
# one level below the Rust workspace. `scripts/stage_addon.sh` therefore publishes a self-contained
# folder (Cargo.toml, Cargo.lock, rust-toolchain.toml, crates/, web/dist/ next to config.yaml).
# The builder copies the whole context and then looks for the entrypoint in both places, so this file
# needs no per-context path list. The cost is the layer cache: any source change recompiles, which is
# what a release build does anyway. Keep this file free of BuildKit-only features: the Supervisor's
# on-device builder may not support them.
#
# `web/dist` is embedded at compile time by rust-embed (crates/energy-server/src/assets.rs), so the
# UI has to be built first: `npm --prefix web ci && npm --prefix web run build`. Release images are
# built from the prebuilt multi-arch GHCR tags; see docs/RELEASE.md.
#
# Build args the Supervisor passes (and .github/workflows/release.yml passes explicitly):
#   BUILD_VERSION  add-on version, also the io.hass.version label
#   BUILD_ARCH     Home Assistant arch name: aarch64 or amd64 (not the BuildKit TARGETARCH name)
ARG BUILD_VERSION=""
ARG BUILD_ARCH=amd64

# slim + build-essential instead of the full rust image: the compiler is all a release build needs,
# and the smaller base keeps the layer cache inside CI runner disks.
FROM rust:1.99.1-slim-bookworm AS builder

WORKDIR /src

# gcc for the vendored C core (crates/energy-opt/csrc, compiled by cc) and for rusqlite's bundled
# SQLite; make/libc headers come with build-essential.
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential \
    && rm -rf /var/lib/apt/lists/*

# The whole context: manifests, `crates/` (with the vendored C core in csrc/ and the bundled SQLite
# of rusqlite), `web/dist/`, and the add-on files. rust-toolchain.toml pins the same toolchain as CI.
COPY . .

RUN cargo build --release --locked --bin energy-server

# The entrypoint is `energy_rs/run.sh` in a repository build and `run.sh` at the root of a staged
# add-on folder (`scripts/stage_addon.sh`). Resolve it here so both contexts share this file.
RUN install -m 0755 "$(if [ -f energy_rs/run.sh ]; then echo energy_rs/run.sh; else echo run.sh; fi)" /entrypoint.sh

FROM debian:bookworm-slim AS runtime

# ca-certificates for Home Assistant, tzdata for Europe/Amsterdam tariff dates.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/energy-server /usr/local/bin/energy-server
COPY --from=builder /entrypoint.sh /run.sh

# /data is the Supervisor's private volume (SQLite plus options.json); run.sh exports it as
# ENERGY_RS_DATA and never writes options.json.
ENV ENERGY_RS_DATA=/data \
    TZ=Europe/Amsterdam

EXPOSE 8099

ARG BUILD_VERSION
ARG BUILD_ARCH
LABEL io.hass.version="${BUILD_VERSION}" \
      io.hass.type="app" \
      io.hass.arch="${BUILD_ARCH}" \
      org.opencontainers.image.source="https://github.com/yazdannl/energy-rs" \
      org.opencontainers.image.licenses="MIT"

# The Supervisor's watchdog checks http://[HOST]:8099/api/v1/health (config.yaml); run.sh execs the
# binary so SIGTERM reaches it directly and the `timeout: 30` shutdown path runs.
ENTRYPOINT ["/run.sh"]
