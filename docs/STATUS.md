# Status

Phase gates from [PLAN.md](PLAN.md) §11. P0–P3, P0.5, P5, the P6 ledger store and the P10 packaging
are done; P4, P6 learning/protection, P7–P9 and the live P10 rollout are open.

| # | Phase | Gate | State |
|---|---|---|---|
| P0 | Repo scaffold, workspace, CI, add-on skeleton, docs | `cargo build` + `cargo test` green, add-on builds for aarch64 | **done** |
| P0.5 | Quality gates: lint policy, structure checker, web linting | fmt, clippy `-D warnings`, structure, test, build, web lint/check/build all green | **done** |
| P1 | `energy-opt` + C core + validation + Rust oracle | differential tests, `-ffp-contract=off`, bench < 50 ms/solve | **done** |
| P2 | `energy-ha` discovery/stats/subscriptions | recorded-fixture tests, live read-only smoke | **done** |
| P3 | `energy-price` + settlement + store | formula unit tests vs researched numbers, toggle tests | **done** |
| P4 | `energy-forecast` | 800/800 exact golden vectors, load-profile tests | todo |
| P5 | `energy-battery` zenSDK client + fake device | protocol tests against an in-process fake, pacing/echo tests | **done** |
| P6 | `energy-ledger` + learning + protection | schema/migration tests, learner golden sessions | **partial**: ledger store done |
| P7 | `energy-core` engine + safety + dispatch + dry-run | ported trap tests, shadow-mode test | todo |
| P8 | `energy-server` API + savings | contract tests, SSE smoke | **partial**: API/SSE contract done; savings math is the engine's (P8b) |
| P9 | `web/` UI | build + render smoke, no console errors, ingress paths | todo |
| P10 | Packaging + install side-by-side + shadow run | live advisory/dry-run verified, go-ahead pending | **partial**: packaging, image and release pipeline done; nothing published, installed or shadowed yet |

## P8 notes (API)

Done in `feat/api` (nothing pushed), contract in [API.md](API.md):

- **Contract location.** The engine/server handshake is `energy-core::stream`: `EngineSnapshot`
  (mode, site, plan, packs with learned profile + latch + override, meter, forecast, last tick,
  faults), the `SnapshotPublisher` trait with a tokio `watch` implementation, and `EngineCommand`
  (`SetMode`, `Override`, `FullCharge`, `Relearn`). The server re-exports them; P8b's loop only has
  to publish a snapshot per tick and drain the command queue. The mode type is the engine's
  `Authority` (serde slugs on it) and `PATCH /mode` persists through `energy_core::mode::save`
  (`control.mode`), so the engine and the API cannot disagree about the stored mode. Overrides use
  the engine's `dispatch::OverrideAction`, so `charge`/`hold`/`off` has one spelling.
- **Routes.** `health`, `site`, `packs`, `prices`, `plan/current`, `plans`, `dispatch`, `savings`,
  `settings` GET/PATCH, `mode` PATCH, `override`/`protection`/`learning` POST, `stream` SSE. Units
  are in every field name (W, Wh, kWh, EUR/kWh); money rounds to the ledger's 1e-6; timestamps are
  RFC3339 UTC. One error envelope with twelve stable codes and 400/404/409/422/503/500 mapping;
  the four body commands are validated by `ApiJson` so even extractor failures use the envelope.
- **Settings.** An eleven-key whitelist with typed validation (choice/number/text); `mode` is
  deliberately not writable here. `PATCH` is timestamped and appends ledger history; a timestamp
  predating a stored change is `409 stale_write`, so history is never rewritten. Private keys
  (e.g. `zonneplan_refresh_token`) are invisible, and a redaction test asserts no token, credential
  name or database path appears in any response.
- **SSE.** `state` on connect and after every change, `plan` when the plan id changes, `fault` per
  new fault, `heartbeat` every 15 s (configurable for tests). Backpressure is the watch channel:
  slow readers get the newest snapshot, never a backlog.
