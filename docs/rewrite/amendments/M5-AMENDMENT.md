# M5 amendment to the frozen rewrite contract

Ruling: `SCENARIOS.md` rows beat `SPEC.md` prose; the owner's M5 brief and its 2026-11-24 solar
ruling outrank both where no row exists; anything still undecided is asked as a single point, not
silently invented. New rows start at S260. Every row below needs reviewer sign-off before merge.
These rows extend, and do not rewrite, the M5 rows already in `SCENARIOS.md`. Reviewer round 10 adds S312-S315, makes measured breach the only runtime arming event, gates manual release on settled evidence and tightens proposal safety to the ceiling.

The SPEC M5 acceptance paragraph names S33–S42, S93–S99, S132–S137, S150, S159, S160, S165 and S169.
S132/S133 belong to M4 flow receipts and S169 to M7 durable sessions; this module claims
S33–S40, S42, S93–S99, S134–S137, S150, S159, S160, S165, S171–S178, S183, S187 and the new
S260–S315 below. M4/M6/M7-only rows are not claimed as M5 tests.

## Implemented override order (one list, first firing command wins)

| Position | Rule | Reason code | Command | Verification | Escalation on verification failure |
|---|---|---|---|---|---|
| 1 | cell discharge latch | `cell_voltage_guard` | 0 W neutral hold | echo + two fresh, distinct, ordered measured `(0,0)` reports | retry 0/2/6 s, then `safety_write_unverified` + `ownership_retention` |
| 2 | cell emergency charge | `cell_voltage_emergency_charge`; impossible: `emergency_charge_impossible` | minimum bounded charge from the cell deficit, capped by pack and aggregate limits; denied without fresh service-headroom evidence; else 0 W hold | echo + cell voltage reaching resume threshold; escalation at 180 s if unresolved | `emergency_charge_unresolved` + ownership retention; hold path as position 1 |
| 3 | low-SoC / minSoc floor | `soc_floor` | no discharge; 0 W hold when a discharge was asked | measured idle flow | same bounded retry + ownership retention |
| 4 | physical service fuse | `physical_service_limit`; unavailable: `service_headroom_unavailable` | protective reduction if fresh headroom is known, else 0 W hold | fresh meter projection ≤ service limit | same bounded retry + ownership retention |
| 5 | import ceiling | `grid_import_ceiling`; impossible: `import_remedy_blocked`; meter gone while armed: `grid_telemetry_missing` | protective discharge/charge reduction from projected import (fleet-total measured battery subtracted, complete selected targets added); else 0 W hold | fresh post-breach settled grid ≤ ceiling − deadband | same bounded retry + ownership retention |
| 6 | full charge / SoC ceiling | `full_charge_protection` | 0 W hold; never charge a 100 % pack | measured idle flow | same bounded retry + ownership retention |
| 7 | export/self-consumption guard | `self_consumption_guard` | 0 W hold when a discharge would export past the hard ceiling | measured flow within the ceiling | same bounded retry + ownership retention |
| 8 | stale pack samples (> 300 s) | `stale_sample_unreachable` | 0 W verified hold; economic reachability only | as position 1 | as position 1 |
| 9 | cumulative observation budget empty | `observation_budget_exhausted` | no nonzero command; 0 W verified hold | as position 1 | as position 1 |
| 10 | orphan hold | `orphan_hold` | 0 W verified hold | as position 1 | as position 1 |
| 11 | cancel ramp | `cancel_ramp` | 0 W verified hold before a changed target | as position 1 | as position 1 |
| 12 | restart grace | `restart_grace` | no ordinary write; 0 W hold if a command is standing | as position 1 | `restart_neutral_unverified` then ownership retention |
| 13 | unresolved safety hold | `hold_unverified` | 0 W hold until the same attempt is verified or a higher rule supersedes it | as position 1 | retry 0/2/6 s then terminal retention until explicit recovery |
| 14 | uncertain write | `controller_uncertain` | 0 W hold for ordinary traffic; safety traffic is never blocked | as position 1 | as position 1 |
| 15 | ordinary fault backoff | `ordinary_backoff` | no command; explicit `Deferred` outcome carrying `retry_at_ms` | no safety command to verify | none; M4 owns the ordinary retry |
| 16 | no command standing | `no_command_needed` | no command; `idle_verified` needs two neutral reports | two fresh `(0,0)` reports | none |
| 17 | ordinary request | `no_safety_hazard` | economic request proceeds | normal M4 receipt | none |

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
| S272 | M5 | emergency charge requires pack limit AND fresh meter | min cell 2.99 V, force-charge latch, `max_charge_w=0`, meter missing; then 800 W pack limit still with no meter; then fresh valid meter | ladder evaluates | no +800 W command while either limit is missing; `emergency_charge_impossible` names `max_charge_w` and `service_headroom_unavailable`; only fresh valid meter evidence permits the charge | unit | no | IN SCOPE | Review item 67: the old row allowed a missing meter with 800 W available. |
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
| S288 | M5 | emergency charge is minimum, bounded and attempt-limited | cell deficit, 1200 W pack cap, four-pack fleet, meter over/under fuse | ladder decides and proof is opened | target is the voltage-deficit estimate capped by pack and 3200 W aggregate share; economic margin may be breached with reason, physical fuse never; proof ATTEMPT duration 180 s bounds the verification attempt only, never physical charging | contract | no | IN SCOPE | Reviewer ruling J1; replaces `max(800 W, requested)`. |
| S289 | M5/M4 | incident regressions assert the execution boundary | each of S260/S261/S262/S263/S264 plus its mutation | dispatcher runs and row/device power are joined to the decision | fixed path asserts device W and report steps; mutation case proves the old defect would be caught at that boundary | regression | yes | IN SCOPE (S276/S277/S279 remain wiring-only) | Review item 43: decision-only assertions could not detect downstream clamps or missing deferral. |

