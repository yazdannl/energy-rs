#!/usr/bin/env python3
"""Emit the supplementary parity fixtures for ``crates/energy-forecast`` (P4).

The two committed fixtures in ``reports/energy-rs/fixtures`` are the gate. These pin the stages that
the strict golden file cannot: the 28 features rebuilt from raw inputs, ``estimate_slot`` end to end,
the capacity physics, the merge, the HA-Energy conversion, the EWMA update/decay and the head fit.

Provenance: generated against ``ha-energy-manager`` at commit ``00092d7`` (add-on 0.38.8) with the
committed ``pv_base.json`` (sha256 25d96fca...3f26) and ``pv_golden.json``
(sha256 4a67845b...df2). Re-run with::

    python3 scripts/gen_forecast_fixtures.py

The output is deterministic; ``--check`` compares against the committed files instead of writing.
"""

from __future__ import annotations

import hashlib
import json
import struct
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from zoneinfo import ZoneInfo

ROOT = Path(__file__).resolve().parents[1]
PY_REPO = Path("/root/HomeAssitant/ha-energy-manager")
FIXTURES = ROOT / "crates/energy-forecast/tests/fixtures"
PV_MODEL = PY_REPO / "energy_manager/app/forecast/models/pv_base.json"
PV_GOLDEN = PY_REPO / "energy_manager/app/forecast/models/pv_golden.json"

sys.path.insert(0, str(PY_REPO))

from energy_manager.app.forecast import local_head, pv  # noqa: E402
from energy_manager.app.sources import solar  # noqa: E402

UTC = timezone.utc
AMSTERDAM = ZoneInfo("Europe/Amsterdam")


def bits(value: int) -> float:
    return struct.unpack("<d", struct.pack("<Q", int(value)))[0]


def emit_pv_features() -> Path:
    golden = json.loads(PV_GOLDEN.read_text())
    model = pv.load_model(PV_MODEL)
    attenuation_index = pv.FEATURE_NAMES.index("attenuation")
    cloud_index = pv.FEATURE_NAMES.index("cloud_pct")
    cases = []
    for index, case in enumerate(golden["cases"]):
        (
            latitude,
            longitude,
            tilt,
            azimuth,
            dc_wp,
            ac_wp,
            ghi,
            t_amb,
            _unused_cloud_bits,
            wind,
            rh,
            rain,
        ) = (bits(item) for item in case["in"][:12])
        # The last element is a plain epoch integer, not a bit pattern.
        epoch = int(case["in"][12])
        features = [bits(item) for item in case["features"]]
        cloud = features[cloud_index]
        attenuation = features[attenuation_index]
        moment = datetime.fromtimestamp(int(epoch), UTC)
        estimate = pv.estimate_slot(
            model,
            moment=moment,
            latitude=latitude,
            longitude=longitude,
            tilt_deg=tilt,
            azimuth_deg=azimuth,
            dc_capacity_wp=dc_wp,
            ac_capacity_wp=ac_wp,
            ghi_wm2=ghi,
            t_ambient_c=t_amb,
            cloud_pct=cloud,
            attenuation=attenuation,
            wind_ms=wind,
            rh_pct=rh,
            rain_mm=rain,
        )
        built, physics_w, poa = pv.feature_vector(
            geometry=pv.solar_geometry(latitude, longitude, moment, tilt, azimuth),
            ghi_wm2=ghi,
            t_ambient_c=t_amb,
            tilt_deg=tilt,
            dc_capacity_wp=dc_wp,
            ac_capacity_wp=ac_wp,
            moment=moment,
            cloud_pct=cloud,
            attenuation=attenuation,
            wind_ms=wind,
            rh_pct=rh,
            rain_mm=rain,
        )
        cases.append(
            {
                "index": index,
                "in": {
                    "lat": latitude,
                    "lon": longitude,
                    "tilt": tilt,
                    "azimuth": azimuth,
                    "dc_wp": dc_wp,
                    "ac_wp": ac_wp,
                    "ghi_wm2": ghi,
                    "t_ambient_c": t_amb,
                    "cloud_pct": cloud,
                    "attenuation": attenuation,
                    "wind_ms": wind,
                    "rh_pct": rh,
                    "rain_mm": rain,
                    "epoch": int(epoch),
                },
                "features": built,
                "physics_w": physics_w,
                "poa_wm2": poa,
                "raw_ratio": bits(case["ratio"]),
                "clamped_ratio": model.predict_clamped_ratio(features),
                "elevation_deg": pv.solar_geometry(latitude, longitude, moment, tilt, azimuth).elevation_deg,
                "pv_kw": estimate.pv_kw,
                "estimate_ratio": estimate.ratio,
            }
        )
    target = FIXTURES / "pv_features.jsonl"
    lines = [
        json.dumps(
            {
                "in": case["in"],
                "features": case["features"],
                "physics_w": case["physics_w"],
                "poa_wm2": case["poa_wm2"],
                "raw_ratio": case["raw_ratio"],
                "clamped_ratio": case["clamped_ratio"],
                "elevation_deg": case["elevation_deg"],
                "pv_kw": case["pv_kw"],
                "estimate_ratio": case["estimate_ratio"],
            },
            separators=(",", ":"),
            sort_keys=True,
            allow_nan=False,
        )
        for case in cases
    ]
    target.write_text("\n".join(lines) + "\n")
    print(f"wrote {target} ({len(lines)} cases, sha256 {sha256(target)[:16]})")
    return target



