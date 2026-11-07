# Status

Phase gates from [PLAN.md](PLAN.md) §11. P0 is done; everything from P1 on is open.

| # | Phase | Gate | State |
|---|---|---|---|
| P0 | Repo scaffold, workspace, CI, add-on skeleton, docs | `cargo build` + `cargo test` green, add-on builds for aarch64 | **done** |
| P1 | `energy-opt` + C core + validation + Rust oracle | differential tests, `-ffp-contract=off`, bench < 50 ms/solve | todo |
| P2 | `energy-ha` discovery/stats/subscriptions | recorded-fixture tests, live read-only smoke | todo |
| P3 | `energy-price` + settlement + store | formula unit tests vs researched numbers, toggle tests | todo |
| P4 | `energy-forecast` | 800/800 exact golden vectors, load-profile tests | todo |
| P5 | `energy-battery` zenSDK client + fake device | protocol tests against an in-process fake, pacing/echo tests | todo |
| P6 | `energy-ledger` + learning + protection | schema/migration tests, learner golden sessions | todo |
| P7 | `energy-core` engine + safety + dispatch + dry-run | ported trap tests, shadow-mode test | todo |
| P8 | `energy-server` API + savings | contract tests, SSE smoke | todo |
| P9 | `web/` UI | build + render smoke, no console errors, ingress paths | todo |
| P10 | Packaging + install side-by-side + shadow run | live advisory/dry-run verified, go-ahead pending | todo |

## P0 notes

Done in the scaffold commit:

- Cargo workspace (edition 2024, resolver 3, rust-version 1.99, `rust-toolchain.toml` 1.99.1) with
  eight crates; every crate has `//!` docs stating its responsibility and stub modules per PLAN.md 2.
- `energy-server` really serves: `GET /api/v1/health` plus the `rust-embed`ed `web/dist` with an
  `index.html` fallback, and a binary that binds `0.0.0.0:8099` and handles SIGTERM.
- `web/` Svelte 5 + TS + Vite + Tailwind 4 shell with a lockfile and a working `npm run build`.
- `energy_rs/` add-on: manifest, multi-stage Dockerfile, `run.sh`, generated icon/logo, README.
  CI runs fmt, clippy, tests, the UI build and a release build; it does not build the image.
- Docs: `PLAN.md` (copy of the design), this file, root README, MIT licence.

Deliberately **not** done yet:

- `_core.c` is not vendored; `crates/energy-opt/build.rs` is a documented no-op (**P1**).
- `web/dist` ships the checked-in placeholder until the UI is real; the add-on image therefore embeds
  the placeholder (**P9**).
- No `/data/options.json` parsing, no `SUPERVISOR_TOKEN` handling, no database: the binary only reads
  `INGRESS_PORT`, `ENERGY_RS_LOG` and `RUST_LOG` (**P6/P8/P10**).
- The Dockerfile builds from the repository root, which is not the context the Supervisor uses for
  an add-on folder; P10 publishes a prebuilt image instead.
- `cargo test --workspace` currently has one contract test (the health payload); all other crates are
  documentation-only stubs. Tests arrive with their phase.
- `lucide-svelte@1.0.1` is deprecated upstream (npm points at `@lucide/svelte`); it is installed but
  unused as specified for P0. Pick one when the UI actually uses icons (P9).