| S290 | M5 | physical service fuse is unconditional | fresh meter projection above service limit; then missing, stale or future-dated meter sample; then operator limit raised | ladder evaluates a charge | protective reduction under the fuse; any unavailable headroom denies the charge with `service_headroom_unavailable`; operator-configured limit is honoured | regression | yes | IN SCOPE (J1 revised) | Review item 47: emergency must fail closed when the fuse argument is absent. |
| S291 | M5 | intermittent fresh samples do not reauthorise | 1000 W charge, stale 30 s, one fresh sample, stale 30 s | ladder evaluates each tick | cumulative observation budget reaches zero and never reauthorises from one fresh sample; 0 W verified hold follows | regression | yes | IN SCOPE (J2 revised) | Ruling J2: per-sample age checks oscillate; budget is credited only by genuinely fresh periods. |
| S292 | M5 | projection uses one fleet measurement basis | grid 6100 W, fleet battery −1600 W, one pack charging and two standing discharges replaced by two +800 W targets | ladder evaluates the transition | battery is fleet-total, the aggregate is subtracted once and complete new targets are added; protective reduction is −800 W and unchanged by stale standing values | regression | yes | IN SCOPE | Review item 48: mixed packs were double-counted. |
| S293 | M5 | nonzero-to-neutral switch makes a new proof | emergency +800 W proof fails once, then a 0 W hold is selected | ladder decides again | new neutral proof has target 0 W, new seq and correlated attempt; superseded escalation history is retained separately | regression | no | IN SCOPE | Review item 49: a neutral command inherited the old nonzero verification target. |
| S294 | M5 | receipts require an active issued attempt | expired, terminal and future-dated receipts offered to matching proofs | proof ingests each receipt | all are rejected with `attempt_not_active` / `future_dated_report`; only explicit recovery leaves terminal state | regression | no | IN SCOPE | Review item 50: matching IDs were enough for inactive attempts. |
| S295 | M5 | future timestamps are rejected before evidence | settled sample dated beyond skew; flow report dated beyond skew | latch and proof ingest them | no release and no verification; skew bound is `FUTURE_SKEW_MS = 5_000` | regression | no | IN SCOPE | Review item 52: future-dated evidence was treated as fresh. |

| S296 | M5 | invalid service evidence fails closed | grid NaN, ±inf, negative, service limit zero or implausible; zero grid reading as the valid boundary | emergency rule evaluates | projection is denied with `service_headroom_unavailable`; `MAX_PLAUSIBLE_POWER_W = 100000 W`; zero import remains a valid reading | regression | no | IN SCOPE | Review item 60: NaN comparisons bypassed the fuse. |
| S297 | M5 | sparse samples drain the bucket | every tick a nearly-stale sample; alternating fresh/stale with each fresh sample almost an interval late | ladder evaluates each tick | each tick debits sample age, credits `max(0, elapsed - age)`, and sustained sparse schedules end in `observation_budget_exhausted` | regression | no | IN SCOPE | Review item 61: interval accounting preserved the bucket. |
| S298 | M5 | terminal proof cannot be escaped by a target change | terminal emergency proof, then a different target is selected | ladder decides again | changed target is recorded as `terminal_target_change_refused`, 0 W is commanded, seq/target stay terminal, and only explicit recovery or reconcile clears it | regression | no | IN SCOPE | Review item 63: a replacement proof restarted escalation. |
| S299 | M5/M4 | disclosed physical limitation | observation budget empties while a nonzero command is standing | bounded hold escalation only | on this hardware the pack retains the last setpoint and exposes no device command expiry; M5 cannot bound physical charging once a command has left. Existing mitigation: verified-hold attempts with bounded escalation; the operator-alert delivery contract is S303 | disclosure | no | DISCLOSED | Review items 62/66: no independent expiry exists; a hold that may not land is not an independent safeguard. |

