# M5 amendment to the frozen rewrite contract

Ruling: `SCENARIOS.md` rows beat `SPEC.md` prose; the owner's M5 brief and its 2026-11-24 solar
ruling outrank both where no row exists; anything still undecided is asked as a single point, not
silently invented. New rows start at S260. Every row below needs reviewer sign-off before merge.
These rows extend, and do not rewrite, the M5 rows already in `SCENARIOS.md`. Reviewer round 2 adds S280-S289.

The SPEC M5 acceptance paragraph names S33–S42, S93–S99, S132–S137, S150, S159, S160, S165 and S169.
S132/S133 belong to M4 flow receipts and S169 to M7 durable sessions; this module claims
S33–S40, S42, S93–S99, S134–S137, S150, S159, S160, S165, S171–S178, S183, S187 and the new
S260–S289 below. M4/M6/M7-only rows are not claimed as M5 tests.

## Implemented override order (one list, first firing command wins)

| Position | Rule | Reason code | Command | Verification | Escalation on verification failure |
|---|---|---|---|---|---|
| 1 | cell discharge latch | `cell_voltage_guard` | 0 W neutral hold | echo + two fresh, distinct, ordered measured `(0,0)` reports | retry 0/2/6 s, then `safety_write_unverified` + `ownership_retention` |
| 2 | cell emergency charge | `cell_voltage_emergency_charge`; impossible: `emergency_charge_impossible` | minimum bounded charge from the cell deficit, capped by pack and aggregate limits; never past the physical service limit; else 0 W hold | echo + cell voltage reaching the resume threshold; escalation at 180 s if unresolved | `emergency_charge_unresolved` + ownership retention; hold path as position 1 |
| 3 | low-SoC / minSoc floor | `soc_floor` | no discharge; 0 W hold when a discharge was asked | measured idle flow | same bounded retry + ownership retention |
| 4 | import ceiling | `grid_import_ceiling`; impossible: `import_remedy_blocked`; meter gone while armed: `grid_telemetry_missing` | protective discharge/charge reduction from **projected** import (demand + requested + coordinated pack commands); else 0 W hold | fresh post-breach settled grid ≤ ceiling − deadband | same bounded retry + ownership retention |
| 5 | full charge / SoC ceiling | `full_charge_protection` | 0 W hold; never charge a 100 % pack | measured idle flow | same bounded retry + ownership retention |
| 6 | export/self-consumption guard | `self_consumption_guard` | 0 W hold when a discharge would export past the hard ceiling | measured flow within the ceiling | same bounded retry + ownership retention |
| 7 | stale pack samples (> 300 s) | `stale_sample_unreachable` | 0 W verified hold; economic reachability only | as position 1 | as position 1 |
| 8 | samples older than two tick periods | `stale_command_sample` | no nonzero command; 0 W verified hold fallback | as position 1 | as position 1 |
| 9 | orphan hold | `orphan_hold` | 0 W verified hold | as position 1 | as position 1 |
| 10 | cancel ramp | `cancel_ramp` | 0 W verified hold before a changed target | as position 1 | as position 1 |
| 11 | restart grace | `restart_grace` | no ordinary write; 0 W hold if a command is standing | as position 1 | `restart_neutral_unverified` then ownership retention |
| 12 | unresolved safety hold | `hold_unverified` | 0 W hold until the same attempt is verified or a higher rule supersedes it | as position 1 | retry 0/2/6 s then terminal retention until explicit recovery |
| 13 | uncertain write | `controller_uncertain` | 0 W hold for ordinary traffic; safety traffic is never blocked | as position 1 | as position 1 |
| 14 | ordinary fault backoff | `ordinary_backoff` | no command; explicit `Deferred` outcome carrying `retry_at_ms` | no safety command to verify | none; M4 owns the ordinary retry |
| 15 | no command standing | `no_command_needed` | no command; `idle_verified` needs two neutral reports | two fresh `(0,0)` reports | none |
| 16 | ordinary request | `no_safety_hazard` | economic request proceeds | normal M4 receipt | none |

