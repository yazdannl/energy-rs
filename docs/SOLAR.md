# PV forecast

The plan's `pv_kw` series. Three sources, in the order `pv_mode` allows.

| `pv_mode` | order |
|---|---|
| `auto` (default) | learned model → Home Assistant provider → measured EWMA |
| `model` | learned model only (no model ⇒ the plan is refused, never a zero-PV day) |
| `provider` | Home Assistant's `energy/solar_forecast` only, then the measured EWMA |

## The learned model

`crates/energy-forecast` carries the GBT port (`models/pv_base.json`, 400 trees, 28 features) and
the physics chain (Haurwitz/Erbs/NOCT/inverter). `energy-core::plan_source::pv_model` builds one
`SlotInputs` per quarter-hour slot and runs `estimate_slot`; the artifact is embedded in the binary
(`PvModel::embedded`), parsed once per process, and refused as a whole if it cannot serve — a
silently zero PV forecast would plan a day without sun.

Inputs per slot: sun geometry, plane tilt/azimuth, DC and AC capacity, and Open-Meteo's **GHI**
(`shortwave_radiation`), ambient temperature, cloud cover, wind, humidity and rain. Open-Meteo's GHI
is already attenuated, so `attenuation` is 1.0 and the model's cloud/radiation features carry the
weather. A missing weather hour is carried forward from the previous one.

## Array settings

| key | unit | notes |
|---|---|---|
| `pv_mode` | `auto`/`model`/`provider` | which forecast drives the plan |
| `pv_dc_kwp` | kWp | module nameplate; `0` means "not configured" |
| `pv_ac_kwp` | kW | inverter ceiling (clipping headroom) |
| `pv_tilt_deg` | ° | 0 flat, 90 vertical |
| `pv_azimuth_deg` | ° | 0 north, 180 south |
| `pv_scale` | × | calibration factor on the model's output |

This install: 2.16 kWp, 25° tilt, 135° azimuth (from the Forecast.Solar sub-entry), inverter ~2.4 kW
(the highest AC power the SolarEdge statistics ever reached).

## Calibration

The model is a port trained on the previous install, so its absolute level is a hypothesis until it
has been compared with this array. Every plan where both exist logs:

```
INFO energy_core::plan_source::ha_supply: PV model against the provider over the horizon
     model_kwh=3.9 provider_kwh=4.1 ratio=0.95
```

`pv_scale` is set from that comparison: the model is primary, the provider is the reference, and a
stable ratio over several days is the calibration. A change is one settings write (it takes effect on
the next plan; the engine re-reads the array every tick).

## Weather

`energy_ha::open_meteo_forecast` calls `api.open-meteo.com/v1/forecast` with Home Assistant's own
coordinates (`get_config`); no key, no account, 3 days hourly, cached for 30 minutes. The configured
`weatherEntity` is tried first (including the legacy one-entity service), and the newest measured
temperature is held flat only when every forecast source fails.

## Tests

* `crates/energy-forecast/tests/pv_golden.rs` — 800 golden vectors, bit-exact.
* `crates/energy-core/src/plan_source/pv_model.rs` — the embedded artifact, a plausible summer noon,
  an unconfigured array and a night.
* `crates/energy-core/src/plan_source/pv_mode.rs` — the setting round-trip.