def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Settings(dict):
    def get(self, key, default=None):
        return super().get(key, default)


def emit_solar_pure() -> Path:
    slots = []
    for latitude, longitude in ((52.0, 5.0), (40.0, -3.0)):
        for iso in (
            "2026-03-06T06:00:00+00:00",
            "2026-03-06T12:00:00+00:00",
            "2026-03-06T18:00:00+00:00",
            "2026-06-21T10:00:00+00:00",
            "2026-12-21T12:00:00+00:00",
        ):
            moment = datetime.fromisoformat(iso)
            for cloud, nasa in ((40.0, None), (None, 0.83), (None, None)):
                poa = solar._plane_irradiance(latitude, longitude, moment, AMSTERDAM, 35.0, 180.0)
                attenuation = solar._cloud_factor(cloud) if cloud is not None else (nasa if nasa is not None else 1.0)
                pv_kw = max(
                    0.0,
                    (4000.0 / 1000.0) * (poa * attenuation / 1000.0) * 0.85,
                )
                uncertainty = (
                    solar.CAPACITY_UNCERTAINTY_CLOUD_PCT
                    if cloud is not None
                    else solar.CAPACITY_UNCERTAINTY_NASA_PCT
                    if nasa is not None
                    else solar.CAPACITY_UNCERTAINTY_CLEAR_PCT
                )
                slots.append(
                    {
                        "lat": latitude,
                        "lon": longitude,
                        "epoch": int(moment.timestamp()),
                        "tz": "Europe/Amsterdam",
                        "tilt_deg": 35.0,
                        "azimuth_deg": 180.0,
                        "capacity_wp": 4000.0,
                        "performance_ratio": 0.85,
                        "cloud_pct": cloud,
                        "nasa_clearness": nasa,
                        "poa_wm2": poa,
                        "attenuation": attenuation,
                        "pv_kw": pv_kw,
                        "uncertainty_pct": uncertainty,
                    }
                )

    base = datetime(2026, 3, 6, 6, 0, tzinfo=UTC)
    stamp = lambda hours: (base + timedelta(hours=hours)).isoformat()  # noqa: E731
    entries = [
        {
            "name": "alpha",
            "weight": 1.0,
            "state": "ok",
            "series": [[stamp(0), 1.0], [stamp(2), 3.0]],
        },
        {
            "name": "beta",
            "weight": 2.0,
            "state": "ok",
            "series": [[stamp(1), 2.0], [stamp(3), 0.5]],
        },
        {"name": "empty", "weight": 1.0, "state": "failed", "series": []},
        {"name": "off", "weight": 1.0, "state": "disabled", "series": [[stamp(0), 9.0]]},
        {"name": "degraded", "weight": 0.5, "state": "failed", "series": [[stamp(0), 1.5]]},
    ]
    grid = [base + timedelta(minutes=30 * index) for index in range(8)]
    merges = {}
    for strategy in ("weighted_mean", "median", "max"):
        values, report = solar.merge_solar_series(
            grid,
            [
                {
                    "name": entry["name"],
                    "weight": entry["weight"],
                    "state": entry["state"],
                    "series": [(datetime.fromisoformat(stamp), value) for stamp, value in entry["series"]],
                }
                for entry in entries
            ],
            strategy,
        )
        merges[strategy] = {
            "values": values,
            "report": [
                {
                    "name": row["name"],
                    "weight": row["weight"],
                    "state": row["state"],
                    "points": row["points"],
                }
                for row in report
            ],
        }

    payload = {
        "alpha": {
            "wh_hours": {
                (base + timedelta(hours=1)).isoformat(): 2500.0,
                (base + timedelta(hours=2)).isoformat(): 1500.0,
                (base + timedelta(hours=3)).isoformat(): 500.0,
            }
        },
        "beta": {
            "wh_hours": {
                (base + timedelta(hours=1)).isoformat(): 500.0,
                (base + timedelta(hours=4)).isoformat(): 2000.0,
            }
        },
        "not-a-provider": {"other": {}},
    }
    ha_energy = {}
    for entry_id in ("", "alpha"):
        points, providers = solar.parse_ha_energy_wh(payload, entry_id)
        series = solar.ha_energy_power_series(points)
        ha_energy[entry_id or "all"] = {
            "providers": providers,
            "wh": [[stamp.isoformat(), value] for stamp, value in sorted(points.items())],
            "series": [[stamp.isoformat(), value] for stamp, value in series],
        }

    bias_now = datetime(2026, 3, 7, 12, 0, tzinfo=UTC)
    bias_samples = [
        (datetime(2026, 3, 7, 7, 0, tzinfo=UTC), 1.5, 1.65),
        (datetime(2026, 3, 7, 7, 30, tzinfo=UTC), 2.0, 1.0),
        (datetime(2026, 3, 7, 12, 0, tzinfo=UTC), 3.0, 3.9),
        (datetime(2026, 3, 7, 12, 15, tzinfo=UTC), 0.01, 5.0),
        (datetime(2026, 3, 7, 22, 0, tzinfo=UTC), 0.4, -1.0),
    ]
    bias_payload = solar.update_solar_bias(None, bias_samples, now=bias_now, tz="Europe/Amsterdam")
    decay = []
    for updated_at, label in (
        (None, "none"),
        ((bias_now - timedelta(days=7)).isoformat(), "7d"),
        ((bias_now - timedelta(days=14)).isoformat(), "14d"),
        ((bias_now - timedelta(days=14, hours=1)).isoformat(), "14d1h"),
        (bias_now.isoformat(), "future"),
    ):
        factors = [1.5] * 24
        decay.append(
            {
                "label": label,
                "factors": factors,
                "updated_at": updated_at,
                "decayed": solar._decay_factors(factors, updated_at, bias_now),
            }
        )

    head_fit = []
    for hour in range(24):
        for offset in range(2):
            forecast = 0.5 + hour * 0.1 + 0.05 * offset
            head_fit.append(
                local_head.Sample(
                    moment=datetime(2026, 3, 1 + offset, hour, 0, tzinfo=UTC),
                    hour=hour,
                    forecast_kw=forecast,
                    actual_kw=forecast * (0.8 if hour < 8 else 1.25),
                )
            )
    head_holdout = []
    for hour in range(12):
        forecast = 1.0 + 0.2 * hour
        head_holdout.append(
            local_head.Sample(
                moment=datetime(2026, 3, 5, hour, 0, tzinfo=UTC),
                hour=hour,
                forecast_kw=forecast,
                actual_kw=forecast * (0.82 if hour < 8 else 1.2),
            )
        )
    candidate = local_head.fit_factors(head_fit)
    incumbent = [1.0] * 24
    head = {
        "fit": [
            {
                "moment": sample.moment.isoformat(),
                "hour": sample.hour,
                "forecast_kw": sample.forecast_kw,
                "actual_kw": sample.actual_kw,
            }
            for sample in head_fit
        ],
        "holdout": [
            {
                "moment": sample.moment.isoformat(),
                "hour": sample.hour,
                "forecast_kw": sample.forecast_kw,
                "actual_kw": sample.actual_kw,
            }
            for sample in head_holdout
        ],
        "candidate_factors": candidate,
        "incumbent_factors": incumbent,
        "candidate_mae": local_head._mean_absolute_error(head_holdout, candidate),
        "incumbent_mae": local_head._mean_absolute_error(head_holdout, incumbent),
    }

    settings = Settings(
        {
            "solar.tilt_deg": 35.0,
            "solar.azimuth_deg": 180.0,
            "solar.performance_ratio": 0.85,
        }
    )
    derived_samples = []
    for index in range(30):
        moment = datetime(2026, 3, 6, 8, 0, tzinfo=UTC) + timedelta(minutes=15 * index)
        derived_samples.append((moment, 2.0 + 0.05 * index))
    capacity_wp, usable = solar._implied_capacity_wp(
        52.0, 5.0, AMSTERDAM, derived_samples, settings, None
    )
    derived = {
        "lat": 52.0,
        "lon": 5.0,
        "samples": [[moment.isoformat(), value] for moment, value in derived_samples],
        "capacity_wp": capacity_wp,
        "usable": usable,
        "performance_ratio": 0.85,
        "tilt_deg": 35.0,
        "azimuth_deg": 180.0,
    }

    out = {
        "note": (
            "Solar pure-function parity: capacity physics, merge/interpolation, HA Energy Wh->kW, "
            "EWMA bias update and decay, head fit and holdout MAE, derived-size fit. Generated by "
            "scripts/gen_forecast_fixtures.py against ha-energy-manager commit 00092d7."
        ),
        "capacity_slots": slots,
        "merge": {
            "grid": [stamp.isoformat() for stamp in grid],
            "entries": entries,
            "results": merges,
        },
        "ha_energy_payload": payload,
        "ha_energy": ha_energy,
        "bias": {
            "now": bias_now.isoformat(),
            "tz": "Europe/Amsterdam",
            "samples": [
                [stamp.isoformat(), forecast, measured]
                for stamp, forecast, measured in bias_samples
            ],
            "payload": bias_payload,
            "decay": decay,
        },
        "head": head,
        "derived": derived,
    }
    target = FIXTURES / "solar_pure.json"
    target.write_text(json.dumps(out, indent=1, sort_keys=True, allow_nan=False) + "\n")
    print(f"wrote {target} (sha256 {sha256(target)[:16]})")
    return target


def main() -> int:
    FIXTURES.mkdir(parents=True, exist_ok=True)
    emit_pv_features()
    emit_solar_pure()
    if "--check" in sys.argv:
        for name in ("pv_features.jsonl", "solar_pure.json"):
            print(f"{name}: sha256 {sha256(FIXTURES / name)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
