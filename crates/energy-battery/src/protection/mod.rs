//! Protection: the latching low-cell guard and the full-charge maintenance window.
//!
//! Both are safety states, not suggestions, so both are pure state machines with an explicit clock
//! argument: the engine feeds readings in, the machine answers with the state the dispatcher must
//! obey, and nothing here talks to a device.
//!
//! * [`cell_guard`] latches per pack: below the cutoff it blocks discharge **and** forces an
//!   emergency charge; between cutoff and resume it blocks discharge; a stale or missing reading
//!   keeps the previous latch; at or above resume it clears. The latch is persisted through
//!   [`latch_store`] and restored at boot, because a restart is exactly when a pack must not be
//!   discharged by surprise.
//! * [`full_charge`] schedules the weekly (or manual) full charge the BMS needs to balance its
//!   cells, in local `Europe/Amsterdam` time, including the two DST transitions.

pub mod cell_config;
pub mod cell_decision;
pub mod cell_guard;
pub mod cell_latch;
pub mod cell_reading;
pub mod full_charge;
pub mod latch_reason;
pub mod latch_store;

pub use cell_config::CellConfig;
pub use cell_decision::CellDecision;
pub use cell_guard::CellVoltageGuard;
pub use cell_latch::CellLatch;
pub use cell_reading::CellReading;
pub use full_charge::{FullChargeAction, FullChargeScheduler, FullChargeSettings};
pub use latch_reason::LatchReason;
pub use latch_store::CellLatchStore;