| S300 | M5 | one missed tick recovers under fresh telemetry | full bucket, one tick with no new sample, then fresh samples every 20 s | ladder evaluates each tick | missed tick leaves only one cadence of credit and denies; genuinely fresh samples refill to capacity and restore authorization within one further tick | regression | no | IN SCOPE | Review item 64: the old debit model could never recover. |
| S301 | M5 | credit is observed coverage, not freshness verdict | sample ages 1, 19, 21 and 39 s at a 20 s decision cadence | ladder observes and decides | `credit = max(0, elapsed - age)`; only the 1 s-old sample sustains authorization, 19/21/39 s samples drain and withdraw; boundary is `age < elapsed` | regression | no | IN SCOPE | Review item 65: a consistently 19 s-old sample previously preserved the bucket. |

| S302 | M5 | accepted future skew is maximally fresh | sample timestamp at `FUTURE_SKEW_MS - 1` and at `FUTURE_SKEW_MS` ahead of decision time | ladder observes and decides | age is clamped to zero before debit/credit; both samples leave the bucket at full `40 s` capacity and authorization is sustained | regression | no | IN SCOPE | Review item 68: negative age became `u32::MAX` and emptied the bucket. |
| S303 | M5/M4/HA | escalation record versus delivered operator alert | a hold proof reaches terminal escalation; HA notify succeeds, fails transiently, then exhausts retries | engine escalation worker, HA client and ledger | RECORDED escalation is written immediately from `HoldProof`; DELIVERED alert requires HA `notify` success; failures retry at 0/30/120 s and are recorded as `safety.alert_delivery_failed`, final failure as `safety.alert_undelivered` | contract | no | REQUIRES WIRING | Review item 69: the alert needs an exact trigger, destination, owner and failure record. |

| S304 | M5 | import remedy converges below the release threshold | breach with valid meter; remedy target compared with `ceiling - deadband - margin`; stable import at the ceiling, then settled import inside the release band | ladder evaluates each snapshot | `IMPORT_CONVERGENCE_MARGIN_W = 50 W`; the protective discharge targets `7500 - 10 - 50 W`, the ceiling-stable case stays latched, and settled evidence at or below `7490 W` releases it | regression | no | IN SCOPE | Review item 76: targeting the ceiling could never satisfy the release predicate. |
| S305 | M5 | hold recovery is proof-only | terminal hold proof plus an armed import latch; recovery called, then latch manually released | ladder recovery and diagnostics | `RecoveryStatus` reports `hold_proof_cleared = true` and `import_protection_active = true`; the row records `import_protection_still_active`; only manual release or valid settled evidence clears import protection | regression | no | IN SCOPE | Review item 77: recovery silently left protection armed. |
| S306 | M5/M4 | manual import release is guarded and audited | armed latch during a live breach; empty reason; after a non-breach snapshot with an explicit actor/reason | ladder manual-release API and row/ledger wiring | live breach and missing audit fields are refused; success returns `ImportReleaseRecord` with pack, actor, reason and instant, records `import_guard_manually_released`, and is retained in `import_release_history()` for the M4 row and ledger | contract | no | IN SCOPE (M4 row and ledger wiring REQUIRES WIRING) | Review item 78: fail-closed release needs an audited escape that cannot bypass live protection. |
| S307 | M5 | unavailable import remedy is explicit | armed latch with `max_discharge_w` below the convergent target and no settled evidence | ladder evaluates | reason is `import_remedy_unavailable`, binding limit names `max_discharge_w`, `settled_grid_evidence_missing` is reported, and the guard stays armed until manual release or valid settled evidence | regression | no | IN SCOPE | Review item 76: an unreachable remedy must not look like normal protection. |

