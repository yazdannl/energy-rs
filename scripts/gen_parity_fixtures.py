#!/usr/bin/env python3
"""Generate the DP parity fixtures from the *old* add-on's Python reference (P1).

The Rust port has three implementations of the same DP: the vendored C core (the plan producer),
the Rust validator (per-solve re-derivation) and the Rust brute-force oracle (the differential).
This script pins all of them to the one implementation with years of field time: the unchanged
pure-Python ``dp._solve_once_numeric`` in ``ha-energy-manager``.

Each case is a full scenario plus the reference's numeric result: the reported scalars, the path,
the windows, the state grid and the *whole* value function.  The Rust test compares the objective
and every path value at 1e-9 (the tolerance the old differential used) and the per-slot decisions
exactly.  Cases are deterministic (fixed formulas, no RNG), so regenerating the file is a no-op
unless the reference or a case changed.

Usage::

    python3 scripts/gen_parity_fixtures.py            # (re)write the fixture
    python3 scripts/gen_parity_fixtures.py --check    # fail when the checked-in file is stale

The old repository is read-only here: this script imports it and writes nothing into it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import pathlib
import sys
from datetime import UTC, datetime, timedelta

#: Where the reference implementation lives (the old add-on; never modified by this project).
OLD_REPO = pathlib.Path("/root/HomeAssitant/ha-energy-manager")

#: The reference's grid budgets, pinned so a change there is visible instead of silently absorbed.
EXPECTED_MAX_STATES = 400
EXPECTED_MAX_ITERATIONS = 2_000_000

#: Two battery aggregates: a four-pack fleet and a single small pack.
BATTERY_FLEET = {
    "capacity_kwh": 15.36,
    "e_min_kwh": 1.536,
    "e_max_kwh": 15.36,
    "p_charge_kw": 3.2,
    "p_discharge_kw": 3.2,
    "eta_charge": 0.95,
    "eta_discharge": 0.95,
}
BATTERY_SMALL = {
    "capacity_kwh": 2.4,
    "e_min_kwh": 0.24,
    "e_max_kwh": 2.4,
    "p_charge_kw": 0.8,
    "p_discharge_kw": 0.8,
    "eta_charge": 0.92,
    "eta_discharge": 0.92,
}

DEFAULTS: dict[str, object] = {
    "slot_minutes": 15.0,
    "soc_step_kwh": 0.4,
    "power_steps": 12,
    "allow_grid_charge": True,
    "allow_export": True,
    "max_import_kw": 0.0,
    "max_export_kw": 0.0,
    "min_spread_eur_per_kwh": 0.0,
    "wear_eur_per_kwh": 0.02,
    "salvage": 0.05,
    "target_penalty": 0.5,
    "max_states": EXPECTED_MAX_STATES,
    "max_iterations": EXPECTED_MAX_ITERATIONS,
    "refuse_battery_export": False,
    "sell_floor_eur_per_kwh": None,
    "pv_charge_preference": "auto",
    "e_target": None,
}


def series(n: int, seed: int, *, scale: float, offset: float = 0.0) -> list[float]:
    """A deterministic, smooth series (no RNG, no platform-dependent rounding)."""
    return [
        offset + scale * math.sin(math.pi * (i + seed) / max(1, n - 1) + seed * 0.7)
        for i in range(n)
    ]


def build_slots(
    n: int,
    seed: int,
    *,
    pv_scale: float,
    load_scale: float,
    minutes: float,
    buy_base: float,
    buy_spread: float,
    sell: float,
) -> dict[str, list[float]]:
    """One scenario's five arrays."""
    pv = [max(0.0, value) for value in series(n, seed, scale=pv_scale, offset=pv_scale * 0.1)]
    load = [max(0.05, value) for value in series(n, seed + 3, scale=load_scale, offset=load_scale)]
    buy = [buy_base + buy_spread * math.sin(2.0 * math.pi * (i + seed) / 7.0) for i in range(n)]
    return {
        "pv_kw": pv,
        "load_kw": load,
        "buy": buy,
        "sell": [sell] * n,
        "minutes": [minutes] * n,
    }


def battery(kind: str) -> dict[str, float]:
    return dict(BATTERY_FLEET if kind == "fleet" else BATTERY_SMALL)


