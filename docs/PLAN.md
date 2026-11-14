# energy-rs — plan (v1)

A ground-up, small Rust reimagination of the HA energy manager. Keeps the two things that work
(the C DP planner, the solar/load forecast models) and replaces everything around them with a
much smaller, better-instrumented system.

Research inputs (full detail):
`reports/energy-rs/survey.md` (old add-on keep/drop), `zensdk.md` (battery protocol),
`prices.md` (Zonneplan + saldering formulas), `ha-apis.md` (HA APIs + add-on packaging).

---

## 1. Decisions (from the user)

| | Decision |
|---|---|
| Deployment | New repo `yazdannl/energy-rs`, new add-on slug `energy_rs`, side-by-side with the old add-on |
| Forecasts | Port model inference to Rust (bit-exact GBT + physics + EWMA); no Python at runtime |
| UI | Svelte 5 + Vite + Tailwind, static build embedded in the Rust binary |
| Battery scope | All 4 SolarFlow 800 Plus packs, per-device ledger and learning |
| Prices | Zonneplan direct API first, HA sensor fallback; cached once per publication |
| Battery onboarding | LAN-only: mDNS + manual IP/SN. No cloud token, no cloud calls |
| Before go-ahead | Full dry-run pipeline (clamps, intended writes logged, nothing sent) |
| Net metering OFF | Feed-in = market + fee (i.e. retail without energy tax/VAT shield) |
| Capacity learning | Long session history with heavy recency weighting (last 3 dominate) |
| Protection | Keep cell-voltage guard + full-charge maintenance |
| Savings | New, accurate: per-day + lifetime |
| Horizon | 120 native 15-min slots (30 h), replan every 15 min |
| Ledger retention | 30 d 1-min, 1 y 5-min, daily forever |
| SoC/power | 10–100 % SoC, hardware max power |
| Screens | Now, Plan timeline, Prices, Batteries (+ Savings, Settings) |
| Old add-on | Keeps running as controller until the user says go |

---

## 2. Architecture

Cargo workspace, seven crates + one Svelte app. Everything is synchronous where it can be and
async only at the edges (HA, HTTP, timers).

```
crates/
  energy-opt       C DP core (vendored _core.c via cc, same gcc flags) + validation + Rust oracle
  energy-ha        HA WebSocket/REST client, energy-dashboard discovery, statistics, subscriptions
  energy-price     Zonneplan provider + HA fallback, price store, settlement (net-metering switch)
  energy-forecast  PV GBT (bit-exact port) + physics + EWMA, load profile builder
  energy-battery   zenSDK client (mDNS/manual discovery, paced writes), learning, protection
  energy-ledger    SQLite schema + migrations, sessions, plans, dispatch log, measurements, savings
  energy-core      engine loop, safety clamp ladder, dispatcher, dry-run/shadow
  energy-server    axum API + SSE + embedded UI (rust-embed), add-on entrypoint
web/               Svelte 5 + TS + Vite + Tailwind 4 + lucide-svelte + uPlot
```

Runtime model: one tokio runtime; a single engine task owns control decisions; DB access via one
`tokio-rusqlite` connection actor; HA/zenSDK I/O via timeouts and restartable supervisors.

Storage: SQLite at `/data/energy_rs.db` (WAL). Money in f64 EUR rounded to 1e-6 on write; energy
in Wh (f64); time as UTC epoch seconds, tariff dates in `Europe/Amsterdam`.

Control modes: `advisory` (compute + store plans only) → `dry_run` (full pipeline incl. clamps,
intended writes logged) → `auto` (writes). Default `dry_run`. Mode is a persisted setting.

---

## 3. What we keep, port, and drop

Keep as-is: `_core.c` (byte-identical source, built with `-O3 -ffp-contract=off`), the GBT model
artifact `pv_base.json` + `pv_golden.json`.

Port faithfully (behaviour, not code): safety clamp ladder + reasons, dispatcher setpoint rules,
fail-safe hold/watchdog, plan-SoC tracking, cost-basis sell floor, exact-or-refuse price handling,
stale-input hold, live-write latch, cell-voltage latch.

Drop: replay harness, terminal-value vector, stage-4 candidates, the hand-rolled WS client, the
811-line C verification loader (replaced by Rust validation with the same checks), the 11k-line
vanilla UI.

---

## 4. Price & settlement model (single source of truth)

Per slot: `t_start`, `buy_all_in` (EUR/kWh, as published), `market` (EUR/kWh ex-VAT market).
Contract: `vat 0.21`, `energy_tax_ex`, `fee_incl` (inkoopvergoeding), `feed_in_fee`, `bonus_pct`,
`teruglever_kosten`, `saldering` (Some ⇒ regime on).

```
market(t)        = (buy_all_in(t) - fee_incl) / (1 + vat) - energy_tax_ex
buy(t)           = buy_all_in(t)
export_price(t)  = if saldering && budget_remaining > 0  -> buy_all_in(t)
                   else max(0, (market(t) + feed_in_fee) * bonus_mult(t) - teruglever_kosten)
budget_remaining = max(0, import_ytd_kwh - netted_export_ytd_kwh)
```