Solar PV reserve has **no position** in this list. The owner's ruling is verbatim: it never commands,
never constrains feasibility and never reduces or zeroes a planned charge. It reports PV-sourced
charge headroom plus a reason; with missing or stale solar/load telemetry the headroom pins to
`0 kWh` (`solar_telemetry_missing` / `solar_load_telemetry_missing`). Zero means *grant no extra
PV-derived allowance*, not "issue a smaller charge". A sunless night with future PV in the forecast
still gets the plan's exact grid charge. Giving the rule a precedence slot would invite someone to
make it constrain later, so `RULE_ORDER` deliberately excludes it.

| ID | Module | Title | Given | When | Then | Type | REAL | Scope | Why |
|---|---|---|---|---|---|---|---|---|---|
| S260 | M5 | safety action not blocked by the failure it protects against | import breach 7900 W, import guard armed, ordinary fault backoff active, stale plan, missing grid telemetry after arming, SoC above floor, cell clear | ladder evaluates an economic -800 W request | protective discharge (or 0 W hold if physics blocks it) is commanded with `grid_import_ceiling` and `ordinary_backoff` in the ordered reasons; no rule suppresses it | regression | yes | IN SCOPE | 2026-11-22/24 auto-mode incident: import protection disabled while engine sat degraded. |
| S261 | M5 | no hold is awaited inside the tick | four packs, one needs a 0 W hold whose flow evidence never arrives | four `preempt` calls run in one slot | every call returns from decision state alone; no sleep, transport call or await; pending verification is recorded as `Deferred` with a retry instant | regression | yes | IN SCOPE | 2026-11-23 15:45Z flow-confirmation stall: health 503 and Supervisor restart loop (S221). |
| S262 | M5 | solar reserve never zeroes a grid charge | reserve 0.786 kWh, SoC 7.131 kWh, ceiling 7.074 kWh, forecast future PV > 0, measured PV 0 and load 500 W, plan +3403 W grid charge | ladder evaluates the plan | exactly +3403 W is allowed; solar headroom is 0 with `no_measured_pv_surplus`; no solar reason reduces or blocks the charge | regression | yes | IN SCOPE | 2026-11-22 sunless November night: post-solve solar clamp zeroed the charge and the ladder enforced it twice. |
| S263 | M5 | missing solar/load telemetry grants nothing | solar or load sample absent or older than 30 s, plan asks any charge | ladder reports solar headroom | headroom pins to 0 kWh with `solar_telemetry_missing` or `solar_load_telemetry_missing`; no PV-derived allowance is granted; grid charge is still governed by plan/import/cell/pack limits only | fault | no | IN SCOPE | Owner ruling 2026-11-24: fail closed means grant nothing, never command less. |
| S264 | M5 | full-charge pack is normal, not a fault | pack at 100 % SoC, `faultLevel=2`, `is_error=false`, plan +800 W, repeated ticks | ladder evaluates the request twice | `full_charge_protection` commands 0 W both times; no fault escalation, no emergency-charge reason, no nonzero command into the pack | regression | yes | IN SCOPE | 2026-11-23/24 live engine kept commanding charge into a 100 % pack, costing about half a planned charge. |
| S265 | M5 | one ordered rule list with visible winner | every mutually satisfiable hazard fires in one snapshot | ladder evaluates one request | winner is the lowest-position rule with a command; all firing reason codes appear in `RULE_ORDER` order; `Rule::position`, reason, command, verification and escalation are asserted per row | contract | no | IN SCOPE | Review M5 item 1 and 4: reporting order was not execution precedence. |
| S266 | M5 | echo alone never verifies a neutral hold | hold issued; echo arrives; no or nonzero flow reports follow | proof observes reports | hold stays unverified with `flow_not_neutral`/`neutral_evidence_stale`; only echo plus two fresh finite idle `(0,0)` reports produce `hold_verified_by_measured_flow` | regression | yes | IN SCOPE | GAPS 20/S24/S85: `inputLimit=500` echoed 150 s with 0 W delivered. |
| S267 | M5 | safety retries are bounded and escalate visibly | hold verification fails at 0 s, 2 s and 6 s | escalation advances | next attempts are derived from elapsed `retry_at_ms` (0/2/6 s); after three failures `safety_write_unverified` and `ownership_retention` are recorded and ordinary backoff is untouched | regression | no | IN SCOPE | Review M5 item 2: safety lane needs its own receipt/retry/escalation state. |
| S268 | M5 | uncertain write blocks ordinary retry, not safety | pack uncertain, economic -800 W and one cell-latch safety request | ladder evaluates both | economic path gets `controller_uncertain` + 0 W hold until neutrality is proven; `safety_traffic` request is never suppressed by uncertainty or backoff | regression | yes | IN SCOPE | S225: retries against a possibly standing command cannot be trusted; invariant 6. |
| S269 | M5 | contradictory soft guideline and hard floor | soft 0.78575 kWh, floor 1.178625 kWh, ceiling 7.8575 kWh | ladder derives target | effective target is 1.178625 kWh (`max(soft, floor).min(ceiling)`), raw 0.78575 kWh is preserved in the decision; no zeroed grid charge | unit | no | IN SCOPE | Invariant from brief; owns only the resolution, M3 remains owner of plan targets. |
| S270 | M5 | stale sample stops reserving a pack silently | last sample 301 s old, no other hazard, plan asks charge | ladder evaluates the request | `stale_sample_unreachable` commands a 0 W verified hold; if neutrality cannot be proven, escalation follows S267 | regression | no | IN SCOPE | Owner ruling 2026-11-24: live reservation had no TTL. New documented bound 300 s needs reviewer sign-off. |
| S271 | M5 | import remedy respects physics but still holds | import breach, `max_discharge_w=0` or SoC at floor, cell clear | ladder evaluates | `import_remedy_blocked` names the binding limit, commands 0 W and records `soc_floor` before `grid_import_ceiling` when both bind | regression | no | IN SCOPE | S37/S135/S150/S160; GAPS 81. |
| S272 | M5 | emergency charge impossible by pack limit | min cell 2.99 V, force-charge latch, `max_charge_w=0`, meter missing | ladder evaluates | no +800 W command; `emergency_charge_impossible` names `max_charge_w`; with 800 W available the charge proceeds despite missing meter | unit | no | IN SCOPE | S93; cell protection must not wait for grid telemetry. |
| S273 | M5 | gate decision and safety execution stay separate | gate preempts with a hold; M4 row reports accepted, rejected or timed out | report joins row and decision | execution comes from M4's `safety_execution`, never from the decision; rejected/timeout is not `Skipped`; order of preemption reasons is unchanged | contract | no | IN SCOPE | S229/S236: a gate reason must not hide a failed safety action. |
| S274 | M5 | protective discharge is expressible through the M4 port | import breach with settled demand 8600 W and ceiling 7500 W | gate returns its intervention | `SafetyAction::ProtectiveDischarge{target_w:-1100.0}` carries the signed W; existing `NeutralHold`/`EmergencyCharge` behaviour is unchanged | contract | no | IN SCOPE (public M4 enum extended; firmware wiring REQUIRES WIRING) | S09/S35 require a discharge, which the provisional two-variant M4 enum could not express. Loud public-behaviour note in the report. |
| S275 | M5 | solar advice is outside the precedence list | any hazard combination plus solar telemetry | ladder evaluates and reports | no solar rule appears in `RULE_ORDER`; solar value/reason is reported after all rule reasons and never changes `winner`, `action` or `command_w` | contract | no | IN SCOPE | Owner ruling 2026-11-24 item 5. |
| S276 | M5/M6 | M5 decides, M6 authorises | cell latch while authority is `dry_run_latched`, then after clearance commits to `auto` | both modules run | M5's decision is unchanged; zero device writes while latched, +800 W after authority commits; M5 never reads or mutates the latch | contract | no | REQUIRES WIRING TO M6 | S134/S145/S156/S197; this crate owns no authority. |
| S277 | M5/M4 | late economic write cannot restore nonzero flow | 0 W safety hold confirmed; old -800 W write lands later | fence and reports are folded | no neutral success while the old command remains effective; the hold must be re-proved or the write fenced | regression | yes | REQUIRES WIRING TO M4 | S159; M5 records the decision, M4 owns the write lane and fence. |
| S278 | M5 | import latch needs settled-grid evidence to release | breach at 7900 W arms the guard; a transient 7400 W sample follows; settled estimate 7480 W arrives later | ladder evaluates each snapshot | transient dip alone does not release the guard; only `settled_grid_w ≤ ceiling − deadband` releases it, with the guard latched until then | regression | yes | IN SCOPE | S35: release only at or below 7500 − 10 W even if a transient meter dip reads 7400 W. |
| S279 | M5 | safety receipt is correlated to seq and attempt | hold seq 7, attempt A1 fails; a late A0 receipt and a fresh A1 receipt both arrive | proof folds each receipt | A0 is `late_uncorrelated` and advances no evidence; only A1's echo plus measured idle flow counts; the retry gets a fresh `attempt_id` | regression | yes | IN SCOPE (M4 transport must expose seq/attempt on the real receipt — REQUIRES WIRING) | S137: a receipt matching both ids alone advances safety state. |