def case(
    name: str,
    *,
    battery_spec: dict[str, float],
    start_kwh: float,
    slots: dict[str, list[float]],
    overrides: dict[str, object] | None = None,
    terminal_row: str | None = None,
    coarsen: bool = False,
) -> dict[str, object]:
    config = dict(DEFAULTS)
    config.update(overrides or {})
    return {
        "name": name,
        "battery": battery_spec,
        "start_kwh": start_kwh,
        "config": config,
        "slots": slots,
        "terminal_row": terminal_row,
        "coarsen": coarsen,
    }


def case_list() -> list[dict[str, object]]:
    """The 20 deterministic scenarios; each name states the behaviour it pins."""
    day = build_slots(
        24, 1, pv_scale=3.5, load_scale=1.1, minutes=15.0, buy_base=0.24, buy_spread=0.09, sell=0.06
    )
    week = build_slots(
        40, 5, pv_scale=4.0, load_scale=1.3, minutes=15.0, buy_base=0.21, buy_spread=0.13, sell=0.07
    )
    hourly = build_slots(
        8, 2, pv_scale=2.0, load_scale=0.9, minutes=60.0, buy_base=0.30, buy_spread=0.05, sell=0.05
    )
    negative = build_slots(
        6, 4, pv_scale=1.5, load_scale=0.8, minutes=60.0, buy_base=0.0, buy_spread=0.3, sell=0.02
    )
    inverted = build_slots(
        12, 7, pv_scale=2.5, load_scale=1.0, minutes=30.0, buy_base=0.12, buy_spread=0.02, sell=0.35
    )
    no_pv = build_slots(
        16, 9, pv_scale=0.0, load_scale=1.2, minutes=30.0, buy_base=0.19, buy_spread=0.11, sell=0.04
    )
    flat = {**week, "buy": [0.20] * len(week["buy"]), "sell": [0.20] * len(week["buy"])}
    zero_minutes = dict(hourly)
    zero_minutes["minutes"] = [0.0] + [60.0] * 7

    return [
        case("fleet-day-salvage", battery_spec=battery("fleet"), start_kwh=7.0, slots=day),
        case(
            "small-hourly-wear",
            battery_spec=battery("small"),
            start_kwh=1.2,
            slots=hourly,
            overrides={"wear_eur_per_kwh": 0.09, "power_steps": 24},
        ),
        case(
            "negative-prices-grid-charge",
            battery_spec=battery("fleet"),
            start_kwh=3.0,
            slots=negative,
        ),
        case(
            "negative-prices-no-grid-charge",
            battery_spec=battery("fleet"),
            start_kwh=3.0,
            slots=negative,
            overrides={"allow_grid_charge": False},
        ),
        case(
            "inverted-tariff-export",
            battery_spec=battery("small"),
            start_kwh=1.0,
            slots=inverted,
            overrides={"min_spread_eur_per_kwh": 0.01},
        ),
        case(
            "export-forbidden",
            battery_spec=battery("fleet"),
            start_kwh=6.0,
            slots=day,
            overrides={"allow_export": False},
        ),
        case(
            "refuse-battery-export",
            battery_spec=battery("fleet"),
            start_kwh=6.0,
            slots=day,
            overrides={"refuse_battery_export": True},
        ),
        case(
            "sell-floor-blocks-discharge",
            battery_spec=battery("fleet"),
            start_kwh=12.0,
            slots=week,
            overrides={"sell_floor_eur_per_kwh": 0.25},
        ),
        case(
            "grid-caps",
            battery_spec=battery("fleet"),
            start_kwh=5.0,
            slots=week,
            overrides={"max_import_kw": 1.5, "max_export_kw": 0.5},
        ),
        case(
            "spread-guard",
            battery_spec=battery("fleet"),
            start_kwh=4.0,
            slots=flat,
            overrides={"min_spread_eur_per_kwh": 0.05},
        ),
        case(
            "prefer-pv",
            battery_spec=battery("fleet"),
            start_kwh=8.0,
            slots=day,
            overrides={"pv_charge_preference": "prefer_pv"},
        ),
        case(
            "avoid-pv",
            battery_spec=battery("fleet"),
            start_kwh=8.0,
            slots=day,
            overrides={"pv_charge_preference": "avoid_pv"},
        ),
        case(
            "terminal-target",
            battery_spec=battery("fleet"),
            start_kwh=4.0,
            slots=day,
            overrides={"e_target": 12.0, "target_penalty": 1.5},
        ),
        case(
            "terminal-target-foot-gun",
            battery_spec=battery("small"),
            start_kwh=2.2,
            slots=hourly,
            overrides={"e_target": 0.3, "target_penalty": 3.0},
        ),
        case(
            "stage3-row",
            battery_spec=battery("fleet"),
            start_kwh=6.5,
            slots=week,
            overrides={"salvage": 0.0, "target_penalty": 0.0},
            terminal_row="taper",
        ),
        case(
            "stage3-row-target",
            battery_spec=battery("fleet"),
            start_kwh=6.5,
            slots=day,
            overrides={"e_target": 10.0, "target_penalty": 0.4},
            terminal_row="taper",
        ),
        case(
            "coarsened-grid",
            battery_spec=battery("fleet"),
            start_kwh=9.0,
            slots=week,
            overrides={"soc_step_kwh": 0.02, "max_iterations": 52_000},
            coarsen=True,
        ),
        case(
            "degenerate-empty",
            battery_spec=battery("small"),
            start_kwh=1.0,
            slots={"pv_kw": [], "load_kw": [], "buy": [], "sell": [], "minutes": []},
            overrides={"salvage": 0.1},
        ),
        case(
            "zero-minute-slot",
            battery_spec=battery("fleet"),
            start_kwh=9.0,
            slots=zero_minutes,
            overrides={"e_target": 9.0},
        ),
        case(
            "no-pv-no-export",
            battery_spec=battery("fleet"),
            start_kwh=2.0,
            slots=no_pv,
            overrides={"allow_export": False, "wear_eur_per_kwh": 0.0},
        ),
    ]


