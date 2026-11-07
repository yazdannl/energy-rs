# energy-rs

A ground-up, deliberately small Rust rewrite of the Home Assistant energy manager: it reads the
Energy dashboard, forecasts PV and load in-process, optimizes a 30-hour battery plan with the proven
C DP solver, and drives the SolarFlow 800 Plus packs over the local zenSDK HTTP API — with the
forecasts, prices, settlement, ledger, savings and UI all rebuilt around that core.

See [docs/PLAN.md](docs/PLAN.md) for the design and [docs/STATUS.md](docs/STATUS.md) for the phase
gates; the system being replaced is [yazdannl/ha-energy-manager](https://github.com/yazdannl/ha-energy-manager).

## Status

P0 only: workspace, crates, UI shell and add-on packaging. Everything builds; no business logic yet.

## Layout

```
crates/energy-opt       C DP core + validation + brute-force oracle   (P1)
crates/energy-ha        HA REST/WS, dashboard discovery, statistics   (P2)
crates/energy-price     Zonneplan provider, store, settlement         (P3)
crates/energy-forecast  PV GBT + physics + EWMA, load profile         (P4)
crates/energy-battery   zenSDK client, discovery, learning, protection (P5)
crates/energy-ledger    SQLite schema, sessions, savings              (P6)
crates/energy-core      engine loop, safety ladder, dispatcher        (P7)
crates/energy-server    axum API + embedded UI + add-on entrypoint    (P8)
web/                    Svelte 5 + Vite + Tailwind 4 UI               (P9/P10)
energy_rs/              Home Assistant add-on (config.yaml, Dockerfile)
```

## Install (Home Assistant)

Add `https://github.com/yazdannl/energy-rs` as an add-on repository, install **Energy RS** and start
it in `dry_run` first: it plans and logs intended setpoints without touching the batteries. The
default `dry_run` mode is deliberate — `auto` is a separate, explicit switch and until then the old
add-on keeps controlling the hardware.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace

cd web
NODE_OPTIONS=--max-old-space-size=4096 npm install
NODE_OPTIONS=--max-old-space-size=4096 npm run build

cd ..
cargo build --release      # embeds web/dist into the binary
```

The server serves `GET /api/v1/health` and the embedded UI on `:8099`
(`ENERGY_RS_LOG=debug cargo run -p energy-server`). The add-on image is built from the repository
root: `docker build -f energy_rs/Dockerfile -t energy-rs:0.1.0 .`

## License

MIT — see [LICENSE](LICENSE).
