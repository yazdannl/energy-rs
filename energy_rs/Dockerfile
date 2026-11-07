# syntax=docker/dockerfile:1
#
# Build context is the repository root (the Rust workspace and web/ live there):
#
#   docker build -f energy_rs/Dockerfile -t energy-rs:0.1.0 .
#
# Home Assistant's Supervisor builds an add-on with the add-on folder as the context, so the
# published add-on either stages this workspace into that folder (release job, P10) or ships the
# prebuilt multi-arch image referenced by `image:` in config.yaml. See energy_rs/README.md.
#
# `web/dist` is embedded at compile time by rust-embed. The checked-in placeholder keeps this file
# buildable; P9 adds a node stage (or a CI prestep) that builds the real UI here.

ARG BUILD_VERSION="0.1.0"
ARG BUILD_ARCH=amd64

FROM rust:1-bookworm AS builder

WORKDIR /src

# Dependency layer first so source edits do not re-resolve the whole graph.
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY web/dist ./web/dist

RUN cargo build --release --locked --bin energy-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/energy-server /usr/local/bin/energy-server
COPY energy_rs/run.sh /run.sh
RUN chmod +x /run.sh

ENV ENERGY_RS_LOG=info \
    TZ=Europe/Amsterdam

EXPOSE 8099

ARG BUILD_VERSION
ARG BUILD_ARCH
LABEL io.hass.version="${BUILD_VERSION}" \
      io.hass.type="app" \
      io.hass.arch="${BUILD_ARCH}"

ENTRYPOINT ["/run.sh"]
