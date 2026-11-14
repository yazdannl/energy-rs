//! Plan generation: assemble the fleet scenario, solve it and keep the refusal typed.
//!
//! One plan is a 15-minute timeline of at most 144 slots (shorter when tomorrow's prices are not
//! published yet; it is never forward-filled). The builder refuses a scenario with missing price
//! coverage or stale forecasts; it never zero-fills a gap (`PLAN.md` §3, §6, survey §7 trap 4).
//! The solver stays [`energy_opt`]; the money stays [`energy_price::Settlement`].

mod aggregate;
mod assemble;
mod checks;
mod clamp;
mod config;
mod differential;
mod export_policy;
mod generated_plan;
mod horizon;
mod pack_spec;
mod pack_state;
mod plan_error;
mod plan_input;
mod plan_summary;
mod policy;
mod summary;
mod timeline;

pub use differential::Differential;
pub use export_policy::ExportPolicy;
pub use generated_plan::Plan;
pub use pack_spec::PackSpec;
pub use pack_state::PackState;
pub use plan_error::PlanError;
pub use plan_input::PlanInput;
pub use plan_summary::PlanSummary;
pub use policy::Policy;

/// Slots in a plan horizon (36 hours at quarter-hour resolution).
///
/// The day-ahead book runs a day and a bit past its publication, so 36 h is the most a plan can
/// see of tomorrow's evening; the load forecast's own default horizon is 36 h for the same reason.
pub const HORIZON_SLOTS: usize = 144;
/// Seconds per plan slot.
pub const SLOT_SECONDS: i64 = 900;
/// Minutes per plan slot.
pub const SLOT_MINUTES: f64 = 15.0;
