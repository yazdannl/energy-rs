//! The safe Rust API around the vendored C DP solver.
//!
//! [`solve`] marshals a scenario and a [`Config`] into the flat C ABI, copies the result out and
//! then runs [`crate::validate::check`] on it.  The returned [`SolveResult`] mirrors the dict the
//! old CPython extension returned, field for field, so a call site can be compared with the
//! add-on's.
//!
//! Every failure is typed ([`SolveError`]) and every failure is a refusal: there is no "use the
//! plan anyway" path (`PLAN.md` §6).

use std::sync::OnceLock;

use crate::config::Config;
use crate::error::{InputError, SolveError};
use crate::ffi;
use crate::validate;

/// The five flat arrays one solve needs.  All of them must have the same length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveInput<'a> {
    /// Forecast PV power per slot, kW.
    pub pv_kw: &'a [f64],
    /// Forecast house load per slot, kW.
    pub load_kw: &'a [f64],
    /// Import price per slot, EUR/kWh.
    pub buy: &'a [f64],
    /// Export price per slot, EUR/kWh (unknown export is `0.0`, never "free").
    pub sell: &'a [f64],
    /// Modelled duration per slot, minutes.
    pub minutes: &'a [f64],
}

impl<'a> SolveInput<'a> {
    pub fn new(
        pv_kw: &'a [f64],
        load_kw: &'a [f64],
        buy: &'a [f64],
        sell: &'a [f64],
        minutes: &'a [f64],
    ) -> Self {
        Self {
            pv_kw,
            load_kw,
            buy,
            sell,
            minutes,
        }
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.pv_kw.len()
    }

    /// True when the scenario has no slots (a legal, degenerate solve).
    pub fn is_empty(&self) -> bool {
        self.pv_kw.is_empty()
    }
}

/// The two power windows `power_bounds_kw` intersected for one slot, in kW.
///
/// `soc_lo_kw ..= soc_hi_kw` is the SoC-reachable envelope, `grid_lo_kw ..= grid_hi_kw` the
/// grid-feasible one.  Kept per slot so a caller can name the constraint the chosen power sits on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub soc_lo_kw: f64,
    pub soc_hi_kw: f64,
    pub grid_lo_kw: f64,
    pub grid_hi_kw: f64,
}

impl Window {
    /// `[soc_lo, soc_hi, grid_lo, grid_hi]`, the order the old core returned.
    pub fn values(&self) -> [f64; 4] {
        [
            self.soc_lo_kw,
            self.soc_hi_kw,
            self.grid_lo_kw,
            self.grid_hi_kw,
        ]
    }
}

/// One validated solve; mirrors the old core's result dict.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveResult {
    /// Stored energy at the start of each slot, kWh.
    pub energies: Vec<f64>,
    /// Battery power per slot, kW (positive = charging).
    pub powers: Vec<f64>,
    /// The SoC and grid windows per slot.
    pub windows: Vec<Window>,
    /// The state grid, ascending, kWh.
    pub grid: Vec<f64>,
    /// The value function, `(slots + 1) * states` row-major; row `slots` is the terminal row.
    pub value: Vec<f64>,
    /// The exact value the solver minimised, EUR.
    pub objective_eur: f64,
    /// The wear accumulated over the returned path, EUR.
    pub cycle_cost_eur: f64,
    /// `|final SoC - e_target|`, kWh; zero when no target was set.
    pub terminal_error_kwh: f64,
    /// The step the grid really used (coarsened when the state budget tightened it), kWh.
    pub effective_step_kwh: f64,
    /// The value function's entry at the start state, EUR.
    pub internal_objective_eur: f64,
    /// Actions evaluated by the sweep.
    pub iterations: i64,
    /// Grid points in the state grid.
    pub states: usize,
}

impl SolveResult {
    /// Row `index` of the value function (`index == slots` is the terminal row).
    pub fn value_row(&self, index: usize) -> &[f64] {
        &self.value[index * self.states..(index + 1) * self.states]
    }

    /// The terminal row the sweep started from.
    pub fn terminal_row(&self) -> &[f64] {
        &self.value[self.powers.len() * self.states..]
    }