| S280 | M5 | import guard prices the requested and coordinated charge | demand 7400 W, ceiling 7500 W, four 800 W pack commands including self | ladder evaluates the slot | projected import 10.6 kW arms `grid_import_ceiling`; the remedy includes the coordinated W | regression | yes | IN SCOPE | Review item 35: demand alone hid the planned breach. |
| S281 | M5 | settled release needs fresh post-breach evidence | breach latched; later contradictory or retained low settled samples; then a fresh post-breach sample | ladder evaluates each snapshot | no release while current evidence breaches or the settled sample is old/pre-breach; release only on fresh post-breach settled evidence after the hold is proven/recovered | regression | yes | IN SCOPE | Review item 36: a retained low estimate cancelled a live breach. |
| S282 | M5 | an unresolved hold blocks economic traffic | cell hold issued, hazard clears before any neutral receipt | ladder evaluates the next economic request | `hold_unverified` keeps a 0 W hold until the same attempt verifies or explicit recovery; higher-priority rules still supersede | regression | yes | IN SCOPE | Review item 37: predicate clearance allowed an economic write before neutrality. |
| S283 | M5 | neutral evidence is distinct, ordered and unbroken | duplicate, out-of-order and nonzero reports interleaved with neutral ones | proof folds receipts | duplicates/out-of-order reports advance nothing; nonzero flow invalidates the neutral run; resumed flow forces re-proof | regression | yes | IN SCOPE | Review item 38: repeated/contradicted reports could verify and stay verified. |
| S284 | M5 | retries never inherit evidence | attempt 1 has echo + neutral; attempt 1 receipts replay after failure | proof retries to attempt 2 | all attempt-local evidence and issue time reset; an old echo plus old neutral plus one new neutral does not verify | regression | yes | IN SCOPE | Review item 39: retries verified from the old attempt. |
| S285 | M5 | escalation advances from elapsed time and stays terminal | no receipts arrive; repeated ticks cross 74 s, retries 0/2/6 s and exhaustion | ladder decides on each tick | deadline/retry transitions happen inside the core; after three failures the outcome is `Failed`, the proof is retained, and only explicit recovery clears it | regression | no | IN SCOPE | Review item 40: missing receipts stayed pending forever and a later tick reset the budget. |
| S286 | M5/M4 | backoff is a deferral, not permission | backoff is the only firing rule | gate and M4 row are evaluated | `DecisionOutcome::Deferred` and `SafetyIntervention::Defer` carry `retry_at_ms`; the M4 row is `Deferred`, never `Proceed` | regression | no | IN SCOPE (M4 port extended; existing variants unchanged) | Review item 41: an actionless candidate became `no_safety_hazard`. |
| S287 | M5 | stale samples stop every nonzero action | 41 s old cell/meter/pack samples under emergency, import and ordinary charge rules | ladder evaluates each case | no nonzero command is granted; each path falls back to a verified hold and bounded escalation; 300 s remains only the economic-reachability TTL | regression | yes | IN SCOPE | Review item 42 and J2: a stale hold sat below the nonzero actions. |
| S288 | M5 | emergency charge is minimum, bounded and time-limited | cell deficit, 1200 W pack cap, four-pack fleet, meter over/under fuse | ladder decides and proof is opened | target is the voltage-deficit estimate capped by pack and 3200 W aggregate share; economic margin may be breached with reason, physical fuse never; proof duration 180 s | contract | no | IN SCOPE | Reviewer ruling J1; replaces `max(800 W, requested)`. |
| S289 | M5/M4 | incident regressions assert the execution boundary | each of S260/S261/S262/S263/S264 plus its mutation | dispatcher runs and row/device power are joined to the decision | fixed path asserts device W and report steps; mutation case proves the old defect would be caught at that boundary | regression | yes | IN SCOPE (S276/S277/S279 remain wiring-only) | Review item 43: decision-only assertions could not detect downstream clamps or missing deferral. |