- **Static UI.** rust-embed wiring kept; only `/`, `/plan`, `/prices`, `/batteries`, `/savings`,
  `/settings` fall back to `index.html`, a missing asset is a 404. `main.rs` now opens the ledger
  and price store from `ENERGY_RS_DATA` and drops the command queue, so write commands answer
  `503 engine_unavailable` until P8b lands.
- **Tests.** 32 in the crate: 9 read-contract, 9 command/validation, 7 settings, 4 envelope/redaction,
  3 SSE (first event, heartbeat, plan event), plus the core snapshot unit tests. `scripts/api_smoke.sh`
  passes against a locally started binary (all routes 200, plan 404 before the first plan, SSE state
  event, redaction).

## P10 notes (packaging)

Done in `chore/packaging`; install and release details in [RELEASE.md](RELEASE.md) and
`energy_rs/README.md`. Nothing was pushed, published or installed.

- **Build context.** One Dockerfile serves `docker build -f energy_rs/Dockerfile .` from the
  repository root *and* a staged add-on folder (what the Supervisor uses as context). Both layouts
  put `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/` and `web/dist/` at the context
  root; the builder resolves the one difference (`energy_rs/run.sh` vs `run.sh`) with a single
  `install`. No BuildKit-only features, so the Supervisor's legacy builder can build it too.
- **Staging.** `scripts/stage_addon.sh` writes `build/energy_rs/`: manifest (with `image:` stripped so
  a local build is actually built), Dockerfile, `run.sh`, store art, `README.md` as `DOCS.md`, the
  workspace manifests, `crates/` and `web/dist/` — 5.4 MB, the whole Supervisor context. It fails
  loudly when a path the Dockerfile copies is missing; `ci.yml`'s `packaging` job stages the folder
  on every push and re-checks the manifest contract (image placeholder, ingress 8099, watchdog path,
  `map: data:rw`, arch list, mode enum).
- **Manifest.** `timeout: 30`, `ingress_entry: /`, `panel_admin`, `watchdog` on
  `http://[HOST]:[PORT:8099]/api/v1/health` (the endpoint never calls HA, so a Core restart cannot
  restart the add-on), `backup: cold` for the SQLite WAL, `image: ghcr.io/yazdannl/energy-rs-{arch}`
  (delete the line to let the Supervisor build on the device). Options: `mode`, `log_level`,
  `price_provider`, `price_entity?`, `zonneplan_refresh_token?` (`password?`, documented plain-text
  trade-off), `batteries` (JSON string, manual LAN `address`/`serial` entries).
- **Entrypoint.** `run.sh` reads `log_level` from `/data/options.json` without jq, exports `LOG_LEVEL`
  and `ENERGY_RS_DATA=/data`, then `exec`s the binary; `main.rs` prefers `LOG_LEVEL` over
  `ENERGY_RS_LOG`/`RUST_LOG` and logs the data folder.
