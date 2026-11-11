# Grid follow: close the loop on the meter, and let the plan sit still

Status: **built and shipped in 0.5.0**, corrected in 0.5.2 (see the decision note at the end). Replaces the power-deviation replan trigger shipped in
0.4.0. One refinement against §3 as first written: the desired grid is not simply the slot's
`load − pv − battery`. That number is only a decision when the plan is *exchanging energy on
purpose* (charging from the grid, or selling); everywhere else it is an artefact of the load and PV
the plan assumed, and the site holds the meter at zero instead — absorb the sun, cover the house.
Both cases are pinned by tests (`a_deliberate_grid_charge_keeps_its_import`,
`a_surplus_is_absorbed_rather_than_exported`).

## 1. What was wrong

0.4.0 added `grid_react_w`/`solar_react_w` (10 W) as a **replan trigger**: it compares the
measured grid with the plan's expected grid and asks for a new solve when they differ. On the live
install that fired permanently — the real house load never equals the planned load, so a *standing*
offset crossed 10 W forever. The ledger shows 50 plans in 2 h 26 m (one solve every ~3 min), all
day, each one re-reading HA statistics, the price book and the forecast.

Two things are being conflated:

- **The meter moves** (a kettle, a cloud edge). The battery rate should follow within a tick. No
  new plan is needed: the plan already says whether this hour is for charging or serving.
- **The plan's assumptions move** (prices, forecast, SoC trajectory, an hour boundary). That is
  when a solve is worth its cost.

## 2. What exists already (research)

`GridFollow` (`crates/energy-core/src/safety/grid_follow.rs`) is a **complete, tested, dead
implementation** of exactly this loop:

```rust
delta = -(measured_grid_w - planned_grid_kw * 1000)          // import error
if |delta| <= deadband_w { return target_w }                  // ignore noise
target_w + delta.clamp(-budget, budget)                       // bounded move
```

It is not called from anywhere except one test (`the_grid_follow_deadband_and_budget_are_pinned`).
Its two constants are wrong for this job:

- `GRID_FOLLOW_DEADBAND_W = 30` — the requirement is 10 W, from a setting.
- `budget = max(500 W, 0.5 × capability)` — because the correction is a **setpoint anchored on the
  plan target, recomputed every tick**, a budget below the fleet's capability means a step larger
  than the budget is *never* closed: the same 1.6 kW correction is re-applied every tick and the
  residual stays for good. The budget must be the fleet's capability; the per-pack ramp limit
  (`RAMP_LIMIT_W = 1000 W/tick`) and the ladder are what actually bound a spike.

The rest of the plumbing is in place:

- `TickFrame` already carries `plan_power_w` (the plan's setpoint) and `target_w` (what is
  dispatched), so the follow's effect is visible in `/api/v1/dispatch` without a new field.
- `slot_power` smooths the plan setpoint across a slot boundary (`TRANSITION_SECONDS`).
- `fleet_grid::clamp` bounds the allocation by the site import/export ceilings after the split.
- The ladder (`safety/check`) enforces per-pack ceiling, SoC floor, ramp, latches and cell limits.

## 3. The loop

```
slot          = plan.slot_at(now)                       // no slot ⇒ no follow
planned_grid  = (slot.load_kw - slot.pv_kw - slot.battery_kw) * 1000
target        = slot_power(plan, now)                    // the plan's setpoint, W
target_follow = GridFollow::correct(target, measured_grid, planned_grid, capability)
```

- **Anchor is the plan target**, not the measured battery power: a wrong battery measurement can
  perturb one tick, it cannot integrate into a drift.
- **Desired grid** is the plan's own expectation, which is 0 for every zero-export slot and every
  slot that already plans to absorb all surplus. It is the planned import during a cheap-charge
  slot, so the loop does not fight a deliberate grid charge.
- **Skip the follow entirely** when the plan is stale, missing, refused, or the tick already holds
  (`grid_hold`): there is no intent to follow.
- **Sign crossing is allowed.** With a surplus that vanishes, a charging fleet becomes a
  discharging one to keep the grid where the plan wants it. The ladder's SoC floor and the
  allocation's headroom bound it; the plan's economics are unchanged because the desired grid is
  the plan's own number.

## 4. Settings and timing

| key | now | proposed | why |
|---|---|---|---|
| `grid_follow_deadband_w` | — (const 30) | **10**, 0–500 | the meter noise the user wants ignored |
| `tick_seconds` | 5 (option) | **1** | one correction per second |
| `replan_seconds` | 900 | **300** | the plan refresh cadence; no longer tied to the meter |
| `grid_react_w`, `solar_react_w` | 10 | **removed** | replanning on power is what this replaces |

