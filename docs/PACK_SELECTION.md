# Pack selection, minimum active power and fast power reaction

Status: **built; shipped from 0.3.8 (§3–§4) through 0.4.0 (§5–§6).** Every setting below is on the
Settings page. §6 ships with a two-tick confirmation and a `max(10 s, replan_seconds/4)` floor
between power-triggered solves; the 10 s moving average named in §6 is not built — the
confirmation covers the same jitter at no cost. Replaces nothing; sits between the plan and the
safety ladder.

## 1. Goal

Today `Allocation::split` (`crates/energy-core/src/allocation/fleet_allocation.rs`) sorts the
packs by serial and spreads the target **proportionally over every pack's headroom**. Four packs
and an 800 W discharge therefore run four packs at ~200 W each. The policy in this file replaces
that with:

1. **As few packs as possible.** One pack carries the whole request while it can; the next one
   starts only when the previous is saturated.
2. **SoC decides who.** The emptiest pack charges first, the fullest discharges first.
3. **A pack is only run if it is worth running.** Nothing below `min_pack_power_w` (default
   50 W); a remainder below that is left to the grid. This is the *only* place a grid deviation
   is allowed — never a blanket deadband.
4. **Selection is latched.** A pack that was picked keeps its slot until the cycle ends or a hold
   timer expires (see §5) — otherwise rising SoC would rotate the choice every replan.
5. **The split follows the grid every tick**, and the *plan* is re-solved as soon as measured
   grid or PV moves more than a configurable threshold (default 10 W each).

Non-goals: changing the plan/DP, the export gate, the ladder, or the reserve semantics. This is
a distribution change plus a replan trigger.

## 2. Settings

| key | default | range | meaning |
|---|---|---|---|
| `min_pack_power_w` | 50 | 0–500 | smallest allocation worth starting a pack for; also the allowed grid deviation at a pack boundary |
| `pack_staggering` | on | off/on | master switch for §1.1–1.3; off restores the proportional split |
| `pack_hold_seconds` | 1800 | 60–14400 | how long a selected pack keeps its slot |
| `pack_switch_soc_pct` | 2 | 0–20 | SoC differences below this count as a tie, so ordering cannot flip on noise |
| `grid_react_w` | 10 | 0–500 | grid deviation that forces a re-solve |
| `solar_react_w` | 10 | 0–500 | PV deviation that forces a re-solve |

All six join the existing spec table (19 → 25 entries) and the Settings page panel; the API DTO
needs no change beyond the new keys appearing in `values`/`schema`.

## 3. Selection algorithm

Inputs per tick: usable packs with `(pack_id, soc_pct, limit_w, headroom_wh, blocked, override)`,
signed target `P` from the plan, and the current latch.

```
1. drop packs that cannot serve this direction:
     unavailable, manual override active, direction-limit 0,
     cell latch / blocked_discharge (discharge only),
     SoC outside [reserve_soc_pct, max_soc_pct] for that direction.
2. rank = floor(soc / pack_switch_soc_pct)          # ties within the band
   order by:  latch holder first if it is in a tie band with the best candidate,
              then rank ascending for charge / descending for discharge,
              then pack_id.
3. remaining = |P|; for each pack in order:
     if remaining < min_pack_power_w: break       # grid covers the rest
     watts = min(remaining, limit_w, headroom_wh / hours)
     remaining -= watts
4. sign back to the target's sign; fleet clamp (import/export caps) runs on the result,
   and when it has to cut it cuts the *last* allocated pack first, never proportionally.
```

`unmet_w` keeps its meaning, except that a shortfall smaller than `min_pack_power_w` counts as
met (it is inside the allowed deviation).

## 4. Minimum active power (50 W)

- A pack whose share would be `< min_pack_power_w` is not started at all.
- A running pack is kept while its share stays `> min_pack_power_w / 2`, so a 20 W wobble does
  not stop it; below that it stops rather than idle at 15 W. Losses at 20 W are dominated by the
  inverter's own draw, so idling is strictly worse than letting the grid cover it.
- A target below `min_pack_power_w` runs nothing. This is deliberate: no pack may start for a
  rounding error, and the grid deviation stays inside the same 50 W.

## 5. Selection latch — the anti-thrash rule

Without it, charging rotates: pack A charges, its SoC rises past B, B is selected, A stops, B
rises past A, A is selected … one start/stop per replan, all day. The latch fixes that.