| S308 | M5 | remedied evidence releases the latch without removing protection | high economic request still projects a breach; measured import and settled evidence are below the release threshold | ladder evaluates without a new measured breach | latch releases from measured evidence only, while `!proposal_safe` keeps commanding the same protective discharge on the release tick and every later tick | regression | no | IN SCOPE | Review item 79: the old release branch zeroed `ask_w` and restored the breach. |
| S309 | M5/M4 | manual release audit carries the engine belief | guarded manual release after a non-breach snapshot | release record reaches the row and ledger | `ImportReleaseRecord` retains snapshot instant, grid sample time, proposed plan W, projected and measured grid import W, ceiling, deadband, release threshold, settled grid W/time, evidence-window bounds and all three breach predicates | contract | no | IN SCOPE (M4 row and ledger wiring REQUIRES WIRING) | Review item 80: actor and reason alone do not explain what the engine believed. |

| S310 | M5 | aged evidence does not re-arm the latch | released latch, stale settled evidence, unchanged unsafe proposal | ladder evaluates with no new measured breach | latch stays released, evidence window start is unchanged, and protective discharge keeps flowing from `proposal_safe` | regression | no | IN SCOPE | Review item 79: release was not monotone. |
| S311 | M5 | a new measured breach starts a new episode | released latch then measured import above the ceiling | ladder evaluates the new snapshot | latch re-arms and `import_episode_start_ms` advances to the new breach instant; old settled evidence cannot release it | regression | no | IN SCOPE | Review item 79: only measured truth may arm the latch. |

| S312 | M5 | a recurring import flag cannot arm or rearm the latch | persistent `import_armed=true`, automatic release, then expired settled evidence with no new measured breach | ladder evaluates each snapshot | latch arms only on measured breach, releases on settled evidence, and stays released after evidence expires because the input flag is ignored | regression | no | IN SCOPE | Review item 81: `import_armed` rearmed a new episode every tick. |
| S313 | M5 | manual release requires the same settled evidence as automatic release | armed latch, measured import below the ceiling but no fresh post-episode settled evidence; then valid evidence | `release_import` evaluates both snapshots | first release is refused with `settled_remediation_required`; valid settled evidence allows it and is recorded | regression | no | IN SCOPE | Review item 82: a human could clear protection with no measurement. |
| S314 | M5 | proposal safety stops at the ceiling | projected import `7505 W`, ceiling `7500 W`, deadband `10 W` | ladder evaluates the economic request | projection inside the extra deadband band is NOT safe and commands protective discharge from `proposal_safe`, independently of latch state | regression | no | IN SCOPE | Review item 83: the deadband made an above-ceiling plan executable. |
| S315 | M5/M6 | restart restore needs explicit provenance | process restarts with a previously armed import episode | restore wiring (not implemented by recurring input) | a one-shot RESTORE carries episode id, original `at_ms` and evidence; it never re-arms from ordinary telemetry | contract | no | REQUIRES WIRING | Review item 81: no recurring flag may stand in for episode provenance. |

## Interpretations that need reviewer attention

* The owner was asked about solar headroom before implementation and ruled "advisory headroom only":
  no precedence slot, no command, no feasibility effect, zero-on-missing-telemetry means grant nothing.
  This is implemented literally in `safety::solar` and recorded as S262/S263/S275.
* J2 as corrected by reviewer rounds 4-5 (2026-11-24): the previous `94 s` derivation is RETRACTED.
  M5 bounds AUTHORISATION to charge, not the physical duration of an already-commanded charge.
  `STALE_SAMPLE_MS = 300_000` remains only the economic reachability/unreachable escalation bound
  (S270). Nonzero permission uses a cumulative leaky bucket: capacity `COMMAND_SAMPLE_MAX_MS =
  40 s`; every tick debits the sample's age at decision (bounded by the accepted window) and credits
  `max(0, elapsed - age)` only for a genuinely new in-window sample, capped at capacity. Credit is
  OBSERVED COVERAGE, not the freshness verdict: on a 20 s cadence, a 1 s-old sample credits 19 s and
  refills, while a 19 s-old sample credits 1 s and debits 19 s, so it drains (S300/S301). The
  lateness boundary is `age < elapsed`; at or beyond it, credit is zero. Authorization requires
  `AUTHORIZATION_MIN_MS = DECISION_CADENCE_MS + FUTURE_SKEW_MS = 25 s`, from the 20 s decision
  cadence plus the 5 s accepted skew. The 74 s `SAFETY_ATTEMPT_DEADLINE_MS` bounds one verification
  ATTEMPT only; it is not a physical bound. Finding 62 is **DISCLOSED**: on this hardware the pack
  retains the last setpoint and exposes no device command expiry or watchdog, so M5 cannot bound
  physical charging once a command has left. The zenSDK command API exposes no setpoint TTL or
  watchdog; the server's `ttlS` override is an API-level override lease, not a device command expiry.
  The mitigation that exists here is verified-hold attempts with bounded escalation to an
  OPERATOR ALERT; wiring that alert path is S303 (`REQUIRES WIRING`). A hold whose landing is not
  guaranteed is not claimed as an independent safeguard. No derived-looking number is presented as
  a physical guarantee.
