# energy-server HTTP API (v1)

The add-on serves one axum process on `0.0.0.0:8099`:

* `/api/v1/...` — the JSON API and the SSE stream (this document).
* everything else — the embedded Svelte UI (`/`, `/plan`, `/prices`, `/batteries`, `/savings`,
  `/settings` render the SPA; a missing asset is a 404).

Behind Home Assistant ingress the Supervisor authenticates the user; the add-on itself is not an
auth boundary. The API is read-mostly: reads answer from the ledger and the engine's published
snapshot, and the four command endpoints validate input and queue one engine command.

## Conventions

| | |
|---|---|
| Field names | `camelCase` |
| Units | in the name: `powerW`, `energyWh`, `capacityKwh`, `buyEurPerKwh`, `coverageHours` |
| Money | EUR, JSON number rounded to **6 decimals** (the ledger's 1e-6 resolution) |
| Energy | Wh (milli-Wh resolution); kWh only where the contract stores it (`capacityKwh`, prices per kWh) |
| Power | W; positive charges the battery, negative discharges |
| Time | RFC3339 UTC with a `Z` suffix, second resolution (`2026-01-01T00:00:00Z`) |
| Days | `Europe/Amsterdam` local dates (`YYYY-MM-DD`) |
| Content type | `application/json` (SSE: `text/event-stream`) |

Unknown fields in a request body are ignored. A body that is not the documented shape is a `400`;
a well-formed value that is not acceptable is a `422`.

## Error envelope

Every failure has the same shape and a stable `code`:

```json
{ "error": { "code": "validation_failed", "message": "mode must be one of advisory, dry_run, auto",
             "details": { "allowed": ["advisory", "dry_run", "auto"] } } }
```

`details` is optional and never contains a token, a credential or a file path. Message and details
are safe to surface in the UI.

| code | status | meaning |
|---|---|---|
| `invalid_query` | 400 | a query parameter is missing, malformed or out of range |
| `invalid_body` | 400 | the JSON body is malformed or not the documented shape |
| `not_found` | 404 | no such resource or endpoint (also: no current plan) |
| `unknown_pack` | 404 | the pack serial/alias/name is not in the ledger |
| `stale_write` | 409 | the timestamped settings write would rewrite history |
| `validation_failed` | 422 | a well-formed value the API refuses (mode, override, action, timestamps) |
| `invalid_setting` | 422 | a known setting with an out-of-range/typed-wrong value |
| `unsupported_setting` | 422 | a key that is not writable through this endpoint |
| `engine_unavailable` | 503 | the engine command queue is closed or full (no runtime is draining it) |
| `corrupt_plan` | 500 | a stored plan summary cannot be rendered (the cause is in the log) |
| `internal` | 500 | anything unexpected; the body never carries the cause |

## Routes

| method | path | query / body | success | errors |
|---|---|---|---|---|
| GET | `/api/v1/health` | — | 200 `health` | — |
| GET | `/api/v1/site` | — | 200 `site` | — |
| GET | `/api/v1/packs` | — | 200 `packs` | — |
| PATCH | `/api/v1/packs/{sn}` | `{alias}` | 200 `{sn, alias}` | 400, 404, 422 |
| GET | `/api/v1/prices` | `from?`, `to?` (RFC3339) | 200 `prices` | 400 |
| GET | `/api/v1/plan/current` | — | 200 `plan` | 404, 500 |
| GET | `/api/v1/plans` | `limit?` (1–200, default 50) | 200 `plans` | 400 |
| GET | `/api/v1/dispatch` | `limit?` (1–500, default 50) | 200 `dispatch` | 400 |
| GET | `/api/v1/savings` | `period=day\|month\|year\|lifetime` (default `day`) | 200 `savings` | 400 |
| GET | `/api/v1/loads` | — | 200 `loads` | 500 |
| PATCH | `/api/v1/loads` | `{holiday, temperatureEntity, weatherEntity, devices}` | 200 `loads` | 400, 422, 500 |
| GET | `/api/v1/settings` | `asOf?` (RFC3339) | 200 `settings` | 400 |
| PATCH | `/api/v1/settings` | `{values, at?}` | 200 `settings` (written keys + schema) | 400, 409, 422 |
| PATCH | `/api/v1/mode` | `{mode}` | 200 `mode` | 400, 422, 503 |
| POST | `/api/v1/override` | `{pack, action, powerW?, ttlS?}` | 202 `override` | 400, 404, 422, 503 |
| POST | `/api/v1/protection` | `{action:"full_charge", pack?}` | 202 `protection` | 400, 404, 422, 503 |
| POST | `/api/v1/learning` | `{action:"relearn", pack?}` | 202 `learning` | 400, 404, 422, 503 |
| GET | `/api/v1/stream` | — | 200 SSE | — |

### `GET /health`

Liveness for the Supervisor watchdog. It touches no database and no Home Assistant.

```json
{ "status": "ok", "version": "0.1.0", "mode": "dry_run", "lastTickAt": "2026-01-01T12:00:00Z" }
```

`status`/`version` are the stable watchdog contract; `mode` and `lastTickAt` are additive.
`lastTickAt` is `null` until the engine publishes its first snapshot (it does so after every tick).

### `GET /site`

The discovered energy dashboard, the known packs (identity) and the capability slugs:

```json
{ "configured": true, "mode": "dry_run", "lastTickAt": "...",
  "capabilities": ["battery_control", "savings"],
  "sources": [{ "kind": "grid", "entityId": "sensor.grid_energy", "label": "Grid" }],
  "packs": [{ "sn": "AB2000-ONE", "name": null, "alias": "Attic A", "labelCapacityKwh": 1.92,
              "reportedCapacityKwh": null, "maxChargeW": 800, "maxDischargeW": 800,
              "firstSeenAt": "...", "lastSeenAt": "..." }] }
```

`name` is what discovery registered (Home Assistant's device name); `alias` is the operator's own
label and `null` until one is set. The UI shows `alias`, then `name`, then the serial.

`configured:false` means HA has no energy dashboard; every list may still hold ledger packs.

### `GET /packs`

One row per known pack: live state (from the engine snapshot, falling back to the newest ledger
sample), the applied learning result with its provenance, the protection latch and the manual
override.

```json
{ "packs": [{
  "sn": "AB2000-ONE", "name": null, "alias": "Attic A",
  "labelCapacityKwh": 1.92, "reportedCapacityKwh": null,
  "limits": { "maxChargeW": 800, "maxDischargeW": 800 },
  "firstSeenAt": "...", "lastSeenAt": "...",
  "state": { "socPct": 60.0, "powerW": 120.0, "sampledAt": "...", "stale": false },
  "learned": { "capacityKwh": 1.85, "capacityConfidence": 0.8, "capacityApplied": true,
               "capacityDecision": "capacity_learned", "capacityBandLowKwh": 1.7,
               "capacityBandHighKwh": 1.92, "roundTrip": 0.9, "etaCharge": 0.9486833,
               "etaDischarge": 0.9486833, "efficiencyConfidence": 0.7,
               "efficiencyApplied": true, "roundTripSource": "measured",
               "sessionsUsed": 5, "updatedAt": "..." },
  "latch": { "blocked": true, "forceCharge": false, "forceChargeW": 300, "since": "...",
             "reason": "cell_below_resume", "reasonAt": "...", "voltageV": 3.01,
             "lastFreshAt": "..." },
  "override": { "action": "charge", "powerW": 800, "expiresAt": "..." }
}] }
```

`learned`, `latch` and `override` are `null` when the engine has no value for them.
`capacityDecision` is the learner's provenance slug (`capacity_learned*`); `latch.reason` is the
protection slug (`cell_*`).

### `PATCH /packs/{sn}`

```json
{ "alias": "Attic A" }
```

Sets the operator's label for one pack, identified by serial. The alias is presentation only: the
engine never reads it, and it is separate from `name`, which discovery rewrites from Home
Assistant — a re-registration never touches the alias. `"alias": null` or a blank string clears
it; the key itself must be present (`{}` is a `422`).

* Trimmed, 1–40 characters, no control characters; longer or malformed values are `422`.
* Unique across the fleet: an alias another pack holds is a `422 validation_failed` whose
  `details.sn` names the holder.
* An unknown serial is `404 unknown_pack`; writing the stored value again is a no-op `200`.
* The alias is another way to name a pack in `/override`, `/protection` and `/learning`.

The answer is the stored identity: `{ "sn": "AB2000-ONE", "alias": "Attic A" }`.

### `GET /prices?from&to`

Published slots on the provider's grid. `from` defaults to now floored to 15 minutes, `to` to
`from + 48 h`; the window may span at most 8 days. An empty range is `200` with `count: 0` — a gap
inside the stored data is not filled.

```json
{ "from": "...", "to": "...", "currency": "EUR", "count": 192,
  "slots": [{ "startAt": "...", "endAt": "...", "buyEurPerKwh": 0.2,
              "marketEurPerKwh": 0.08, "feedInEurPerKwh": 0.06,
              "source": "zonneplan_quarter_hourly" }] }
```

`buyEurPerKwh` is the published all-in import price; `market` is derived at ingest; `feedIn` is
`Settlement::export_price` for the slot (net metering while the saldering budget lasts, the
post-saldering feed-in otherwise, clamped at 0). Sources: `zonneplan_quarter_hourly`,
`zonneplan_hourly`, `ha_quarter_hourly`, `ha_hourly`.

### `GET /plan/current`

The plan the engine is following (newest ledger plan), its 120-slot timeline and its binding
reasons. No plan yet is `404 not_found`.

```json
{ "planId": 7, "createdAt": "...", "mode": "dry_run",
  "horizonStartAt": "...", "horizonEndAt": "...", "feasible": true,
  "summary": { "costEur": 1.234568, "baselineCostEur": 2.0, "savedEur": 0.765432,
               "chargedWh": 2500.0, "dischargedWh": 2000.0, "gridChargeWh": 1500.0,
               "pvChargeWh": 1000.0, "cycles": 0.5, "terminalTargetWh": null,
               "infeasibleReason": null },
  "bindingReasons": ["price_low"],
  "slots": [{ "idx": 0, "startAt": "...", "endAt": "...",
              "buyEurPerKwh": 0.2, "feedInEurPerKwh": 0.08,
              "pvWh": 250.0, "loadWh": 125.0, "batteryPowerW": 1500.0,
              "socWh": 1000.0, "targetSocWh": 1250.0, "reason": "price_low" }] }
```

Slot energy is the average power over the 15 minutes (`1 kW × 900 s = 250 Wh`). `socPct` is the
slot's state of charge against the fleet's learned capacity (`null` while no pack reports one), the
same denominator `/packs` prints per pack.
`bindingReasons` comes from the engine's snapshot for that plan id, else from the distinct slot
reasons; a missed terminal target adds `terminal_target_unreachable`.

### `GET /plans?limit`

Newest stored plans first. `costEur`, `baselineCostEur`, `savedEur`, `feasible` and
`infeasibleReason` are `null` when a stored summary cannot be parsed (the row stays visible).

```json
{ "count": 1, "plans": [{ "planId": 7, "createdAt": "...", "mode": "dry_run",
  "horizonStartAt": "...", "horizonEndAt": "...", "costEur": 1.234568,
  "baselineCostEur": 2.0, "savedEur": 0.765432, "feasible": true,
  "infeasibleReason": null }] }
```

### `GET /dispatch?limit`

Newest dispatch attempts first over the last 7 days.

```json
{ "windowSeconds": 604800, "count": 2,
  "entries": [{ "at": "...", "packSn": "AB2000-ONE", "requestedW": -400.0,
                "appliedW": -395.0, "reason": "dispatch", "mode": "auto",
                "verified": true, "error": null }] }
```

`verified` is the device echo check; `appliedW` is 0 when nothing was sent.

### `GET /savings?period=...`

Measured accounting from `savings_daily`, never from planned values. `day` is today (local),
`month` the current calendar month to today, `year` the current year to today, `lifetime` all rows.
The method, the counterfactual and the saldering-budget approximations are in `docs/SAVINGS.md`.

```json
{ "period": "lifetime", "fromDay": "1970-01-01", "toDay": "2026-01-01",
  "days": [{ "day": "2026-01-01", "actualEur": 0.5, "baselineEur": 1.0, "savingEur": 0.5,
             "arbitrageEur": 0.3, "selfConsumptionEur": 0.4, "feedInEur": -0.1,
             "batteryBalanceEur": -0.1,
             "importWh": 2000.0, "exportWh": 0.0, "pvWh": 3000.0,
             "batteryChargeWh": 2000.0, "batteryDischargeWh": 1800.0,
             "coverageHours": 24.0, "complete": true },
           { "day": "2026-01-02", "actualEur": null, "baselineEur": null, "savingEur": null,
             "arbitrageEur": null, "selfConsumptionEur": null, "feedInEur": null,
             "batteryBalanceEur": null,
             "importWh": 500.0, "exportWh": 0.0, "pvWh": 0.0,
             "batteryChargeWh": 0.0, "batteryDischargeWh": 0.0,
             "coverageHours": 6.0, "complete": false }],
  "totals": { "actualEur": 1.5, "baselineEur": 2.5, "savingEur": 1.0,
              "arbitrageEur": 0.4, "selfConsumptionEur": 0.5,
              "feedInEur": 0.2, "batteryBalanceEur": -0.1,
              "importWh": 4000.0, "exportWh": 1000.0, "pvWh": 9000.0,
              "batteryChargeWh": 2000.0, "batteryDischargeWh": 1800.0,
              "days": 2, "completeDays": 1 } }
```

* `savingEur = baselineEur - actualEur`, and the four split fields sum to it exactly:
  battery arbitrage, PV self-consumption, the feed-in effect and the battery's own energy balance.
* `complete` means every quarter-hour of the local day had a price and an import, export, PV and
  load measurement. Money is `null` for a **closed** day with a gap (it keeps its measured energy
  and `coverageHours` instead of a fake number); the current day is provisional — money present,
  `complete: false` — and `period=day` is that measured-so-far value.
* `lifetime` totals sum complete days only. `month`/`year`/`day` also include the provisional current
  day; a `null` day is in `days` but in no total. `days` counts the rows, `completeDays` the complete
  ones.

### `GET /settings` / `PATCH /settings`

Only the whitelisted keys are visible or writable; the control mode (`control.mode`, also accepted
as the alias `mode`) is deliberately **not** writable here (use `PATCH /mode`). Tokens, battery
credentials and other private options never appear.

```json
{ "asOf": "2026-01-01T12:00:00Z", "values": { "reserve_soc_pct": 20, "grid_import_limit_w": 0 },
  "schema": [{ "key": "grid_import_limit_w", "kind": "number", "default": 0,
               "min": 0, "max": 20000 }] }
```

`GET` returns every key (stored value or default) at `asOf` (default now). `PATCH` body:

```json
{ "values": { "grid_import_limit_w": 2000 }, "at": "2026-01-01T12:00:00Z" }
```

* Writable keys: `grid_import_limit_w`, `grid_export_limit_w`, `reserve_soc_pct`, `max_soc_pct`,
  `battery_power_limit_w`, `price_provider`, `price_entity`, `replan_seconds`,
  `stale_input_seconds`, `full_charge_weekday`, `full_charge_time`, and the PV array
  (`pv_planes`, `pv_mode`, `pv_ac_kwp`, `pv_scale`, and the legacy `pv_dc_kwp`, `pv_tilt_deg`,
  `pv_azimuth_deg` — see [SOLAR.md](SOLAR.md)).
* `at` defaults to now and may not lie more than 60 s in the future (`422`).
* A write whose `at` predates a stored change for that key is `409 stale_write`; history is never
  rewritten. Writes at or after the newest change append a history row.
* The response contains the keys that were written plus the schema; `200`.

### `GET /loads` / `PATCH /loads`

The load-prediction settings (see [LOAD.md](LOAD.md)). `GET` returns the stored configuration plus
what the planner is using now:

```json
{ "holiday": false, "temperatureEntity": "sensor.outdoor_temperature",
  "weatherEntity": "weather.home",
  "devices": [{ "entity": "sensor.heat_pump_power", "kind": "heat_pump",
                "name": "Heat pump", "keepWhileAway": true }],
  "problems": [],
  "profile": { "days": 34, "samples": 812, "baseKw": 0.18,
               "updatedAt": "2026-01-01T09:00:00Z", "temperatureHours": 640 },
  "modelled": [{ "entity": "sensor.heat_pump_power", "modelled": true }] }
```

`PATCH` replaces the **whole** configuration (there is no per-field merge; the device list is a
list) and answers the same shape. `profile` is `null` when no profile is fresh enough.
`modelled` says, per configured entity, whether its meter is currently split off the base and added
back above it — after a write that is `false` until the relearn it triggers has finished.

* Every device needs a non-empty, unique `entity`; anything else is `422`.
* `kind` is one of `heat_pump`, `ev_charger`, `water_heater`, `pool_pump`; any other string is a
  plain meter with no temperature term (reported in `problems`, not refused).
* `holiday: true` serves the base floor plus the `keepWhileAway` loads only, and pauses learning.

### `PATCH /mode`

```json
{ "mode": "auto" }
```

`advisory | dry_run | auto`. The ledger setting `control.mode` is written first (through
`energy_core::mode::save`), then the command is
queued; `200` answers `{ "mode": "auto", "updatedAt": "...", "engineNotified": true }`. If no
engine loop is running the answer is `503 engine_unavailable` with
`details: { "reason": "...", "persisted": true, "mode": "auto" }` — the setting did change.

### `POST /override`

```json
{ "pack": "AB2000-ONE", "action": "charge", "powerW": 600, "ttlS": 900 }
```

`pack` is a serial or a display name. `action` is `charge | hold | off`; `charge` takes an optional
`powerW` (default and ceiling: the pack's charge limit, 800 W when unknown) and `hold`/`off` ignore
it. `ttlS` defaults to 900 and must be 1–86400. `202` returns
`{ "accepted": true, "pack": ..., "action": ..., "powerW": ..., "ttlS": ..., "expiresAt": ... }`.
Unknown pack `404`, bad action/power/TTL `422`, no engine `503`.

### `POST /protection` and `POST /learning`

```json
{ "action": "full_charge", "pack": "AB2000-ONE" }
{ "action": "relearn" }
```

`protection` accepts only `full_charge`, `learning` only `relearn`; `pack` is optional (fleet-wide
when absent). `202` returns `{ "accepted": true, "action": ..., "pack": ... }`.
Unknown pack `404`, other actions `422`, no engine `503`.

## SSE stream

`GET /api/v1/stream` keeps the connection open (`text/event-stream`). Events:

| event | payload | when |
|---|---|---|
| `state` | mode, lastTickAt, planId, packs (soc/power/stale/blocked), meter, forecast, faults | once on connect, then after every published snapshot |
| `plan` | `{planId, at}` | the plan id changed; the client reads `/plan/current` |
| `fault` | `{kind, message, since}` | a fault opened (or re-opened with a new `since`) |
| `heartbeat` | `{at}` | the stream was quiet for the configured interval (15 s in the add-on) |

```
event: state
data: {"mode":"dry_run","lastTickAt":"2026-01-01T12:00:00Z", ...}

event: heartbeat
data: {"at":"2026-01-01T12:00:15Z"}
```

Backpressure: the source is a `tokio::sync::watch` channel, so a slow client is woken for the newest
snapshot and intermediate ticks are coalesced — the server never buffers an unbounded queue and
never blocks the engine. A fault that disappears is visible only through the next `state`; there is
no separate `fault_cleared` event.

## Live smoke check

`scripts/api_smoke.sh` exercises the contract against a running add-on with `curl` only:

```bash
scripts/api_smoke.sh                         # http://127.0.0.1:8099
BASE_URL=http://192.168.1.2:8099 scripts/api_smoke.sh
```

It prints one line per route with the HTTP status (and fails on an unexpected one), reads the first
SSE event, and checks that no response contains a token-like key. `plan/current` is allowed to
answer 404 before the first plan. Safe to run against a live system: every request is a read.
