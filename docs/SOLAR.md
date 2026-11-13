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

The array is a **list of planes**, because a roof usually is one:

```json
[{"name":"PV SE","kwp":2.16,"tilt":25,"azimuth":135},
 {"name":"PV SW","kwp":1.08,"tilt":25,"azimuth":225},
 {"name":"PV NW","kwp":1.44,"tilt":25,"azimuth":315}]
```

| key | unit | notes |
|---|---|---|
| `pv_planes` | JSON | one entry per plane: `name`, `kwp`, `tilt`, `azimuth` |
| `pv_ac_kwp` | kW | the single inverter ceiling over all planes |
| `pv_mode` | `auto`/`model`/`provider` | which forecast drives the plan |
| `pv_scale` | × | calibration factor on the model's output |
| `pv_dc_kwp` / `pv_tilt_deg` / `pv_azimuth_deg` | kWp / ° / ° | legacy single-plane fallback, used only while `pv_planes` is empty |

Each plane is served at its own azimuth with the plane's DC capacity and the **total** AC ceiling as
its clip, the results are summed and the ceiling is applied once — clipping belongs to the inverter,
not to a roof. The Settings page edits the list row by row.

This install: 4.68 kWp over three planes (SE 2.16 @ 135°, SW 1.08 @ 225°, NW 1.44 @ 315°, all 25°
tilt), imported from the previous add-on's `solar.sources`; the same array the embedded model was
trained on. No explicit inverter rating was configured there, so `pv_ac_kwp` equals the DC total
(no clipping assumed) until the real ceiling is known.

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
