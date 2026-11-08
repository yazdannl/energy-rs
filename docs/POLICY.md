# Dispatch policy after net metering

Status: **spec, not implemented**. Applies from the moment `Settlement` reports netting off
(`NetMetering::is_active` false: date-driven from 2027-01-01, or the manual override). While netting
is on (2026 today) every rule here is inert and today's behaviour is unchanged.

Scope: what the planner may do with stored kWh — export, grid charging, and the two reserves — plus
the guard clauses that enforce the same rules at dispatch time.

---

## 1. Why today's plan is not good enough after 2027

Today (`crates/energy-core/src/plan/config.rs`):

* `allow_export = false` — the DP never plans a battery export. Correct while saldering makes an
  exported kWh worth the all-in price, wrong when feed-in is 30 ct below the buy price.
* `sell_floor_eur_per_kwh = min(marginal_export)` — a settlement number (while netting: `buy −
  feed_in`), not a cost basis. It gates the safety ladder (`safety/ladder.rs::floor`), not the DP's
  export decision.
* `wear_eur_per_kwh` defaults to 0, `salvage` is 0 and there is no terminal target: stored energy is
  worth nothing at the horizon end.
* `reserve_soc_pct` (settings, default 10) is one static floor. No solar headroom, no peak reserve,
  and no notion of what a stored kWh cost.

After 2027 three questions decide the money: *may I sell this kWh, may I buy a kWh to store, and how
much must I keep back?* This document answers all three with explicit numbers.

## 2. Vocabulary and units

Energy kWh on the **AC side** unless marked `_dc`. Money EUR/kWh.

| Symbol | Meaning | Source |
|---|---|---|
| `E_cap` | summed configured capacity | `Σ pack.spec.capacity_kwh` |
| `E_min_hw` | summed hardware floor | `Σ pack.spec.e_min_kwh(0)` (`min_soc_pct`, default 10 %) |
| `E_max_hw` | summed hardware ceiling | `Σ pack.spec.e_max_kwh()` (`max_soc_pct`, default 100 %) |
| `S` | stored energy at plan time | `Σ pack.spec.energy_kwh(soc_pct)` |
| `η_c`, `η_d` | capacity-weighted efficiencies | `plan/aggregate.rs` |
| `η_rt` | `η_c · η_d` | derived |
| `w` | wear, EUR per **AC kWh delivered** | setting `wear_eur_per_kwh` (default 0.02) |
| `b` | cost basis: EUR per AC kWh **currently stored** | §5, the pool |
| `f(t)` | feed-in price | `Settlement::export_price` |
| `p(t)` | buy price | `Slot::buy_all_in` |
| `P_c` | summed charge power, kW | `Σ pack.spec.max_charge_w / 1000` |

The DP's wear term is `wear_rate = w · η_d` applied to DC energy dropped (`csrc/core_solver.c:328`),
i.e. exactly `w` per AC kWh delivered — the same unit as `w` here.

## 3. The rules

Two hard rules bound everything else:

* **the policy never forces an import** — no clause may block a discharge the DP chose, and no clause
  may require buying energy the DP would not have bought;
* **no solar is lost to a reserve** — the reserves shape buying and holding, never PV charging.

Both reserves are therefore *planning guidelines*, not caps on what the house may use.

### R1 — Export only above the break-even of the stored kWh

Export of **battery** energy in slot `t` is allowed only when

```
f(t) ≥ break_even          break_even = b / η_rt + w          f(t) > 0
```

One exported AC kWh empties `1/η_d` DC kWh, which costs `1/(η_c·η_d)` AC kWh to replace; if the
replacement costs more than the feed-in earns, exporting destroys value.

**Mechanism: a post-solve export clamp with a bounded capture exemption.**

The DP runs unchanged (`sell = f(t)` everywhere, so PV valuation and PV export are untouched). The
clamp then works in two passes over the plan's own action path.

**Pass 1 — what would actually be curtailed.** Simulate the **clamped, no-exemption** path (the
reserve ceiling and the absolute export gate applied, exemption zero) and record, per slot, the
surplus the pack cannot take:

```
surplus_ac(u) = max(0, pv(u) − load(u)) · Δt
chargeable(u) = min(surplus_ac(u), room_dc(u) / η_c, p_charge · Δt)
curtailed(u)  = surplus_ac(u) − chargeable(u)                        # AC kWh thrown away
rescuable(u)  = max(0, curtailed(u) − max(0, surplus_ac(u) − p_charge · Δt))
```

`rescuable` keeps only the **room-limited** part: surplus above the charge power is lost whatever the
SoC, so extra room cannot rescue it and it must not license a sale. The baseline is the clamped path,
not the raw DP path: if the DP already sells below break-even to make room, that sale is not in the
baseline, so its PV shows up as curtailment and can be licensed once — not zero times.

**Budget — no reuse.** A single curtailed kWh must not license a sale in two earlier slots. Walk the
horizon **backwards** once and hand each slot a budget:

```
budget(last) = 0
for t from last−1 down to 0:
    budget(t) = budget(t+1) + rescuable(t+1)
```

then walk forward applying the clamp; a slot that uses `x` AC of exemption consumes `x/η_rt` AC of
the budget, and `x` may never exceed `budget(t) · η_rt`.

**Pass 2 — clamp, with the exemption bounded by the budget.**

```
if f(t) < break_even and power(t) < 0 and base + power(t) < 0:
    power(t) = max(power(t), −(max(0, base) + budget(t) · η_rt / Δt))
```

The conversion keeps the bound honest in both directions: selling `x` AC frees `x/η_d` DC of room,
which absorbs `x/η_rt` AC of PV, so rescuing `a` AC of PV licenses `a · η_rt` AC of sale. At sunset
the budget is zero and the gate is absolute; on a day the pack can absorb, the budget is zero too.

The budget is conservative in the safe direction: a sale lowers SoC, which can only reduce later
curtailment, so the no-exemption baseline is an upper bound on what a sale can rescue. The DP still
decides *whether* to sell (`sell = f` is its true input); the clamp only declines to forbid up to the
budget. It never touches a slot that serves the house, never touches PV export, and never increases a
discharge.