**State per fleet** (persisted, ledger schema v7, table `pack_selection`, single row):

```
{ direction: 'charge' | 'discharge', holder: pack_id, selected_at: unix, target_w: f64 }
```

**The holder keeps priority until the earliest of:**

1. **Hold timer expires** — `now − selected_at > pack_hold_seconds` (default 1800 s). The timer
   is *not* extended by continued running; it bounds stickiness, never renews it. A half-hour
   is chosen because it spans several `replan_seconds` and roughly two tariff slots, so a pack
   holds through the peak it was picked for instead of handing over mid-slot; the setting is
   there for anyone who wants faster rotation (60 s) or a full evening (14 400 s).
2. **The cycle ends** — the fleet target falls below `min_pack_power_w` (idle) or flips sign
   (charge ↔ discharge). Both mean the cycle is over, so the latch is released and the next
   start is free to choose fresh. This is checked before the timer, so a stop-and-restart at
   noon is not held hostage by an 11:00 selection — the 30 min hold never keeps a pack through
   an idle period.
3. **The holder cannot serve** — unavailable, latched, override, at its floor/ceiling for this
   direction, direction limit 0, or its headroom is under `min_pack_power_w`. A hard limit always
   beats the hold; we never keep a pack selected that cannot do the work.
4. **The target needs more packs than the holder can absorb** — the holder stays first and the
   *additional* packs are chosen fresh. Growth does not disrupt the running pack.

While latched, the holder is ranked first **only inside its SoC tie band** (§3 step 2). If another
pack is a whole band better, the holder still stays until (1)–(4) fires: churn on small SoC
differences is exactly what the band and the timer exist to prevent, and the cost of running a
slightly-worse pack for at most `pack_hold_seconds` is a few Wh of efficiency, against a
start/stop cycle every replan.

The latch is written on every tick the fleet is active (cheap single-row upsert), cleared on
condition (2), and ignored after `pack_hold_seconds` without a writer — so a stale row can never
pin a pack after a long idle or a restart.

## 6. Fast power reaction

Two layers, because they cost very differently:

**Every tick (no setting, no solve):** the split is re-derived from the live grid reading.
`pack_decision::allocate` already clamps against `grid_telemetry::base` each tick; §3 makes the
*assignment* follow it too, so a load step moves the watts on the same tick.

**On deviation (new):** the plan is re-solved when either measured input moved more than its
threshold, compared against **the values the current plan was solved from** (stored with the
plan), not tick-to-tick — otherwise slow drift never crosses:

```
react = |grid_now − plan.grid_at_solve| > grid_react_w
     or |pv_now   − plan.pv_at_solve|   > solar_react_w
```

- Both are evaluated in the same tick ⇒ one replan, never two.
- Inputs are a 10 s moving average, so sensor noise cannot fire the trigger.
- Minimum 10 s between power-triggered replans (`replan_seconds` floor) to bound the DP cost on a
  cloudy day; every trigger is logged with its reason and both deltas.
- Stale input still wins: `stale_input_seconds` holds, it never triggers a re-solve.
- A plan that refuses to solve holds the old setpoints (existing rule, PLAN.md §6) — no stale
  plan is ever executed.

## 7. Commit sequence

