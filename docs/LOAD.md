# Load prediction

The plan's `load_kw` series is a model output, not a meter reading. One learned profile per house,
plus one profile per named load, plus the outdoor temperature and the operator's at-home state.

Code: `crates/energy-forecast/src/load` (model), `crates/energy-core/src/plan_source`
(learning and serving), `crates/energy-server/src/api/loads.rs` (settings), and
`web/src/components/settings/SettingsLoads.svelte` (UI).

---

## 1. What is learned

| quantity | source | notes |
|---|---|---|
| house hourly load | a configured **load meter** (`loadEntity`), else the energy dashboard balance `import − export + PV + discharge − charge` | an hour missing any required counter is skipped, never zeroed |
| served level | the `loadQuantile` of each bucket, default **p75** | a plan built on the median hour runs out of energy exactly when the house peaks |
| 48 buckets | weekday/weekend × hour, median + p10/p90 | 56-day window |
| level `scale` | `recent_7d_mean / profile_mean`, clamped `[0.5, 2.0]` | absorbs a growing or shrinking house |
| `bias[48]` | EWMA of the last seven days' per-bucket mean over median, `α = 0.2`, clamped `[0.5, 2.0]` | a recent weekday pattern |
| `base_kw` | **p10 of the last 14 days' local night hours (00:00–05:00)** | the standing load; falls back to the whole window's p10 when the night has no samples |
| temperature fit | `load = a + b·(T − T̄)`, `r² ≥ 0.05` to be applied | needs an outdoor temperature source |
| named loads | each meter's own series, after being subtracted from the house | same model, own temperature term for heat pump / water heater / pool pump |

`base_kw` is a **floor** for the house base component (`house_bands`), and `house_base_kw` is the
floor of the **served total** (`summed_bands`), learned from the house *before* any named load was
subtracted. Two numbers because they answer two questions: the base component's own standing load,
and what the house never draws less than whatever the components say. A bucket that learned near
zero — an unoccupied afternoon, or a mis-metered balance — can therefore never plan a house that is
darker than its own standing load. The night is used rather than the whole day on purpose: a
systematically bad daytime measurement must not be able to drag the floor down with it. Named loads
are *not* floored by their own `base_kw`: a cycling heat pump would be over-predicted every hour by
its night percentile, and the total floor already covers the house.

## 2. Named loads (`load.devices`)

Stored as JSON beside the profiles (`GET/PATCH /api/v1/loads`, ledger key
`forecast.learning.load.config`). One entry:

```json
{ "entity": "sensor.heat_pump_power", "kind": "heat_pump", "name": "Heat pump",
  "keep_while_away": true }
```

Kinds: `heat_pump`, `ev_charger`, `water_heater`, `pool_pump`, anything else = a plain meter with
no temperature term. The kind is the authority on the temperature term at learning time **and** at
serving time: a slope fitted under another kind is dropped.

Rules, in order:

1. A device is subtracted from the house only when its meter covers ≥ 50 % of the house's learning
   hours and has ≥ 48 usable hours; the split also needs the joint coverage to stay ≥ 50 %. The
   meter may be either an **energy counter** (its `change`, converted to kWh by the recorder) or a
   **power sensor** (its hourly `mean`, scaled to kW, which is the same kWh for one hour). The
   recorder's own metadata decides which; anything else (a temperature, a percentage) is refused
   with a reason instead of being subtracted as garbage.
2. The base profile is learned from what remains, and records `devices_subtracted` plus one reason
   per device it refused (`devices_excluded`).
3. At serving time a device is added back **only** when the served base profile says it was
   subtracted and it has a profile of its own. Adding a device that is still inside the base would
   count it twice, and the doubling hides in the temperature slope instead of looking like an error.

A meter that cannot be read is absent from the split, so the device stays inside the base with a
reason; it is never silently dropped. An hour where the meters claim **more** than the house total is
a measurement contradiction: it is dropped (unknown, not zero), and when that would leave less than
half the window, the whole split is refused and every device stays inside the base.