## Interpretations that need reviewer attention

* The owner was asked about solar headroom before implementation and ruled "advisory headroom only":
  no precedence slot, no command, no feasibility effect, zero-on-missing-telemetry means grant nothing.
  This is implemented literally in `safety::solar` and recorded as S262/S263/S275.
* The owner also ruled that a permanently stale reservation is unsafe and asked for a documented
  bound. Two numbers now apply. `STALE_SAMPLE_MS = 300_000` (5 min) is only the economic
  reachability/unreachable escalation bound (S270). A nonzero command needs a sample no older than
  `COMMAND_SAMPLE_MAX_MS = 40_000`: two 20 s tick/sampler periods, so one missed sample does not
  revoke permission but two consecutive missed periods do. Past 40 s M5 stops granting the nonzero
  command, issues a verified-hold fallback and escalates (S287). Reviewer ruling J2, 2026-11-24.
* SPEC's ordered list names six steps but omits the floor, ceiling, backoff, uncertainty and solar
  cases. SCENARIOS S37/S150/S225 decide floor and safety-lane exemption; the remaining positions
  follow M4's own `ordinary_deferral` order (transport, uncertainty, backoff, pause) and are listed
  above for sign-off.
* M4's provisional `SafetyAction` had only hold and emergency charge. A protective discharge is a
  different signed command, so M5 adds `ProtectiveDischarge { target_w }` to that enum. Existing
  variants and their `target_w()` values are untouched. `SafetyIntervention` also gains
  `Defer { reason, retry_at_ms }`, and `preparation.rs` maps it to the existing `Deferred` M4 row;
  existing safety variants remain unchanged. These public-enum additions are called out loudly in
  the implementation report.
* M5 verifies holds from echo plus two distinct, ordered, fresh measured `(0,0)` reports and
  device-clock coherent status, reusing M4's `FlowStatus`/echo predicates. A contradictory report
  invalidates the neutral run and resumed flow forces re-proof. The 30 s report freshness bound and
  74 s total attempt deadline come from SPEC/0.15.12; the two-report requirement comes from
  S83/S99/S178. Emergency charge uses a separate 180 s attempt duration (J1).
* Clearance, latch persistence, dashboard text and all M6 authority states stay in M6. M5 only
  states what protection would do; M4 obeys and records, M6 authorises (S276).
* The no-write guarantee in `dry_run_latched` is not reimplemented in M5; the gate is never asked
  after M6 denial. No M5 code path writes to a device.
