# Provenance — forecast parity fixtures

Four fixtures drive the P4 parity tests. Two are extracted from the Python add-on at
`ha-energy-manager` commit `00092d769d1ec574dc56a152771e83e5aa3f70cc` (add-on 0.38.8); two are
generated from the same commit by `scripts/gen_forecast_fixtures.py`.

| File | sha256 | Origin |
| --- | --- | --- |
| `pv_golden.jsonl` | `5aaa4c518a1b6acd8f4d8bc039351c2192fd0f00198dbb4f6a3f3404a8c5347f` | extracted from `reports/energy-rs/fixtures/` (from `models/pv_golden.json`, sha256 `4a67845b…df2`) |
| `pv_golden_full.jsonl` | `46638b5a2a4631cd6206182787fb1f0d8ffa07e5876fa633b7c69d9674d4225d` | same, adds `raw_ratio`, `physics_w`, `elevation_deg` |
| `load_profile.json` | `df3bdfc978f67890cc8e2caf778dd8b1d93750eb4ca2d4f66bf748cd19534396` | extracted build/serving golden (§5.7 of the port spec) |
| `load_devices.json` | `2f25e1e3b30417155bbfb209d75730332258f65620245ccb2916a9e8e5289723` | extracted device-subtraction golden |
| `pv_features.jsonl` | `8ac14a96c4362518e92015a78cf3beb83ad900716a7cc91385b7326abc3e7af6` | generated: the golden inputs, the rebuilt feature vector, `physics_w`, `poa_wm2`, `raw_ratio`, `clamped_ratio`, `elevation_deg` and `estimate_slot`'s `pv_kw` for all 800 cases |
| `solar_pure.json` | `0b325986d3bca02e82301db6c9934b176a3386421f74e923fb5ee6050b0bfef4` | generated: capacity physics, merge (three strategies), HA Energy Wh→kW, EWMA bias update and decay, head fit/gate, derived-size fit |

## Regenerating the generated fixtures

```bash
python3 scripts/gen_forecast_fixtures.py
```

The script imports the Python add-on from `/root/HomeAssitant/ha-energy-manager` (path constant at
the top of the file), uses its committed `pv_base.json` and `pv_golden.json`, and writes
deterministically. The output is **not** a substitute for the add-on's own golden file: the gate is
that the artifact's 800 ratios match bit for bit, and the extra fixtures pin the stages the strict
file cannot.

`pv_features.jsonl` carries the decoded `in` tuple (latitude, longitude, tilt, azimuth, capacities,
irradiance, ambient temperature, cloud, wind, humidity, rain, epoch). The epoch is the plain integer
the golden file stores; every other input is a decoded `u64` bit pattern. Note that the golden's
`in` timestamps are one hour earlier than the emitter's loop clock (they were stamped from a naive
local datetime), which is why the reconstructed feature vectors use hour `23` for a case the emitter
built at local midnight. The regenerated features are bit-identical to the committed golden's.