### The house source

The balance is only as good as the dashboard's counters. A wrong battery counter, a lagging PV
counter or a stale solar-day counter shows up as a house that draws less than it does — and
sometimes less than nothing, which is how 290 of 1317 hours came to contradict the heat pump meter.
When a whole-house meter is configured (`POST/PATCH /loads`, `loadEntity`), its hourly means are the
house instead: one sensor, one number, no balance arithmetic. It may be a W power sensor (the hourly
mean is the kW) or a kWh counter (`change` is the kWh). Served history, savings and the price book
are unaffected; only the learned profile changes source.

### The planning band

Each bucket stores p10, p50 and p90 per weekday-hour. Serving the median is the middle of the house's
day, not its peak: the oven, the heat pump and the EV line up in the same hour often enough that a
p50 plan under-buys, then imports the shortfall at the evening price. `loadQuantile` (`POST/PATCH
/loads`) serves a quantile above the median instead — the shipped default is **0.75**, interpolated
between the stored p50 and p90 (the percentiles are the only evidence there is, so nothing is
invented). `0.5` restores the old median behaviour; `0.85` reserves more.

The house floor (`houseBaseKw`) still applies, and the served *display* bands stay p10/p90 scaled
from the served level.

## 3. Weather

Two optional entities, both trimmed:

* `weatherEntity` falls back to the deprecated one-entity `weather.get_forecast` when the entity
  never implemented `weather.get_forecasts` (OpenWeatherMap among them).
* With no usable weather entity, the horizon comes from **Open-Meteo** (`api.open-meteo.com`, the
  free no-key endpoint) over Home Assistant's own coordinates (`get_config`). Answers are cached for
  30 minutes, and the request is skipped when Home Assistant reports no location.
* With no usable weather entity, the horizon comes from **Open-Meteo** (`api.open-meteo.com`, the
  free no-key endpoint) over Home Assistant's own coordinates (`get_config`). Answers are cached for
  30 minutes, and the request is skipped when Home Assistant reports no location.
* `temperatureEntity` — a numeric °C sensor with long-term statistics (`state_class: measurement`).
  A heat pump's own outdoor sensor is a fine choice: it is the temperature the pump responds to.
  Its hourly recorder **means** fit the temperature regression for the house and for each
  temperature-driven load; without statistics the series is empty and the term stays off.
* `weatherEntity` — a `weather.*` entity. `weather.get_forecasts` (hourly) supplies the temperature
  for each hour of the plan horizon; without it the newest measured hour is held flat (a measured
  sensor says what today is, not what tomorrow will be, but a flat line still carries the heat
  pump's present response).

When a source is configured but nothing arrives, the plan is marked degraded with
`no outdoor temperature available; the temperature term is off for this plan` — never silently
"no temperature effect".

## 4. Holiday mode

A single toggle (`holiday: true`).

* **Serving**: the learned house shape is replaced by a flat `base_kw`, and only loads with
  `keep_while_away` are added above it. Everything else is off by definition.
* **Learning**: paused while the toggle is on. An empty house would otherwise teach the buckets
  and the seven-day `scale` a house that does not exist, and the plan would keep under-predicting
  for a week after the return. The last at-home profile stays in service until it ages out
  (seven days).
* The plan note reads `holiday mode: the house shape is replaced by the base floor …`.

## 5. Freshness and failure

`LoadProfiles::current` refuses a profile that is older than seven days or has fewer than three
distinct local days; the plan then falls back to the residential shape (or the measured EWMA when
no daily shape is known) and says so in the summary. A learning failure is a fault in the status
snapshot and a log line, never a reason to stop the engine.

## 6. Tests

* `crates/energy-forecast/tests/load_profile.rs` — bucket/level/bias parity, the night floor, the
  holiday shape.
* `crates/energy-forecast/tests/load_devices.rs` — subtraction parity, add-back rules, keep-while-away.
* `crates/energy-forecast/src/load/config.rs` — payload round-trip and defaults.
