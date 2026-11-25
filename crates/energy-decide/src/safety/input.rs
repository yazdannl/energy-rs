//! One pack's hazard snapshot. M5 reads it; M6 still owns write authority.

use crate::plan_assemble::Targets;
use crate::plan_assemble::targets::fleet_targets;

/// M5-owned observation of one pack. No device I/O happens here.
// Independent hazard flags are the point of a snapshot; they are not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyInput {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Snapshot instant, UTC epoch milliseconds.
    pub at_ms: i64,
    /// Minimum measured cell voltage, V.
    pub cell_min_v: f64,
    /// Device cell guard currently blocks discharge.
    pub cell_latch: bool,
    /// Device cell guard latches a forced charge.
    pub cell_force_charge: bool,
    /// Time of the cell observation; `None` means unknown/stale.
    pub cell_observed_at_ms: Option<i64>,
    /// Measured grid power, import positive; `None` is missing telemetry.
    pub grid_w: Option<f64>,
    /// Time of the grid/meter sample; `None` means service-headroom evidence is unavailable.
    pub grid_sample_at_ms: Option<i64>,
    /// FLEET-TOTAL measured battery power, charge positive; `None` is missing telemetry.
    /// Projections subtract this whole aggregate before adding selected fleet targets.
    pub battery_w: Option<f64>,
    /// Settled grid estimate used to release the import latch; a transient dip is not enough.
    pub settled_grid_w: Option<f64>,
    /// Observation time of the settled estimate, epoch milliseconds.
    pub settled_at_ms: Option<i64>,
    /// Complete new targets already selected for the other packs in this slot, W.
    pub coordinated_pack_w: Vec<f64>,
    /// Measured non-battery house load, W; `None` is missing telemetry.
    pub load_w: Option<f64>,
    /// Measured PV generation, W; `None` is missing telemetry.
    pub pv_w: Option<f64>,
    /// Forecast PV surplus later in the horizon, kWh. Reported only.
    pub forecast_future_pv_kwh: f64,
    /// Configured PV-sourced charge reserve, kWh. Reported only.
    pub solar_reserve_kwh: f64,
    /// Horizon length used for the advisory headroom, hours.
    pub horizon_hours: f64,
    /// Number of packs in the coordinated fleet, for the emergency aggregate bound.
    pub fleet_pack_count: usize,
    /// Usable capacity of this pack, kWh, for the emergency charge estimate.
    pub pack_capacity_kwh: f64,
    /// Time of the solar/load sample; `None` is missing telemetry.
    pub solar_sample_at_ms: Option<i64>,
    /// Import guard currently armed by a measured breach.
    pub import_armed: bool,
    /// Effective import ceiling after margin, W.
    pub import_ceiling_w: f64,
    /// Physical service limit; an emergency may breach the margin, never this fuse.
    pub physical_service_limit_w: f64,
    /// Import release deadband, W.
    pub import_deadband_w: f64,
    /// Export/self-consumption hard ceiling magnitude, W.
    pub export_ceiling_w: f64,
    /// Stored energy, kWh.
    pub soc_kwh: f64,
    /// Reported state of charge, percent.
    pub soc_pct: f64,
    /// Device charge power limit, W.
    pub max_charge_w: f64,
    /// Device discharge power limit, W.
    pub max_discharge_w: f64,
    /// Temperature inside the device operating range.
    pub temperature_ok: bool,
    /// Capacity-based current limit satisfied.
    pub current_limit_ok: bool,
    /// Device-reported fault level; 2 at 100 % SoC with `is_error == false` is normal.
    pub fault_level: i32,
    /// Device-reported error flag.
    pub is_error: bool,
    /// End of ordinary fault backoff, epoch milliseconds.
    pub backoff_until_ms: i64,
    /// An earlier write's neutrality is unproven.
    pub uncertain: bool,
    /// Ownership of this pack was lost.
    pub ownership_lost: bool,
    /// A nonzero ramp is standing and a different target arrived.
    pub cancel_ramp: bool,
    /// Device restart time, epoch milliseconds; 120 s grace follows it.
    pub restart_at_ms: Option<i64>,
    /// Command currently standing on this pack, W; subtracted before the new target is added.
    pub standing_command_w: f64,
    /// Remaining cumulative unobserved-time budget, ms; set by the ladder from its leaky bucket.
    pub command_budget_remaining_ms: u32,
    /// Time of the newest pack sample, epoch milliseconds.
    pub last_sample_at_ms: i64,
    /// Effective/floor/ceiling target inputs from M3, never redefined here.
    pub targets: Targets,
    /// Economic setpoint the plan asked for, W.
    pub plan_w: f64,
}