    fn from_ffi(owned: &ffi::OwnedResult) -> Self {
        let windows = owned
            .windows()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|window| Window {
                soc_lo_kw: window[0],
                soc_hi_kw: window[1],
                grid_lo_kw: window[2],
                grid_hi_kw: window[3],
            })
            .collect();
        Self {
            energies: owned.energies().to_vec(),
            powers: owned.powers().to_vec(),
            windows,
            grid: owned.grid().to_vec(),
            value: owned.value().to_vec(),
            objective_eur: owned.0.objective_eur,
            cycle_cost_eur: owned.0.cycle_cost_eur,
            terminal_error_kwh: owned.0.terminal_error_kwh,
            effective_step_kwh: owned.0.effective_step_kwh,
            internal_objective_eur: owned.0.internal_objective_eur,
            iterations: owned.0.iterations,
            states: owned.0.states as usize,
        }
    }
}

/// The C sources this binary was built from, as `build.rs` framed them.
fn source_digest() -> String {
    crate::sha256::digest_framed(&[
        (
            "csrc/core_solver.h",
            include_bytes!("../csrc/core_solver.h"),
        ),
        (
            "csrc/core_solver.c",
            include_bytes!("../csrc/core_solver.c"),
        ),
        (
            "csrc/core_adapter.c",
            include_bytes!("../csrc/core_adapter.c"),
        ),
        (
            "csrc/energy_opt_ffi.c",
            include_bytes!("../csrc/energy_opt_ffi.c"),
        ),
    ])
}

/// The digest the C artifact reports (`-DCORE_BUILD_SHA`), or `""` when it was not compiled in.
pub fn core_build_sha() -> String {
    ffi::build_sha()
}

/// Prove the loaded C artifact was built from the sources shipped in this binary.
///
/// This is the port of `_fast.stale_reason`: the digest covers the sources, so an object file that
/// was cached, copied or restored from a different revision cannot pass as a build of the current
/// source.  Checked once per process; every [`solve`] calls it.
pub fn verify_core_build() -> Result<(), SolveError> {
    static VERIFIED: OnceLock<Result<(), SolveError>> = OnceLock::new();
    VERIFIED
        .get_or_init(|| {
            let expected = env!("EO_CORE_SOURCE_SHA256");
            let shipped = source_digest();
            if shipped != expected {
                return Err(SolveError::StaleCore {
                    built: expected.chars().take(16).collect(),
                    expected: shipped.chars().take(16).collect(),
                });
            }
            let built = ffi::build_sha();
            let wanted = &expected[..16];
            if built != wanted {
                return Err(SolveError::StaleCore {
                    built,
                    expected: wanted.to_string(),
                });
            }
            Ok(())
        })
        .clone()
}

/// Why the C core refused the call, as a typed error.
fn core_error(code: i32, message: String) -> SolveError {
    match code {
        ffi::EO_ERR_TERMINAL_ROW => SolveError::TerminalRow(message),
        ffi::EO_ERR_ARGUMENT | ffi::EO_ERR_DOMAIN | ffi::EO_ERR_LIMITS | ffi::EO_ERR_GRID => {
            SolveError::Input(InputError::Rejected(message))
        }
        ffi::EO_ERR_MEMORY | ffi::EO_ERR_SWEEP => SolveError::Core { code, message },
        _ => SolveError::Core { code, message },
    }
}

/// Solve one scenario with the salvage terminal model.
pub fn solve(input: &SolveInput<'_>, config: &Config) -> Result<SolveResult, SolveError> {
    solve_inner(input, config, None)
}

/// Solve one scenario with an optimizer v2 stage-3 continuation row.
///
/// The row *is* the terminal condition (the caller's `dp._row_with_penalty` already carries the
/// target penalty); its length must equal the state grid the solver will build.
pub fn solve_with_terminal_row(
    input: &SolveInput<'_>,
    config: &Config,
    terminal_row: &[f64],
) -> Result<SolveResult, SolveError> {
    solve_inner(input, config, Some(terminal_row))
}

fn solve_inner(
    input: &SolveInput<'_>,
    config: &Config,
    terminal_row: Option<&[f64]>,
) -> Result<SolveResult, SolveError> {
    verify_core_build()?;
    config.validate()?;
    validate::check_input(input)?;
    let params = config.to_core_params()?;

    if let Some(row) = terminal_row {
        let (grid, _) = validate::reference_grid(config, input.len());
        if row.len() != grid.len() {
            return Err(SolveError::TerminalRow(format!(
                "the row has {} entries for a {}-state grid",
                row.len(),
                grid.len()
            )));
        }
        if row.iter().any(|value| !value.is_finite()) {
            return Err(SolveError::TerminalRow("the row is not finite".into()));
        }
    }

    let owned = ffi::solve(&params, input, terminal_row)
        .map_err(|code| core_error(code, ffi::last_error()))?;
    let result = SolveResult::from_ffi(&owned);
    validate::check(&result, input, config, terminal_row)?;
    Ok(result)
}
