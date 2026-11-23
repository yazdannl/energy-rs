# Rewrite specification — battery decision core (read-only analysis)

## Scope and reference points

This document specifies a ground-up **pure Rust** rewrite of the decision core only. Hardware I/O
(`energy-ha`, `energy-price`, zendure client), ledger, server API and dashboard stay as they are.
The C optimizer (`crates/energy-opt/csrc/`, `energy_opt::solve`) is **replaced**, not wrapped.

Line references are `path:line`. Unprefixed references are against the current checkout
`main @ 0c3e7a0` (2026-11-13 14:02:36 +0100). References prefixed `0.15.9:` are against the
shipped add-on branch `release/0.15.9`; that is the live ground truth where it differs from `main`.
Every CURRENT statement is grounded in one of those trees. Every INTENDED statement is a
checkable contract: formula, coefficient, unit, live value, precedence, state transition and
acceptance condition. `SCENARIOS.md` is the objective test matrix; `TRACEABILITY.md` maps every
review item to a contract and a scenario.

Seven decision modules are specified:

| # | Module | Main source areas |
|---|---|---|
| M1 | Horizon assembly and price input | `crates/energy-core/src/plan/*`, `crates/energy-price/src` |
| M2 | Optimizer objective and constraints | `crates/energy-opt/src`, `crates/energy-opt/csrc`, `plan/config.rs`, `plan/clamp.rs` |
| M3 | Plan assembly and feasibility | `plan/assemble.rs`, `plan/checks.rs`, `plan/generated_plan.rs`, `plan/summary.rs`, `maintenance/*` |
| M4 | Dispatch policy | `dispatch/*` |
| M5 | Safety ladder | `safety/*`, `engine/tick.rs` |
| M6 | Replanning and mode handling | `replan/*`, `mode.rs`, `engine/tick.rs`, `engine/commands.rs` |
| M7 | Learning and maintenance | `energy-battery/src/learning/*`, `energy-battery/src/protection/full_charge/*`, `wiring/sessions.rs` |

### Verified live price fixture (authoritative for all examples)

An earlier draft labelled tomorrow's `0.5088–0.5303` prices as today's. That was wrong and is
corrected everywhere. Verified from `GET /api/v1/prices?from=2026-11-22T12:00:00Z&to=2026-11-22T20:30:00Z`
(all-in buy, feed-in identical):

| Slot start UTC | Buy = sell (EUR/kWh) |
|---|---:|
| 12:00 12:15 12:30 12:45 | 0.3475 0.3470 0.3449 0.3505 |
| 13:00 13:15 13:30 13:45 | 0.3551 0.3573 0.3609 0.3742 |
| 14:00 14:15 14:30 14:45 | 0.3663 0.3754 0.3833 0.3946 |
| 15:00 15:15 15:30 15:45 | 0.3714 0.3791 0.3913 0.3980 |
| 16:00 16:15 16:30 16:45 | 0.4037 0.4044 0.4072 0.4111 |
| 17:00 17:15 17:30 17:45 | 0.4130 0.4064 0.4074 0.4061 |
| 18:00 18:15 18:30 18:45 | 0.4188 0.4084 0.4029 0.3969 |
| 19:00 … | 0.3980, falling to 0.3498 overnight |

**Today's maximum was 0.4188 at 18:00Z.** The 16:00–18:00Z window was 0.4037–0.4130, not
0.5303. The `0.5088–0.5303` range belongs to **tomorrow, 2026-11-23, 15:45–18:00Z**. Tonight
19:00Z–04:00Z is 0.3980 down to 0.3498, cheaper than today's afternoon.

The observed plan was built at 15:47Z with 89–92 % SoC, a 15 % reserve floor, four Zendure
SolarFlow 800 Plus packs, learned pooled capacity 7.8575 kWh (nameplate 7.68 kWh) and 3.2 kW aggregate AC power. Under these
prices, holding 0 W through today was **correct**; the real defect is narrower: the plan refuses
the profitable 0.3498–0.35 charge tonight needed to sell at 0.5303 tomorrow, so the fleet stays
idle for days and the efficiency learner never sees a cycle.

Cross-cutting invariants of the rewrite: no silent idling, no fabricated zero with a missing
measurement, no whole-plan failure for one unreachable pack, no advisory/dry_run parking, and
every decision explainable from a typed reason plus measured values.

---

## M1 — Horizon assembly and price input

### CURRENT

- **Given** a plan input with `now` and a queried price slot, **when** `plan::horizon::slots` runs,
  **then** it starts at the quarter-hour containing `now`, demands `expected_s = 900`, and requires
  every one of `horizon_slots` slots to exist exactly at `start + i*900`; any hole or misaligned slot
  is `PlanError::PriceGap`, never forward-filled or zero-filled (`plan/horizon.rs:8-36`,
  `plan/mod.rs:7-13`). The shipped live branch also defines `PriceResolution`
  (`0.15.9:crates/energy-core/src/plan/horizon.rs:15`,
  `0.15.9:crates/energy-core/src/plan/plan_error.rs:70-76`).
- **Given** `horizon_slots` outside `1..=120`, **when** validation runs, **then** the plan is refused
  with `PlanError::HorizonSlots` (`plan/checks.rs:12-19`, `plan/plan_error.rs:92-97`).
- **Given** a live price supply, **when** the source computes coverage, **then** it walks contiguous
  quarter-hours from `slot_at(now)`, breaks at the first uncovered slot, refuses fewer than
  `MIN_HORIZON_SLOTS`, and hands the shortened length to the solver; no missing-tail metadata reaches
  the plan (`0.15.9:crates/energy-core/src/plan_source/live.rs:256-289`,
  `0.15.9:crates/energy-core/src/plan_source/config.rs:36-39`).
- **Given** a publication older than `price_max_age_s` (shipped 6 h), **when** the source assembles
  a plan, **then** `PlanError::StalePrices` is returned (`0.15.9:crates/energy-core/src/plan_source/live.rs:144-154`).
- **Given** a fresh provider forecast covering the shortened horizon, **when** the source selects a
  forecast, **then** it slices it to `slots`; otherwise a measured EWMA fallback is used and marked
  degraded; a missing/failed fallback is a typed refusal, not zero (`0.15.9:crates/energy-core/src/plan_source/live.rs:156-199`).
- **Given** PV/load series whose capture is missing, older than `max(2×replan, stale_plan)`, wrong
  length, or non-finite, **when** checks run, **then** the plan is refused
  (`plan/checks.rs:41-75`, `plan/plan_error.rs:28-61`).
- **Given** a settlement, **when** a slot is priced, **then** import pays `buy_all_in`; export earns
  `buy_all_in` while a netting budget remains and `feed_in_post` afterwards; the Zonnebonus applies
  only to PV, daylight, positive feed-in and below the annual cap
  (`0.15.9:crates/energy-price/src/settlement/billing.rs:108-185`,
  `0.15.9:crates/energy-price/src/settlement/contract.rs:119-162`,
  `0.15.9:crates/energy-price/src/settlement/bonus.rs:19-38`).
- **Given** no configured saldering budget, **when** netting is on, **then** CURRENT assumes
  `1e9 kWh` of remaining budget — effectively unlimited 1:1 export
  (`0.15.9:crates/energy-price/src/settlement/billing.rs:11-12,120-129`).
- **Given** the operator switch `net_metering=off|on`, **when** the server builds a plan, **then** it
  overwrites the settlement regime (`set_regime`) before solving; an unrecognised stored value
  defaults to **on** (`0.15.9:crates/energy-core/src/plan_source/live.rs:100-105`,
  `0.15.9:crates/energy-server/src/bootstrap/config.rs:191-201`).
- **Given** several Home Assistant grid sources, **when** histories are summed, **then** empty
  sources are skipped and a one-time INFO/WARN is emitted; the required statistics are
  `import`, `export` and `pv`, while battery charge/discharge counters are optional and default to
  zero (`0.15.9:crates/energy-core/src/plan_source/ha_series.rs:81-99,136-143`,
  `0.15.9:crates/energy-core/src/plan_source/profile_build.rs:142,174-178`,
  `0.15.9:crates/energy-core/src/plan_source/source_notice.rs:1-121`).

### INTENDED contract

**Coverage.** `requested_slots = 120`, `slot_s = 900 s`, `t0 = floor(now/900)×900`,
`requested_end = t0 + 108000 s` (30 h). `actual_slots` is the number of contiguous, valid required
slots from `t0`. `requested` and `actual` are separate fields; no error or fallback may silently
change `requested_slots`.

| Actual coverage | Outcome | Dispatch permission |
|---|---|---|
| `actual_slots = 120` | `healthy` | economic dispatch allowed |
| `96 ≤ actual_slots < 120`, missing tail only, no interior gap | `degraded_accepted` | economic dispatch allowed only if M3 accepts; safety lane always active; `coverage_end`, `missing_slots`, `first_missing_t_start` exposed |
| `actual_slots < 96` or any interior gap | `PriceGap` / `InsufficientCoverage` refusal | no economic dispatch; hold with named cause; safety lane active |
| required grid source unavailable | `InputsUnavailable` refusal | same as above |

The 96-slot minimum is 24 h: dropping below it can hide tomorrow's 15:45–18:00Z peak while
keeping tonight's cheap window, which materially changes storage decisions. A shorter tail is
degraded evidence, not a newly defined problem.

**Resolution and precedence.** Every required slot must have `slot_seconds = 900`. A different
duration is `PriceResolution{expected_s:900,got_s,t_start}`. An absent/misaligned slot **inside**
the contiguous covered region is `PriceGap{first_missing_t_start,missing_count}`. An accepted
missing **tail** is explicitly exempt from `PriceGap`; it is provisional `tail_accepted` under the
96–119-slot rule, not a final successful outcome. Mandatory refusal checks run next; only if all
pass is the provisional state promoted to `degraded_accepted`. Numbered **error** precedence:
(1) `HorizonSlots`; (2) `PriceResolution`; (3) interior `PriceGap`; (4)
`InsufficientCoverage` when `actual < 96`; (5) `StalePrices`; (6) `InputsUnavailable`;
(7) `ForecastUnavailable`. Successful-outcome selection is a separate, later step: `healthy` when
`actual = 120`, otherwise the provisional tail becomes `degraded_accepted` with coverage metadata.
A tail with stale prices, a missing required source or an unavailable forecast therefore refuses at
steps 5–7; it never returns `degraded_accepted`. After any refusal, no implicit resampling,
zero-fill, interpolation or further shortening is permitted. Testing must include hourly input,
mixed durations, a mid-slot misalignment, a missing tail and an interior gap.

**Net-metering schedule.** Authoritative timezone is `Europe/Amsterdam`
(`0.15.9:crates/energy-price/src/book/query.rs:10`,
  `0.15.9:crates/energy-price/src/settlement/billing.rs:235`). Regime is
selected **per slot** by that slot's local tariff date:

- `DateDriven` (default): netting active iff `local_date.year() < 2027`
  (`0.15.9:crates/energy-price/src/settlement/net_metering.rs:6-28`). The boundary is
  **exclusive**: slots whose `t_start < 2026-12-31T23:00:00Z` are netted; slots whose
  `t_start ≥ 2026-12-31T23:00:00Z` are not. Europe/Amsterdam is CET (`UTC+01:00`) on that date,
  so `2027-01-01T00:00:00+01:00 = 2026-12-31T23:00:00Z`; the 22:45Z slot is the last netted one.
- Explicit `On` / `Off` overrides are allowed only with an operator acknowledgement, an
  `override_expires_at` and an event. An invalid/unknown configured value is `UnknownRegime`;
  it never defaults to `On`.
- Export price policy is one signed rule for every non-1:1 case: `export_price = feed_in_post`,
  including a negative tariff (a real cost of exporting), for `unknown`, `zero` and `exhausted`
  budgets. Do not apply `max(0, ·)` to one branch only. Reason is respectively
  `saldering_budget_unknown`, `saldering_budget_zero` or `saldering_budget_exhausted`. An operator
  acknowledgement may switch to `buy_all_in` with `budget_source=untracked`; it is the only 1:1
  path outside an active known budget, **and only inside an active netting regime**. A slot in the
  non-netted regime must use `feed_in_post` regardless of any acknowledgement; a regime change
  requires the explicit `On`/`Off` override contract with acknowledgement, `override_expires_at`
  and an event. The same signed price is used for marginal export.

**Grid-source completeness.** Required sources are configured `grid_import`, `grid_export` and
`pv` histories; optional sources are battery charge/discharge counters and any extra phase marked
optional. **One missing slot in one required source refuses the plan with `InputsUnavailable`**;
there is no degraded numeric fallback for required grid data. An all-required-empty group is the
same refusal with `empty_reason=all_empty`. Optional missing slots are summed as zero and labelled
`optional_zero`. `plan.quality.grid_source_coverage` carries `source_id`, `required`,
`slots_present`, `slots_missing`, `empty_reason` and the typed `outcome` (`ok`, `optional_zero`,
`InputsUnavailable`) for every source, so the refusal reason reaches the plan consumer.

**Acceptance.** S01–S06, S61–S66 and S124–S125 are the objective tests. In particular a 20-slot
book is refused, a 108-slot missing tail is accepted only as `degraded_accepted`, the 22:45Z slot
is netted and the 23:00Z slot is not, and one missing required-source slot refuses.

---

## M2 — Optimizer objective and constraints

### CURRENT (C core / Rust configuration)

