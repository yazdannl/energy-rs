//! Pure decision-core modules M1 (horizon), M2 (optimizer), M3 (plan assembly), M4 (dispatch),
//! M5 (safety) and M6 (write authority).
//!
//! This crate never talks to hardware or writes a setpoint. It turns price/source input into a
//! typed horizon, minimizes the stated economic objective over hard physical bounds, and turns a
//! finished timeline into a typed plan whose state says what dispatch may do. Every refusal
//! stays a typed value.

pub mod authority;
pub mod dispatch;
pub mod horizon;
pub mod optimizer;
pub mod plan_assemble;
pub mod safety;

/// Slots in a full requested horizon: 30 hours at quarter-hour resolution.
pub const REQUESTED_SLOTS: usize = 120;
///Seconds in one tariff slot.
pub const SLOT_SECONDS: i64 = 900;
///Shortest accepted contiguous price coverage: 24 hours.
pub const MIN_HORIZON_SLOTS: usize = 96;
///Planned site import ceiling after the configured margin, W.
pub const IMPORT_CEILING_W: f64 = 7500.0;
///Aggregate power of four 800 W packs, W.
pub const FLEET_POWER_W: f64 = 3200.0;
///Learned pooled capacity used as the planning basis, kWh (nameplate 7.68 is a prior only).
pub const LEARNED_CAPACITY_KWH: f64 = 7.8575;
///Nameplate capacity, only a prior/floor guard, kWh.
pub const LABEL_CAPACITY_KWH: f64 = 7.68;
///Hard reserve floor at 15 percent of learned capacity, kWh.
pub const HARD_FLOOR_KWH: f64 = 1.178_625;
///Soft peak guideline at 10 percent of learned capacity, kWh.
pub const SOFT_GUIDELINE_KWH: f64 = 0.78575;
///Terminal acceptance tolerance, kWh.
pub const TERMINAL_TOLERANCE_KWH: f64 = 0.05;
