//! The solver configuration itself.

use std::os::raw::c_long;

use super::{BatterySpec, PvChargePreference};
use crate::error::ConfigError;
use crate::ffi::EoCoreParams;

/// The dimension ceilings the C core enforces (`CORE_MAX_DIMENSION`, `CORE_MIN_STATES`).
pub const MAX_DIMENSION: i64 = 1_000_000;
/// The smallest state grid the C core accepts.
pub const MIN_STATES: i64 = 8;

/// The solver configuration.
///
/// [`Config::new`] fills the tunables with the shipped defaults from the old add-on
/// (`OptimizerOptions`, `models.py`) and no salvage, target, floor or export refusal; override the
/// fields a call site cares about.  [`Config::validate`] is the domain gate:
///
/// ```
/// use energy_opt::{BatterySpec, Config};
///
/// let battery = BatterySpec {
///     e_min_kwh: 0.5,
///     e_max_kwh: 9.5,
///     p_charge_kw: 3.0,
///     p_discharge_kw: 3.0,
///     eta_charge: 0.95,
///     eta_discharge: 0.95,
/// };
/// let mut config = Config::new(battery, 4.0);
/// config.soc_step_kwh = 0.5;
/// config.power_steps = 8;
/// config.salvage = 0.05;
/// config.validate().unwrap();
/// ```
#[derive(Debug, Clone, PartialEq)]
// One flag per independent policy switch, never a state machine: charging from the grid, exporting,
// refusing a battery export and aligning the action set are four separate decisions.
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    // ---- battery aggregate (the 6 required physical keys) ----
    /// Lowest usable stored energy, kWh.
    pub e_min_kwh: f64,
    /// Highest usable stored energy, kWh.
    pub e_max_kwh: f64,
    /// Maximum charge power, kW.
    pub p_charge_kw: f64,
    /// Maximum discharge power, kW.
    pub p_discharge_kw: f64,
    /// Charge efficiency.
    pub eta_charge: f64,
    /// Discharge efficiency.
    pub eta_discharge: f64,
    // ---- required state / horizon keys ----
    /// Stored energy at the start of slot 0, kWh.
    pub start_kwh: f64,
    /// Nominal slot length, minutes.
    pub slot_minutes: f64,
    /// Energy step of the state grid, kWh.
    pub soc_step_kwh: f64,
    /// Power steps of the action set.
    pub power_steps: i64,
    /// State-grid ceiling.
    pub max_states: i64,
    /// Sweep work ceiling.
    pub max_iterations: i64,
    // ---- required policy keys ----
    /// Whether the pack may charge from the grid.
    pub allow_grid_charge: bool,
    /// Whether export is allowed at all.
    pub allow_export: bool,
    /// Import ceiling, kW (0 = unlimited).
    pub max_import_kw: f64,
    /// Export ceiling, kW (0 = unlimited).
    pub max_export_kw: f64,
    /// Minimum buy/sell spread before grid charging pays, EUR/kWh.
    pub min_spread_eur_per_kwh: f64,
    /// Wear per kWh delivered, EUR/kWh.
    pub wear_eur_per_kwh: f64,
    /// Value of stored energy left at the horizon, EUR/kWh.
    pub salvage: f64,
    /// Penalty per kWh of terminal target error, EUR/kWh.
    pub target_penalty: f64,
    // ---- optional keys (presence is the C core's `has_*` flag) ----
    /// Terminal SoC target, kWh.
    pub e_target: Option<f64>,
    /// Refuse any action that exports battery energy.
    pub refuse_battery_export: bool,
    /// Minimum sell price for export, EUR/kWh.
    pub sell_floor_eur_per_kwh: Option<f64>,
    // ---- enum with a default, not an optional key ----
    /// How PV energy charged into the pack is priced.
    pub pv_charge_preference: PvChargePreference,
    /// Add the two grid-neutral powers to the action set: charge exactly the PV surplus, and
    /// discharge exactly the house's load. They are what a 24-step lattice cannot otherwise express
    /// on a small surplus or a load between two steps.
    pub align_actions: bool,
}

impl Config {
    /// A config with the shipped optimizer defaults (`models.py` `OptimizerOptions`) and no
    /// salvage, terminal target, sell floor or export refusal.
    pub fn new(battery: BatterySpec, start_kwh: f64) -> Self {
        Self {
            e_min_kwh: battery.e_min_kwh,
            e_max_kwh: battery.e_max_kwh,
            p_charge_kw: battery.p_charge_kw,
            p_discharge_kw: battery.p_discharge_kw,
            eta_charge: battery.eta_charge,
            eta_discharge: battery.eta_discharge,
            start_kwh,
            slot_minutes: 60.0,
            soc_step_kwh: 0.02,
            power_steps: 24,
            max_states: 400,
            max_iterations: 2_000_000,
            allow_grid_charge: true,
            allow_export: true,
            max_import_kw: 0.0,
            max_export_kw: 0.0,
            min_spread_eur_per_kwh: 0.0,
            wear_eur_per_kwh: 0.0,
            salvage: 0.0,
            target_penalty: 0.0,
            e_target: None,
            refuse_battery_export: false,
            sell_floor_eur_per_kwh: None,
            pv_charge_preference: PvChargePreference::Auto,
            align_actions: false,
        }
    }