def coarsened_state_grid(dp, agg, opts, state_grid, slot_count, max_iterations, anchors):
    """The grid the C core builds when ``MAX_ITERATIONS / (slots * actions)`` binds.

    ``dp._main_grid`` uses the module's ``MAX_ITERATIONS``; a case that pins the coarsening path
    sets a smaller budget in its config, so the reference is handed the grid the core must build
    (the same ``max(step, span / (budget - 1))`` rebuild), index for index.
    """
    action_count = max(2, int(opts.power_steps) + 1)
    budget = max(2, int(max_iterations) // max(1, slot_count * action_count))
    if len(state_grid.points) <= budget:
        raise SystemExit("coarsened-grid case does not actually coarsen; adjust its budget")
    span = max(0.0, agg.e_max_kwh - agg.e_min_kwh)
    widened = max(state_grid.step_kwh, span / max(1, budget - 1))
    points, step = dp._build_grid(agg, widened, anchors)
    return dp._StateGrid(points, step, coarsened=True)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--old-repo", default=str(OLD_REPO))
    parser.add_argument(
        "--out",
        default=str(
            pathlib.Path(__file__).resolve().parent.parent
            / "crates/energy-opt/tests/fixtures/parity.json"
        ),
    )
    parser.add_argument("--check", action="store_true", help="verify the fixture is current")
    args = parser.parse_args(argv)

    repo = pathlib.Path(args.old_repo)
    dp_source = repo / "energy_manager/app/optimize/dp.py"
    if not dp_source.exists():
        raise SystemExit(f"reference implementation not found: {dp_source}")
    dp_digest = hashlib.sha256(dp_source.read_bytes()).hexdigest()

    sys.path.insert(0, str(repo))
    from energy_manager.app.optimize import dp  # noqa: PLC0415 - after sys.path

    if dp.MAX_STATES != EXPECTED_MAX_STATES or dp.MAX_ITERATIONS != EXPECTED_MAX_ITERATIONS:
        raise SystemExit("the reference's MAX_STATES/MAX_ITERATIONS changed; re-pin the fixtures")

    out_cases = []
    for item in case_list():
        config = item["config"]
        slots_raw = item["slots"]
        minutes = list(slots_raw["minutes"])
        t0 = datetime(2026, 1, 1, tzinfo=UTC)
        slots = [
            dp.SlotInput(
                t=t0 + timedelta(minutes=config["slot_minutes"] * i),
                pv_kw=slots_raw["pv_kw"][i],
                load_kw=slots_raw["load_kw"][i],
                buy=slots_raw["buy"][i],
                sell=slots_raw["sell"][i],
            )
            for i in range(len(slots_raw["pv_kw"]))
        ]
        spec = item["battery"]
        agg = dp.BatteryAggregate(
            capacity_kwh=spec["capacity_kwh"],
            e_min_kwh=spec["e_min_kwh"],
            e_max_kwh=spec["e_max_kwh"],
            p_charge_kw=spec["p_charge_kw"],
            p_discharge_kw=spec["p_discharge_kw"],
            eta_charge=spec["eta_charge"],
            eta_discharge=spec["eta_discharge"],
            usable_kwh=max(0.0, spec["e_max_kwh"] - spec["e_min_kwh"]),
            batteries=1,
            wear_eur_per_kwh=config["wear_eur_per_kwh"],
        )
        opts = dp.OptimizerOptions(
            slot_minutes=int(config["slot_minutes"]),
            soc_step_kwh=config["soc_step_kwh"],
            power_steps=int(config["power_steps"]),
            wear_eur_per_kwh=config["wear_eur_per_kwh"],
            allow_grid_charge=config["allow_grid_charge"],
            allow_export=config["allow_export"],
            max_import_kw=config["max_import_kw"],
            max_export_kw=config["max_export_kw"],
            min_spread_eur_per_kwh=config["min_spread_eur_per_kwh"],
            pv_charge_preference=config["pv_charge_preference"],
            sell_floor_eur_per_kwh=config["sell_floor_eur_per_kwh"],
            refuse_battery_export=config["refuse_battery_export"],
        )
        start_kwh = item["start_kwh"]
        e_target = config["e_target"]
        anchors = dp._anchors(start_kwh, agg, e_target)
        state_grid = dp._main_grid(agg, opts, len(slots), anchors)
        if item["coarsen"]:
            state_grid = coarsened_state_grid(
                dp, agg, opts, state_grid, len(slots), config["max_iterations"], anchors
            )
        grid = state_grid.points
        row = None
        if item["terminal_row"] == "taper":
            # A smooth, strictly decreasing continuation row: a plausible tail value for a pack
            # whose remaining energy is worth less the less it holds.
            row = [-0.04 * energy - 0.02 for energy in grid]

        result = dp._solve_once_numeric(
            slots,
            agg,
            start_kwh,
            opts,
            config["target_penalty"],
            e_target,
            config["salvage"],
            minutes,
            terminal_row=row,
            state_grid=state_grid,
        )
        sweep_row = row or dp._salvage_terminal_row(
            grid, config["salvage"], config["target_penalty"], e_target
        )
        value, _argmin, _iterations = dp._backward_sweep(
            grid, slots, minutes, agg, opts, sweep_row, e_target
        )
        out_cases.append(
            {
                "name": item["name"],
                "battery": spec,
                "start_kwh": start_kwh,
                "config": config,
                "slots": slots_raw,
                "terminal_row": item["terminal_row"],
                "expected": {
                    "objective_eur": result.objective_eur,
                    "cycle_cost_eur": result.cycle_cost_eur,
                    "terminal_error_kwh": result.terminal_error_kwh,
                    "internal_objective_eur": result.internal_objective_eur,
                    "iterations": result.iterations,
                    "effective_step_kwh": result.effective_step_kwh,
                    "states": result.states,
                    "energies": result.energies,
                    "powers": result.powers,
                    "windows": [value for window in result.windows for value in window],
                    "grid": grid,
                    "value": [entry for line in value for entry in line],
                },
            }
        )

    fixture = {
        "generated_by": "scripts/gen_parity_fixtures.py",
        "reference": {
            "repo": repo.name,
            "file": "energy_manager/app/optimize/dp.py",
            "sha256": dp_digest,
            "max_states": dp.MAX_STATES,
            "max_iterations": dp.MAX_ITERATIONS,
        },
        "tolerance": {"objective": 1e-9, "path": 1e-9, "value": 1e-9},
        "cases": out_cases,
    }
    payload = json.dumps(fixture, indent=1, sort_keys=True) + "\n"

    target = pathlib.Path(args.out)
    if args.check:
        current = target.read_text(encoding="utf-8") if target.exists() else ""
        if current != payload:
            print(f"gen_parity_fixtures: {target} is stale; rerun without --check", file=sys.stderr)
            return 1
        print(f"gen_parity_fixtures: {target} matches dp.py {dp_digest[:16]}")
        return 0

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(payload, encoding="utf-8")
    print(
        f"gen_parity_fixtures: wrote {target} ({len(out_cases)} cases, "
        f"{len(payload) // 1024} KiB) from dp.py {dp_digest[:16]}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