- **Given** slot `i`, **when** the C core scores battery power `p` (positive = charge), **then**
  `base = load - pv`, `surplus = max(pv-load,0)`, `grid = base + p`, and
  `cost = dt × (buy×max(grid,0) − sell×max(−grid,0))`; a PV preference term
  `dt × (pv_price − sell) × min(p, surplus)` and optional `min_spread × max(p−surplus,0) × dt`
  are added for charging
  (`0.15.9:crates/energy-opt/csrc/core_solver.c:240-252,357-374`). Wear is
  `wear_rate × max(e_from − e_to, 0)` on stored kWh (`core_solver.c:345-349`).
- **Given** terminal state, **when** the last slot is scored, **then** energy left is worth
  `salvage = cheapest_buy / (eta_charge × eta_discharge) + wear` and a
  `target_penalty × |e − e_target|` term is added
  (`0.15.9:crates/energy-opt/csrc/core_solver.c:412-423`,
  `0.15.9:crates/energy-core/src/plan/assemble.rs:185-202`). Stage 3 replaces that row with a
  continuation row whose terminal term is already included (`core_solver.c:495-507`).
- **Given** configured caps, **when** the grid window is built, **then** `max_import_kw` and
  `max_export_kw` can bound the action set, export can be refused by `zero_export`, and a
  `sell_floor` can forbid unprofitable battery export
  (`0.15.9:crates/energy-opt/csrc/core_solver.c:198-238`). Shipped planning sets neither site
  ceiling and sets `sell_floor_eur_per_kwh = None`
  (`0.15.9:crates/energy-core/src/plan/config.rs:19-34`), so the optimizer sees only pack physics
  and prices; site ceilings are enforced later and reactively.
- **Given** the four-pack fleet, **when** the aggregate spec is built, **then** charge/discharge
  powers are summed (4×800 W = 3.2 kW), capacity-weighted efficiencies are used, and
  `e_min = Σ max(pack.minSoc, reserve_soc_pct) × capacity`, `e_max = Σ maxSoc × capacity`
  (`0.15.9:crates/energy-core/src/plan/aggregate.rs:21-58`, `plan/pack_spec.rs:62-75`).
- **Given** `reserve_soc_pct`, `peak_reserve_pct` and `solar_reserve_pct`, **when** policy is applied,
  **then** reserve is a hard floor, peak is a soft end-of-horizon target with a
  `peak_reserve_value_eur_per_kwh` penalty, and solar is headroom free of grid energy
  (`0.15.9:crates/energy-core/src/plan/policy.rs:10-35,70-79`,
  `0.15.9:crates/energy-core/src/plan/config.rs:37-65`).
- **Given** the live solar clamp, **when** a solved path is post-processed, **then** the permitted
  grid-charge ceiling at slot `i` is
  `e_max − solar_reserve − Σ_{j>i} max(pv_j − load_j, 0) × 0.25 h`, clamped into the SoC band; the
  PV portion survives, the grid portion is cut, and a floor violation is repaired by a minimum
  discharge (`0.15.9:crates/energy-core/src/plan/clamp.rs:96-175,126-151`).
- **Given** the owner's electrical ceiling, **when** dispatch runs, **then** a reactive guard uses
  `target_import = max(0, 7700 − 200) = 7500 W`: arm above `7500 + deadband`, ask for
  `measured_battery_w − (measured_grid_w − 7500)`, release only at or below `7500 − deadband`
  (`0.15.9:crates/energy-core/src/dispatch/import_ceiling.rs:9-36,93-151`,
  `0.15.9:crates/energy-server/src/api/settings/schema.rs:19-41`).
- **Given** a per-pack request, **when** allocation and final validation run, **then** per-pack
  caps, `min_pack_power_w` (default 50 W), fleet caps and grid caps bound or zero the command
  (`0.15.9:crates/energy-core/src/dispatch/pack_decision.rs:12-48`,
  `0.15.9:crates/energy-core/src/dispatch/fleet_clamp.rs:11-99`).

### INTENDED contract — complete objective

Every term is minimised in EUR over 120 slots. `i` is the slot index, `dt_i = minutes_i/60 h`,
`p_i` is aggregate AC battery power (kW, positive = charge), `e_i` is stored energy in kWh,
`c_i` is **actually curtailed** PV power (kW), `pv_i` is forecast available PV, and
`g_i = load_i − (pv_i − c_i) + p_i` in kW. `buy_i`/`sell_i` are EUR/kWh.
`b_i = max(0, pv_i − c_i − load_i)` is actual available PV surplus after curtailment.
All energy quantities use `dt_i`; no hardcoded `0.25 h` remains. **Capacity basis:** the planner
uses learned pooled capacity `7.8575 kWh` (`2.00029+1.96725+1.93220+1.95772`, source `pooled`);
the `7.68 kWh` nameplate is only the four 1.92 kWh labels, used as a prior/floor guard. The hard
reserve floor is `15 % × 7.8575 = 1.178625 kWh` (`1.1786 kWh` reported), never 15 % of 7.68.

```
minimize  Σ_i dt_i [ buy_i·max(g_i,0) − sell_i·max(−g_i,0)
                     + min_spread·max(p_i−b_i,0) ]
          + wear_rate·Σ_i max(e_i − e_{i+1}, 0)
          − salvage·e_N
          + target_penalty·max(0, e_target − e_N − 0.05)
```
`N = actual_slots` and `e_N` is the single terminal state used by the transition recursion, salvage,
terminal penalty and effective target: `N=120` for a full plan and `N=96..119` for a degraded
tail. `e_N` is the stored energy **after** slot `N−1` is applied; the recursion starts at `e_0` and
ends at `e_N`. The `0.05 kWh` is a **deductible deadband**, not an activation threshold: the first
0.05 kWh of shortfall is free and every kWh beyond it is penalised by `0.15 EUR/stored kWh`.

There is **no separate curtailment-price term**. Curtailment changes the grid balance, and the
grid revenue term prices the resulting import/export once. The former
`(pv_price_i − sell_i)·min(p_i,b_i)` term is removed from the economic objective: with
`pv_preference=Auto`, `pv_price_i = sell_i`, so the term was zero; under any configured
`PreferPv`/`AvoidPv` it could reward charging that displaced no export (notably under
`zero_export`). Configured preferences are reported separately as
`policy_preference_eur = (sell_i − pv_price_i)·min(max(p_i, 0), exportable_surplus_i)·dt_i` where
`exportable_surplus_i = min(b_i, export_limit_kw)` (0 when `zero_export`, `b_i` when export is
unlimited). This is a **policy preference only**: it is excluded from `objective_eur`, `saved_eur`,
profit and all financial assertions, and is emitted with `reported_separately=true`.

**Site inequalities are hard.** For every slot:

```
0 ≤ c_i ≤ pv_i
−export_limit_kw ≤ g_i = load_i − pv_i + c_i + p_i ≤ import_limit_kw
```

`import_limit_kw = (7700 − 200)/1000 = 7.5 kW`. `export_limit_kw = +∞` when configured
`grid_export_limit_w = 0` (unlimited export); `export_limit_kw = 0` when `zero_export = true`;
otherwise it is the configured watts / 1000. Forecast net load is `load_i − pv_i` from the selected
forecast (or EWMA fallback, with its source in plan quality). Intersect the resulting `g` window
with battery `lo/hi`; if `lo > hi`, refuse with `infeasible` and every binding bound. The current
slot uses only unelapsed minutes (`10 min ⇒ dt = 1/6 h`) in grid energy, stored energy, wear and
curtailment. Reactive guard stays only as a forecast-error backstop.

**Energy boundaries and units:**

- charge: `e_{i+1} = e_i + dt_i·p_i·η_c`; discharge: `e_{i+1} = e_i + dt_i·p_i/η_d`.
  `η_c = η_d = 0.90`, `η_rt = 0.81` (`0.15.9:crates/energy-battery/src/learning/efficiency.rs:44-48`;
  capacity-weighted in `plan/aggregate.rs:21-58`). Comparing stored reserve with AC PV uses
  `AC_headroom = (e_max − e_i)/η_c`.
- `wear_rate = 0.02 EUR per stored kWh discharged`
  (`0.15.9:crates/energy-server/src/bootstrap/config.rs:59`), so per AC kWh charged
  `wear_cycle = 0.018 EUR`.
- `salvage = cheapest_buy / η_c` in EUR per **stored** kWh: with 0.3498 EUR/kWh,
  `0.3498/0.90 = 0.38866667 EUR/kWh`. That is the actual AC replacement cost of one stored kWh.
  The old `cheapest/η_rt + wear = 0.45185185` valued stored energy 16 % above replacement and
  created a terminal accumulation incentive unrelated to cycle profit; it is removed. The export
  gate's `basis/η_rt + wear` is a separate **per delivered AC kWh** break-even and is unchanged.
- `target_penalty = 0.15 EUR per stored kWh`, one-sided with a `0.05 kWh` deductible deadband:
  penalty is `0.15×max(0, e_target − e_N − 0.05) EUR` (`0.15.9:crates/energy-core/src/plan/config.rs:31`).
  A 0.04 kWh shortfall costs 0; a 0.10 kWh shortfall costs `0.15×0.05 = 0.0075 EUR`.
- `min_spread = 0.0 EUR/kWh`, `soc_step_kwh = 0.05 kWh`, `max_states = 400`, `power_steps = 24`,
  `align_actions = true`, `pv_preference = Auto ⇒ pv_price_i = sell_i` and the removed economic
term is identically zero
  (`0.15.9:crates/energy-core/src/plan/config.rs:9-34`,
  `0.15.9:crates/energy-opt/src/config/settings.rs:104-131`,
  `0.15.9:crates/energy-opt/src/config/pv_charge_preference.rs:3-13`).

**Economics contract.** For one AC kWh charged now and delivered later:

```
profit_per_ac_kWh_charged = η_rt·sell_later − buy_now − wear_cycle
```

At tomorrow's single highest slot `0.5303`, `0.81×0.5303 − 0.3498 − 0.018 = +0.061743 EUR` per
AC kWh charged. For the actual fixture slots used below, tomorrow's weighted sale price is
`2.568660/5 = 0.513732 EUR/kWh AC`, so the realised profit is `0.81×0.513732 − 0.3498 − 0.018
= +0.048323 EUR` per AC kWh charged — still positive. Sell-today→hold-tomorrow is not a cycling
decision: the same stored AC kWh earns at most `0.4188` today but `0.513732` on the fixture's
tomorrow slots, a `0.094932 EUR/kWh AC` difference with identical wear. A merely decent earlier
price must not trigger a sale when a higher later price is visible and affordable to wait for.

**Curtailment algebra and no double payment.** There is no two-pass solve, no window sum and no
separate curtailment price. Let `pv_i` be forecast PV, `c_i` curtailed PV, `0 ≤ c_i ≤ pv_i`, and
actual PV `= pv_i − c_i`. Then `g_i = load_i − pv_i + c_i + p_i`. Let the uncurtailed net be
`g0_i = load_i − pv_i + p_i`. If `g0_i < 0` and export is unlimited, revenue is
`−sell_i·g0_i = sell_i·(pv_i − load_i − p_i)`. Curtailing `c_i` makes
`g_i = g0_i + c_i` and revenue `sell_i·(pv_i − c_i − load_i − p_i) = −sell_i·g0_i − sell_i·c_i`
while still exporting. The `−sell_i·c_i` loss is already in the grid term; adding a separate
penalty would pay twice. If export is capped at `E_i`, feasibility requires `g_i ≥ −E_i`, so any
excess `max(0, available_export − E_i)` must be curtailed; revenue stays capped at `sell_i·E_i`
and the unearned excess could not be sold anyway. If `zero_export` sets `E_i = 0`, all surplus
above load plus battery charge is curtailed and no export revenue exists to lose. Battery charging
that absorbs otherwise-curtailed PV is rewarded only once, by the later discharge/grid revenue and
the terminal value. `solar_reserve_pct = 20 %` is reported headroom on the learned basis
(`0.2×7.8575 = 1.571500 kWh`); the nameplate product `0.2×7.68 = 1.536 kWh` is labelled
`solar_reserve_nameplate_kwh` and is never used for feasibility. Neither is a second constraint.

### INTENDED contract — honest defect, timestamped fixture and tie-break

The C core sweeps slots backward (`core_solver.c:426`), sorts actions ascending
(`core_solver.c:309-313`) and uses strict `<` (`core_solver.c:445-455`), so genuine ties go to the
lowest power, not calendar order; reconstruction reuses the nearest grid point's `argmin`
(`core_solver.c:462-483`). That mechanism is still replaced.

**Verified prices make holding today correct.** Today's maximum was `0.4188` at 18:00Z; tomorrow's
15:45–18:00Z range is `0.5088–0.5303`. A plan that discharges today at 0.4188 must fail the test;
the correct plan holds today and sells tomorrow. The reviewer-verified 6.4 AC kWh arithmetic
(`charge 6.4/0.81×0.3498 = 2.76385185`, `wear 6.4/0.90×0.02 = 0.14222222`, sale today at 0.4188
`2.68032`, sale tomorrow `3.39392`, nets `-0.22575407` and `+0.48784593`, delta `0.7136`) is a
scaling check only: a 6.4 AC discharge from 89 % SoC and a 15 % floor is physically infeasible
before any recharge, so it is not the acceptance fixture.

