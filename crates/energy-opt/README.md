# energy-opt

DP optimizer core (`PLAN.md` §2, §6): the vendored C solver, the Rust API around it, the
verification that refuses a bad plan, and a brute-force oracle for the differential checks.

```rust
use energy_opt::{BatterySpec, Config, SolveInput, solve};

let mut config = Config::new(
    BatterySpec {
        e_min_kwh: 1.536,
        e_max_kwh: 15.36,
        p_charge_kw: 3.2,
        p_discharge_kw: 3.2,
        eta_charge: 0.95,
        eta_discharge: 0.95,
    },
    7.0,
);
config.slot_minutes = 15.0;
config.salvage = 0.05;
config.wear_eur_per_kwh = 0.02;

let input = SolveInput::new(&pv_kw, &load_kw, &buy, &sell, &minutes);
let plan = solve(&input, &config)?;          // Result<SolveResult, SolveError>
for (energy, power) in plan.energies.iter().zip(&plan.powers) {
    // plan.windows[i], plan.objective_eur, plan.grid, plan.value, ...
}
```

`solve` mirrors the old `solve(pv, load, buy, sell, minutes, config[, terminal_row]) -> dict` call
field for field, and every failure is typed. There is no "use the plan anyway" path.

## Layout

```
csrc/core_solver.c     generated: the numeric core of the old _core.c, verbatim (see csrc/UPSTREAM.md)
csrc/core_solver.h     the CPython shim, the parameter block and the C ABI
csrc/core_adapter.c    maps the flat ABI onto the extracted static helpers (includes core_solver.c)
csrc/energy_opt_ffi.c  the plain C ABI: arrays + params -> eo_result, eo_free, typed status codes
src/config.rs          the 20 required + 3 optional config keys, domain checks
src/dp.rs              SolveInput / SolveResult / solve / solve_with_terminal_row, build identity
src/validate.rs        the dp.py reference helpers + the _fast._validated port (refuses on mismatch)
src/oracle.rs          a deliberately naive Rust DP (differential only)
src/ffi.rs             the #[repr(C)] declarations and the owning result wrapper
tests/parity.rs        the C core against the Python reference (20 fixtures, 1e-9 + exact decisions)
tests/oracle.rs        the C core against the Rust oracle (whole value function, every argmin)
tests/bench.rs         120 slots x 398 states x 24 steps < 50 ms
scripts/extract_core.py, scripts/gen_parity_fixtures.py   (repository root)
```

## What "validated" means here

Every solve re-derives the plan before it is returned (`validate::check`, a port of
`_fast._validated`):

* structure, lengths and finiteness of every number, including the whole value buffer;
* the state grid rebuilt from the config and compared point by point, step included;
* every power inside the re-derived SoC/grid bounds, and every window equal to the reference's;
* every per-slot decision **equal** to the reference's strict argmin (ties → lowest power), at the
  grid-snapped state, with the same action set and the same reconstruction clamp;
* the objective, terminal error and cycle cost re-derived from the returned path;
* every transition inside `[e_min, e_max]`, and a sampled backward Bellman pass over the value
  function — the whole last row when a stage-3 continuation row was supplied.

Any mismatch returns `SolveError::Refused`; the caller holds. Interior optimality is not proven by
these checks — that is what the periodic full differential against `oracle` is for.

## Parity and performance

* 20 scenarios (`tests/fixtures/parity.json`) compared against the unchanged Python reference:
  objective and every path/value entry at 1e-9, decisions exactly, iteration counts exactly.
* The Rust oracle reproduces the value function and every `slots × states` decision on the same
  scenarios.
* `tests/bench.rs` measures `solve()` (C sweep **and** validation) on the production shape —
  120 slots, 398 states, 24 power steps, 1.19M actions — and asserts < 50 ms. Measured on the
  aarch64 dev box: **18.8 ms** median in a debug build and **17.7 ms** in release; the cost is the C
  sweep itself (the action list is rebuilt and sorted for every state), not the validation.

## The build identity

`build.rs` compiles the C with `-O3 -ffp-contract=off -fno-fast-math` and
`-DCORE_BUILD_SHA=<sha16>` (the SHA-256 prefix of the four C sources). The Rust binary carries the
full digest and re-hashes the sources it shipped on the first solve
(`verify_core_build`), so a stale object file cannot run. `-ffp-contract=off` is not stylistic: FMA
contraction flips near-tie argmins.

Inputs are fixed-point-free `f64` throughout; prices are EUR/kWh, powers kW (positive = charging),
energies kWh, durations minutes.
