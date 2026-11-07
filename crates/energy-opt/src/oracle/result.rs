//! One brute-force solve's result.

/// One brute-force solve; the fields line up with [`crate::dp::SolveResult`].
#[derive(Debug, Clone, PartialEq)]
pub struct OracleResult {
    /// The state grid, ascending, kWh.
    pub grid: Vec<f64>,
    /// The value function, `(slots + 1) * states` row-major.
    pub value: Vec<f64>,
    /// The stored choice per `(slot, state)`, `slots * states` row-major.
    pub argmin: Vec<f64>,
    /// Stored energy at the start of each slot, kWh.
    pub energies: Vec<f64>,
    /// Battery power per slot, kW.
    pub powers: Vec<f64>,
    /// SoC and grid windows per slot: `[soc_lo, soc_hi, grid_lo, grid_hi]`.
    pub windows: Vec<[f64; 4]>,
    /// The objective the reference recomputed, EUR.
    pub objective_eur: f64,
    /// The wear accumulated over the path, EUR.
    pub cycle_cost_eur: f64,
    /// `|final SoC - e_target|`, kWh; zero without a target.
    pub terminal_error_kwh: f64,
    /// The step the grid really used, kWh.
    pub effective_step_kwh: f64,
    /// The value function's entry at the start state, EUR.
    pub internal_objective_eur: f64,
    /// Actions evaluated by the sweep.
    pub iterations: i64,
    /// Grid points in the state grid.
    pub states: usize,
}