* Import decoupling (reviewer round 9): the module computes four independent things every tick.
  `breach_now` is MEASURED grid import above the ceiling only, and it is the only runtime
  latch-arming event; the recurring `input.import_armed` flag neither arms nor re-arms the latch.
  Restoring an episode after restart, if ever needed, requires an explicit one-shot RESTORE with its
  own episode id, original `at_ms` and evidence provenance (S315, REQUIRES WIRING). `proposal_safe` is the projected import of the proposed economic plan (including its
  battery contribution) and only decides whether that plan may run. `protective_command_w =
  min(0, ceiling - deadband - 50 W - fleet_baseline_w - coordinated_w)` is computed from the current
  snapshot, never from latch state, and is the command whenever `proposal_safe` is false. The latch
  itself is diagnostic state: armed only by `breach_now`, released only by fresh settled MEASURED
  import at or below `release_threshold_w = ceiling - deadband`, and never armed or released by a
  proposal projection or a recurring input flag. Only a new measured breach starts a new evidence window, so aged evidence
  cannot re-arm the latch and an unchanged unsafe proposal cannot either (S310/S311). Command
  selection is exactly `if !proposal_safe { protective_command } else { plan }`; because protection
  comes from `proposal_safe`, releasing the latch cannot remove it (S308). `recover_hold` is
  proof-only and returns `RecoveryStatus`; the guarded `release_import(pack, actor, reason, at_ms)`
  refuses a blank audit or a measured breach and records pack, actor, reason, instant, snapshot and
  grid sample times, projected and measured watts, thresholds, settled evidence, evidence-window
  bounds and all breach predicates for the M4 row and ledger wiring (S306/S309). Nothing here has a
  silent timeout.
* Operator-alert path (reviewer round 6, S303): the EXACT RECORDED trigger is a `HoldProof` whose
  `attempt_state == Terminal` and `escalation.exhausted()` after the third failed bounded attempt
  (`0/2/6 s`) or after `SAFETY_ATTEMPT_DEADLINE_MS = 74 s` without verification. At that instant
  `energy-decide` records `safety_write_unverified` plus `ownership_retention`
  (`emergency_charge_unresolved` on the cell-emergency path) and exposes the terminal proof; this is
  RECORDED escalation and needs no transport. DELIVERED alert is separate: `energy-server`'s engine
  escalation worker reads that terminal proof and calls the HA `notify` service through the existing
  `energy-ha` WebSocket client's `call_service`; the integration owner is `energy-ha`, with the
  configured HA notify target, and no new service is introduced. The checker's findings channel may
  surface the ledger event but is not the primary alert transport. Delivery failure retries at
  0/30/120 s; every failure appends a ledger event `safety.alert_delivery_failed` containing pack,
  `safety_seq`, `attempt_id`, escalation path, failure reason and retry count, and exhaustion appends
  `safety.alert_undelivered` at ERROR level. The failure is therefore visible even when no user
  notification arrives; only HA success may set `safety.alert_delivered`.
* J1 revised (reviewer ruling, 2026-11-24): `physical_service_limit_w` defaults to 7700 W and is
  operator-configurable. It is an unconditional hard bound on EVERY nonzero path. Emergency charge
  is permitted only when projected import including the emergency charge is at or below it, using
  fresh meter evidence (`grid_sample_at_ms` within `COMMAND_SAMPLE_MAX_MS`, no future skew beyond
  5 s). Missing, stale or future-dated service evidence DENIES the emergency with
  `service_headroom_unavailable` and escalates. The 3200 W aggregate share and 180 s attempt
  duration bound only the emergency AUTHORIZATION magnitude and VERIFICATION ATTEMPT duration; they
  are not physical runtime guarantees and not fuse safety. Emergency charge requires fresh valid
  meter evidence, full stop. The fuse argument is the service rating plus measured load, and only
  that (S290).
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
  S83/S99/S178. Emergency charge uses a separate 180 s verification ATTEMPT duration, not a physical runtime bound (J1).
* Clearance, latch persistence, dashboard text and all M6 authority states stay in M6. M5 only
  states what protection would do; M4 obeys and records, M6 authorises (S276).
* The no-write guarantee in `dry_run_latched` is not reimplemented in M5; the gate is never asked
  after M6 denial. No M5 code path writes to a device.