impl SafetyInput {
    /// Snapshot with safe, neutral defaults; tests override only their hazard.
    pub fn new(pack_id: &str, at_ms: i64) -> Self {
        Self {
            pack_id: pack_id.to_owned(),
            at_ms,
            cell_min_v: 3.3,
            cell_latch: false,
            cell_force_charge: false,
            cell_observed_at_ms: Some(at_ms),
            grid_w: Some(0.0),
            grid_sample_at_ms: Some(at_ms),
            battery_w: Some(0.0),
            settled_grid_w: None,
            settled_at_ms: None,
            coordinated_pack_w: Vec::new(),
            load_w: Some(0.0),
            pv_w: Some(0.0),
            forecast_future_pv_kwh: 0.0,
            solar_reserve_kwh: 0.0,
            horizon_hours: 30.0,
            fleet_pack_count: 4,
            pack_capacity_kwh: 1.92,
            solar_sample_at_ms: Some(at_ms),
            import_armed: false,
            import_ceiling_w: crate::IMPORT_CEILING_W,
            physical_service_limit_w: 7_700.0,
            import_deadband_w: 10.0,
            export_ceiling_w: f64::INFINITY,
            soc_kwh: 4.0,
            soc_pct: 50.0,
            max_charge_w: 800.0,
            max_discharge_w: 800.0,
            temperature_ok: true,
            current_limit_ok: true,
            fault_level: 0,
            is_error: false,
            backoff_until_ms: 0,
            uncertain: false,
            ownership_lost: false,
            cancel_ramp: false,
            restart_at_ms: None,
            standing_command_w: 0.0,
            command_budget_remaining_ms: UNOBSERVED_BUDGET_MS as u32,
            last_sample_at_ms: at_ms,
            targets: fleet_targets(),
            plan_w: 0.0,
        }
    }

    /// Stored energy below the hard floor.
    pub fn at_floor(&self) -> bool {
        self.soc_kwh <= self.targets.hard_floor_kwh
    }

    /// Stored energy at or above the usable ceiling, including a 100 % pack.
    pub fn at_ceiling(&self) -> bool {
        self.soc_kwh >= self.targets.ceiling_kwh || self.soc_pct >= 100.0
    }

    /// Cell guard evidence is older than the shipped 180 s stale bound.
    pub fn cell_stale(&self) -> bool {
        self.cell_observed_at_ms
            .is_none_or(|at| self.at_ms - at > 180_000)
    }

    /// Pack samples are older than the M5 economic-reachability TTL.
    pub fn sample_stale(&self) -> bool {
        self.at_ms - self.last_sample_at_ms > STALE_SAMPLE_MS
    }

    /// Cell sample is too old to permission an emergency charge.
    pub fn cell_command_sample_stale(&self) -> bool {
        self.cell_observed_at_ms
            .is_none_or(|at| self.at_ms - at > COMMAND_SAMPLE_MAX_MS)
    }
}

/// Documented TTL after which a sample-stale pack stops reserving a share silently.
/// This is an economic-reachability bound only, not permission to keep a nonzero command.
pub const STALE_SAMPLE_MS: i64 = 300_000;
/// Two 20 s tick/sampler periods: no nonzero command may be granted on older evidence.
pub const COMMAND_SAMPLE_MAX_MS: i64 = 40_000;
/// Decision/sampler cadence from the live tick, used for the observation-budget derivation.
pub const DECISION_CADENCE_MS: i64 = 20_000;
/// Maximum future timestamp skew accepted before evidence is rejected.
pub const FUTURE_SKEW_MS: i64 = 5_000;
/// Largest physically plausible magnitude for meter, battery and service-limit values, W.
pub const MAX_PLAUSIBLE_POWER_W: f64 = 100_000.0;
/// Cumulative observation bucket capacity: one full maximum accepted sample age.
/// Authorization requires at least one decision cadence (`DECISION_CADENCE_MS`) in the bucket.
pub const UNOBSERVED_BUDGET_MS: i64 = COMMAND_SAMPLE_MAX_MS;
/// Credit required before a pack is authorised for a nonzero command: one cadence plus skew.
pub const AUTHORIZATION_MIN_MS: i64 = DECISION_CADENCE_MS + FUTURE_SKEW_MS;

/// Freshness predicate with an explicit future-skew policy.
pub fn fresh_enough(at: Option<i64>, now_ms: i64, max_age_ms: i64) -> bool {
    at.is_some_and(|at| {
        at <= now_ms.saturating_add(FUTURE_SKEW_MS) && now_ms.saturating_sub(at) <= max_age_ms
    })
}