Why not the DP's `sell_floor_eur_per_kwh`: its second clause (`base > 0 && buy < floor → no
discharge`, `core_solver.c:229`) blocks self-consumption whenever the buy price is below the
break-even, which the user explicitly forbids. Why not price shaping (`sell = 0` in blocked slots):
the DP uses the same number to value PV charged into the pack (`pv_charge_preference = Auto`), so
shaping would make PV look free and could turn a storable surplus into a curtailment. A post-solve
clamp touches only battery export and only where no PV is at risk.

The clamp never increases a discharge and never changes a slot that serves the house, so it cannot
create an import the DP did not choose. `plan.sell_floor` carries `break_even`, so the ladder applies exactly the same decision (§R5).

### R2 — Self-consumption is never gated

* Discharging to serve the house is worth `p(t)`; exporting is worth `f(t)`; `p(t) ≥ f(t)` on every
  slot of the live tariff, so the DP prefers self-consumption by itself.
* No clause of this policy blocks a discharge except at the hardware minimum (§R4), and none blocks a
  PV charge. The export clamp of R1 only ever *reduces a discharge that would have exported*.
* `pv_charge_preference` stays `Auto` and `sell` stays the true feed-in, so PV is valued exactly as
  the DP has always valued it.

### R3 — Grid charging fills only the gap solar leaves

PV is always cheaper than grid energy, so all of it must be captured, and grid energy may only fill
what PV does not cover. That is a **time-varying ceiling on grid charging**, never a limit on PV:

```
S_pv_after(t) = Σ_{u ≥ t} max(0, pv(u) − load(u)) · Δt              Δt = 0.25 h
cap_raw(t)    = E_cap − E_solar_reserve − S_pv_after(t)
cap(t)        = clamp(cap_raw(t), E_min_hw, E_max_hw)
```

`cap(t)` is the highest SoC a *grid* charge may reach in slot `t`: room for every kWh of surplus the
forecast still promises, plus the solar reserve. Early on a sunny day the ceiling is low; as the day
passes it rises and by evening grid charging may fill the pack. Overnight it already holds
tomorrow's surplus, so a sunny winter day still allows cheap night charging for the part tomorrow's
PV does not need. `cap(t)` is nondecreasing, so an early cheap hour is never followed by a lower
ceiling.

The ceiling bounds the *grid portion* only (§R4), so PV always fills to `E_max_hw` — no solar is
ever lost to it, and it cannot force an import (it only defers a purchase).

The DP's own economics decide whether charging is worth it: a delivered kWh later is worth
`p(future)`, and charging it now costs

```
p(t) / η_rt + w + min_spread          # 1 AC kWh delivered needs 1/η_rt AC kWh charged
```

so it charges only when that is below a later `p`. `min_spread_eur_per_kwh` stays the knob for "do
not cycle for a rounding error". Buying cheap to sell later through the pack is allowed only where
the later `f` clears `break_even` (R1), so it cannot lose money by construction.

### R4 — Reserves are planning guidelines

| Guideline | Default | Meaning | Mechanism |
|---|---|---|---|
| `peak_reserve_pct` | 20 % | energy worth keeping for a load peak the forecast missed | **soft**: terminal target + penalty |
| `solar_reserve_pct` | 20 % | headroom worth keeping free of **grid** energy for a PV peak the forecast missed | **soft-ish**: per-slot ceiling on grid charging only |

**Peak reserve = soft target.** The DP is configured with

```
e_target       = min(peak_reserve_pct% · E_cap, S)      # the guideline, never above what exists
target_penalty = peak_reserve_value_eur_per_kwh          # default 0.15
```

so the plan *aims* to end the horizon at the reserve and pays `penalty` per kWh it ends below it. It
discharges into the reserve whenever the avoided buy (or the avoided curtailment) is worth more than
the penalty — which is exactly "never import while stored energy can serve the load". No hard floor:
`e_min_kwh` stays the hardware minimum, and the ladder enforces nothing above it. The guideline
therefore cannot force an import.

`target_penalty` is the only new economic number: it says what a kWh of headroom is worth. 0.15
€/kWh is a little above the current spread between an average buy and the feed-in, so the reserve is
kept in normal weather and released in a price spike.

**Solar reserve = ceiling on buying.** It bounds only the grid portion of a charge (§R4 clamp), so
PV is never blocked and no import is ever forced. When `cap_raw(t) < E_min_hw` the forecast surplus
exceeds the whole usable band: the ceiling sits at the hardware floor, grid charging stops, and the
plan records `reserve_saturated` — a sunny day the pack cannot swallow, not a failure.

**The clamp.** `plan/clamp.rs` walks the solver's action path in slot order, applying the reserve
ceiling, the hardware floor and the export gate in one pass (sign convention is the DP's:
`power > 0` charges, `power < 0` discharges):

```
e = min(S, E_max_hw)                               # out-of-band start is clamped and flagged
for each slot i in order:
    base = load(i) − pv(i)                          # negative = PV surplus
    p    = power(i)
    if p > 0:
        pv_part   = min(p, max(0, -base))           # PV that feeds the pack in this slot
        grid_part = p - pv_part                     # the rest is grid energy
        room      = max(0, cap(i) − e)              # kWh of grid energy still allowed
        p         = pv_part + min(grid_part, room / (Δt · η_c))
    e_next = e + p·Δt·η_c  (p ≥ 0)   |   e + p·Δt/η_d  (p < 0)
    if e_next < E_min_hw:                           # only reachable after an earlier clamp
        p = max(p, (E_min_hw − e)·η_d / Δt)         # reduce the discharge magnitude, never deepen it
        e_next = e + p·Δt/η_d
    if p < 0 and base + p < 0 and f(i) < break_even:
        p = max(p, −(max(0.0, base) + budget(i) · η_rt / Δt))  # R1 + capture budget
        e_next = e + p·Δt/η_d
    if e_next > E_max_hw:                           # a raised discharge left energy the DP did not
        p = (E_max_hw − e) / (Δt · η_c)             # expect: clip the later charge to the ceiling
        e_next = E_max_hw
        clipped(i) = true                           # reported as pv_clipped
    power(i) = p ; e = e_next