**One chronologically feasible fixture.** Start `2026-11-22T15:45:00Z`, end
`2026-11-23T21:45:00Z` (120×900 s). Learned-basis fleet `e_max = 7.8575 kWh`, floor
`1.178625 kWh` (`1.1786 kWh` reported), power cap `3.2 kW`; nameplate/label basis is
`7.68 kWh = 4×1.92 kWh` and is used only as the capacity prior. Every energy assertion below uses
the learned basis. Start and required final energy `E0 = E_final = 6.995 kWh` (89 %). Both strategies
deliver `5.0 AC kWh` and replace it with `5.0/0.81 = 6.172840 AC kWh` at `0.3498` (cost
`2.159259 EUR`; stored drop/gain `5.555556 kWh`; wear `0.111111 EUR`). One AC kWh sold removes
`1/0.90 = 1.111111 kWh` stored; one AC kWh charged adds `0.90 kWh` stored.

| Time (UTC) | Transaction | Price | E after A | E after B |
|---|---|---:|---:|---:|
| 22 Nov 15:45 | start | — | 6.995000 | 6.995000 |
| 22 Nov 16:30–16:45 | A sells 0.8 AC | 0.4072 | 6.106111 | 6.995000 |
| 22 Nov 16:45–17:00 | A sells 0.8 AC | 0.4111 | 5.217222 | 6.995000 |
| 22 Nov 17:00–17:15 | A sells 0.8 AC | 0.4130 | 4.328333 | 6.995000 |
| 22 Nov 17:15–17:30 | A sells 0.2 AC at 0.8 kW | 0.4064 | 4.106111 | 6.995000 |
| 22 Nov 17:30–17:45 | A sells 0.8 AC | 0.4074 | 3.217222 | 6.995000 |
| 22 Nov 18:00–18:15 | A sells 0.8 AC | 0.4188 | 2.328333 | 6.995000 |
| 22 Nov 18:15–18:30 | A sells 0.8 AC | 0.4084 | 1.439444 | 6.995000 |
| 22 Nov 23:00–23:15 | A charges 0.8 AC | 0.3498 | 2.159444 | 6.995000 |
| 22 Nov 23:15–23:30 | A charges 0.8 AC | 0.3498 | 2.879444 | 6.995000 |
| 22 Nov 23:30–23:45 | A charges 0.8 AC | 0.3498 | 3.599444 | 6.995000 |
| 22 Nov 23:45–00:00 | A charges 0.8 AC | 0.3498 | 4.319444 | 6.995000 |
| 23 Nov 00:00–00:15 | A charges 0.8 AC | 0.3498 | 5.039444 | 6.995000 |
| 23 Nov 00:15–00:30 | A charges 0.8 AC | 0.3498 | 5.759444 | 6.995000 |
| 23 Nov 00:30–00:45 | A charges 0.8 AC | 0.3498 | 6.479444 | 6.995000 |
| 23 Nov 00:45–01:00 | A charges 0.572840 AC at 2.29136 kW | 0.3498 | 6.995000 | 6.995000 |
| 23 Nov 16:00–16:15 | B sells 0.8 AC | 0.4968 | 6.995000 | 6.106111 |
| 23 Nov 16:15–16:30 | B sells 0.8 AC | 0.5098 | 6.995000 | 5.217222 |
| 23 Nov 16:30–16:45 | B sells 0.8 AC | 0.5114 | 6.995000 | 4.328333 |
| 23 Nov 16:45–17:00 | B sells 0.8 AC | 0.5303 | 6.995000 | 3.439444 |
| 23 Nov 17:00–17:15 | B sells 0.8 AC | 0.5188 | 6.995000 | 2.550556 |
| 23 Nov 17:15–17:30 | B sells 0.8 AC | 0.5156 | 6.995000 | 1.661667 |
| 23 Nov 17:30–17:45 | B sells 0.2 AC at 0.8 kW | 0.5125 | 6.995000 | 1.439444 |
| 23 Nov 19:00–19:15 | B charges 0.8 AC | 0.3498 | 6.995000 | 2.159444 |
| 23 Nov 19:15–19:30 | B charges 0.8 AC | 0.3498 | 6.995000 | 2.879444 |
| 23 Nov 19:30–19:45 | B charges 0.8 AC | 0.3498 | 6.995000 | 3.599444 |
| 23 Nov 19:45–20:00 | B charges 0.8 AC | 0.3498 | 6.995000 | 4.319444 |
| 23 Nov 20:00–20:15 | B charges 0.8 AC | 0.3498 | 6.995000 | 5.039444 |
| 23 Nov 20:15–20:30 | B charges 0.8 AC | 0.3498 | 6.995000 | 5.759444 |
| 23 Nov 20:30–20:45 | B charges 0.8 AC | 0.3498 | 6.995000 | 6.479444 |
| 23 Nov 20:45–21:00 | B charges 0.572840 AC at 2.29136 kW | 0.3498 | 6.995000 | 6.995000 |
| 23 Nov 21:45 | end | — | 6.995000 | 6.995000 |

Revenue A `0.8×(0.4072+0.4111+0.4130+0.4074+0.4188+0.4084)+0.2×0.4064 = 2.054000 EUR`.
Revenue B uses the verified slot prices, not a flat constant:
`0.8×(0.4968+0.5098+0.5114+0.5303+0.5188+0.5156)+0.2×0.5125 = 2.568660 EUR`. Net A
`-0.216370 EUR`, B `+0.298290 EUR`, advantage `+0.514660 EUR`. Every stored value stays in
`[1.178625, 7.8575]`; every power ≤3.2 kW; both recharges use the same 0.3498 price and energy; final
energies are identical.

**Deterministic tie-break.** Quantify stored energy as `Q(e)=round(1000·e)/1000 kWh` and power as
`Q(p)=round(1000·p)/1000 kW`. Compare, in order: (1) `round(objective_eur,6)`; (2)
`first_discharge_slot_index`, with no discharge encoded as `120`, earlier wins; (3)
`round(Σ dt_i·p_i²,3)`; (4) `nonzero_slot_count`; (5) state signature `[Q(e_1),…,Q(e_N)]`
lexicographically; (6) power signature `[Q(p_0),…,Q(p_{N-1})]` lexicographically, where `N=actual_slots`. If all six keys
are equal the paths are identical at 1 Wh/1 W resolution and either may be returned with
`tie_resolved_by=canonical_signatures`. Enumeration order is never decisive. The 1 Wh rounding is
used in keys 5–6, not as a substitute for exact objective arithmetic.

**Acceptance.** S07–S18, S67–S74, S115–S120 and S158/S161/S164 are the objective tests. S115 is
the strategy counterexample that fails any plan discharging at today's 0.4188 when the fixture
constraints apply; S116 replays the timestamped fixture; S158 and S164 are oracle-based optimizer
acceptance; S161 is the generated physical-property check; S117
asserts curtailment is counted once and unlimited export adds no penalty; S118 asserts the
consistent terminal salvage; S119 the canonical tie-break; S120 the site inequality. Learning
uncertainty intervals remain out of M2 scope (reviewer cut/defer).

---

## M3 — Plan assembly and feasibility

### CURRENT

- **Given** a `PlanInput`, **when** `assemble::build` runs, **then** it validates, selects slots,
  aggregates packs, resolves the basis, builds config, solves, clamps, builds timeline rows,
  summarizes and sets `feasible = infeasible_reason.is_none()`
  (`0.15.9:crates/energy-core/src/plan/assemble.rs:16-69`).
- **Given** a configured terminal target and solved terminal error > `0.05 kWh`, **when** summary is
  built, **then** `infeasible_reason = "terminal_target_unreachable"`; no other infeasibility is
  represented, even when a clamp shortens a charge or a discharge
  (`0.15.9:crates/energy-core/src/plan/assemble.rs:45-47`, `plan/checks.rs:77-84`,
  `plan/clamp.rs:126-151`).
- **Given** a soft peak target lower than the hard reserve floor, **when** `terminal_target` is
  derived, **then** the effective target is `max(soft, e_min)` capped at `e_max`, while
  `summary.peak_reserve_kwh` keeps the raw soft guideline
  (`0.15.9:crates/energy-core/src/plan/config.rs:37-65`, `plan/plan_summary.rs:28-48`).
- **Given** a plan summary, **when** serialized, **then** it carries cost, charge, discharge, grid,
  PV, cycles, terminal target, infeasible reason, export floor, zero-export flag, cost basis,
  reserves, clipped and gated kWh (`0.15.9:crates/energy-core/src/plan/plan_summary.rs:10-58`).
- **Given** PV plus battery discharge, **when** summary is computed, **then** export is a negative
  cost (credit), grid charge is only the part above PV surplus, and cycles = discharged kWh /
  capacity (`0.15.9:crates/energy-core/src/plan/summary.rs:33-71`).
- **Given** maintenance candidates, **when** the source places full-charge assignments, **then** it
  adds them after economic planning, recomputes the SoC path and summary, and publishes
  `plan.maintenance` and `plan.maintenance_unassigned`
  (`0.15.9:crates/energy-core/src/plan_source/live.rs:116-142`,
  `0.15.9:crates/energy-core/src/maintenance/apply.rs:12-68`).
- **Given** a due pack that cannot be scheduled, **when** the planner reports it, **then**
  `UnassignedPack` carries a reason (`horizon_too_short`, `no_headroom`, `no_cheap_slots`,
  `not_enough_eligible_slots`, `insufficient_capacity`, `soc_ceiling_below_target`), required,
  available and missing kWh, and the 07:00 local deadline
  (`0.15.9:crates/energy-battery/src/protection/full_charge/unassigned_pack.rs:9-103`).

### INTENDED contract

**Plan states and dispatch permissions.**

| State | Meaning | Dispatch permission |
|---|---|---|
| `healthy` | all hard constraints satisfied; all required outcomes met; final timeline validated | economic + safety dispatch |
| `degraded_accepted` | hard constraints satisfied, but optional coverage/forecast/maintenance obligations unmet; operator acceptance has `scope`, `accepted_by`, `expires_at` | economic dispatch within the accepted scope + safety; shortfall stays visible |
| `infeasible` | any hard constraint violated, bound conflict, or required outcome unmet | safety lane only; no economic setpoint from this plan |

Delivery failure is a separate axis: `delivery = {full, partial, failed}` with `failed_pack_ids[]`.
One unreachable pack sets `partial`, never `plan.infeasible` for the whole fleet.

**Final validation.** Run feasibility **after** solver, clamp replacement, maintenance application and
SoC-path recomputation. Validate every slot against: `e_min ≤ e_i ≤ e_max`, pack power caps, site
import ≤ 7500 W planned ceiling, export ceiling, cell/thermal hard limits, terminal minimum, and
maintenance deadlines. Clipping itself is a `binding_bound`, not an infeasibility; an unmet
**required** outcome (> 0.05 kWh shortfall or missed deadline) is infeasible. If floor > ceiling,
refuse with `InvalidBounds` **before** solving; do not let `min(max(soft,floor),ceiling)` produce a
target below the hard floor.

**Terminal target.** Effective target `T = min(max(soft_guideline, hard_floor), ceiling)`;
`T` does **not** depend on horizon length. The learned capacity basis is `7.8575 kWh`
(learned pooled), the nameplate/label basis is `7.68 kWh` and is only a prior/floor guard.
Live `reserve_soc_pct = 15` is the **hard floor**: `0.15 × 7.8575 = 1.178625 kWh`
(`1.1786 kWh`). Live `peak_reserve_pct = 10` is a **soft guideline**, not 20:
`0.10 × 7.8575 = 0.785750 kWh`, reported separately as `peakReserveWh = 785.75 Wh`.
`solar_reserve_pct` supplies charge headroom and does not alter `T`. Therefore
`T = max(0.10, 0.15) × 7.8575 = 1.178625 kWh`, capped at `e_max`. Let `N = actual_slots`
(120 for a full plan, 96..119 for a degraded tail). Acceptance is `e_N ≥ T − 0.05 kWh`; the same
`e_N` is the terminal state in the transition recursion, salvage and target penalty. There is no
`e_120` check on a shorter plan. The solver uses the same `T` at index `N`, with the one-sided
penalty. A below-floor guideline is raised to the floor; an above-ceiling guideline is
capped and reported as clipped; a conflicting floor/ceiling is refused.

**Structured infeasibility.** Every non-healthy plan carries
`binding_bounds[] {reason, slot, kW_or_kWh}`, `unmet_kwh[] {obligation, pack_id, required_kwh,
available_kwh, shortfall_kwh, reason, deadline, obligation_class}`,
`plan_vs_delivered[] {slot, planned_w, delivered_w, delta_w}` and ordered `reasons[]`. **Every**
obligation record, required or optional, carries `reason` and `deadline`; `obligation_class` is
`required` or `optional`. Required when the due deadline is inside the horizon and the pack is
eligible; otherwise optional and `degraded_accepted` with the full reason/deadline/shortfall.
Operator acceptance cannot waive a hard constraint.

**Acceptance.** S19–S23, S75–S80, S126–S127, S152 and S161 are the objective tests. S20 asserts a required 0.4 kWh
shortfall is infeasible while binding bounds remain separate; S79 asserts final maintenance that
breaks the floor turns the assembled plan infeasible; S77 distinguishes clipping from unmet
obligation.

---

## M4 — Dispatch policy

### CURRENT

- **Given** a tick, **when** `Dispatcher::tick` runs, **then** it prunes faults, builds a frame
  (slot, plan power, follow target, import guard, allocation), plans every pack, validates the final
  vector, sends in pack order and returns one `PackReport` per pack
  (`0.15.9:crates/energy-core/src/dispatch/dispatcher.rs:68-90,92-145`).
- **Given** `advisory` or `dry_run`, **when** a tick runs, **then** the ladder is evaluated in shadow,
  `intended_w` populated, and `written = false`, `applied_w = 0`; even holds are not sent
  (`dispatch/mod.rs:3-13`, `dispatch/dispatcher.rs:156-213`).
