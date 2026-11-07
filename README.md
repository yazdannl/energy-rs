# energy-rs

A ground-up, deliberately small Rust rewrite of the Home Assistant energy manager: it reads the
Energy dashboard, forecasts PV and load in-process, optimizes a 30-hour battery plan with the proven
C DP solver, and drives the SolarFlow 800 Plus packs over the local zenSDK HTTP API — with the
forecasts, prices, settlement, ledger, savings and UI all rebuilt around that core.

See [docs/PLAN.md](docs/PLAN.md) for the design, [docs/STATUS.md](docs/STATUS.md) for the phase
gates, [docs/RUNBOOK.md](docs/RUNBOOK.md) for installing and operating the add-on and
[docs/DOCTOR.md](docs/DOCTOR.md) for the read-only `doctor`/`plan-once` diagnostics; the system
being replaced is [yazdannl/ha-energy-manager](https://github.com/yazdannl/ha-energy-manager).

## Status

P0 and P1 are done: workspace, crates, UI shell, add-on packaging, and the DP optimizer core
(`energy-opt`) — the old add-on's C solver vendored verbatim behind a plain C ABI, a validating Rust
API, an independent brute-force oracle and 20 parity fixtures pinned to the Python reference. The
remaining phases (HA, price, forecast, battery, ledger, engine, server, UI) are still stubs.

## Layout

```
crates/energy-opt       C DP core + validation + brute-force oracle   (done)
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

The manifest pulls a prebuilt per-arch image; building the add-on locally (image or release tarball)
is covered in [energy_rs/README.md](energy_rs/README.md) and [docs/RELEASE.md](docs/RELEASE.md).

### Validate before you switch to `auto`

The same binary carries two read-only diagnostics. They open the ledger and the price store without
writing, read device reports with `GET /properties/report`, never send a write and never change the
mode:

```bash
docker exec addon_energy_rs energy-server doctor      # PASS/WARN/FAIL table, 0/1/2
docker exec addon_energy_rs energy-server plan-once   # one real plan, 0 or 2
```

`doctor` checks the HA transport and token kind, the energy dashboard sources, 5-minute/24 h and
hourly/30 d statistics coverage, the live meter, the price publication/coverage/derivation, every
battery's report (reachable, SN, pack label, limits, SoC, wedge state), the ledger and one real plan,
then prints `next steps` for each failure. `plan-once` prints the 15-minute timeline
(`slot, buy, feed-in, pv, load, battery W, soc, reason`) with the summary and any degraded/refusal
reason; `--json` is for scripting. Both accept `--data-dir`, `--json` and `--verbose` (timings only).
See [docs/DOCTOR.md](docs/DOCTOR.md) for every row and its limits.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace

# The C core is generated and the parity fixtures come from the old add-on:
python3 scripts/extract_core.py --check          # the vendored C matches upstream, byte for byte
python3 scripts/gen_parity_fixtures.py --check   # the fixtures match the Python reference

cd web
NODE_OPTIONS=--max-old-space-size=4096 npm install
NODE_OPTIONS=--max-old-space-size=4096 npm run build

cd ..
cargo build --release      # embeds web/dist into the binary
```

The server serves `GET /api/v1/health` and the embedded UI on `:8099`
(`ENERGY_RS_LOG=debug cargo run -p energy-server`). The add-on image builds from the repository root
with `docker build -f energy_rs/Dockerfile -t energy-rs:0.1.0 .`, or through the staged
Supervisor-context folder with `scripts/build_addon_local.sh`; releases are described in
[docs/RELEASE.md](docs/RELEASE.md).

## License

MIT — see [LICENSE](LICENSE).
