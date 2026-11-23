//! Optimizer configuration: the live learned-basis values and the solver lattice knobs.

/// Hard physics, site ceilings and solver discretisation for one optimization.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerConfig {
    /// Stored energy at the first slot, kWh.
    pub start_kwh: f64,
    /// Learned pooled capacity used as the planning ceiling, kWh.
    pub ceiling_kwh: f64,
    /// Hard learned-basis reserve floor, kWh.
    pub hard_floor_kwh: f64,
    /// Soft end-of-horizon guideline, kWh; reported separately from the effective target.
    pub soft_guideline_kwh: f64,
    /// Aggregate AC charge power, W.
    pub charge_power_w: f64,
    /// Aggregate AC discharge power, W.
    pub discharge_power_w: f64,
    /// Charge efficiency, 0 < eta <= 1.
    pub eta_charge: f64,
    /// Discharge efficiency, 0 < eta <= 1.
    pub eta_discharge: f64,
    /// Wear per **stored** kWh discharged, EUR/kWh.
    pub wear_eur_per_stored_kwh: f64,
    /// One-sided shortfall penalty per stored kWh, EUR/kWh.
    pub target_penalty_eur_per_kwh: f64,
    /// Deductible terminal deadband, kWh.
    pub terminal_deadband_kwh: f64,
    /// Planned site import ceiling after margin, kW.
    pub import_limit_kw: f64,
    /// Policy-only grid-charge spread floor per AC kWh above PV surplus, EUR/kWh.
    /// Reported as a preference; never settled money or part of the minimised objective.
    pub min_spread_eur_per_kwh: f64,
    /// Configured state lattice step, kWh.
    pub soc_step_kwh: f64,
    /// State budget before the lattice is coarsened.
    pub max_states: usize,
    /// Number of evenly spaced power actions across the battery window (plus zero).
    pub power_steps: usize,
    /// Number of packs behind the aggregate power cap.
    pub pack_count: usize,
    /// Per-pack AC charge cap, W.
    pub pack_charge_w: f64,
    /// Per-pack AC discharge cap, W.
    pub pack_discharge_w: f64,
    /// Smallest allocatable per-pack setpoint, W.
    pub min_pack_power_w: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            start_kwh: 6.995,
            ceiling_kwh: crate::LEARNED_CAPACITY_KWH,
            hard_floor_kwh: crate::HARD_FLOOR_KWH,
            soft_guideline_kwh: crate::SOFT_GUIDELINE_KWH,
            charge_power_w: 4000.0,
            discharge_power_w: 3200.0,
            eta_charge: 0.9,
            eta_discharge: 0.9,
            wear_eur_per_stored_kwh: 0.02,
            target_penalty_eur_per_kwh: 0.15,
            terminal_deadband_kwh: crate::TERMINAL_TOLERANCE_KWH,
            import_limit_kw: crate::IMPORT_CEILING_W / 1000.0,
            min_spread_eur_per_kwh: 0.0,
            soc_step_kwh: 0.05,
            max_states: 400,
            power_steps: 24,
            pack_count: 4,
            pack_charge_w: 1000.0,
            pack_discharge_w: 800.0,
            min_pack_power_w: 50.0,
        }
    }
}

impl OptimizerConfig {
    /// Effective end-of-horizon minimum: soft guideline raised to the hard floor, capped at ceiling.
    pub fn effective_target_kwh(&self) -> f64 {
        self.soft_guideline_kwh
            .max(self.hard_floor_kwh)
            .min(self.ceiling_kwh)
    }

    /// Learned-basis solar headroom reported separately; never a second constraint.
    pub fn solar_headroom_kwh(&self) -> f64 {
        0.2 * self.ceiling_kwh
    }
}