- **Given** `now` inside a slot, **when** the setpoint is interpolated, **then** CURRENT eases from
  the previous value over `TRANSITION_SECONDS = 120`; a command ≤ `HOLD_DEADBAND_W = 30 W` is sent
  as a hold (`dispatch/mod.rs:82-85`, `0.15.9:crates/energy-core/src/dispatch/dispatcher/write.rs:24-30`).
- **Given** a netted zero-rate slot, **when** grid-follow considers a correction, **then** it leaves
  the 0 W setpoint alone (no wear-only discharge); an exporting slot is also left alone, while a
  self-consumption slot follows the meter within `max(500 W, 0.5×capability)`
  (`0.15.9:crates/energy-core/src/dispatch/follow_target.rs:26-53,56-95`, `safety/grid_follow.rs:30-45`).
- **Given** a measured import breach, **when** the ceiling guard runs, **then** it can lower the
  fleet target even with a stale/absent plan (`NO_PLAN_TARGET_W = 0`) but never raise a charge;
  skewed timestamps hold the last measured output while armed
  (`0.15.9:crates/energy-core/src/dispatch/import_ceiling.rs:43-50,93-164`).
- **Given** a write, **when** a receipt arrives, **then** `verified` requires an echo plus measured
  flow in the commanded direction; `applied_w` is the pack's own measurement, not the echo
  (`0.15.9:crates/energy-core/src/dispatch/write_receipt.rs:3-18`,
  `0.15.9:crates/energy-core/src/dispatch/battery_sink.rs:155-182`,
  `0.15.9:crates/energy-battery/src/control/flow.rs:23-32,72-81,108-132`).
- **Given** a fault, **when** `send` fails, **then** one fail-safe hold is written, the pack backs
  off (300 s for restart/no-flow, otherwise configured), and safety traffic still gets through
  (`0.15.9:crates/energy-core/src/dispatch/dispatcher/write.rs:100-157`,
  `0.15.9:crates/energy-core/src/dispatch/fault_guard.rs:1-12,117-120`).
- **Given** a suppressed backoff tick, **when** the row is reported, **then** fresh measured flow is
  recorded with `flow_measured = true`, `written = false`; the detail distinguishes "previous
  command still standing" from "no command standing" using last **issued** commands
  (`0.15.9:crates/energy-core/src/dispatch/dispatcher/backoff.rs:14-91`,
  `0.15.9:crates/energy-core/src/dispatch/pack_report.rs:5-45`).

### INTENDED contract

**Mode, applicability and precedence.** Economic writes require `auto` authority. `protection_active`
suppresses economic writes but never safety writes; true `dry_run` writes nothing (M5/M6). Per pack,
first evaluate applicability, then choose the highest-priority applicable rule:

| Rule | Applicable when | Target | Limits | Not applicable / blocked outcome |
|---|---|---|---|---|
| cell guard | latch `blocked` or `force_charge` | `0 W` or `max(800 W, requested)` | universal physical limits only (M5) | `cell_safety_impossible` if physical limits prevent it |
| import/export guard | measured breach and a remedy direction exists | minimum correction to reach target grid | universal physical limits + SoC floor + cell latch | `import_remedy_blocked` naming the physical limit |
| operator override | unexpired explicit request in `auto` | requested W | site + pack + reserve | ignored/expired with `operator_override_expired` |
| maintenance | assignment exists, deadline inside horizon, pack eligible, not completed | assigned W | cheap-window headroom + import ceiling | `unassigned` with reason/deadline/shortfall |
| plan target | eligible plan and a target is defined for the current slot | plan W | site + pack + reserve | `no_eligible_plan` if none |
| grid-follow self-consumption | plan target is 0 **and** no higher rule has a nonzero unmet obligation | net-load correction ≤`max(500 W, 0.5×capability)` | site + pack + reserve | `desired_grid_w=None` when netted or exporting |

A plan target of `0 W` is applicable only as **authorisation to hold** when no applicable rule has a
nonzero target and no required obligation is unmet. If a higher rule had a nonzero target but was
blocked, the row is `blocked` with that reason, never `justified_hold`. A successful 0 W write is
not evidence that the energy objective was satisfied. Ramp is a transition mechanism after target
selection, not a selection rule.

**Outcome table.** `applied_w` is exclusively a fresh signed measurement; `null` means no
measurement. `written` = transport acknowledgement. `verified` means **command verification only**:
two fresh post-ramp reports that confirm the issued command's intended flow state. It is never set
by an anomaly. Anomalies use `anomaly_confirmed=true` and `evidence_kind=confirmed_anomaly`;
command evidence uses `evidence_kind=command_verification`, ramp evidence `provisional`, and absent
measurement `no_measurement`. `flow_measured` means a fresh reading exists. Status mapping is exact:
charge `(acStatus=2, dcStatus=1)`, discharge `(1,2)`, idle `(0,0)`. Direction must match the
command; reversed status is a mapping alarm, not successful flow.

Define `C=|requested_w|`, `M=|measured_w|`, tolerance `τ(C)=max(30 W, 0.10·C)`, and status
`S ∈ {charge, discharge, idle, unknown, invalid}`. Valid status pairs are charge `(2,1)`,
discharge `(1,2)`, idle `(0,0)`. The following predicates are evaluated **in this order and stop
at the first match**, so they are mutually exclusive and exhaustive for measured receipts:

1. `status_inconsistent` — `S=invalid`, or `M>30 W` and the status direction contradicts the
   sign of the measured power; mapping/data alarm.
2. `idle_with_flow` — `S=idle` and `M>30 W`, for **any** command including a hold.
3. `unknown_direction_flow` — `S=unknown` and `M>30 W`, for any command including a hold.
4. `hold_unexpected_flow` — `C≤30 W`, `M>30 W`, and `S` is a valid charge/discharge direction.
5. `wrong_direction` — `C>30 W`, `M>30 W`, and the valid status direction opposes the command.
6. `unknown_direction_no_flow` — `S=unknown` and `M≤30 W`; a restarting pack with a trickle is
   not verified neutral.
7. `opposite_direction_no_flow` — `C>30 W`, `M≤30 W`, and a valid status direction opposes the
   command.
8. `hold_status_directional` — `C≤30 W`, `M≤30 W`, and `S` is charge or discharge; a hold cannot
   be verified from a directional status pair.
9. `hold_verified` — `C≤30 W`, `M≤30 W` and `S=idle`.
10. `no_flow` — `C>30 W`, `M≤30 W`, and `S` is idle or the command's direction.
11. `full_flow` — direction matches and `|M−C|≤τ(C)` (includes 2800/3000 and 80/100).
12. `partial_flow` — direction matches and `30 W<M<C−τ(C)`.
13. `over_flow` — direction matches and `M>C+τ(C)`.

The same predicates decide pause entry, release and escalation: only `partial_flow` enters or
sustains a pause; only `full_flow` releases it; `persistent_partial`, `over_flow`,
`idle_with_flow`, `unknown_direction_flow`, `hold_unexpected_flow`, `wrong_direction`,
`unknown_direction_no_flow`, `opposite_direction_no_flow`, `hold_status_directional` and
`status_inconsistent` all bypass pause release and escalate with their typed reason and a fallback
hold. No bare `<90 %` or bare 30 W rule is used. Pre-ramp matches are suffixed `_provisional` with
`verified=false` and never enter a pause. Missing measurement is classified separately as
`echo_only` (echo present, no flow reading) or `read_failure` (read transport failed).

| Outcome | written | verified | anomaly_confirmed | evidence_kind | flow_measured | applied_w | Required evidence |
|---|---|---|---|---|---|---|---|
| `hold_verified` | true | true | false | `command_verification` | true | signed measured W, `M≤30 W` | two post-ramp reports, exactly `(0,0)` |
| `full_flow` | true | true | false | `command_verification` | true | signed measured W, `|M−C|≤τ(C)` | two post-ramp reports with correct status pair |
| `partial_flow_provisional` | true | false | false | `provisional` | true | signed measured W in rule 12 | reports inside the 45 s ramp; `provisional=true` |
| `partial_flow_confirmed` | true | true | false | `command_verification` | true | signed measured W in rule 12 | two post-ramp reports; starts 120 s pause |
| `persistent_partial` | true | false | true | `confirmed_anomaly` | true | measured W still in rule 12 after pause expiry | two bounded retries, then escalation/fallback hold |
| `over_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W in rule 13 | two post-ramp reports; immediate escalation |
| `wrong_direction` | true | false | true | `confirmed_anomaly` | true | signed measured W | valid opposite direction with nonzero flow; mapping alarm |
| `status_inconsistent` | true | false | true | `confirmed_anomaly` | true | signed measured W | invalid enum or status contradicts measured sign |
| `idle_with_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W | status `(0,0)` with `M>30 W` |
| `unknown_direction_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W | unknown status with `M>30 W` |
| `hold_unexpected_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W | hold command, `M>30 W`, directional status |
| `unknown_direction_no_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W | unknown status with `M≤30 W` |
| `opposite_direction_no_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W | opposite status with `M≤30 W` |
| `hold_status_directional` | true | false | true | `confirmed_anomaly` | true | signed measured W | hold command, `M≤30 W`, directional status |
| `echo_only` | true | false | false | `no_measurement` | false | `null` | echo with correct limit but no flow report |
| `read_failure` | true | false | false | `no_measurement` | false | `null` | transport ack, read missed |
| `no_flow` | true | false | true | `confirmed_anomaly` | true | signed measured W, `M≤30 W` for `C>30 W` | two post-ramp reports with no flow; `NoFlow` |
| `suppressed` / `deferred` | false | false | false | `no_measurement` | per fresh reading | measured W or `null` | backoff, pause or `protection_active`; reason and `retry_at` |

Every row reports `requested_w`, `written_w`, `confirmed_w` and `applied_w` separately. Standing
lifecycle: `requested` → `write_ack` → `confirmed` (two post-ramp reports) → `released`; fallback
hold retries at 0/2/6 s then escalates `safety_hold_unverified`.

**Boundary truth table** (`τ(3200)=320`, `τ(800)=80`, `τ(100)=30`; all powers W; status shown
explicitly; `D`/`C`/`I`/`U` = discharge/charge/idle/unknown):

| Command | M | Status | Outcome |
|---:|---:|---|---|
| -3200 | 0 | I | `no_flow` |
| -3200 | 5 | I | `no_flow` |
| -3200 | 5 | U | `unknown_direction_no_flow` |
| -3200 | 5 | C | `opposite_direction_no_flow` |
| -3200 | 30 | U | `unknown_direction_no_flow` |
| -3200 | 31 | U | `unknown_direction_flow` |
| -3200 | 30 | C | `opposite_direction_no_flow` |
| -3200 | 31 | C | `wrong_direction` |
| -3200 | 80 | D | `partial_flow` |
| -3200 | 80 | I | `idle_with_flow` |
| -3200 | 80 | U | `unknown_direction_flow` |
| -3200 | 80 | C | `wrong_direction` |
| -3200 | 90 | D | `partial_flow` |
| -3200 | 2880 | D | `full_flow` (error=τ) |
| -3200 | 3200 | D | `full_flow` |
| +800 | 0 | I | `no_flow` |
| +800 | 5 | I | `no_flow` |
| +800 | 5 | U | `unknown_direction_no_flow` |
| +800 | 5 | D | `opposite_direction_no_flow` |
| +800 | 30 | U | `unknown_direction_no_flow` |
| +800 | 31 | U | `unknown_direction_flow` |
| +800 | 30 | D | `opposite_direction_no_flow` |
| +800 | 31 | D | `wrong_direction` |
| +800 | 80 | C | `partial_flow` |
| +800 | 80 | I | `idle_with_flow` |
| +800 | 80 | U | `unknown_direction_flow` |
| +800 | 80 | D | `wrong_direction` |
| +800 | 90 | C | `partial_flow` |
| +800 | 2880 | C | `over_flow` |
| +800 | 3200 | C | `over_flow` |
| +100 | 0 | I | `no_flow` |
| +100 | 5 | I | `no_flow` |
| +100 | 5 | U | `unknown_direction_no_flow` |
| +100 | 5 | D | `opposite_direction_no_flow` |
| +100 | 30 | U | `unknown_direction_no_flow` |
| +100 | 31 | U | `unknown_direction_flow` |
| +100 | 30 | D | `opposite_direction_no_flow` |
| +100 | 31 | D | `wrong_direction` |
| +100 | 30 | C | `no_flow` (M=τ boundary, rule 10) |
| +100 | 80 | C | `full_flow` (error 20≤τ) |
| +100 | 80 | I | `idle_with_flow` |
| +100 | 80 | U | `unknown_direction_flow` |
| +100 | 80 | D | `wrong_direction` |
| +100 | 90 | C | `full_flow` |
| +100 | 2880 | C | `over_flow` |
| +100 | 3200 | C | `over_flow` |
| 0 | 80 | I | `idle_with_flow` |
| 0 | 80 | U | `unknown_direction_flow` |
| 0 | 80 | C | `hold_unexpected_flow` |
| 0 | 80 | D | `hold_unexpected_flow` |
| 0 | 5 | I | `hold_verified` |
| 0 | 5 | U | `unknown_direction_no_flow` |
| 0 | 5 | C | `hold_status_directional` |
| 0 | 5 | D | `hold_status_directional` |
| 0 | 30 | C | `hold_status_directional` |
| 0 | 31 | C | `hold_unexpected_flow` |
| 0 | 30 | D | `hold_status_directional` |
| 0 | 31 | D | `hold_unexpected_flow` |
| 0 | 30 | U | `unknown_direction_no_flow` |
| 0 | 31 | U | `unknown_direction_flow` |

