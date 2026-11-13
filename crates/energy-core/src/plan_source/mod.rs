//! The real plan source: measured history, prices and forecasts into one solve.
//!
//! The engine's [`PlanSource`](crate::engine::PlanSource) seam is filled by
//! [`LivePlanSource`], which takes one [`Supply`] per plan and assembles a
//! [`PlanInput`](crate::plan::PlanInput). The supply is the read side (the real adapter is
//! [`HaPlanSupply`], over `energy-ha` statistics and `energy-price` publications); the assembly
//! (horizon, refusals, fallbacks) is pure and tested without a Home Assistant.
//!
//! The edge cases are the point of this module:
//!
//! * tomorrow's prices are not published before ~13:00 CET, so the horizon is **shortened** to the
//!   contiguous slots the book really covers -- never forward-filled;
//! * a price publication older than the configured bound is a typed refusal
//!   ([`PlanError::StalePrices`](crate::plan::PlanError::StalePrices));
//! * a hole in the measured statistics is a typed refusal, never a zero: a zero-fill turns a
//!   missing meter into a sunny, empty house and the optimizer happily charges from the grid;
//! * a missing forecast falls back to an EWMA of the same slot of the previous days and the plan is
//!   marked degraded in its summary, so the UI never presents a guess as a model output.

mod config;
mod device_meter;
mod forecast;
mod ha_series;
mod ha_supply;
mod history;
mod live;
pub mod load_profiles;
mod load_serve;
mod policy_handle;
mod policy_source;
mod profile_build;
mod quality;
mod supply;
mod supply_port;

pub use config::SourceConfig;
pub use forecast::Forecast;
pub use ha_supply::HaPlanSupply;
pub use history::MeasuredHistory;
pub use live::LivePlanSource;
pub use load_profiles::LoadProfiles;
pub use policy_handle::PolicyHandle;
pub use policy_source::PolicySource;
pub use profile_build::learn as learn_load_profile;
pub use quality::ForecastQuality;
pub use supply::Supply;
pub use supply_port::PlanSupply;

/// Shortest horizon, in slots, a price book may support; below this the plan is refused.
pub const MIN_HORIZON_SLOTS: usize = 4;
/// Measured history the EWMA fallback reads, hours.
pub const HISTORY_HOURS: i64 = 48;
