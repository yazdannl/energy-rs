//! Full-charge maintenance: the price-aware interval rule and its legacy weekly scheduler.
//!
//! The scheduler is kept for the documented compatibility path; the engine plans maintenance
//! through [`interval`], which chooses cheap quarter-hours from the live price book.

pub mod action;
pub mod assignment;
pub mod candidate;
pub mod charge_slot;
pub mod due_pack;
pub mod interval;
pub mod scheduler;
pub mod settings;

pub use action::FullChargeAction;
pub use assignment::FullChargeAssignment;
pub use candidate::ChargeCandidate;
pub use charge_slot::ChargeSlot;
pub use due_pack::DuePack;
pub use scheduler::{FullChargeScheduler, MAX_RUN_HOURS, TOLERANCE_PCT, ZONE};
pub use settings::{
    DEFAULT_DWELL_MINUTES, DEFAULT_INTERVAL_DAYS, FullChargeSettings, MAX_DWELL_MINUTES,
    MAX_INTERVAL_DAYS, MIN_DWELL_MINUTES, MIN_INTERVAL_DAYS,
};