**Ramp, pause and verification timing.** Direction changes ramp linearly over `45 s`; commands
with `|requested_w|≤30 W` are holds. One timing rule governs all outcomes: evidence collected
before the ramp ends is provisional and can never set `verified=true`; only two fresh reports after
the ramp window confirm. A `120 s` pause starts at the first confirmed partial-flow receipt;
release requires unchanged valid target, `pause_until` passed and two post-ramp `full_flow`
receipts under the same ordered predicates. If the pause expires with flow still classified
`partial_flow`, classify
`persistent_partial`, retry twice at 120 s intervals, then escalate and write a fallback hold.
A changed target, reserve protection or safety stop interrupts and restarts the pause.

**Acceptance.** S24–S32, S41, S43, S81–S92, S128–S131, S151, S163, S166 and S170 are the objective
tests. S151/S163 are clock-driven incident replays; S170 is the flow-predicate truth table; S86
echo-only has null applied power; S87 no-flow after two post-ramp reports.

---

## M5 — Safety ladder

### CURRENT

- **Given** a request, **when** `safety::check` runs, **then** order is authority gate → pack
  available/fresh/state → SoC band → plan tracking → pack power → grid import/export → per-pack
  ramp (1000 W/tick) → sell floor → solar ceiling → cell latch; all bounds are collected as
  `[lo,hi]`, and a conflict is a named block
  (`safety/check.rs:13-38`, `safety/ladder.rs:83-96`, `safety/finish.rs:8-50`, `safety/mod.rs:26-35`).
- **Given** a forced charge latch, **when** the ladder starts, **then** the target is
  `max(requested_w, force_charge_w)` (default 800 W) with reason
  `cell_voltage_emergency_charge` (`safety/ladder.rs:43-56`, `safety/finish.rs:181-191`).
- **Given** a blocked-discharge latch, **when** dispatch plans the pack, **then** 0.15.9 bypasses
  write backoff for the neutral hold and the emergency charge, mode-gated so `dry_run` writes
  nothing (`0.15.9:crates/energy-core/src/dispatch/pack_context.rs:76-97`,
  `dispatcher.rs:189-213,223-231`).
- **Given** a measured import guard, **when** `charge_room` would be negative, **then** the ladder
  relaxes it to 0 only when `Measured::grid_ceiling_armed` is true
  (`0.15.9:crates/energy-core/src/safety/ladder.rs:167-218`, `safety/measured.rs:23-30`).
- **Given** an import breach and a latched cell or empty pack, **when** bounds are combined,
  **then** `lo > hi` blocks and names every binding reason; the guard cannot override the SoC floor
  or the cell latch (`safety/finish.rs:20-95`).
- **Given** missing meter/forecast base with a configured grid cap, **when** the ladder runs,
  **then** the whole pack blocks with `grid_telemetry_missing`
  (`safety/ladder.rs:177-216`, `safety/finish.rs:12-19`).

### INTENDED contract

**Execution precedence** (execution order, not merely report order):

1. cell discharge latch → verified neutral hold; no authority, plan, ramp, backoff or import gate
   may delay it;
2. cell emergency charge (min cell `<3.00 V`, resume `≥3.18 V`, 180 s stale; shipped defaults in
   `0.15.9:crates/energy-battery/src/protection/cell_config.rs:24-58`, state machine at
   `0.15.9:crates/energy-battery/src/protection/cell_guard.rs:19-21,115-117`) → target `max(800 W, requested_w)`;
3. import-ceiling remedy → protective discharge;
4. export/self-consumption hard guard;
5. orphan hold, cancel ramp, restart grace, no-command-standing transitions;
6. ordinary economic ladder.

In `auto`, cell actions ignore plan staleness, grid telemetry, import ceiling, solar headroom, sell
floor, ramp and ordinary fault backoff; in `dry_run_latched` and `clearing` the M6 write-authority
gate dominates. **Universal physical limits bind every safety action, including
import remedies:** per-pack `max_charge_w`/`max_discharge_w`, temperature range, SoC ceiling and
`max(reserve, pack minSoc)` floor, cell-voltage latch, and capacity-based current limits. If any
prevents a safety action, emit the typed block (`emergency_charge_impossible` or
`import_remedy_blocked`) naming every binding limit; never relax physics. `lo > hi` always emits all
binding reasons. When cell hold and import remedy conflict with the SoC floor, the floor wins and
both reasons appear in precedence order.

**Safety write lane, fencing and attempt deadlines.** Safety actions carry `safety_seq`
(one command), `safety_attempt_id` (fresh for every retry) and their own receipt state.
One write at a time is enforced. Preemption requires a **confirmed** cancellation or an
**ordered transport barrier** that serialises writes and drops queued/retry economic writes; a
send-cancel alone does not free the lane. Both proofs carry a `fence_id` that supersedes all older
economic writes. If neither cancellation confirmation nor barrier is established within `2 s`,
emit `safety_preemption_unverified`, retain lane ownership, block any economic retry, and proceed
only on the separate ordered safety channel with the fence; a delayed old economic write reaching
the device is fenced out. If no such channel exists, emit `safety_write_unverified` with
`escalation=ownership_retention` and keep the pack owned; never report neutral.

Timing uses a bounded total attempt deadline `t_issue + 2 s ack + 60 s max ramp + 12 s receipt
= t_issue + 74 s`. Acknowledgement must arrive by `t_issue+2 s` (`ack_timeout` otherwise).
Ramp completion must be evidenced by `t_issue+62 s`; missing evidence is `ramp_end_unverified`
and consumes the receipt window. Receipt deadline is `confirmed_ramp_end+12 s`, never later than
`t_issue+74 s`. Retries at 0/2/6 s after a failed attempt each use a new `safety_attempt_id`;
only a receipt matching **both** `safety_seq` and the current `safety_attempt_id` advances state.
Late completion from an older attempt is `late_uncorrelated` and cannot validate neutrality.
After three failures emit `safety_write_unverified` with `escalation=ownership_retention`.

**Write-authority latch — single source of truth (M5/M6/API).** This subsection is the sole
owner of dry-run authority; M5 safety writes, M6 mode handling and all API entry points reference
it and must not restate a conflicting rule.

The authoritative invariant is: **while in `dry_run_latched` the engine writes nothing, ever; the
only state permitting any write without economic authority is `clearing`.** `clearing` is entered
solely by a logged operator command and is limited to safety-lane neutralisation.

Operational states are distinct and ordered: `dry_run_latched` → `clearing` → `auto`.
`dry_run_latched` reports `mode=dry_run_latched`, `protection_inactive=true`,
`safetyActive=false`, `writesEnabled=false`, `degraded=true`, reason `persisted_dry_run`; plans,
proposals and diagnostics are computed, but **device** writes — including holds and safety writes —
have count zero. Control-plane writes follow M6. The engine never sets `dry_run` and never enters `clearing` on its own;
API requests to enter `dry_run` are rejected with `dry_run_not_operational`.

`clearing` requires a logged operator command containing `actor`, `at` and `clearance_id`, plus a
health check. Its **only** permitted writes are safety-lane neutralisation writes carrying a
`fence_id`; economic, maintenance and planning write code paths are unavailable in this state,
not merely flag-disabled. `clearing` is bounded to **120 s** and terminal. The transition to
`clearing` happens **first**, before any write:

1. Validate the logged command and persist `state=clearing` with `clearance_id`; start the 120 s
   window. A missing field refuses with `clearance_authorization_invalid`, no state change and zero
   writes. In `dry_run_latched` no write call exists on any path.
2. Create a durable **clearance-entry barrier** at the instant `clearing` is entered. Its identity
   is `clearance_entry_barrier_id = clearance_id + ":" + generation`, where `generation` is a
   monotonically increasing per-pack integer stored in `write_authority_latch` before any telemetry
   read is used as evidence. Record `barrier[pack_id] = generation` and `barrier_at` for **every**
   pack before any report is accepted.
3. Require a fresh plan (`generated_at ≤900 s`). For **every** pack, obtain two fresh `(0,0)`
   reports with `observed_at` within 30 s, correct status, no standing-command echo, and both
   carrying `barrier_generation = barrier[pack_id]`. Reports older than the barrier, missing the
   generation, or carrying a different generation are discarded as `stale_pre_barrier_evidence` and
   cannot be counted. These two reports are mandatory whether or not a hold write is needed.
4. For a pack without that evidence, issue a fenced 0 W safety-lane neutral hold; retry at 0/2/6 s
   with a fresh `fence_id` each time. The hold's `fence_id` invalidates older device writes but does
   not replace the clearance-entry barrier; post-hold reports must still cite `barrier[pack_id]`
   plus the hold `fence_id`. No other write path exists in `clearing`.
5. Success: every pack has two fresh post-barrier `(0,0)` reports matching its generation → commit
   the durable latch to `auto`, emit `dry_run_latch_cleared{clearance_id}`, enter `auto`, and only
   then allow economic dispatch.
6. Failure or timeout at 120 s: return to `dry_run_latched`, emit
   `dry_run_clearance_failed{reason}` with no partial clearance and zero further writes; a new
   operator command is required for another bounded attempt.

Durability and bypass prevention:

- The latch is an independent durable row `write_authority_latch{latch_generation, latch_token,
  state, created_at, last_transition}`. It is written before every settings/ledger write and takes
  precedence on every write of those rows; `auto` cannot be persisted while the latch says
  `dry_run_latched`.
- A manual persisted-state edit that removes or alters the latch token/generation is detected;
  authoritative writes are refused with `latch_token_invalid` and the engine runs as
  `dry_run_latched` until operator recovery. Unexplained generations are never trusted.
- Exactly one authority gate decides write permission for **all** entry points: mode endpoints,
  legacy aliases, scheduler, watchdog, internal recovery and generic `ws call` passthrough. Every
  path calls the same gate; no handler may bypass it.
- No path — startup, mode migration, audit expiry, config reload, API request, generic ws
  passthrough, schedule, watchdog restart or internal recovery — can clear or overwrite the latch.
  Each emits `dry_run_latch_preserved{path}` and remains write-free.

**First-class transitions.**

| Transition | Required action | Precedence / release |
|---|---|---|
| `orphan_hold` (ownership lost) | explicit verified `0 W` when reachable | below cell/import guards; owns the pack until a fresh ordered report confirms neutral |
| `cancel_ramp` | explicit verified `0 W` if a nonzero command is standing | preempts ordinary plan; releases only after hold confirmation |
| `restart_grace` (120 s after `Restarted`) | no ordinary writes; ordered neutral evidence: (1) two fresh post-restart reports with `|applied_w|≤30 W` **and** no standing-command echo; (2) if either is missing or nonzero, explicit safety-lane `0 W` write verified by two fresh reports; (3) if still unverified at 120 s, `restart_neutral_unverified` and retain ownership | cell/import actions bypass immediately; ordinary writes resume only after grace expiry **and** neutral evidence |
| `no_command_standing` | `idle_verified` only after two fresh reports show `|applied_w|≤30 W` and no standing-command echo | justified hold, not a failure; any later nonzero plan goes through normal selection |

Ordinary backoff, stale plan, missing grid telemetry or `protection_active` never suppress these
transitions. An orphan hold failure is a `TickOutcome` failure for that pack, not a silent `Ok`.

**Acceptance.** S33–S42, S93–S99, S132–S137, S150, S159, S160, S165 and S169 are the objective
tests. S132 asserts dry_run no-write; S133 asserts universal physical limits on import remedy; S134
neutral evidence; S135 timeouts and late-receipt correlation; S159 fences a late economic write;
S160 keeps a 0 W hold when discharge is impossible; S165 asserts sink calls.

---

## M6 — Replanning and mode handling

### CURRENT

- **Given** a stored plan, **when** triggers check reasons, **then** priority is `NoPlan` → stale
  (`now − generated_at > max(2×replan, stale_plan)`) → expired → price changed → forecast changed →
  SoC deviation ≥3 % → slot boundary; staleness wins over a crossed boundary
  (`replan/trigger.rs:32-81`, `replan/mod.rs:13-18`, `engine/config.rs:73-79`).
- **Given** a missing/expired/stale plan, **when** the tick prepares, **then** it holds instead of
  keeping the last setpoint; other triggers replan and dispatch in the same tick
  (`engine/tick.rs:60-85,117-137`).
- **Given** a refused solve, **when** the reason is not staleness, **then** `main` logs and continues
  using the previous plan (`engine/tick.rs:88-115`). 0.15.9 holds and uses a refusal backoff of
  30 s (`0.15.9:crates/energy-core/src/engine/tick.rs:96-118`,
  `0.15.9:crates/energy-core/src/engine/refusal_backoff.rs:7-19`).
- **Given** `control.mode`, **when** the engine loads it, **then** the value is persisted; unknown
  values fall back to `dry_run`, and the default is `dry_run` (`mode.rs:3-31`,
  `safety/authority.rs:9-25`, `engine/config.rs:50-70`).
- **Given** `PATCH /api/v1/mode` with `advisory`, `dry_run` or `auto`, **when** handled, **then** it
  persists first, queues `SetMode`, and returns 200 with `engineNotified = true`
  (`0.15.9:crates/energy-server/src/api/mode.rs:28-69`).
- **Given** a manual relearn or replan command, **when** `Engine::commands` handles it, **then** a
  failed replan is explicitly discarded with `let _ = self.replan_now(now).await`
  (`0.15.9:crates/energy-core/src/engine/commands.rs:70`).

### INTENDED contract