* One settlement function `revenue(t, kwh, source)` used by optimizer, dispatch, floor and reporting.
* Net-metering toggle is date-driven (`local_date < 2027-01-01` ⇒ on) with a manual override.
* Saldering ON: battery export consumes PV budget ⇒ marginal export ≈ `buy − feed_in_post` (~0.12).
* Negative prices: block export when `export_price < 0`; grid-charge at max when `buy < 0`.
* Constants are yearly and configurable; no per-slot tax scraping. Exact-or-refuse on gaps.
* Prices persist once per publication (detect by max end timestamp / forecast hash), never refetched
  per tick; provider is a trait (`ZonneplanDirect { refresh_token }`, `HaSensors` fallback).

---

## 5. Battery: discovery, control, learning, protection

Transport (zenSDK): plain HTTP `:80`, one host per device, **no local auth**.
`GET /properties/report` → state; `POST /properties/write` → `{sn,id,source:2,properties:{…}}`,
charge `{smartMode:1,acMode:1,outputLimit:0,inputLimit:W}`, discharge mirrored, hold both 0.
Success must be confirmed by the next report's limit echo (HTTP 200 + code 200 proves nothing).
Pace ≥2 s between writes per device, one command at a time; `smartMode:1` (RAM) on every control
write; never write `acMode` on a hold. Capacity label from `packData[].packType` (AB2000 = 1.92 kWh).

Discovery: mDNS `_zendure._tcp` (+ hostname `zendure-<model>-<sn>.local`), plus manual IP+SN entry.

Ledger (per pack): charge/discharge **sessions** (contiguous same-direction runs) with start/end SoC,
AC energy in/out, duration, sample count. Learning:
`capacity` and `round_trip` from sessions, recency-weighted (3-session half-life, min 3 sessions,
outlier-trimmed), energy-balance form (solve `dSOC*C = I*x − O/x`), applied inside a ±25 % band
around the label, never above reported. Every estimate keeps a provenance row and is visible in UI.

Protection: latching low-cell guard (block discharge below cutoff, force emergency charge to resume
voltage; stale reading keeps the latch; persisted across restarts) + weekly full-charge maintenance.

---

## 6. Optimizer

C DP core: `solve(pv[], load[], buy[], sell[], minutes[], config[, terminal_row])`. The numerics are
the vendored `_core.c` lines; the one deliberate addition is the `align_actions` candidate pair
(0.9.1): charging exactly the PV surplus and discharging exactly the house's load. A 24-step lattice
cannot express a 0.10 kW surplus without importing the difference, nor a 0.88 kW load without
spilling 0.13 kW to the grid, and both showed up as real exports on live plans. The candidates are
config-gated, so the Python parity rows (and `tests/fixtures/parity.json`) still run the old lattice.

Rust wrapper owns: input validation, feasibility/bounds/path re-derivation, per-slot argmin decision
equality, sampled backward Bellman, and a periodic full differential against a small Rust brute-force
DP. Any mismatch ⇒ refuse the solve and hold (never "use a slightly wrong plan").

Sell prices fed to the DP come from `export_price(t)`; `allow_export` stays false unless the
settlement says export is actually worth something.

---

## 7. Forecaster

* PV: bit-exact port of the shipped GBT (28 features, 400 trees, `threshold_bits`/`value_bits` as
  IEEE-754 u64, `missing_left`, clamp 0.3–1.8) validated against `pv_golden.json` on all 800 vectors
  with **exact** equality, plus physics clear-sky + cloud attenuation and EWMA fallback.
* Load: 48-bucket profile from HA long-term statistics (`change`, not `sum`), night-derived tier,
  named device subtraction (`load.devices`) as in the old app, EWMA online update.
* Sources: HA energy dashboard (grid/solar/load sensors + `energy/solar_forecast`), own measurements.

---

## 8. Savings accounting

Per slot, from measured slot integrals: `actual_cost` (settled import/export) vs `baseline_cost`
(no-battery counterfactual: same PV, same load, battery idle, same prices and budget rules).
`saving = baseline − actual`, split into arbitrage / PV self-consumption / feed-in effect.
Daily rollups + lifetime total, all derived from ledger rows (never from planned values).

---

## 9. UI (Svelte 5, embedded)

Screens: **Now** (animated energy-flow diagram, live PV/grid/battery/load, current slot, today's
money), **Plan** (30 h × 15 min timeline: one panel with PV, load and battery bars in kW on the left axis
and SoC in kWh on the right, plus a short strip underneath carrying buy and feed-in on their own
euro axis, sharing the time axis and the cursor),
**Prices** (heatmap + net-metering toggle preview), **Batteries** (per-pack cards: SoC, power,
learned capacity/efficiency with confidence, protection latches, manual override), **Savings**
(daily bars + lifetime), **Settings** (onboarding, provider, contract constants, control mode).
Minimalist dark-first design, lucide icons, subtle motion (Svelte transitions), uPlot charts,
respect `prefers-reduced-motion`. Ingress-safe asset paths.