| # | commit | contents | tests |
|---|---|---|---|
| 1 | **done (0.3.8)** `fix(dispatch)` | the hold path was already sound — `reconcile` + the latch hold an unavailable or vanished pack we own — the four tests only assumed the old proportional split. They now drive both packs on their own merit (a 1.6 kW request, not a 0.8 kW halved one), so ownership no longer depends on the split policy. Discovery is pinned off in `boot`/`doctor`, which had started seeing the real LAN packs. | `an_unavailable_pack_is_held_and_keeps_its_latch`, `a_pack_that_vanished_is_held_and_keeps_its_latch`, `a_failing_tick_holds_what_we_own_and_stops_marking_progress`, `a_bookkeeping_failure_still_holds_every_target` |
| 2 | **done (0.3.8)** `feat(dispatch)` | SoC ordering (emptiest charges, fullest serves, id breaks ties), greedy fill, `min_pack_power_w`, `SplitInput`, `pack_staggering` switch with the proportional path kept | `discharge_uses_the_fullest_pack_alone`, `charge_uses_the_emptiest_pack_alone`, `a_saturated_pack_spills_into_the_next_in_soc_order`, `a_sliver_never_starts_another_pack`, `a_target_below_the_minimum_runs_nothing`, `a_pack_that_can_only_take_a_sliver_is_skipped`, `the_proportional_split_is_still_available` |
| 3 | **done (0.3.9)** `feat(dispatch)` | ledger v7 `pack_selection`, hold timer, cycle-end release, hard-limit release, growth keeps the holder | `a_rising_soc_does_not_rotate_the_pack_within_the_hold`, `the_hold_expires_and_the_emptiest_pack_takes_over`, `the_hold_defaults_to_thirty_minutes`, `an_idle_target_releases_the_hold`, `a_sign_flip_releases_the_hold`, `a_pack_at_its_floor_is_replaced_immediately`, `a_stale_hold_never_pins_a_pack` |
| 4 | **done by 0.3.9** — the split is re-derived every tick from the live SoC and the live grid (the fleet clamp already ran per tick); no separate change was needed | — | — |
| 5 | **done (0.4.0)** `feat(plan)` | `grid_react_w`, `solar_react_w`, `plan.grid_at_solve`/`pv_at_solve`, `ReplanReason::PowerChanged`, 10 s floor, moving average | `a_ten_watt_grid_step_replans`, `a_nine_watt_step_does_not`, `pv_and_grid_moving_together_replans_once`, `a_stale_grid_reading_holds_instead_of_replanning`, `the_replan_floor_bounds_solves` |
| 6 | **done (0.4.0)** `feat(web)` | `SettingsPolicy`/limits panel rows, mock fixtures | `web-test` fixtures for the new keys |

Release 0.4.0 after commit 6; 0.3.8 may ship commits 1–2 alone if 3–6 need more review.

## 8. Edge cases

| case | behaviour |
|---|---|
| pack unavailable / stale | excluded; if it was running, written 0; latch released (hard limit) |
| cell latch | never discharge; charge only if the latch permits |
| manual override | excluded from the automatic split; the override owns that pack |
| SoC tie within the band | latch holder wins, then lower pack id |
| different pack limits (300 W beside 3000 W) | a pack is never asked past its own limit; the larger saturates first on a tie |
| pack inside solar reserve / at floor | excluded for that direction, so staggering cannot push it somewhere it cannot go |
| target below the minimum | nothing runs; the grid covers it |
| sign flip in a slot | one sign per tick; no pack is asked to reverse mid-tick |
| grid cap after staggering | cuts the last allocated pack, never a proportional slice |
| ramp limit | a newly selected pack ramps from 0 at the existing `ramp_limit_w` |
| zero-export mode | unaffected: the gate is on the plan; the 50 W rule is what stops 12 W drift starting a pack |
| restart mid-cycle | the latch row survives; if older than `pack_hold_seconds` it is ignored |
| fewer packs than the target needs | `unmet_w` reports it, the grid covers it |
| battery still ramping when a replan lands | the ramped pack is not double-counted; the ladder clamps per pack per tick |

## 9. Rollout

1. Commits 1–2 on `main`, `dry_run`, compare `dispatch_log` intended setpoints against the old
   add-on for a day: expect *fewer* active packs, same grid trace.
2. Commits 3–5 behind `pack_staggering` — watch `/api/v1/dispatch` for start/stop counts: the
   latch should reduce them to roughly one per cycle.
3. Turn on `pack_staggering`, keep `grid_react_w`/`solar_react_w` at 10 W for a day; confirm the
   replan log shows power triggers with sane deltas and no solve storm on a cloudy morning.
4. Only then consider `auto` (P10), which still needs the old add-on stopped.

## 10. Risks

- **Latch pins a bad pack** — bounded by `pack_hold_seconds`, released by every hard limit, and
  ignored when stale. Worst case: one pack runs slightly off-optimal for 30 minutes (the
  default). An idle target or a sign flip releases it early in every case.
- **More starts, not fewer** — if the hold were shorter than a replan, every replan would
  rotate. The default (1800 s) is deliberately many replans long; the setting's floor (60 s) is
  still above the 10 s reaction floor, and the docs warn when it is set under `replan_seconds`.
- **DP load from the reaction trigger** — bounded by the 10 s floor; if it still shows up in the
  tick budget, raise `grid_react_w` rather than the floor.
- **Hidden coupling with reserves** — staggering must never be the reason a pack is pushed into
  the solar reserve; the reserve check stays in the plan, the split only respects it.