**Operational mode state machine.** M6 consumes the M5 write-authority state machine and does not
redefine it. The only write-authority states are `dry_run_latched`, `clearing` and `auto`.
Operational sub-states inside `auto` are `auto` (economic + safety writes) and
`protection_active` (economic writes suppressed, M5 safety active); `protection_active_invalid_config`
is a guarded variant that never expires into `auto`. The write state takes precedence over all mode
persistence: on every settings/ledger write, including startup, the durable
`write_authority_latch{latch_generation,latch_token,state}` row is read first and an `auto` mode row
cannot overwrite `dry_run_latched` or `clearing`. A missing or unexplained token/generation refuses
authoritative writes with `latch_token_invalid` and runs as `dry_run_latched`.

**Single authority gate.** Exactly one function decides write permission for every entry point:
mode endpoints, legacy aliases, generic `ws call` passthrough, schedules, watchdog, internal
recovery, tick dispatch, maintenance and learning persistence. Its answer is derived from the M5
state only:

| M5 state | **Device-only** gate answer |
|---|---|
| `dry_run_latched` | deny every **device** write, including safety and holds |
| `clearing` | allow only safety-lane neutralisation **device** writes carrying a **fresh** `fence_id` |
| `auto` | allow economic **device** writes, subject to normal operational rules |

This table is **DEVICE-ONLY** and applies only when authority is initialized. The bootstrap and
recovery exceptions are defined separately below.

**Two disjoint write classes pass through this gate once authority is initialized.**

- **Control-plane writes** — authority token/generation, operator journal, ledger events, attempt
  outcomes/checkpoints and authority transitions — are permitted in **every** initialized state
  (`dry_run_latched`, `clearing`, `auto`) under the same gate when required to journal authority,
  clearance or operational results. Domain persistence belonging to learning, maintenance,
  journal/dashboard mutation and non-authority attempt outcomes is permitted only in `auto`;
  while `dry_run_latched` or `clearing` those requests return a runtime-only
  `persistence_denied{state}` with no durable mutation and no false success.
- **Device writes** — anything sent to a pack, including holds and safety writes — obey the state
  rules in the table above.

**Indirect device commands are forbidden.** No journal entry, ledger event, checkpoint, dashboard
mutation or control-plane record may contain a device command or setpoint; command-bearing
control-plane mutations are prohibited. Every device write — including queued, scheduled, retried
and replayed work — must pass the device gate **at execution time**, not when enqueued. A queued
command survives a state change only as an **intent**, never as an authorisation; if authority has
changed, the intent is discarded with `intent_not_authorized` and no device write occurs.

No other code path may perform either class. Adding a write path that does not call this gate is a
design error. Clearance ordering is: (1) journal the operator entry and persist the per-pack
barrier generation before any device read; (2) journal each barrier result; (3) commit authority
`auto` only after every pack is neutral, in **one durable transaction with the journal entry**.
A restart never rewrites authority to `auto`; it only rewrites non-authoritative operational mode
and does not persist the diagnostic session. API replies carry `mode`,
`diagnosticSessionExpiresAt` (null in `auto`/invalid-config), `safetyActive=true` only for
`auto`/`protection_active`, `false` in `dry_run_latched`/`clearing`, and a typed 4xx for `dry_run`
operational requests.

**Health/dashboard protection text by state.** The typed codes must be accompanied by visible,
prominent human-readable text in the dashboard and the **same text** in the API health payload:

- `dry_run_latched`: codes `protection=none,battery_unmanaged=true,warning=no_protection_writes_forbidden`;
  text **"Battery unmanaged — protection writes are disabled. The battery is not being protected."**
- `clearing`: codes `protection=neutralisation_only,battery_unmanaged=true,warning=clearance_in_progress`;
  text **"Clearance in progress — neutralisation only. Full protection has not resumed."**
- `authority_recovery_required`: codes `protection=none,battery_unmanaged=true,warning=latch_token_invalid`;
  text **"Authority recovery required — protection writes are disabled. An operator must restore or
  reinitialise the latch token."**
- `auto` (including `protection_active`): codes `protection=active,battery_unmanaged=false`;
  text **"Battery protection active."**

Full safety precedence applies only in `auto`; neutralisation is the only protection in `clearing`,
and no independent protection remains in `dry_run_latched`.

Clearance health is not a service-level `healthy` flag: it requires a fresh `(0,0)` report from
**every** pack inside M5's bounded 120 s `clearing` window, with no standing-command echo. An
`advisory` API request is accepted as a runtime-only `protection_active` session with
`expires_at = start + 900 s`, a visible countdown and an event; on expiry the engine returns to
`auto` only if configuration is valid, a fresh solve is installed and two consecutive ticks are
healthy. Unknown persisted mode is handled **only** by the bootstrap rule below as
`unknown_mode_migrated_to_auto`; no install may silently rename an unknown value to
`protection_active` or gain authority without an audited token. Invalid
configuration → `protection_active_invalid_config` with bounded safe inputs (below), no economic
plan, safety lane active; it never expires into `auto`, survives restart, and returns only through
the guarded transition above.

**Authority phases and permitted mutations.** There are two distinct notions; do not conflate
them.

1. **`AUTHORITY_UNINITIALIZED` / `AUTHORITY_INVALID`** — no valid token. Only the narrow
   bootstrap/recovery transaction may write, and it may write **nothing except** the authority
   token, its generation and the matching initialization or recovery event. Authority journals,
   clearance mutations, attempt outcomes, domain persistence and all device writes are forbidden.
   The transaction scope is **non-extendable**; no handler may add fields, side effects or queued
   work to it; this is enforced by the sealed transaction type below, not by prose.
2. **`AUTHORITY_INITIALIZED`** — a valid token and matching event exist. The M5 state machine
   applies: control-plane writes are permitted in every state as specified above, and device writes
   follow the device-only table.

**Token bootstrap and crash recovery.** Authority is never inferred from a persisted mode without
an audit record. The token is authoritative only when its generation is present, well-formed and
explained by an initialization event. The device gate stays closed throughout bootstrap. It opens
only after the sealed token/event transaction, durable anchor creation and a successful startup
reconciliation have all completed.

Legacy bootstrap requires a **three-condition provenance test**, all true:

1. **schema-old:** `schema_version < WRITE_AUTHORITY_SCHEMA`;
2. **journal-complete:** the journal integrity check below returns `complete=true`;
3. **consistent:** no `latch_token_initialized`, `authority_transition`, `clearance_*` or other
   latch/clearance event anywhere in the verified journal.

Only a ledger satisfying all three may take the legacy path. Provenance that cannot be established
is **AMBIGUOUS** and goes to `authority_recovery_required` with a logged operator decision. A
restored backup, a migrated database or any ledger whose history cannot be proven complete is
expected to land in recovery, **not** in silent generation-1 auto bootstrap. A missing or unreadable
token on a ledger showing any latch/clearance event is never legacy migration.

**Authority anchor, checkpoint and journal completeness.** The authority anchor lives **outside
the ledger** in a separate file in the add-on data directory (same filesystem, separate file),
holding `generation`, `token_hash`, `committed_head`, `pending_head`,
`journal_hash_chain_head`, the authority `checkpoint_event_id` and `last_startup_resolution_ref`. The checkpoint row is owned by the authority module. Every relevant
append advances the checkpoint **atomically in the same durable transaction as the event**, so no
durable event exists without a matching advanced checkpoint. The ordinary append API has no
capability to write, lower or replace the checkpoint; such a request returns
`checkpoint_write_denied`.

Startup verification starts at the stored checkpoint and requires a contiguous, hash-chained set of
events from there to the current head. A truncated tail, missing range, stale checkpoint, unreadable
journal, missing anchor or unverifiable chain returns `journal_incomplete` or `anchor_mismatch`;
the result is `authority_recovery_required`, never automatic bootstrap.

**Anchor lifecycle.** The anchor is a separate file replaced atomically (temp file, `fsync`, rename,
`fsync` directory) at each lifecycle step; no torn file is ever observable.

- **Bootstrap:** after the sealed ledger transaction commits the token and initialization event,
  create the anchor once with `generation=1`, `token_hash`, `committed_head=event_id`,
  `pending_head=0`, the hash-chain head, the checkpoint and `last_startup_resolution_ref` from the
  single Exact ordering above. A crash between the ledger commit
  and anchor creation leaves no anchor and fails closed to `authority_recovery_required`; the
  bootstrap transaction is never guessed to have succeeded.
- **Every append:** (1) atomically write `pending_head=next_event_id`; (2) commit the ledger event
  and checkpoint atomically; (3) atomically promote `committed_head=next_event_id` and clear
  `pending_head`.
- **Crash recovery:** if ledger head equals `pending_head`, adopt it and promote `committed_head`;
  if ledger head equals `committed_head` while `pending_head` is newer, the prepare was never
  committed — discard `pending_head` and record `reconciled`; if ledger head is ahead of
  `committed_head` without a matching `pending_head`, or any other combination occurs, fail closed
  to `authority_recovery_required`. Never guess authority from an unmatched anchor.

- **Fresh install** (no ledger, no token): create token generation `1`, authority `auto`, and log
  `latch_token_initialized{reason:fresh_install}`; durable commit completes before any economic
  write.
- **Legacy upgrade, persisted mode `auto`**: create token generation `1`, authority `auto`, log
  `latch_token_initialized{reason:legacy_auto_migration,previous_mode:auto,source_release:<release>}`;
  commit before any write.
- **Legacy upgrade, persisted mode `dry_run`**: create token generation `1`, authority
  `dry_run_latched`, log `latch_token_initialized{reason:legacy_dry_run_preserved,previous_mode:dry_run,source_release:<release>}`,
  report the loud typed write-free state, and remain latched until the operator clears it.
- **Legacy upgrade, persisted mode `advisory`**: create token generation `1`, authority `auto`, log
  `latch_token_initialized{reason:legacy_advisory_migrated_to_auto,previous_mode:advisory,source_release:<release>}`,
  and show a prominent warning that advisory was compute-only and has been migrated to `auto`; the
  owner forbids parking in advisory.
- **Legacy upgrade, unknown or missing mode on an existing ledger**: create token generation `1`,
  authority `auto`, log `latch_token_initialized{reason:unknown_mode_migrated_to_auto,previous_mode:<value-or-missing>,source_release:<release>}`,
  and show a prominent warning naming the exact value found or `missing`. No silent rename to
  `protection_active` is permitted.
- **Missing/unreadable/malformed token or unexplained generation on a post-token ledger**: enter
  the explicit `authority_recovery_required` state; refuse all writes except the narrow recovery
  transaction with `latch_token_invalid`; show the prominent warning
  **"Authority recovery required — protection writes are disabled. An operator must restore or
  reinitialise the latch token."** Recovery requires a logged operator action that verifies the
  ledger event for that generation, restores the last valid committed token from backup, or creates
  a new initialization/recovery event with an explicit reason. Silent repair or mode-based
  migration is forbidden.

The former standalone bootstrap ordering list is removed. The **Exact ordering (single source of
truth)** for bootstrap and every later start is defined once in the startup-resolution durability
section below; no second ordering exists in this module.

**Sealed bootstrap transaction interface.** The transaction type exposes only these capabilities:

```text
BootstrapAuthorityTx {
  write_token(token: LatchToken, generation: u64) -> Result<(), BootstrapTxError>;
  write_event(event: InitializationOrRecoveryEvent) -> Result<(), BootstrapTxError>;
  commit(self) -> Result<(), BootstrapTxError>;
}
```

It exposes **no** general ledger append, device-send, queue, scheduler, settings or maintenance
capability. `LatchToken` and `InitializationOrRecoveryEvent` are narrowly typed structs; no free
form payload or command field exists. An attempt to send a device write, append an unrelated journal
entry, enqueue work or mutate settings inside this transaction must fail at compile time; where the
language cannot enforce it, the runtime rejects it with `bootstrap_tx_capability_denied`. Negative
tests must prove both the device-write and extra-journal-append attempts are rejected. The type is
non-extendable: adding a capability requires a schema/API version change and review.

**Universal startup resolution.** Every startup resolves to exactly one of two cases:

| Resolution | Condition | Dispatch eligibility |
|---|---|---|
| `reconciled` | anchor and ledger reconcile; includes clean restart, crash, power loss, prepare-abort and coordinated snapshot restore | allowed with valid `auto` authority |
| `recovery_required` | anchor missing/mismatched, journal incomplete, startup-resolution record lost or authority invalid | no dispatch |

**This deployment cannot prove continuity across a restart; startup is conservative by design and
always reports it.** Every startup writes exactly one startup-resolution record and raises the
dashboard banner **"Engine restarted - state continuity not provable; verify mode and protection"**,
including in `reconciled`. There is no `continuity_proven` category, no separate restore path and no
silent resume.

**Startup-resolution record.** A narrowly permitted append-only diagnostic record outside the
authority journal, at `data/startup_resolution.jsonl`, is writable in **every** authority phase,
including `AUTHORITY_UNINITIALIZED`, `AUTHORITY_INVALID` and incomplete-journal states. It contains
only `case`, `at`, `anchor_hash`, `checkpoint_hash`, `journal_head_ref` and a monotonic record id. It
has **no** capability to change authority, journal head, checkpoint, domain state or device commands;
it is not part of the authority journal. The anchor stores `last_startup_resolution_ref`; if that
referenced record is missing or corrupt on the next startup, the result is `recovery_required`.

**Exact ordering (single source of truth).** Bootstrap and every later start follow this one list:

0. Read persisted mode, ledger, token, source release and the separate anchor; verify journal
   integrity/completeness from the checkpoint and apply the three-condition provenance test; decide
   authority by the rules above. For bootstrap, run the sealed token/event transaction and commit
   it first.