- **Image.** `rust:1.99.1-slim-bookworm` + `build-essential` builder → `debian:bookworm-slim` runtime
  (ca-certificates, tzdata), `io.hass.{version,arch,type}` labels, `EXPOSE 8099`, `ENTRYPOINT
  ["/run.sh"]`, bin only (`cargo build --release --locked --bin energy-server`, so the release
  profile's `strip`/thin LTO apply).
- **Release pipeline.** `release.yml` on a `v*` tag: version lockstep + tag match → gates → native
  matrix images (`ubuntu-24.04`/amd64, `ubuntu-24.04-arm`/aarch64) pushed to
  `ghcr.io/yazdannl/energy-rs-{amd64,aarch64}:<version>` → staged tarball
  `energy-rs-addon-<version>.tar.gz` attached to a GitHub release. A manual dispatch builds both
  images without pushing (default) and smoke-tests `/api/v1/health`.
- **Versioning.** `scripts/bump_version.sh <x.y.z>` rewrites `[workspace.package] version`,
  the eight internal `Cargo.lock` entries (offline, no re-resolve) and `energy_rs/config.yaml`, then
  verifies with `cargo metadata --locked --offline`; `--check` is a CI job and a release precondition,
  `--show` feeds the workflows. `cargo build --release --locked` in the image and in CI's `build` job
  now refuses a stale lock.
- **Local validation (this sandbox: aarch64, 6.4 GB Docker volume, no route for build containers).**
  `docker build -f energy_rs/Dockerfile .` (repository-root context) produced a 110 MB image whose
  `GET /api/v1/health` answers `{"status":"ok","version":"0.1.0"}` and whose labels are
  `io.hass.version=0.1.0`, `io.hass.arch=aarch64`, `io.hass.type=app`;
  `scripts/build_addon_local.sh` (staged Supervisor context) produced the same image and answer. The
  final `run.sh` mounted into that image reads `log_level` from `/data/options.json`: `warn`
  suppresses the startup INFO line, `debug` shows it, so the option really reaches the binary. Both
  builds needed `--network host` here: the daemon pulls images fine but gives build containers no
  default route, so `apt-get` and crates.io are unreachable without it (GitHub runners do not need
  it). ~6 min per build, almost all of it the Rust release compile. Gates were run in this worktree:
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`,
  `python3 scripts/check_structure.py`, `npm ci && npm run build` — all green.

## P6 notes (store)

Done in `feat/ledger` (no learning yet, nothing pushed), details in [LEDGER.md](LEDGER.md):

- **Schema and migrations.** SQLite at `/data/energy_rs.db` (configurable), WAL, foreign keys,
  5 s busy timeout, `synchronous=NORMAL`; in-code forward-only numbered migrations recorded in
  `meta.schema_version`, refused when the file is newer than the binary. Tables owned here:
  packs, measurements, measurements_5m, sessions, plans/plan_slots, dispatch_log, events,
  savings_daily, meta, settings/settings_history. Price tables stay with `energy-price`.
- **Ingestion and rollup.** 1-minute sample upsert on `(pack_id, ts)`, batch writer; 5-minute
  rollup writes per-pack bucket means and explicit empty buckets, and a conflict guard stops a
  recomputation after retention from erasing populated buckets.
- **Sessions.** Deterministic, re-runnable window extraction from either tier: deadband,
  minimum samples, gap splitting, measured coverage (no full interval on a partial edge), SoC
  gaps and one-sample reversals flagged, complete only when bounded both sides.
- **Retention.** 1-minute 30 d, 5-minute 1 y, plans 200, dispatch 20 000, sessions/events/daily
  forever, current local day never trimmed; per-table deleted counts; idempotent.
- **Queries.** Range reads for measurements/sessions/plans/savings, latest pack state, current
  plan, plan+dispatch insert helpers, settings get/set/as-of.
- **Tests.** 38 in the crate (4 migration, 8 ingest/rollup, 9 query/settings, 7 retention,
  8 sessions, 2 WAL concurrency) on top of the workspace suites.

## P5 notes

Done in `feat/battery` (no hardware touched, nothing pushed):

- **Transport.** Plain HTTP `:80` per device (no TLS, no proxy, 5 s timeout), `GET /properties/report`
  and `POST /properties/write` with `{sn,id,source:2,properties}`, a 512-byte body cap enforced
  before sending, and typed errors for transport/HTTP/`code!=200`/401 (serial mismatch)/400/413.
- **Report.** Both envelopes (packs top-level or under `properties`), unknown keys ignored, all §3.1
  unit conversions as pure functions, `gridReverse`/`socLimit`/`oldMode` decoded, no kWh fabricated,
  pack capacity labels from the serial prefix + `packType` (AB2000 = 1.92 kWh).
- **Control.** Charge/discharge/hold/trim payloads, clamping to `min(config, chargeMaxLimit /
  inverseMaxPower)`, ≥2 s pacing with a single in-flight claim, echo confirmation within 5 W on the
  next report, `smartMode: 1` on every control write, a hold that never writes `acMode`, wedge
  (`acMode ∉ {1,2}`) and restart (`messageId`/clock going back) faults that refuse writes until a
  fresh report says otherwise, with a 10 s back-off pushed into the pacer.
- **Discovery / fleet.** mDNS `_zendure._tcp` browse plus manual IP + SN, merged and deduped by
  serial; staggered polling (default 5 s per device), a freshness-stamped cache, capacity-weighted
  SoC, summed AC power, fan-out with per-device sequencing and partial-failure reporting.
- **Fake device.** In-process axum server on `127.0.0.1:0` with echo, dropped writes <1.3 s, wedge
  mode and 401/400/413 injection; 72 tests, one `#[ignore]`d read-only live test.

## P0.5 notes

Done in `chore/quality-gates`, policy documented in [LINTING.md](LINTING.md):

- **Workspace lints.** Clippy `all` + `pedantic` at deny, `unwrap_used`/`expect_used`/`panic`/
  `todo` denied, size limits (`too_many_arguments`, `too_many_lines`, `cognitive_complexity`,
  `large_enum_variant`) denied with thresholds in `clippy.toml` (6 params, 60 lines, 15). The
  eight deliberate allow reasons are in LINTING.md; every crate opts in with
  `[lints] workspace = true`. `rustfmt.toml` sets `max_width = 100`.
- **Structure checker.** `scripts/check_structure.py` (stdlib only) enforces one primary public
  type per file, 300/500 line files, 60-line functions, 6 parameters, no
  `utils/helpers/misc/common` modules, 250-line Svelte components named after the component, and
  prints a file/line/rule table. It is a CI job of its own.
- **Web.** ESLint 9 flat config (`@eslint/js` + `typescript-eslint` + `eslint-plugin-svelte`,
  `no-explicit-any` error), Prettier with `prettier-plugin-svelte`, `svelte-check`; `npm run
  lint|format:check|check|build` are the four web gates.
- **Retrofit.** `energy-opt` was split by aspect: `error/` (one enum per file), `config/`,
  `dp/`, `ffi/`, `validate/` (arithmetic, windows, grid, one-step, verdict, path, backward) and
  `oracle/` (grid, sweep, replay). `tests/common/` became `tests/support/` (split reply/server),
  and the long parity/oracle tests were split into per-case helpers. Behaviour is unchanged: the
  full parity, oracle and build-digest suites still pass, and `scripts/extract_core.py --check`
  still proves `csrc/core_solver.c` is the pinned upstream text.
- **CI.** Eight jobs: `fmt`, `clippy`, `structure`, `test`, `web-lint`, `web-check`, `web-build`,
  `build` (release binary after the UI build so `web/dist` is embedded).

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

## P1 notes

Done in `energy-opt` (`feat/opt-core`):

- **Vendored C core.** `crates/energy-opt/csrc/core_solver.c` is generated by
  `scripts/extract_core.py` from `ha-energy-manager`'s `_core.c` (SHA-256 `019c1d04…25894`), lines
  `43-50, 52-98, 100-535, 660-708` — the `Config` struct, every helper, `run_solve` and
  `validate_slots`, byte for byte. `--check` proves the checked-in file is current; the script
  refuses any other upstream digest. See `csrc/UPSTREAM.md`.
- **No Python.** `csrc/energy_opt_ffi.c` is a plain C ABI (flat arrays + one parameter block →
  `eo_result`, `eo_free`); `csrc/core_adapter.c` maps it onto the extracted `static` helpers and
  carries the two `PyErr_*` call sites into a local error buffer instead of editing upstream text.
  `build.rs` compiles with `-O3 -ffp-contract=off -fno-fast-math -DCORE_BUILD_SHA=<sha16>`
  (`cc`), and `verify_core_build()` re-hashes the shipped sources on the first solve, so a stale
  object file cannot run.
- **Rust API.** `Config` (20 required + 3 optional keys, `pv_charge_preference` as an enum),
  `solve()` / `solve_with_terminal_row()` mirroring the old result dict, typed errors, and
  `validate::check` — a port of `_fast._validated`: finiteness of every number, rebuilt grid,
  bounds, windows, per-slot argmin equality, objective/cycle/terminal re-derivation and a sampled
  backward Bellman pass. Any mismatch is a refusal.
- **Oracle + parity.** `oracle` is an independent naive Rust DP. 20 deterministic scenarios in
  `tests/fixtures/parity.json` (generated by `scripts/gen_parity_fixtures.py` from the unchanged
  `dp._solve_once_numeric`) match the C core at 1e-9 on the objective, the path, the grid and the
  whole value function, with decisions exactly; the oracle agrees with the C core on every
  `slots × states` argmin.
- **Bench.** 120 slots × 398 states × 24 power steps (1.19M actions) through `solve()`: **18.8 ms**
  median in a debug build, **17.7 ms** in release (gate: < 50 ms); the time is the C sweep, not the
  validation.

Open after P1: the engine's periodic full differential (`PLAN.md` §6) still has to call
`oracle::solve`; the C core does not yet feed `energy-core` (P7).

## P3 notes

`energy-price` is the money authority; every formula is `reports/energy-rs/prices.md` restated once.

- `book`: `Slot` (`t_start`, `buy_all_in` as published, `market` derived at ingest), `PriceBook`
  refuses duplicates, holes, off-grid starts and mixed 15-minute/hourly ranges; providers may differ
  inside one book (a fallback switch mid-day is normal).  Nothing is ever zero-filled.
- `provider`: `ZonneplanDirect` (refresh-token OAuth2, `electricity-quarter-hourly` with the hourly
  chart as fallback, raw amounts `/1e7` and gas-decoy rejection by grid and by unit) and
  `HaSensorFallback` (HA REST states + `forecast` attribute, both entry shapes).  Credentials are
  redacted from `Debug` and never appear in errors.  Both require a slot covering *now* only: before
  ~13:00 CET tomorrow is unpublished and the caller shortens its horizon.
- `settlement`: `market = (buy - fee_incl)/(1+vat) - energy_tax_ex`, `feed_in_post = market + fee`
  with the Zonnebonus guards (PV, daylight, positive, 7 500 kWh/yr, per calendar year), yearly tax
  table (an unknown year is refused), and the four functions the rest of the system may use:
  `buy`, `export_price`, `marginal_export` (`buy - feed_in_post` while netting lasts, ~0.12), and
  `revenue`.  Net metering is date-driven (`local year < 2027`) with a manual override; export at a
  negative price is refused (clamped to 0), negative import prices pay you.
- `store`: `PriceRepository` with `MemoryPriceRepository` and `SqlitePriceRepository` (WAL, in-code
  migrations).  Publications are append-only, keyed by content hash; a save is idempotent, and a
  later poll that adds no coverage and no changed slot is a no-op.  `load_book` refuses a hole.
- Tests (42): worked examples from prices.md (0.20 -> market 0.057150 -> feed-in 0.07715, the
  negative-price thresholds, marginal export), the toggle under the 2026/2027 boundary and a manual
  override, budget exhaustion, bonus guards and year rollover, both payload shapes and the gas
  decoy, past-slot filtering, coverage staleness, gap refusal and save-twice idempotency in both
  repositories.
- Verified read-only against the live instance: the quarter-hourly sensor publishes `€/kWh` plus a
  144-entry `forecast` of `price_tax_included.amount` fixed-point integers (15-minute grid, no
  `tariff_group`), the legacy hourly sensor a 36-entry `forecast`; the gas decoy is one entry in
  `€/m³` and is rejected.
- Not covered by tests (needs network): the OAuth token exchange and the two HTTP fetch paths; the
  recorded-payload parsers behind them are tested.