`PowerMove`, `ReplanReason::PowerChanged`, `power_watch.rs` and the two-tick confirmation go away
with them; `replan` keeps its timer, price, forecast, SoC-drift, slot-boundary, stale and expired
triggers.

### The 1 s tick needs a cheaper meter read

`HaMeter` (`crates/energy-server/src/bootstrap/meter.rs`) reads **`/api/states`** — every entity in
the house — and caches it for 4 s. At a 1 s tick that is 1 s of 4 s stale *and* a full state dump
per second. The read becomes targeted:

1. `SiteModel::power_entity_ids()` — the grid, PV, battery and live-power ids it already holds from
   the energy dashboard preferences (a handful, deduplicated).
2. `HaMeter` reads those ids with `client.state(id)` (a few kB), not `states()`.
3. Cache TTL = the tick interval, so one read per tick and never a stale one.
4. If a targeted read fails, the reading is empty (`None` fields) — the ladder holds, as today.

## 5. Edge cases

| case | behaviour |
|---|---|
| zero export | planned grid is 0, so discharge stops exactly at the house need and charging absorbs exactly the surplus |
| surplus larger than the headroom | the allocation fills what fits, the rest exports (permitted by the gate), no error |
| cheap-import charge slot | planned grid is the planned import, so the loop holds that import even when the house load moves |
| 2 kW load step | corrected within one tick, rate-limited to 1000 W/pack/tick by the ramp |
| grid sensor spike | one bounded tick (capability, ramp-limited), corrected back the next |
| grid sensor dropout | no measurement ⇒ no correction and the ladder's missing-telemetry hold applies |
| stale/refused/expired plan | no follow at all; the tick holds |
| SoC floor reached mid-discharge | the ladder blocks the packs, the grid imports, no overshoot |
| cell latch / manual override | that pack leaves the split; the rest follows |
| sign crossing | allowed, bounded by capability and the ladder (see §3) |
| slot boundary | `slot_power` still ramps the *plan* part; the follow is added on top, so the total does not jump |
| 12 W error, no pack running | the correction exists but is below `min_pack_power_w` (50 W), so no pack starts — intended |
| meter jitter inside the deadband | nothing changes; no pack is started or stopped for noise |
| missed ticks (slow read) | `MissedTickBehavior::Delay`: no burst of catch-up ticks |

## 6. Commits

| # | commit | tests |
|---|---|---|
| 1 | **done (0.5.0)** `feat(dispatch)` — wire `GridFollow` into `TickFrame`, deadband from the setting, budget = capability, skip on stale/missing slot | `a_load_step_is_matched_within_one_tick`, `a_grid_charge_slot_keeps_its_import`, `zero_export_never_overshoots`, `no_slot_no_follow`, `stale_plan_no_follow`, `a_spike_is_rate_limited` |
| 2 | **done (0.5.0)** `feat(server)` — `power_entity_ids`, targeted reads, TTL = tick | `the_meter_reads_only_the_power_entities`, `the_cache_follows_the_tick` |
| 3 | **done (0.5.0)** `refactor(plan)` — remove `PowerMove`, `PowerChanged`, `power_watch`, the two settings; `replan_seconds` default 300 | `a_power_step_does_not_replan` |
| 4 | **done (0.5.0)** `feat` — defaults, add-on option, Settings page rows, docs | config/golden tests |

Release 0.5.0. Everything lands in `dry_run` first: the dispatch log then shows `planPowerW`
versus `targetW` per tick, which is exactly the follow's contribution, so the loop can be judged on
real data before `auto`.

## 7. Decision note (0.5.2): the controller, and what it may decide

**The loop is incremental, not positional.** The first version added the error to the *plan*
setpoint. Since the meter already reflects the previous correction, a steady 500 W error produced
1000 → 500 → 1000 → 500 W instead of settling. The correction is now added to the battery's own
measured output, which makes the resulting grid the desired one in a single tick whatever the house
and the sun are doing; `the_grid_follow_settles_instead_of_alternating` simulates a device that
answers one tick late and asserts the target never reverses and then holds.

**Inside the deadband the battery stays where it is.** Returning the plan's setpoint there stopped
the very pack that was holding the meter, and the next tick corrected from scratch again — the same
oscillation from the other side.

**Authority: the follow may not buy or sell on its own.** It only ever holds the site at zero
exchange. A slot that deliberately exchanges energy — charging from the grid, or selling — is not
followed at all: its number is a *battery rate*, and following the grid there would trade the
planned charge for a net-import target. So the economy-sensitive directions stay with the plan:
the follow can never import to charge, and it can never export to sell, beyond what the plan already
chose. What it does do is self-consume — absorb the surplus, cover the house — including turning a
planned charge into a discharge when the sun does not arrive, which is the stated policy ("never
force an import"). Missing telemetry, a stale plan, a missing slot or a slot with non-finite inputs
all mean no follow at all.