1. Append the resolution record to `data/startup_resolution.jsonl`, `fsync` the file, and, when
   the file is first created, `fsync` the parent `data/` directory so its directory entry is
   durable. **Chosen option:** parent-directory fsync for each first creation, not reliance on a
   shared single directory fsync. This covers create/rename of the record and anchor entries.
2. Prepare `pending_startup_ref`. For an existing anchor, install it with the same two-phase
   discipline as the journal head: atomic pending write, then atomic promotion of
   `last_startup_resolution_ref` and clearing of pending.
3. **First-bootstrap exception:** no prior anchor exists to update. After the record and directory
   are durable, prepare the ref, then create the anchor in **one atomic creation transaction** whose
   initial content already carries `last_startup_resolution_ref`. There is never a two-step anchor
   creation.
4. Only after record, file fsync, directory fsync and anchor ref are durable does reconciliation
   complete, the device gate open and dispatch become eligible.
5. Persistence failure of either side leaves the engine in `recovery_required` with runtime-only
   reporting and zero device writes. Typed reasons: `resolution_record_storage_full` (disk full),
   `resolution_record_read_only` (read-only filesystem), `resolution_record_fsync_failed` (file
   fsync error), `resolution_dir_fsync_failed` (parent-directory fsync error),
   `resolution_record_corrupt` (corrupt append/readback), `anchor_ref_pending_failed` (pending-ref
   atomic write failed) and `anchor_ref_promotion_failed` (promotion failed). A crash before the
   sealed commit leaves neither token nor event and bootstrap re-runs; a crash after commit but
   before anchor creation fails closed to `authority_recovery_required`.

**Crash windows.** A crash after the record but before its reference leaves an orphan record; the
next startup detects that no anchor ref matches it and reports it with `anchor_mismatch` — the orphan
is never adopted as the current resolution. A crash during anchor promotion follows the anchor
lifecycle: if the ledger/record state matches the pending ref, promote it; otherwise fail closed to
`recovery_required`. A crash after both are durable but before the gate opens simply repeats the
resolution on the next startup.

**Restore/reconcile preserves authority.** The startup resolution never changes authority. Economic
dispatch is permitted only when the restored/initialized authority is already validly `auto` **and**
the resolution is `reconciled`. A snapshot containing `dry_run_latched` stays latched until the full
clearance sequence completes: logged operator command, barrier, per-pack neutralisation and two
fresh post-barrier `(0,0)` reports. A restore event can never substitute for operator clearance or
neutralisation.

**Bypass tests are mandatory for:** startup, mode migration, audit expiry, config reload, API
request, generic ws passthrough, schedule, watchdog restart and manual persisted-state edit; each
must observe the gate decision, not a handler-local flag.

**Invalid-config input freshness.** While `protection_active_invalid_config`, each safe input has
a hard age limit: SoC/power ≤ 120 s, grid import ≤ 30 s, cell voltage ≤ 180 s (the shipped guard
staleness), forecast ≤ 900 s; each row carries `observed_at` and `age_s`. If no valid snapshot
exists, inputs are `unknown` and the plan is not executable. With fresh inputs, neutral holds,
cell emergency charge and import-ceiling correction are permitted within universal physical limits.
Without fresh SoC or pack power, nonzero safety actions refuse with `safety_inputs_unavailable`;
without fresh grid import, import-ceiling remedies refuse with `grid_telemetry_missing`; without
fresh cell voltage, cell emergency charge refuses with `cell_voltage_unknown` while blocked-discharge
latch actions continue from latched state. 0 W neutral holds are permitted from known latched state
even when numeric telemetry is stale. A cold start with never-valid config and no snapshot refuses
all nonzero safety actions and emits `no_safe_inputs_snapshot`; it still attempts 0 W holds when a
latch exists.

**Replan cadence and recovery.** The guaranteed deadline is an **attempt** every `900 s`, not a
successful solve: additional attempt triggers are slot boundary, price change, forecast refresh,
SoC deviation ≥3 points, `NoPlan`, or `InputsUnavailable`/`UnknownRegime`/poison. Solve success is
conditional on inputs. When `t_due = generated_at + 900 s` arrives, the replacement solve starts;
the current plan stays executable until a replacement is installed or `t_due + 5 s`, whichever comes
first. If the solve is still pending after `5 s`, hold with reason `solve_pending` and `retry_at`;
never let plan eligibility end exactly at the replacement due time without this ordering. A plan is
also stale when `now − generated_at > 1800 s`, but that bound only refuses an already held plan.
First refusal retries after `max(30 s, replan/10) = 90 s`; subsequent refusals `90, 180, 300, … s`.
After three refusals emit `inputs_unavailable_persistent`; attempts continue without new input.
Success resets the backoff and holds carry the reason and `retry_at`.

**Plan-source precedence.** Only a freshly solved `LivePlanSource` result with
`status ∈ {healthy, degraded_accepted}`, `eligible = true`, `generated_at` no older than
`900 s + 5 s` during a pending replacement solve, and a successful ledger persist is executable. Stored/previous plans are archival evidence only. On any
failed replan, immediately set the previous plan's `eligible_until = failure_time` and record
`rejected_generation`; dispatch holds economically until a successful replacement is installed.
No explicit fallback to a previous plan is permitted.

**Change detection and lock failures.** A failed read never consumes a change marker. Bounded
contention: three attempts over 3 s, then `InputsUnavailable`. Poison recovery rebuilds the source
once and emits `plan_source_poison_recovered`; until then the tick holds. After recovery, force a
fresh solve before any dispatch, even if the marker was seen during the failed read.

**Command-triggered replans.** Manual/tick replans share one policy. The command reply carries
`requested`, `attempted`, `outcome ∈ {installed, refused, failed}`, `reason[]`, `retry_at` and
`plan_generation`. A refusal/failure invalidates the previous plan and schedules the same retry
ladder; `let _ =` and `202 {"accepted":true}` without a result are forbidden.

**Acceptance.** S44–S48, S100–S106, S136–S137, S157 and S163 are the objective tests. S44 expects
`protection_active` with safety active; S48 expects expiry into `auto` only after a valid guarded
transition and rejects operational `dry_run`; S104 asserts a stored plan is never dispatched after
a failed replan.

---

## M7 — Learning and maintenance

### CURRENT

- **Given** completed sessions, **when** extraction runs, **then** it re-extracts
  `max(4×check_seconds, 3600) s`, reads a 720 h window, builds a same-label capacity pool, and
  relearns only packs with a completed session in the last `2×check_seconds`; other packs are
  skipped silently while `last_session_check` advances
  (`0.15.9:crates/energy-core/src/wiring/sessions.rs:13-49,76-149`,
  `0.15.9:crates/energy-core/src/engine/tick.rs:188-200`).
- **Given** invalidated data, **when** it is mapped for the learner, **then** it is kept with a
  refusal marker so a direction change cannot be erased (`0.15.9:crates/energy-core/src/wiring/sessions.rs:172-193`,
  `0.15.9:crates/energy-battery/src/learning/report.rs:162-183`).
- **Given** capacity samples, **when** learning runs, **then** 0.15.9 uses ≥3 samples, MAD 3.5σ,
  weighted median, confidence from span, shrinkage to a label/peer prior at ≥0.20 confidence,
  and an aging band at ≥0.80 confidence; the pool includes all packs with that label, including
  the pack being learned (`0.15.9:crates/energy-battery/src/learning/capacity.rs:9-20,67-119,152-180`,
  `sample.rs:7-74`, `pool.rs:37-99`, `bounds.rs:9-15`).
- **Given** charge/discharge blocks, **when** efficiency is learned, **then** a charge is paired
  with a later discharge within 7 days, SoC closes within 15 %/2 points, ≥3 pairs, effective pairs,
  MAD ≤0.02 and confidence ≥0.30 are required, then `round_trip` is split into equal legs
  (`0.15.9:crates/energy-battery/src/learning/efficiency.rs:10-58,109-177`,
  `pair_diagnostics.rs:12-57`, `session.rs:8-12`, `blocks.rs:65-117`).
- **Given** a blocked efficiency pass, **when** the profile is saved, **then** 0.15.9 can keep a
  previous confident profile but stamps `updated_at = now`
  (`0.15.9:crates/energy-battery/src/learning/profile.rs:62-86`,
  `efficiency.rs:206-227`).
- **Given** `POST /api/v1/learning {"action":"relearn"}`, **when** accepted, **then** the API only
  queues an engine command and returns `202 {"accepted":true}`
  (`0.15.9:crates/energy-server/src/api/learning.rs:38-63`).
- **Given** full-charge policy, **when** maintenance plans a run, **then** 0.15.9 uses a 7-day
  interval, 100 % target, 60 min dwell, cheapest slots before 07:00 local and import headroom; a
  partial fill gets `missingKwh` and `partialReason`, no dwell and no completion stamp
  (`0.15.9:crates/energy-battery/src/protection/full_charge/settings.rs:23-87`,
  `0.15.9:crates/energy-core/src/maintenance/candidates.rs:12-35`, `apply.rs:21-45`,
  `progress.rs:25-57,129-167`, `assignment.rs:34-41`).

### INTENDED contract — capacity numerics

Input session eligibility uses this **reason precedence** (first match wins):
(0) `invalid_sample` for any **present** numeric field that is non-finite or physically invalid:
`energy_ac_kwh < 0`, `charge_ac_kwh < 0`, `discharge_ac_kwh < 0`, `duration_s ≤ 0`, SoC endpoints
outside `[0,100]`, `sample_count < 0`, `soc_span_pct < 0`, and discharge energy above charge energy
where the recorded closed cycle forbids it. This is a refusal barrier that splits blocks.
(1) `idle_blip` when both `energy_ac_kwh` and `duration_s` are present and valid and either
`energy_ac_kwh < 0.15 kWh` (including exactly `0 kWh`) or
`avg_power_w = energy_ac_kwh×1000/(duration_s/3600) < 50 W`, even if `complete=false`; (2)
`session_incomplete_pending_measurement` when those provisional fields are absent; (3)
`session_incomplete` when `complete=false` and not a micro-session; (4) `invalid_sample` for a
complete record with `soc_span_pct = 0` or `sample_count < 20`; (5) `session_short` for a complete
record below block/span thresholds. Zero energy with valid duration is intentionally `idle_blip`:
it is a genuine idle period, not corruption. A corrupt negative-energy or negative-duration record
is a refusal barrier; a valid record with `energy_ac_kwh=0.0026 kWh` and `duration_s=900 s` is
`idle_blip` and remains reopenable by a completion revision. S189 pins the deliberate zero-energy-is-idle choice.

For accepted samples with span `s ≥ 5` SoC points:

- `quality(s) = clamp((s − 5)/(50 − 5), 0, 1)`;
- `weight = 0.5^(rank/3) × quality(s)` (rank 0 = newest for that pack);
- charge `C_implied = I × x / (s/100)` kWh; discharge fallback `C_implied = O / x / (s/100)`
  only when fewer than three charge samples exist (`x = sqrt(round_trip)`);
- plausibility `[1.0, 200.0] kWh`, MAD limit `3.5σ`, minimum 3 inliers, weighted median;
- `confidence = 1 − exp(−Σ s×quality(s)/400)`;
- AB2000 label 1.92 kWh: ordinary band `[1.728, min(reported, 2.0352)]`; aging band
  `[1.6128, min(reported, 2.0352)]` only at `confidence ≥ 0.80`
  (`0.15.9:crates/energy-battery/src/learning/bounds.rs:9-15`).
- Publication requires `used ≥ 3` and `confidence ≥ 0.30`; below that, keep a confident previous
  value with `confidence_rejected`, otherwise publish nothing. An out-of-band posterior is
  **clamped** into the active band and reported `clamped_low`/`clamped_high`; `bounds_rejected`
  is reserved for non-finite or implausible `[1,200] kWh` raw samples, never for a clamp.
- Aging band opens only when `confidence ≥ 0.80` **and** at least one surviving sample spans
  `≥ 30` points; otherwise the ordinary band applies. Fresh long-span evidence can therefore
  move an aged estimate; no estimate is permanently frozen.
- Peer pool: same label keyed in milli-kWh, **excluding the pack being learned**, weighted median,
  confidence capped at `0.85`, usable as prior only when `confidence ≥ 0.20` **and** its newest
  contributing session ended within 30 days; otherwise fall back to the label prior. Posterior
  `prior + confidence×(raw − prior)`, then clamped. Source `Own` at confidence `≥ 0.50`.

### INTENDED contract — efficiency pairing

A **meaningful cycle** has a charge block and a discharge block:

- each block `energy_ac_kwh ≥ 0.20 kWh` and `soc_span_pct ≥ 10` points (integer-percent endpoints
  carry ±1 point each, so a 10-point span keeps the ratio error ≤20 %);
- gap from charge end to discharge start `≤ 7 days`;
- hold drift `≤ max(1 point, 10 % of the larger leg span)`;
- SoC closure `≤ max(2 points, 15 % of the span)`;
- implied capacity within `[1, 200] kWh`; **raw** round trip checked against `[0.50, 1.00]`
  only after the closure correction, before any clamp.
- Block joining is exact: for adjacent same-direction sessions, gap `b.started_at − a.ended_at
  ≤ 7 days`, SoC drift `|b.soc_start − a.soc_end| ≤ max(1 point, 10 % of max span)`, and progress
  in the block direction `≥ −2 points`; otherwise split. A 4 h 100 % hold with 0 W is not a
  session and never splits a block. An invalid session splits and marks the block unusable — a
  refusal barrier; a charge block can never be reused for two discharges.

