# Reviewer traceability — rewrite documentation pass

One row per numbered reviewer item in `reviews/M1.md` … `reviews/M7.md`, plus the additional live M7
evidence supplied after the review. "Resolved" means the contract in `SPEC.md` and at least one
objective scenario in `SCENARIOS.md` now make the item checkable. No reviewer item was rejected; the
only deferred proposal is M2's optional learning-uncertainty propagation, which the reviewer
themselves marked "cut/defer".

| Module | Item | What changed | Spec section | Scenarios |
|---|---|---|---|---|
| M1 | 1 | Requested coverage fixed at 120×900 s from the containing slot; actual coverage separate; <96 slots or interior gap refuses; 96–119 tail-only is degraded with typed metadata reaching dispatch | M1 / Coverage | S03, S61 |
| M1 | 2 | `PriceResolution` for non-900 s slots, `PriceGap` for absent/misaligned required slots, deterministic precedence, no resampling/zero-fill/shortening after either error | M1 / Resolution and precedence | S01, S02, S62 |
| M1 | 3 | Europe/Amsterdam per-slot regime; 2027-01-01 local transition; explicit override with acknowledgement/expiry; invalid value is `UnknownRegime` | M1 / Net-metering schedule | S04, S05, S63, S64 |
| M1 | 4 | Required vs optional grid sources; partial and all-empty typed outcomes; `plan.quality.grid_source_coverage` | M1 / Grid-source completeness | S06, S66 |
| M1 | 5 | Unknown/zero/exhausted budget states; conservative `feed_in_post` valuation by default; explicit acknowledged untracked budget permits 1:1 | M1 / Net-metering schedule | S04, S65 |
| M2 | 1 | Complete minimised objective written with every term, coefficient, unit and live value; absolute terminal-target penalty and salvage sign made explicit | M2 / INTENDED contract — complete objective | S15, S16, S17, S67, S74 |
| M2 | 2 | Horizon-wide post-solve clamp removed; per-slot curtailment account with actual `curtail_kw·dt`, no overlapping window feedback; hold condition qualified by losses, wear, constraints and terminal costs | M2 / Curtailment, not overlapping “spill windows” | S08, S70, S117 |
| M2 | 3 | Verified prices replace mislabelled fixture; today's max 0.4188, tomorrow 0.5303; holding today is correct; real defect narrowed to the refused 0.3498 charge for tomorrow's sale; timestamped 5.0 kWh fixture with equal final energy | M2 / honest defect, timestamped fixture and tie-break | S07, S08, S68, S115, S116 |
| M2 | 4 | Hard inequality `−export_limit ≤ load−pv+p ≤ import_limit` stated, unlimited export as `+∞`, forecast net load source specified, infeasible window intersection named, remaining-duration rule applied everywhere | M2 / complete objective, site inequalities | S09, S71, S72, S120 |
| M2 | 5 | Energy transition with η boundaries, EUR/kWh prices, wear on stored kWh, terminal minimum semantics and the `η_rt·sell − buy − wear_cycle` identity; salvage later corrected to consistent replacement cost | M2 / Energy boundaries and units, Economics contract | S67, S68, S73, S74, S118 |
| M2 | cut/defer | Learning uncertainty interval intentionally not propagated through optimizer; reviewer marked it cut/defer. Conservative measurement and floor handling stay in M7/M2; no scenario needed | M2 / Acceptance (explicit defer note) | — (reasoned defer) |
| M3 | 1 | `healthy`, `degraded_accepted`, `infeasible` states with dispatch permissions; delivery failure independent; safety lane allowed in every state | M3 / Plan states and dispatch permissions | S20, S23, S75, S79 |
| M3 | 2 | Final timeline validated after maintenance and SoC recomputation; shortfall/deadline/reason preserved; required vs optional maintenance classified | M3 / Final validation, Structured infeasibility | S22, S77, S78, S79 |
| M3 | 3 | Clipping recorded as `binding_bound`; infeasibility only for violated hard constraints or unmet required outcomes; terminal target is a minimum with 0.05 kWh tolerance | M3 / Final validation, Terminal target | S20, S76, S77 |
| M3 | 4 | `floor > ceiling` refused as `InvalidBounds` before solve; same effective target in solver and validator; raw guideline separately labelled; below/above/conflicting/post-clamp cases specified | M3 / Terminal target | S15, S21, S75, S76 |
| M4 | 1 | Eligibility, target, limits and precedence table for cell, import, operator, maintenance, plan, grid-follow and ramp; justified hold vs failure distinguished | M4 / Mode and precedence | S81, S99 |
| M4 | 2 | Six-row outcome table: hold, full, partial, echo-only, read failure, no-flow; written/verified/flow_measured/applied_w independent; per-pack evidence required after relevant command | M4 / Outcome table | S24, S30, S41, S82–S87 |
| M4 | 3 | Standing lifecycle requested→ack→confirmed→released; requested/written/confirmed/applied reported separately; fail-safe hold retries at 0/2/6 s then escalation | M4 / Outcome table | S88, S89 |
| M4 | 4 | 45 s ramp defined with 30 W deadband and post-ramp verification; 120 s partial-flow pause release and interruption conditions specified | M4 / Ramp and pause | S25, S84, S90, S91 |
| M4 | 5 | `applied_w` exclusively fresh signed measurement or null; advisory diagnostic rows no longer claim 0 W; backoff rows carry attribution and freshness | M4 / Outcome table | S29, S41, S43, S92 |
| M5 | 1 | Execution precedence for cell discharge latch and emergency charge; hard-limit conflict resolution; `emergency_charge_impossible` with binding limits and `ProtectionBlocked` | M5 / Execution precedence | S33, S34, S93, S94 |
| M5 | 2 | Safety lane enumerated with separate sequence/receipt/retry state; bypasses ordinary backoff, plan tracking, grid telemetry and import gates; physical limits and dry-run economic suppression preserved | M5 / Safety write lane | S33, S34, S95, S92 |
| M5 | 3 | Orphan hold, cancel ramp, restart grace and no-command-standing defined as first-class transitions with verified neutral writes and precedence | M5 / First-class transitions | S28, S42, S96–S99 |
| M5 | 4 | Simultaneous-hazard and write-path-failure acceptance matrix with ordered reasons and bypass outcomes | M5 / Acceptance | S33–S38, S93–S99 |
| M6 | 1 | Never parked in advisory/dry_run: only `auto` persists; diagnostic session 900 s with countdown; safety active; startup, invalid config, persisted state, API and expiry all specified | M6 / Operational mode state machine | S43, S44, S48, S100–S102 |
| M6 | 2 | 900 s normal cadence, 1800 s stale bound, 90/180/300 s refusal retry, persistent-refusal event and recovery without a new input change | M6 / Replan cadence and recovery | S45, S46, S103 |
| M6 | 3 | Source precedence: only fresh solved and persisted plan executable; previous/stored invalidated on failure; freshness/eligibility checks | M6 / Plan-source precedence | S46, S104 |
| M6 | 4 | Command-triggered replans share the failure policy; typed reply includes requested/attempted/outcome/reason/retry_at/generation | M6 / Command-triggered replans | S105 |
| M6 | 5 | Failed reads preserve change markers; 3-attempt/3 s contention bound; poison rebuild with event; forced solve before dispatch on recovery | M6 / Change detection and lock failures | S47, S106 |
| M7 | 1 | Capacity numerics made enforceable: span ≥5, linear span quality, recency 0.5^(rank/3), formula for charge/discharge samples, plausible [1,200] kWh, MAD 3.5σ, confidence `1−exp(−points/400)`, ordinary/aging bands and self-excluding pool | M7 / capacity numerics | S14, S49, S107 |
| M7 | 2 | Verbatim contract adopted: durable extraction and per-pack consumption checkpoints advanced only after results are persisted, idempotent and restart-safe; retry from start of open runs | M7 / durable progress and outcomes | S51, S108 |
| M7 | 3 | Per-pack attempt outcomes with separate capacity/efficiency results, accepted/rejected counts, all named blockers, attempt IDs and persistence outcomes on every due/manual pass | M7 / durable progress and outcomes | S109, S110 |
| M7 | 4 | Meaningful-cycle contract: block construction, 0.20 kWh/10-point floors, 7-day gap, hold drift, closure, no reuse, refusal barriers, effective pairs, physical [0.50,1.00] bounds, independence of capacity/efficiency | M7 / efficiency pairing | S50, S110, S114 |
| M7 | 5 | `updatedAt` per metric = newest accepted measurement's `ended_at`; pooled/replay/manual passes preserve it; `lastAttemptAt` and `persistedAt` separate | M7 / durable progress and outcomes | S52, S53, S111 |
| M7 | 6 | Full-charge due anchoring, partial-fill continuation with `missing_kwh`, telemetry-based qualification, 60 min dwell interruption rules, idempotent completion and persistence-failure outcome | M7 / durable progress and outcomes | S54–S59, S112 |
| M7 | 7 | Concrete acceptance tests for stall, restart/persistence, exhaustive outcomes, shallow-session confidence, stale prior, invalid efficiency bounds, partial-fill continuation and unchanged timestamps while engine active | M7 / Acceptance | S49, S51, S107–S114 |
| M7 | L1 | Accepted-pair shape defined: 0.20 kWh and 10 SoC points per leg, ≤7-day gap, drift/closure bounds, multi-segment charge is one block, 0 W 100 % hold is not a session | M7 / efficiency pairing | S114 |
| M7 | L2 | Micro-sessions below 50 W or 0.15 kWh discarded before candidates as `idle_blip`; 0 W day yields `no_meaningful_cycle`, not 400 refusals | M7 / capacity numerics, efficiency pairing | S113 |
| M7 | L3 | Explicit rule: learner never manufactures a number; `no_measurement`/`no_meaningful_cycle` with reasons is correct | M7 / efficiency pairing, durable outcomes | S109, S113, S114 |
| M7 | L4 | Two live-evidence scenarios added: 48 h of 0 W micro-sessions, and multi-segment charge + 4 h hold + evening discharge | M7 / Acceptance | S113, S114 |
| M7 | L5 | Reviewer wording copied verbatim into the contract sentence on durable extraction and per-pack consumption checkpoints | M7 / durable progress and outcomes | S108 |
| M2-R2 | 1 | Replaced the infeasible 6.4 kWh comparison with one 120-slot timestamped fixture: initial/final 6.995 kWh, every transaction, capacity checks, equal final energy and identical recharge cost/price | M2 / honest defect, timestamped fixture and tie-break | S116, S123 |
| M2-R2 | 2 | Removed overlapping prospective-shortage sums; curtailment is a physical variable in the grid balance, uses `dt_i`, and stored-vs-AC boundary uses `(e_max−e_i)/η_c` | M2 / Curtailment algebra and no double payment | S70, S117, S121 |
| M2-R2 | 3 | Removed “foregone export revenue” penalty under unlimited export; no separate curtailment price at all; grid term prices actual export once | M2 / Curtailment algebra and no double payment | S70, S117, S121 |
| M2-R2 | 4 | Site constraints imposed as explicit inequality, unlimited export representation, forecast net-load source, infeasibility outcome and remaining-duration rule in every energy term | M2 / site inequalities | S71, S72, S120 |
| M2-R2 | 5 | Salvage changed from 0.451852 to 0.388667 EUR/stored kWh (`cheapest/η_c`) so terminal value equals replacement cost; wear remains only on actual discharge; terminal-behaviour test added | M2 / Energy boundaries and units | S17, S118 |
| M2-R2 | 6 | Tie-break made fully deterministic: exact 1 Wh/1 W quantization role, six canonical keys, no-discharge index 120, no provisional-discharge pass or window feedback | M2 / deterministic tie-break | S69, S119 |
| M2-R3 | 1 | Site inequality now uses actual PV after curtailment: `g=load−pv+c+p`, `0≤c≤pv`, explicit export-limit semantics including `zero_export`, algebra showing the grid term prices curtailment once and no separate penalty exists | M2 / site inequalities, Curtailment algebra and no double payment | S117, S120, S121 |
| M2-R3 | 2 | Replaced flat-tomorrow revenue with the verified 16 quarter-hour prices; fixture uses six full slots plus 17:30-17:45 partial, revenue B 2.568660, net +0.298290, advantage +0.514660 | M2 / timestamped fixture | S116, S122 |
| M2-R3 | 3 | Corrected midnight dates and added stored-energy balances after every transaction; explicit start 22 Nov 15:45Z, end 23 Nov 21:45Z, partial recharge ends 23 Nov 01:00Z | M2 / timestamped fixture | S116, S123 |
| FACT | F1 | Corrected invalid claim that 16:00–23:00Z was expensive and overlapped the cheap 19:00–04:00Z window | Scope / verified live price fixture | S07, S08, S68, S115 |
| FACT | F2 | Corrected the second price error: 0.5088–0.5303 is tomorrow 15:45–18:00Z, not today; today max 0.4188 at 18:00Z, 16:00–18:00Z 0.4037–0.4130, tonight 0.3980→0.3498; holding today is now stated as correct | Scope / verified live price fixture | S08, S115, S116 |
| M1-R2 | 1 | Net-metering boundary made exclusive: netted only for `t_start < 2026-12-31T23:00Z`; CET→UTC conversion shown; 22:45Z last netted, 23:00Z first unnetted | M1 / Net-metering schedule | S63, S124 |
| M1-R2 | 2 | Numbered precedence with accepted suffix tail exempt from fatal `PriceGap`; interior gaps and <96 slots still refuse | M1 / Resolution and precedence | S03, S61, S62, S125 |
| M1-R2 | 3 | One missing required-source slot now refuses with `InputsUnavailable`; optional zero is separate; typed outcome reaches `grid_source_coverage` | M1 / Grid-source completeness | S06, S66, S126 |
| M1-R2 | 4 | One signed export-price rule for unknown/zero/exhausted: raw `feed_in_post`, including negative tariffs; no branch-specific clamp | M1 / Net-metering schedule | S04, S65, S127 |
| M3-R2 | 1 | Terminal index is `actual_slots−1`; target unchanged by horizon length; acceptance `e_j ≥ T−0.05`; no `e_120` access on shorter plans | M3 / Terminal target | S76, S128 |
| M3-R2 | 2 | Every obligation record, required and optional, carries reason, deadline, class and shortfall kWh | M3 / Structured infeasibility | S78, S129 |
| M4-R2 | 1 | Applicability table with target/limits/blocked outcome for each rule; zero plan is only a hold when no higher nonzero obligation is unmet | M4 / Mode, applicability and precedence | S81, S130 |
| M4-R2 | 2 | Persistent partial defined after 120 s pause with two retries, escalation and fallback hold; not `no_flow` | M4 / Ramp, pause and verification timing | S84, S131 |
| M4-R2 | 3 | Exact status pairs and absolute command error with matching direction; zero cannot satisfy a nonzero full-flow command | M4 / Outcome table | S30, S82, S132 |
| M4-R2 | 4 | One verification rule: pre-ramp evidence provisional and `verified=false`; two post-ramp reports required for confirmation | M4 / Outcome table and timing | S133 |
| M5-R2 | 1 | dry_run invariant: persisted dry_run honoured write-free and loud; operational requests rejected; no silent write-enabled migration | M5 / Safety write lane; M6 / mode state machine | S43, S92, S134, S145, S156 |
| M5-R2 | 2 | Universal physical limits enumerated for every safety action including import remedy; blocked outcome names the binding limit | M5 / Execution precedence | S93, S135, S150 |
| M5-R2 | 3 | Restart grace neutral evidence ordered, with verified write fallback and grace-expiry escalation | M5 / First-class transitions | S98, S136 |
| M5-R2 | 4 | Write/receipt timeouts, in-flight economic cancellation and late-receipt correlation added to safety retry state | M5 / Safety write lane and timeouts | S137 |
| M6-R2 | 1 | `protection_active_invalid_config` with safe inputs, no expiry into auto, restart policy and guarded return | M6 / Operational mode state machine | S101, S138, S146 |
| M6-R2 | 2 | Attempt deadline distinct from conditional solve success; eligibility through replacement install or `t_due+5s`, then `solve_pending` hold | M6 / Replan cadence and recovery | S103, S139 |
| M7-R2 | 1 | Capacity prior freshness 30 days, publication thresholds, aging transition, clamp-vs-reject decision | M7 / capacity numerics | S107, S140 |
| M7-R2 | 2 | Efficiency formulas: block joining, refusal barriers, session/pair weights, effective pairs, confidence and round-trip energy calculation | M7 / efficiency pairing | S50, S141 |
| M7-R2 | 3 | Independent 900 s learning schedule, 900 s extraction overlap, late-completion re-extraction and stable consumption order | M7 / durable progress and outcomes | S51, S108, S142 |
| M7-R2 | 4 | Pooled-capacity timestamp provenance: peer-only changes never move local updatedAt; blended uses newest local session | M7 / durable progress and outcomes | S111, S143 |
| M7-R2 | 5 | Candidate-pair acceptance separated from metric publication; S114 keeps previous metric until 3 pairs/effective pairs | M7 / efficiency pairing | S114, S144 |
| INV | 1-6 | Invariants section added with six never-violated properties; property scenarios added for each | SPEC / INVARIANTS | S145–S150 |
| M5-R3 | 1 | Persisted dry_run honoured as loud write-free latch with `protection_inactive`, API rejection, restart persistence and explicit operator clearance | M5 / Dry-run invariant; M6 / mode state machine | S134, S145, S156 |
| M5-R3 | 2 | Preemption requires confirmed cancel or ordered fence; unverified preemption escalates and never releases lane | M5 / Safety write lane | S137, S159 |
| M5-R3 | 4 | Bounded 74 s total attempt deadline, attempt ids, ramp-end bound, ack/receipt timeouts and late receipt correlation | M5 / Safety write lane | S137, S159 |
| M2-R3 | 3 | PV preference removed from economic objective; configured preference reported separately using actual exportable surplus | M2 / complete objective | S127, S122 |
| M2-R3 | 5 | Capacity basis explicit: 7.8575 learned for planning, 7.68 nameplate only prior; floor 1.178625 learned | M2 / objective and fixture; M3 / terminal target | S15, S76, S116, S128 |
| M1-R3 | 3 | Tail acceptance provisional until stale/inputs/forecast refusals pass; separate error and outcome selection order | M1 / resolution and precedence | S125 |
| M1-R3 | 4 | Untracked-budget acknowledgement limited to active netting regime; regime change needs explicit override | M1 / net-metering schedule | S63, S127 |
| M3-R3 | 1 | Terminal target = max(15 % hard floor learned 1.178625, 10 % soft 0.78575), not 20 %/1.572 | M3 / terminal target | S15, S76, S128 |
| M4-R3 | 2 | Mutually exclusive exhaustive predicates with ordered precedence, τ=max(30,0.1C), truth table and shared pause rules | M4 / outcome table | S84, S132, S170 |
| M6-R3 | 1 | Durable no-write authority latch with explicit logged clearance and health check; no startup/config/expiry clearing | M6 / mode state machine | S100, S134, S145, S156 |
| M6-R3 | 2 | Invalid-config input freshness limits, cold-start fallback and permitted/refused protection actions | M6 / invalid-config inputs | S101, S138, S165 |
| M7-R3 | 3 | Durable open-session and completion-event log with completion-revision consumption independent of start order | M7 / durable progress | S142, S169 |
| M7-R3 | 4 | Corrected closure sign with symbolic balance and worked δ±1 examples | M7 / closure correction | S141, S168 |
| M7-R3 | 5 | Raw η validity before clamping; `eta_out_of_range` with raw value | M7 / validity | S141, S168 |
| M7-R3 | 6 | Seven-step aggregation order with post-rejection inlier counts and weighted median/MAD | M7 / aggregation | S141, S168 |
| M7-R3 | 7 | Micro-session reason precedence before incomplete/short; incomplete provisional values classified idle_blip | M7 / eligibility | S113, S153 |
| T1 | signs | Import-positive grid and charge-positive battery convention with end-of-tick timing; demands 8600 W/500 W and asks -1100 W/-500 W | SPEC / Test conventions | S09, S35, S39 |
| T2 | replay | Clock-driven replays combine command, measured output, restart and recovery | SPEC / Test conventions; M4/M6 | S151, S152, S163 |
| T3 | learning | Bounded progress, exact captured counts and independent metric success/refusal | M7 | S153, S154, S155 |
| T4/T8 | oracle | Complete deterministic fixtures plus independent DP oracle; strategy comparison separated from optimizer acceptance | M2 | S07, S08, S115, S116, S158, S164 |
| T5 | safety | Dry-run isolation and safety precedence asserted at sink-call level | M5/ALL | S156, S165 |
| T6 | fixture math | Corrected S01/S03/S15/S61/S71 arithmetic and endpoint/basis conventions | M1/M2 | S01, S03, S15, S61, S71 |
| T7 | determinism | Exact boundaries, tolerances, quantization and reason precedence | M4/M7 | S57, S69, S82, S84, S166, S167 |
| T9 | fencing | Late economic write cannot restore nonzero flow after a confirmed hold | M5 | S137, S159 |
| T10 | safety limits | Action-specific applicability; 0 W neutral hold never suppressed by nonzero power limits | M5 | S150, S160, S165 |
| T11 | receipts | Independent and mixed post-ramp evidence with explicit aggregate state | M4 | S132, S133 |
| T12 | numerics | Exact formulas, weights and outputs for efficiency cases | M7 | S114, S119, S141, S144 |
| T13 | envelope | Common terminal envelope mapped from domain states via `detail.domain_state` | SPEC / Result envelope | S109, S148, S162 |
| R1–R12 | replay suite | Twelve required incident/golden/property/schema scenarios added | SPEC / acceptance and scenarios | S151–S162 |
| M5-FINAL | 1 | Explicit six-step clearance sequence after logged command and health check; fenced safety-lane neutralisation permitted, economic writes blocked until verified neutral | M5 / Dry-run invariant; M6 / mode state machine | S178 |
| M5-FINAL | 2 | No startup/migration/expiry/reload/API/schedule/watchdog path can clear or overwrite the latch; acceptance case per path | M5 / Latch lifecycle | S171–S177 |
| M2-FINAL | 3 | Policy preference restricted to charging via `min(max(p_i,0), exportable_surplus_i)` | M2 / complete objective | S179 |
| M2-FINAL | 4 | Terminal 0.05 kWh chosen as deductible deadband and encoded directly in objective formula | M2 / complete objective and units | S180 |
| M4-FINAL | 5 | Status-aware exhaustive predicates, explicit idle/unknown/hold-flow outcomes and status-bearing truth table | M4 / outcome table | S170, S181 |
| M6-FINAL | 6 | Clearance with nonzero setpoints: measured-power reconciliation, fenced neutral hold, eligibility only after all packs neutral | M6 / mode state machine | S178 |
| M7-FINAL | 7 | Physical validation of present numeric fields before `idle_blip`, while valid provisional measurements still classify as idle first | M7 / eligibility precedence | S182 |
| M5-SIGN1 | state | Invariant restated on `dry_run_latched` state; `clearing` entered first by logged operator command, bounded/terminal, fenced neutralisation only | M5 / Write-authority latch | S145, S178, S183 |
| M4-SIGN2 | precedence | Hold+idle/unknown rows aligned with rules 2/3; hold+directional only is `hold_unexpected_flow` | M4 / outcome table | S170, S181 |
| M4-SIGN3 | exhaustiveness | Unknown/opposite status at `M≤30 W` and directional status during hold now typed and escalated | M4 / outcome table | S170, S181 |
| M5-SIGN4 | evidence | Two fresh post-fence `(0,0)` reports required for every pack, including already-idle packs | M5 / clearance sequence | S178, S183 |
| M2-SIGN5 | terminal index | Single `N=actual_slots` terminal state for transition, salvage, penalty and target, including 96–119 degraded tails | M2 / objective; M3 / terminal target | S128, S180, S184 |
| M5-SIGN2 | barrier | Durable clearance-entry barrier generation recorded per pack before evidence; two post-barrier reports required and stale pre-barrier reports rejected | M5 / clearance sequence | S178, S183, S187 |
| M4-SIGN4 | evidence | `verified` reserved for command verification; anomalies use `anomaly_confirmed` and `evidence_kind=confirmed_anomaly`; hold_verified requires exactly `(0,0)` | M4 / outcome table | S132, S181, S188 |
| M4-SIGN5 | boundary | 30 W and 31 W rows added for unknown, opposite and directional-hold cases | M4 / truth table | S170, S181 |
| M2-SIGN6 | tie-break | Power signature is `[Q(p_0)..Q(p_{N-1})]` with `N=actual_slots` | M2 / deterministic tie-break | S69, S119 |
| M6-SIGN | gate | M6 single write-authority gate consumes M5 states, all entry points, durability precedence, token detection, fresh per-pack clearance evidence | M6 / Operational mode state machine | S171–S177, S185, S186 |
| M7-SIGN | precedence | Zero energy with valid duration is idle_blip; invalid_sample reserved for non-finite/negative/internally inconsistent data | M7 / eligibility precedence | S182, S189 |
| M6-FINAL-BOOT | token | Fresh/legacy-auto/legacy-dry-run bootstrap, malformed-token recovery and atomic token+event crash ordering | M6 / Token bootstrap | S190–S194 |
| M6-FINAL-WRITES | classes | Control-plane writes permitted under gate in every state; device writes state-scoped; clearance journal/generation/commit ordering; runtime-only persistence denials | M6 / Single authority gate | S195, S196 |
| M6-FINAL-SAFETY | scope | Invariant 6 scoped to auto; no protection in latched, neutralisation only in clearing, exact dashboard/health text | M6 / health text; INVARIANTS | S196, S197 |
| M6-FINAL-LEGACY | bootstrap | Legacy advisory and unknown/missing mode bootstrap fully audited to auto with prominent warnings; no silent protection_active rename | M6 / Token bootstrap | S198, S199 |
| M6-FINAL-INDIRECT | gate | Command-bearing control-plane mutations prohibited; all device writes recheck gate at execution time; queued intents survive only as intent | M6 / Single authority gate | S200 |
| M6-FINAL-TEXT | health | Prominent human-readable battery-unmanaged/clearance-active/protection-active text mirrored in dashboard and API health | M6 / health text | S201 |
| M6-FINAL-EVIDENCE | legacy | Legacy bootstrap requires pre-token schema evidence and absence of any initialization event; post-token missing/unreadable token enters operator recovery | M6 / Token bootstrap | S202–S204 |
| M6-FINAL-PHASES | scope | Authority uninitialized/invalid permits only token+event transaction; initialized phases allow control-plane writes and device-only state rules | M6 / Authority phases | S205, S206 |
| M6-FINAL-RECOVERY | text | Authority-recovery state has loud human-readable warning in dashboard and API health | M6 / health text | S204, S207 |
| M6-FINAL-PROVENANCE | evidence | Three-condition legacy test plus journal high-water/hash-chain completeness; ambiguous restored/migrated ledgers enter recovery | M6 / Token bootstrap | S208, S209 |
| M6-FINAL-SEALED | structure | Sealed BootstrapAuthorityTx exposes only token/generation/event/commit with negative device-write and journal-append tests | M6 / Authority phases | S210 |
| M6-FINAL-CHECKPOINT | atomicity | Checkpoint advances atomically with every relevant append, owned by authority module, ordinary append returns `checkpoint_write_denied`; startup verifies contiguous hash-chained range from checkpoint to head | M6 / Authority anchor and checkpoint | S209 |
| M6-FINAL-ANCHOR | boundary | Separate authority anchor detects database-only rollback; full snapshot restore is documented limitation that lands in `reconciled` with universal banner and journalled startup resolution before dispatch | SPEC / LIMITATIONS; M6 / Universal startup resolution | S208 |
| M6-FINAL-STARTUP | resolution | Only reconciled/recovery_required; no continuity_proven; banner on every startup; continuity cannot be proven on this platform | M6 / Universal startup resolution | S208, S211 |
| M6-FINAL-AUTHORITY | preserve | Startup resolution never changes authority; latched snapshots require full clearance; dispatch only with valid auto | M6 / Restore/reconcile preserves authority | S212 |
| M6-FINAL-ANCHOR-LIFE | lifecycle | Anchor created at bootstrap, two-phase pending/committed heads on each append, exact crash recovery rules | M6 / Anchor lifecycle | S213–S216 |
| M6-FINAL-RESREC | journaling | Append-only startup-resolution record writable in every authority phase outside authority journal; loss/corruption is recovery_required | M6 / Startup-resolution record | S217 |
| M6-FINAL-GATEORDER | bootstrap | Device gate opens only after sealed token/event commit, durable anchor creation and successful reconciliation | M6 / Token bootstrap crash recovery | S194, S218 |
| M6-FINAL-RESORDER | durability | Startup-resolution record append+fsync and two-phase anchor ref installed before reconciliation/gate; first-bootstrap atomic anchor creation; exact crash windows and typed persistence failures | M6 / Startup-resolution record | S219, S220 |
| M6-FINAL-ONEORDER | ordering | Single Exact ordering list merges bootstrap and startup resolution: fsync file, fsync parent directory, pending ref, atomic first anchor creation with ref, reconciliation, gate, dispatch | M6 / Exact ordering | S219 |
| M6-FINAL-DIRFSYNC | durability | Parent-directory fsync chosen for first record/anchor creation; typed `resolution_dir_fsync_failed` and gate withheld until it completes | M6 / Exact ordering | S219, S220 |