    /// The config's domain checks.  Transcribed from `parse_config`'s tail (`_core.c:630-658`);
    /// the C core repeats them, so a direct C ABI caller is refused the same way.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.check_finite()?;
        self.check_dimensions()?;
        self.check_domain()
    }

    /// Every number the core reads must be finite, optional keys included.
    fn check_finite(&self) -> Result<(), ConfigError> {
        for (field, value) in [
            ("e_min_kwh", self.e_min_kwh),
            ("e_max_kwh", self.e_max_kwh),
            ("p_charge_kw", self.p_charge_kw),
            ("p_discharge_kw", self.p_discharge_kw),
            ("eta_charge", self.eta_charge),
            ("eta_discharge", self.eta_discharge),
            ("start_kwh", self.start_kwh),
            ("slot_minutes", self.slot_minutes),
            ("soc_step_kwh", self.soc_step_kwh),
            ("max_import_kw", self.max_import_kw),
            ("max_export_kw", self.max_export_kw),
            ("min_spread_eur_per_kwh", self.min_spread_eur_per_kwh),
            ("wear_eur_per_kwh", self.wear_eur_per_kwh),
            ("salvage", self.salvage),
            ("target_penalty", self.target_penalty),
        ] {
            if !value.is_finite() {
                return Err(ConfigError::NonFinite { field });
            }
        }
        for (field, value) in [
            ("e_target", self.e_target),
            ("sell_floor_eur_per_kwh", self.sell_floor_eur_per_kwh),
        ] {
            if value.is_some_and(|number| !number.is_finite()) {
                return Err(ConfigError::NonFinite { field });
            }
        }
        Ok(())
    }

    /// Counts the C core indexes with, inside the ranges it documents.
    fn check_dimensions(&self) -> Result<(), ConfigError> {
        if self.power_steps < 0 || self.power_steps > MAX_DIMENSION {
            return Err(ConfigError::PowerSteps(self.power_steps));
        }
        if self.max_states < MIN_STATES || self.max_states > MAX_DIMENSION {
            return Err(ConfigError::MaxStates(self.max_states));
        }
        if self.max_iterations < 1 {
            return Err(ConfigError::MaxIterations(self.max_iterations));
        }
        Ok(())
    }

    /// The physical and policy coherence checks.
    fn check_domain(&self) -> Result<(), ConfigError> {
        if self.e_min_kwh > self.e_max_kwh || !(self.e_max_kwh - self.e_min_kwh).is_finite() {
            return Err(ConfigError::EnergyBounds {
                e_min_kwh: self.e_min_kwh,
                e_max_kwh: self.e_max_kwh,
            });
        }
        if self.eta_charge <= 0.0 || self.eta_discharge <= 0.0 {
            return Err(ConfigError::Efficiencies {
                eta_charge: self.eta_charge,
                eta_discharge: self.eta_discharge,
            });
        }
        if self.p_charge_kw < 0.0 || self.p_discharge_kw < 0.0 {
            return Err(ConfigError::PowerLimits {
                p_charge_kw: self.p_charge_kw,
                p_discharge_kw: self.p_discharge_kw,
            });
        }
        if self.slot_minutes < 0.0 {
            return Err(ConfigError::SlotMinutes(self.slot_minutes));
        }
        Ok(())
    }

    /// The C parameter block, or a typed error when a count does not fit the C `long`.
    pub(crate) fn to_core_params(&self) -> Result<EoCoreParams, ConfigError> {
        let narrow = |value: i64, field: &'static str| {
            c_long::try_from(value).map_err(|_| ConfigError::CountTooLarge { field })
        };
        Ok(EoCoreParams {
            e_min_kwh: self.e_min_kwh,
            e_max_kwh: self.e_max_kwh,
            p_charge_kw: self.p_charge_kw,
            p_discharge_kw: self.p_discharge_kw,
            eta_charge: self.eta_charge,
            eta_discharge: self.eta_discharge,
            start_kwh: self.start_kwh,
            e_target: self.e_target.unwrap_or(0.0),
            slot_minutes: self.slot_minutes,
            soc_step_kwh: self.soc_step_kwh,
            power_steps: narrow(self.power_steps, "power_steps")?,
            max_states: narrow(self.max_states, "max_states")?,
            max_iterations: narrow(self.max_iterations, "max_iterations")?,
            max_import_kw: self.max_import_kw,
            max_export_kw: self.max_export_kw,
            min_spread_eur_per_kwh: self.min_spread_eur_per_kwh,
            sell_floor_eur_per_kwh: self.sell_floor_eur_per_kwh.unwrap_or(0.0),
            wear_eur_per_kwh: self.wear_eur_per_kwh,
            salvage: self.salvage,
            target_penalty: self.target_penalty,
            has_target: i32::from(self.e_target.is_some()),
            allow_grid_charge: i32::from(self.allow_grid_charge),
            allow_export: i32::from(self.allow_export),
            refuse_battery_export: i32::from(self.refuse_battery_export),
            has_sell_floor: i32::from(self.sell_floor_eur_per_kwh.is_some()),
            pv_preference: self.pv_charge_preference.code(),
            align_actions: i32::from(self.align_actions),
        })
    }
}