Define `w_s = 0.5^(rank/3) × quality(s)` per session and block weight `W_b = Σ w_s`. A pair's
weight is `w_pair = (W_charge + W_discharge)/2`; effective pairs
`n_eff = (Σ w_pair)² / Σ w_pair²`; confidence `1 − exp(−n_eff/2)`.

**Closure correction (single energy balance).** `η_c` converts AC charge into stored energy;
`η_d` converts stored energy into AC output. For charge `a→b` and discharge `c→d`, let
`span_c=b−a`, `span_d=c−d`, `δ=span_c−span_d=(b−c)−(a−d)` SoC points, AC input `I`,
AC output `O`, and `C_eff` the learned capacity (fallback 1.92 kWh label). Then
`ΔE_c=span_c·C_eff/100`, `ΔE_d=span_d·C_eff/100=ΔE_c−δ·C_eff/100`. When `δ>0`, energy was
**retained** (`ΔE_d<ΔE_c`), so output is too low by the retained amount and the correction is
added; when `δ<0`, the discharge drew extra stored energy, so it is subtracted:
`η_rt_raw = (O + δ·C_eff/100)/I`. This replaces the previous wrong-sign
`(O − δ·C_eff/100)/I`. Worked examples with `C_eff=7.8575 kWh`, `I=0.8 kWh`, `O=0.65 kWh`:
`δ=+1 ⇒ (0.65+0.078575)/0.8=0.910719`; `δ=−1 ⇒ (0.65−0.078575)/0.8=0.714281`.
The exact equal-leg solve is `I·x² − (δ·C_eff/100)·x − O = 0`, `x=√η`, positive root
`x=(δ·C_eff/100+√((δ·C_eff/100)²+4·I·O))/(2I)`; the linearised estimate above is used for
publication and both forms are reported.

**Validity before clamping.** A candidate with `η_rt_raw < 0.50` or `> 1.00` is refused with
`eta_out_of_range{raw_value, lower:0.50, upper:1.00}`; a corrupted raw `0.35` is refused, never
published as `0.50`. The numerical MAD tolerance `0.02` is a dispersion check only and never
widens the physical bounds.

**Aggregation order.** (1) build structurally valid candidate pairs; (2) compute `η_rt_raw` and
reject out-of-range values with their reason; (3) compute `w_pair`, `n_eff` and confidence from
the **remaining** pairs; (4) take the weighted median of remaining `η_rt_raw` as `round_trip_raw`;
(5) compute weighted MAD and reject outliers with `|η−median| > 0.02`, naming each rejected pair;
(6) recompute `w_pair`, `n_eff`, confidence and the weighted median on inliers only; (7) publish
`round_trip = clamp(round_trip_inlier_median, 0.50, 1.00)` and `η_c=η_d=sqrt(round_trip)` only if
inlier `pairs ≥ 3`, `n_eff ≥ 3`, MAD `≤ 0.02`, confidence `≥ 0.30`; otherwise keep the previous
metric with `no_measurement` and a named blocker. Published counts/confidence describe the
**post-rejection inliers**. Capacity and efficiency eligibility are independent: one may publish
while the other is blocked; neither vetoes the other.

Live 0.15.9 diagnostic evidence that drives this contract: pack `EOC1NLN9N464770` reported
`sessionsSeen=891`, `sessionsValid=55`, `candidates=406`, `accepted=0`, `refused=406`,
`no_charge_before=22`, `session_incomplete=289`, `session_short=95`, `blocker=no_measurement`;
`EOC1NLN9N465216` reported 896/50/417/417 with 22/297/98; two further packs reported
912 and 940 sessions with the same shape. About 70 % of candidates were incomplete and 23 % short
because the engine had dispatched 0 W for days, leaving 0.003–0.1 kWh micro-sessions (e.g.
`dischargeKwh=0.0026`, `socFrom=16`, `socTo=16`). That is a data-shape consequence, not a guard
calibration problem. With no meaningful cycle, emit `no_meaningful_cycle`, `idle_blip_count` and
`no meaningful cycle was commanded`; the learner must never manufacture a number to look healthy.

### INTENDED contract — durable progress and outcomes

Verbatim reviewer requirement adopted as a contract: **durable extraction and per-pack consumption
checkpoints advanced only after results are persisted, idempotent and restart-safe.** The learning
schedule is independent of replan cadence: an attempt is due every `900 s`. Discovery is by
**durable event log**, not a time overlap: store `learning_sessions(session_id, pack_id,
started_at, ended_at|null, status open|complete, revision)` and
`completion_events(event_id, session_id, completed_at, revision)` plus `new_session_events`.
Extraction consumes new-session and completion events; an open session is registered when first
seen and re-emitted with its full original record when a completion revision arrives, regardless of
how much later it closes. Consumption order is `(completed_at, event_id)` for completion events
and `(started_at, session_id)` for never-completed sessions — never original `started_at` alone.
Per-pack `consumed_completion_ids` and global `extraction_event_cursor` are the checkpoints.
Exactly-once is keyed by `event_id`: an older session closing after newer sessions were consumed
is still consumed exactly once on its completion event. The required test registers sessions A
(start 01:00), B (02:00), C (03:00); consumes B and C; then A closes with revision 2 at 04:00; A
is consumed once at 04:00, no duplicate of B/C, and cursor moves only after persistence.
Checkpoints move only after profile, diagnostics and attempt outcome are persisted; a failed
attempt retries next tick and a restart replays unacknowledged events deduplicated by `event_id`.

Every due or manual attempt emits a per-pack row with `attempt_id`, `last_attempt_at`,
`sessions_in_window`, `sessions_complete`, `accepted_samples`, `rejected_samples`,
`capacity_outcome`, `efficiency_outcome`, `reasons[]`, `measured_at_capacity`,
`measured_at_efficiency`, `persistence_outcome`, and `updated_at` per metric. Outcomes:
`applied`, `kept_previous`, `blocked`, `skipped`, `failed`. Blockers include (not limited to)
`no_sessions_in_window`, `no_completed_session`, `no_new_completed_session`, `no_meaningful_cycle`,
`invalid_sample`, `confidence_rejected`, `bounds_rejected`, `extraction_failed`,
`persistence_failed`. Manual relearn bypasses the "new completed session" gate but not sample
eligibility and never changes `updatedAt` without an accepted measurement.

`updatedAt` is the timestamp of the newest **accepted local measurement** used for that metric:
capacity uses the `ended_at` of the newest accepted local session; efficiency uses the `ended_at`
of the newest accepted local discharge block. Peer-only prior changes do **not** move local
`updatedAt`; a blended update still uses the newest local session timestamp, not the peer's; with
no local evidence the label prior is not published as a measurement. Unchanged-input manual replay
preserves the timestamp. Pass time is `lastAttemptAt`; persistence time is `persistedAt`.
Skipped, blocked or failed passes never move `updatedAt`.

Full-charge maintenance: due time is anchored at `lastFullCharge + interval` and persisted;
partial fills carry `missing_kwh`, `partial_reason` and the original due time into the next cheap
window before the 07:00 local deadline. A run qualifies as full only when telemetry reports
SoC ≥100 % **and** measured AC input covers the required energy; dwell qualifies only after 60 min
with SoC ≥99 % and no discharge. Interruption resets dwell. Completion is idempotent by
`completion_id`; persistence failure emits `dwell_persistence_failed` and retries without claiming
balance. Import-ceiling protection always applies; advisory/diagnostic is not a recovery mechanism.

**Acceptance.** S49–S60, S107–S114, S138–S141 and S153–S155, S167–S169 are the objective tests. S113 asserts 48 h of 0 W
micro-sessions produce `no_meaningful_cycle` with unchanged `updatedAt`; S114 asserts the
multi-segment charge/4 h hold/evening discharge produces exactly one accepted pair with
`roundTrip ∈ [0.75,0.95]` and a stated confidence, without breaking the charge block.

---

## Test conventions and oracles (authoritative for the matrix)

**Signs and timing.** Meter power is **import positive**; battery power is **charge positive**,
**discharge negative**. With `load_w` non-battery demand and `pv_w` generation, the balance is
`grid_w = load_w − pv_w + battery_w`, so `load_w − pv_w = grid_w − battery_w`. To reach a target
`G` from a measured pair `(grid_w, battery_w)`, the required battery setpoint is
`p_req = G − (grid_w − battery_w)`. Measurements are read at the **end of each 20 s tick**, with
meter and pack samples timestamped together (skew ≤2 s); a command issued in tick `n` can first be
measured in tick `n+1`. Example: `grid=7800 W, battery=-800 W ⇒ demand=8600 W`, and `G=7500 W
⇒ p_req=-1100 W` (not -1500 W). `grid=700 W, battery=0 W ⇒ p_req=-700 W` for zero import.

**Independent oracle.** Optimizer golden tests compare against an exhaustive or dynamic-programming
oracle over the same quantized states (1 Wh/1 W), not against a hand-picked schedule. Acceptance
requires `|objective_oracle − objective_impl| ≤ 0.01 EUR`, identical constraint feasibility, and
schedule equality only when the canonical six-key tie-break leaves one optimum. Structural strategy
comparisons (A vs B) are not optimizer acceptance tests.

**Result envelope mapping.** Every result has a common terminal outcome in
`{applied, blocked, deferred, skipped, failed}` plus `detail.domain_state`. Learning
`kept_previous` maps to `skipped` (not a sixth top-level outcome); `confidence_rejected` maps to
`skipped` or `blocked` by whether a previous metric was kept; plan `healthy`/
`degraded_accepted`/`infeasible` and receipt `full_flow`/`partial_flow`/`no_flow` are
`detail.domain_state` values, never top-level outcomes. Ledger/API schema version is explicit and
unsupported versions return `unsupported_schema_version`.

**Deterministic replay.** Recorded fixtures are versioned input/event streams with fixed clock and
RNG seed. Replay performs zero live I/O by injecting fake device, price, forecast and ledger
clients; two replays must produce byte-identical canonical plans, command intents, ledger events and
final state.

## INVARIANTS — properties no module may violate

1. **State-scoped no-write invariant.** While in `dry_run_latched` the engine writes nothing,
   ever; the only state permitting any write without economic authority is `clearing`, entered by
   logged operator command and limited to safety-lane neutralisation. Property tests S145/S186.
2. **The engine is never parked in advisory or dry_run operationally.** `advisory` is a bounded
   `protection_active` session; the write-free latch is a loud degraded state requiring explicit
   operator clearance, not an operational mode; expiry, restart and invalid config have guarded
   exits. Property test S146.
3. **Applied power is measured, never assumed.** `applied_w` is `null` without a fresh signed
   measurement; echoes, written watts and plan watts are separate fields. Property test S147.
4. **No path can silently do nothing.** Every tick, plan, learning and maintenance pass ends in
   `applied`, `blocked`, `deferred`, `skipped` or `failed` with typed evidence. Property test S148.
5. **Capacity and efficiency eligibility never veto each other.** Each metric publishes or blocks
   on its own evidence and keeps its own `updatedAt`. Property test S149.
6. **Scoped safety precedence.** **In `auto`, no safety action may be blocked by the failure it
   protects against.** Ordinary backoff, stale plan, missing grid telemetry and diagnostic
   suppression never suppress cell/import/orphan holds; physical limits always still bind. In
   `dry_run_latched` the owner's no-write rule dominates: no protection is active and health/dashboard
   must say `battery_unmanaged=true,warning=no_protection_writes_forbidden`. In `clearing`, the only
   active protection is safety-lane neutralisation; full protection resumes at the instant `auto`
   commits. Property test S150.

## LIMITATIONS — authority provenance on this platform

The reviewer requested a trusted freshness/provenance anchor outside the restored snapshot. That
guarantee is **not achievable** here and is partially rejected on technical grounds. This is a
Home Assistant add-on on one machine with a local SQLite ledger: there is no TPM, no remote
attestation, no server-side clock and no signing account. Any anchor stored in the add-on data
directory is captured by the same snapshot that captures the ledger. A key on the same filesystem
is restored with the data it protects, so a keyed or unkeyed local hash gives integrity against
accidental corruption, not against a deliberate rollback by the owner of the host and backups.
The threat model that matters is accidental or mistaken restore, not an adversary with root on the
owner's own machine; conflating them would add unverifiable machinery.

**Detectable:** database-only replace/copy/rollback, missing or mismatched separate anchor,
truncated or incomplete journal, stale checkpoint, malformed token, unexplained generation.
Each enters `authority_recovery_required` or `journal_incomplete` recovery.

**Not detectable locally:** a full add-on snapshot restore that replaces ledger and anchor
together, including an internally consistent old snapshot with matching anchor. It lands in the
universal `reconciled` startup case, indistinguishable from a crash; the required behaviour is the
visible banner and a journalled startup-resolution event, not a false claim of proven continuity.

## Acceptance rule derived from the audit

The final lifecycle and status-gap cases are S171–S182: latch preservation per path, nonzero-power
clearance, charging-only policy preference, deductible terminal deadband, status-aware flow
classification and invalid-record-before-idle_blip validation.

A tick, plan, learning pass or maintenance pass never ends with only an absence. Each ends in
exactly one of `applied` (measured evidence), `blocked` (all binding reasons), `deferred` (retry
time and cause), `skipped` (named blocker and unchanged measurement timestamps) or `failed`
(typed error and escalation). `updatedAt` moves only on a real measurement; `lastAttemptAt` moves
on every attempt. Skipped or unreachable packs are per-pack rows, never whole-plan delivery failure.