---

## 10. Packaging & rollout

Add-on `energy_rs`: `arch: [aarch64, amd64]`, `ingress: true`, `ingress_port: 8099`,
`homeassistant_api: true`, no `hassio_api`. Multi-stage Dockerfile: rust builder → compile C core
with `gcc` → static musl or debian-slim runtime. Release: GitHub release + add-on repo metadata.

Rollout: install side-by-side → `advisory` for a day → `dry_run` for a day (compare intended setpoints
with the old add-on's actual dispatch) → user flips `auto` and stops the old add-on.

---

## 11. Phases and gates (delegation order)

| # | Phase | Gate |
|---|---|---|
| P0 | Repo scaffold, workspace, CI, add-on skeleton, docs | `cargo build` + `cargo test` green, add-on builds for aarch64 |
| P1 | `energy-opt` + C core + validation + Rust oracle | differential tests, `-ffp-contract=off`, bench < 50 ms/solve |
| P2 | `energy-ha` discovery/stats/subscriptions | recorded-fixture tests, live read-only smoke |
| P3 | `energy-price` + settlement + store | formula unit tests vs researched numbers, toggle tests |
| P4 | `energy-forecast` | 800/800 exact golden vectors, load-profile tests |
| P5 | `energy-battery` zenSDK client + fake device | protocol tests against an in-process fake, pacing/echo tests |
| P6 | `energy-ledger` + learning + protection | schema/migration tests, learner golden sessions |
| P7 | `energy-core` engine + safety + dispatch + dry-run | ported trap tests, shadow-mode test |
| P8 | `energy-server` API + savings | contract tests, SSE smoke |
| P9 | `web/` UI | build + render smoke, no console errors, ingress paths |
| P10 | Packaging + install side-by-side + shadow run | live advisory/dry-run verified, go-ahead pending |

Phases added while executing the plan (same rules, same gates):

| # | Phase | Gate |
|---|---|---|
| P11 | `web/` real UI: Now, Plan, Prices, Batteries, Savings, Settings | build + render smoke, no console errors, ingress paths |
| P12 | Runtime assembly: bootstrap, engine/API bridge, load profile, lifecycle | end-to-end boot test with fake HA + fake devices, graceful stop |
| P13 | Savings accounting: measured counterfactual, split, rollups, catch-up | accounting/rollup tests, `docs/SAVINGS.md` |
| P14 | Read-only diagnostics: `doctor` and `plan-once` | `docs/DOCTOR.md`, CLI tests |
| P15 | Control-layer hardening from the independent safety review | one regression test per finding, `docs/RUNBOOK.md` §2 |
| P16 | Battery-layer hardening from the independent safety review | one regression test per finding |

P1 ∥ P2 after P0; P4 ∥ P5 ∥ P3 after P2/P1 land; P9 starts when the P8 contract is frozen; P11
replaces P9's placeholder shell; P13–P16 land as independent reviews report.

Standing rules: one worktree per task, merge to `main`, green gates before merge, push every merge,
no pushes/deploys without explicit instruction, no secrets in code or logs.

---

## 12. Code quality policy (enforced in CI)

**Structure** (`scripts/check_structure.py`, hard failure):
- One primary public type (`struct`/`enum`/`trait`) per `.rs` file; impl blocks live with their type.
- Domain folders: `src/<domain>/<type>.rs` + `mod.rs` re-exports. No flat `src/` dumping ground.
- No `utils.rs`, `helpers.rs`, `misc.rs`, `common.rs` grab-bags.
- Max 300 lines per file (500 under `tests/`), max 60 lines per function body, max 6 parameters.
- One Svelte component per `.svelte` file, file named after the component; max 250 lines per component.
- CSrc (`crates/*/csrc/`) is vendored from upstream and exempt, including the extraction script guards.

**Rust lints** (`[workspace.lints]`, `-D warnings`): rust `unsafe_op_in_unsafe_fn`, `unused_lifetimes`,
`missing_debug_implementations`; clippy `all` + `pedantic` (noisy pedantic lints explicitly allowed),
`unwrap_used`/`expect_used`/`panic`/`todo` denied outside tests, `too_many_arguments`,
`too_many_lines`, `cognitive_complexity`, `large_enum_variant`, `struct_excessive_bools`.
`rustfmt` with `max_width = 100`, `edition = 2024`.

**Web lints**: ESLint 9 flat config + `typescript-eslint` + `eslint-plugin-svelte`, Prettier check,
`svelte-check` with zero errors, `no-explicit-any` denied, max-lines per component.

**CI jobs**: `fmt`, `clippy -D warnings`, `structure`, `test`, `build`, `web-lint`, `web-build`, `web-check`.
A task that cannot meet the policy must state the exception explicitly in its reply; silent violations
fail the merge.