```

Only the grid portion is ever removed, and only a discharge that would break the *hardware* minimum
or export below the break-even beyond the capture budget is ever shortened. The ceiling clip can only
fire from the reserve clamp, and the plan reports `pv_clipped` in kWh if it does. Invariants, by construction and by test: charge is never
increased except by that clip, discharge is never deepened, every `e_next` is within
`[E_min_hw, E_max_hw]` when `S` is in band, the grid portion never exceeds `cap(i)`, and a second
clamp pass changes nothing.

The rows and the money are recomputed from the clamped path (`timeline::timeline` +
`summary::summarize` derive everything from `powers`, including the grid flows the clamp changes, the
freed PV and the import a reduced discharge forces — the same formula the DP optimises, applied to
the path that will run). The DP's `objective_eur` is **replaced** by the recomputed cost; it is a
report, never a ranking key. If the clamp leaves the terminal target unmet, the plan says
`terminal_target_unreachable` (existing reason) — which is the guideline being released, not a fault.

### R5 — Enforcement at dispatch

The plan is advisory input to the guard; the ladder must not be able to break a rule even if the plan
is stale or clamped differently.

* **No discharge reserve.** `ladder.rs::soc` keeps bounding at `e_min_kwh(reserve_soc_pct)` — the
  hardware floor only. The peak reserve is a planning guideline and must never hold a pack back from
  serving the house.
* **Export gate on measured flow.** The plan's clamp used the forecast; the ladder must use what is
  actually happening. With `measured_base = measured_load − measured_pv` (the battery-idle base
  `grid()` already derives) and `plan.sell_floor = break_even`, a commanded `p < 0` in a slot whose
  `f < break_even` is bounded by

  ```
  p ≥ −max(0, measured_base)        # discharge no more than the house is actually using
  ```

  so a plan clamped to the *forecast* household need cannot export battery energy when the real need
  is lower. A self-consumption discharge is never refused, whatever the buy price.
* **Same gate at dispatch.** The ladder applies the same rule with measured values: in a slot whose
  `f < break_even`, a commanded discharge is bounded by the measured need (`p ≥ −max(0,
  measured_base)`). No second interpretation of the forecast, no divergence between plan and
  dispatch.
* **Grid-portion charge ceiling.** With `pv_surplus = max(0, measured_pv − measured_load)` and the
  pack's share `pv_share = pv_surplus · share()` (the capacity share the plan split already uses),
  a commanded `p > 0` at or above `cap(now)` may not exceed `pv_share`; the rest is grid energy and
  is clamped to zero. The share matters: four packs must not each claim the fleet's whole surplus. A
  0.05 kWh hysteresis band keeps a pack on the ceiling from chattering.

## 4. Where each piece lives

| Piece | File | Change |
|---|---|---|
| policy inputs | `energy-core/src/plan/plan_input.rs` | `cost_basis_eur_per_kwh`, `peak_reserve_pct`, `solar_reserve_pct`, `peak_reserve_value_eur_per_kwh`, `export_allowed` |
| solver config | `energy-core/src/plan/config.rs` | `allow_export = true` when netting is off, `sell_floor = None`, `e_target`/`target_penalty` from the peak guideline |
| export clamp | `energy-core/src/plan/clamp.rs` (new) | R1 export clamp, gate flag, unit tests |
| grid-charge ceiling | `energy-core/src/plan/assemble.rs` | compute `S_pv_after`/`cap(t)`, hand it to the clamp |
| reserve clamp | `energy-core/src/plan/clamp.rs` | the R4 loop, in the same forward pass |
| cost pool | `energy-ledger` (new table `energy_pool`) + `energy-core/src/savings/pool.rs` | §5 |
| settings | `energy-server/src/api/settings/schema.rs`, `bootstrap/config.rs` | four keys (§7) |
| ladder | `energy-core/src/safety/ladder.rs`, `limits.rs` | peak floor, grid-charge ceiling, measured floor |
| reporting | `energy-server/src/api/plan.rs`, `web/src/lib/api/*` | `exportFloor`, `costBasis`, reserves, reasons |

`Settlement` already answers "is netting on" per slot; nothing in `energy-price` changes.

## 5. The cost basis `b`

`b` is the average price paid for the energy that is in the pack **now**, including the opportunity
cost of PV that was stored instead of exported. A rolling average of recent charges is *not* that: it
replaces an expensive pack's basis with cheap recent charging. The basis is therefore a **cost pool**
carried forward, not a window.

### 5.1 The pool

Persisted state (`energy_pool`: one row, `kwh`, `cost_eur`, `updated_at`, `anchor_source`), advanced
once per settled slot from the measured flows (`SlotFlow`, the savings accountant's source):

```
price_pv   = f(t)                                    # PV stored instead of sold: the forgone feed-in
pv_to_batt = min(charge_kwh, max(0, pv_kwh − load_kwh))
grid_to_batt = charge_kwh − pv_to_batt
cost_in    = grid_to_batt · p(t) + pv_to_batt · price_pv
kwh_in     = charge_kwh

relief     = if pool.kwh > 0 { pool.cost / pool.kwh } else { 0.0 }   # average-cost relief
pool.kwh  += kwh_in − discharge_kwh
pool.cost += cost_in − discharge_kwh · relief
b          = if pool.kwh > 0 { pool.cost / pool.kwh } else { None }
```

`pv_to_batt` subtracts the **load**, not the export: with 3 kWh PV, 3 kWh load and 1 kWh charging,
the pack charged from the grid, and `max(0, pv − load)` is the PV that was actually available for the
pack. Using `pv − export` would misattribute that kWh.

Discharge relieves cost at the current average, so the pool stays the average cost of what is
physically stored; charging at a different price moves it. Exporting does not lower `b` (it removes
energy and its cost together), so a plan that sells cannot make the next plan cheaper to justify
itself.

### 5.2 Anchoring, gaps and drift

* **Fresh start / missing history:** the pool is anchored at the first observation with
  `kwh = S` and `cost = S · anchor`, where `anchor` is the operator setting
  `cost_basis_eur_per_kwh` if set, else `max_t p(t)` over the current horizon (the most expensive kWh
  the plan could need to replace). `anchor_source` records which, and the plan is degraded with
  `cost_basis_anchored` until the pool has been advanced by real measurements.
* **Gaps:** a slot with missing measurements advances nothing (never zero-filled). The pool ages but
  never invents cost.
* **Drift:** the pool is reconciled against `S` on every plan (the measured SoC is the truth for
  `kwh`): `kwh := S`, `cost := min(cost, S · max_t p(t))` only when `kwh` would exceed `E_cap` — a
  clamp applied to the *quantity*, never to `b` itself (clamping `b` to a forecast price could lower
  it and loosen the guard). A reconciliation that changes `kwh` by more than 5 % is logged.
* **Empty pool:** `pool.kwh ≤ 0` means nothing is stored and export cannot happen anyway; `b` is
  reported as `None` and the floor falls back to `max_t p(t)`.

Rounding: `b` is rounded **up** to 1e-6 and `break_even` up again, so rounding can only make export
stricter.

### 5.3 Why not the DP's shadow price

The DP knows future prices and could price every export itself, but `salvage = 0` and no terminal
target make a stored kWh worth zero after the horizon, so it would dump the pack in the last slots.
The pool is the standing answer to "what is a stored kWh worth when the horizon ends" and is what
makes R1 stable across replans.

## 6. Edge cases

Every row is a decided behaviour.

| # | Case | Behaviour |
|---|---|---|
| 1 | `S` below the peak guideline (deep discharge, manual override) | `e_target = min(reserve, S)`: the guideline is capped at what exists, so nothing is unreachable. The DP still discharges to serve the house (never an import while energy is stored) and refills toward the guideline when cheap. No recovery mode, no hard floor. |
| 2 | `S ≥ cap(t₀)` (PV already filled the reserve) | no grid charge in that slot; `cap(t)` rises as the forecast surplus is consumed, so a later slot may allow it. PV may always charge to `E_max_hw`. |
| 3 | Pack full, PV surplus, `f < break_even`, surplus below the fleet charge power | the battery does not export (R1); the PV surplus still exports at `f` because it is not battery energy. `battery_kw = 0`, reason `export_below_floor` on the battery leg only. |
| 3b | Pack full, PV arriving that the pack cannot take | the capture budget licenses a below-break-even sale up to `budget · η_rt`; beyond that the surplus is curtailed and `pv_clipped` reports it. |
| 3c | PV surplus with the battery at the ceiling and `f < break_even`, pack *not* full | R1 clamps the battery export; the PV surplus is exported at `f` as before (PV valuation untouched). |
| 4 | `f(t) < 0` | export refused by the existing `sell > 0` rule, independent of the floor. |
| 5 | `p(t) < 0` (negative buy) | grid charging is paid for: the DP will want it. The ceiling still applies (it is a hardware/reserve bound, not an economic one). No special case — being paid to charge is already the cheapest possible energy and the DP sees it. |
| 6 | netting on (2026) | rules inert: `allow_export` stays false, floor stays `marginal_export`, reserves default to today's `reserve_soc_pct` unless the operator sets the new keys. |
| 7 | horizon straddles the netting boundary (2026-12-31 23:45 → 2027) | `Settlement` is per slot, so `f(t)` already switches at the boundary and the export clamp is per slot: netted slots have `f = buy` (above any break-even, so nothing is clamped), post-flip slots are clamped by R1. `plan.sell_floor = max(marginal_export over netted slots, break_even)` only tightens the ladder's export clause. |
| 8 | horizon shorter than 120 slots (tomorrow unpublished) | `S_pv_after` and `cap(t)` are evaluated over the slots that exist; the ceiling is horizon-scoped and re-evaluated next replan. |
| 9 | forecast degraded (EWMA) | the ceiling uses the degraded PV, the plan is already marked degraded, and the policy still runs. A degraded plan is dispatched, not held. |
| 10 | efficiencies not yet learned | `η = 0.95/0.95` defaults (`PackSpec`), so `break_even = b/0.9025 + w` — conservative. |
| 11 | `wear_eur_per_kwh = 0` | allowed; `break_even = b/η_rt`. |
| 12 | mixed packs (different η, different SoC) | capacity-weighted η for the aggregate; floor and ceilings are fleet sums; the per-pack split stays the ladder's `share()`. |
| 13 | operator `terminal_target_pct` also set | one soft target only: the operator value replaces the peak guideline (`e_target`), with the same penalty. Neither is ever a hard floor. |
| 14 | plan refused / stale / held | unchanged: the previous plan is never executed (0.1.1), the packs hold, and the policy never runs on a refused plan. |
| 15 | manual override on a pack | the override wins; the reserves re-assert on the next tick without an override. |
| 16 | protection latch (low cell) | unchanged: the latch forces hold and emergency charge; the reserve logic does not fight it. |
| 17 | two plans in the same slot (boundary + change trigger) | deterministic: same inputs → same pool → same floor; the clamp is pure. |
| 18 | `E_cap = 0` / no packs | refused before the policy (existing `checks`). |
| 19 | pool empty or never anchored | `b` falls back to `max_t p(t)`; the plan is degraded with `cost_basis_anchored` / `cost_basis_missing`. |
| 20 | rounding | `b` up to 1e-6, `break_even` up again: rounding only tightens. |
| 21 | `peak + solar > max_soc − min_soc` | settings validation refuses it against the **hardware** band (`max_soc_pct`, `min_soc_pct`), not against a fixed 100 − min. |
| 22 | sub-slot price variation | 15-minute granularity: the whole slot exports or not, decided by the slot price. |
| 23 | clamp removed charge from every slot that wanted it | the plan still ships (feasible, less profitable) and carries `grid_charge_blocked`. |
| 24 | pool vs measured SoC drift | `kwh := S` on every plan; a >5 % jump is logged; `b` itself is never clamped to a forecast price. |
| 25 | forecast flips between replans | the ceiling is continuous in the forecast; there is no binary gate left, so a flip changes a few Wh of allowed charge, not the plan. |
| 26 | `cap_raw(t) < E_min_hw` | clamped to the hardware floor, `reserve_saturated`, grid charging disabled for the remaining horizon; PV still fills to `E_max_hw`. |
| 27 | `S > E_max_hw` (bad SoC reading) | the clamp starts from `min(S, E_max_hw)` and the plan carries `soc_out_of_band`; the ladder's own bounds still hold. |
| 28 | terminal target above `cap(t)` | the ceiling bounds grid charging only, so PV may still reach the target; a target above `E_max_hw` is refused by the existing `checks`. |
| 29 | export power limit / grid export cap | unchanged: the DP's action window applies both; R1 changes only the price floor. |
| 30 | charging and exporting in the same slot | impossible: one battery power per slot and the grid flow is `base + power`. |
| 31 | netting flips mid-horizon | see row 7. |
| 32 | degraded plan with the policy active | policy runs; the degraded flag is already surfaced; the plan is dispatched. |
| 33 | pack at the peak guideline and the house needs energy | the DP discharges below the guideline — it pays the terminal penalty only at the horizon end — because an import is never preferred over stored energy. Tested explicitly: with `S` at the guideline and a positive load, no slot plans an import while the pack has energy above the hardware minimum. |
| 34 | price spike with the pack at the guideline | the guideline is released: the penalty (0.15) is below the spike, so the DP discharges. The reserve is a preference, not a promise. |

## 7. Configuration

New settings (`energy-server/src/api/settings/schema.rs`, stored in the ledger, editable from the UI):

| Key | Default | Range | Meaning |
|---|---|---|---|
| `peak_reserve_pct` | 20 | 0..50 | floor kept for an unexpected peak |
| `solar_reserve_pct` | 20 | 0..50 | headroom kept free of grid energy |
| `wear_eur_per_kwh` | 0.02 | 0..0.10 | wear per delivered AC kWh |
| `peak_reserve_value_eur_per_kwh` | 0.15 | 0..1 | what a kWh of peak headroom is worth (the soft-target penalty) |
| `cost_basis_eur_per_kwh` | unset | 0..1 | pool anchor / override |
| `export_policy` | `legacy` | `legacy`/`cost_basis` | rollout flag |

`reserve_soc_pct` stays as the hardware floor; the peak guideline is *not* folded into it (that was
the old hard-floor behaviour and is removed). Validation: the two reserves together must fit the
hardware band (`peak + solar ≤ max_soc_pct − min_soc_pct`).

## 8. Observability

Per plan: `exportFloorEurPerKwh` (= `break_even`), `costBasisEurPerKwh` (= `b`, `null` when the pool
is empty), `basisSource` (`pool` / `anchor_override` / `anchor_horizon`), `reserves`
(`peakWh`, `solarWh`, `capWh` at `t₀`), and `bindingReasons` += `export_below_floor`,
`solar_reserve`, `peak_reserve_released`, `reserve_saturated`, `grid_charge_blocked`,
`pv_clipped`, `export_below_floor`, `cost_basis_anchored`, `cost_basis_missing`, `soc_out_of_band`.

The Now page gains one line: *"Selling needs ≥ €0.112/kWh (stored €0.078, wear €0.020)"* and, when
the clamp fired, *"kept 1.5 kWh free for solar"*.

Logs: one `INFO` per plan with basis, source, floor, gate decisions and the kWh the clamp removed.

## 9. Tests

Unit:

* `break_even` for `b = 0`, `b = 0.30`, `η_rt = 1`, `w = 0`, monotonic in each input.
* pool: grid vs PV charge split with load present, average-cost relief on discharge, gaps advance
  nothing, anchor paths, reconciliation, `kwh ≤ 0`, rounding direction.
* ceiling: `S_pv_after` over a horizon, monotonicity, saturation below the hardware floor, empty
  horizon.
* export clamp: a battery export below `break_even` is shortened to the house's need; a
  self-consumption discharge is untouched; PV export is untouched; a slot at exactly `break_even`
  exports; the gate is closed only in slots after which the horizon holds no PV surplus.
* clamp: grid portion only (the 1 kW PV + 2 kW charge case), PV-only charge untouched, discharge
  shortened only at the hardware floor, no change when legal, idempotent, `S` out of band.

Integration (`crates/energy-core/tests/policy.rs`, new), each asserting actual grid flows and cost:

* **never import while stored energy is available**: pack at the peak guideline, positive load, no
  slot imports while energy above the hardware minimum remains;
* **the guideline is released in a spike**: same fixture with a price spike, the pack discharges
  below the guideline, the plan reports `peak_reserve_released`;
* **self-consumption below break-even is not blocked**: buy below `break_even`, pack with energy,
  the plan still discharges into the house;
* cheap-feed-in day: no export, PV self-consumed;
* expensive-feed-in evening: export happens and only above `break_even`;
* **gate placement**: a below-break-even battery export is clamped to the house's need plus the
  capture exemption (`export_below_floor`); the exemption is zero at sunset, zero when the pack can
  absorb the surplus, and capped by the backward budget when it cannot;
* mixed charging: PV 1 kW + grid 2 kW at the ceiling keeps the PV kW;
* four-pack PV allocation: the fleet does not charge above the ceiling from the grid when every pack
  claims the fleet surplus;
* costly stored energy followed by cheap charging: the pool keeps the expensive basis;
* sunny day with a full pack: PV fills to `E_max_hw`, no grid charge above `cap(t)`;
* winter day with no PV: grid charging allowed up to `cap(t)`;
* mid-horizon settlement flip: netted slots sell at `buy`, post-flip slots obey `break_even`;
* forecast vs measurement divergence: the ladder refuses an export the plan expected to be
  self-consumption;
* netting on: byte-for-byte the same plan as today for a fixed fixture (regression).

Property tests (proptest):

* no plan slot exports battery energy with `f < break_even` beyond the capture budget; the budget
  never exceeds the room-limited curtailment of the clamped baseline and is never spent twice; PV
  export is never constrained;
* the clamped path stays within `[E_min_hw, E_max_hw]` and the grid portion within `cap(t)`;
* clamping never increases the plan's cost by more than the removed arbitrage;
* `clamp(clamp(x)) == clamp(x)`.

The differential/oracle is untouched (it validates the solve, the clamp runs after it); one test
asserts the clamp runs after `Differential::verify`.

## 10. Rollout

1. `export_policy = legacy` ships first: code paths and settings exist, behaviour unchanged.
2. `export_policy = cost_basis` in `dry_run` for a week: compare `dispatch_log` intended setpoints
   with the old add-on's actual ones and the pool's basis against the real Zonneplan invoice.
3. Advisory → dry_run → `auto`, per RUNBOOK.
4. After 2027-01-01 the default flips to `cost_basis`; `legacy` stays for one release.

## 11. Open questions for the user

1. **Per-layer basis.** One `b` per plan means a pack holding both cheap PV and expensive grid energy
   uses the blended cost for every slot. A per-layer basis (FIFO lots) would let PV-charged kWh clear
   a lower break-even than grid-charged kWh, which is the direction the user described ("the
   break-even for a PV charge can almost always be reached"). Worth a second spec once the pool has
   run for a month.
2. **The 0.15 €/kWh headroom value.** It decides when the peak guideline is released. It should be
   compared against the real spread between the buy price and the feed-in once the pool has data.
   The guideline is a terminal target, so a plan can still import in an early slot and hold the
   reserve for later; that is the DP's arbitrage, not a policy-forced import. If the user wants the
   reserve released *earlier* than the horizon end, the penalty has to become per-slot (a DP change).
3. **Degraded forecast and the solar ceiling.** With an EWMA PV fallback the ceiling may reserve room
   for PV that will not arrive. Proposal: relax `cap(t)` toward `E_max_hw` in proportion to the
   forecast's confidence when it is degraded; not implemented until the degradation path is measured.
