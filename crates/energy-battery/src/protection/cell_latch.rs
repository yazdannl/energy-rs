//! The persisted per-pack low-cell latch.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::protection::latch_reason::LatchReason;

/// One pack's latched cell-voltage protection state.
///
/// This is the exact object that survives a restart: `blocked` and `force_charge` are the decisions
/// the dispatcher obeys, `since` is when they changed, and `reason`/`reason_at` are the last thing
/// that moved them (or kept them). `voltage_v` and `last_fresh_at` make a restored latch explainable
/// after the process that created it is gone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellLatch {
    /// Serial (or stable ledger id) of the pack.
    pub pack_id: String,
    /// Discharge must be blocked while this is true.
    pub blocked: bool,
    /// An emergency charge is requested while this is true.
    pub force_charge: bool,
    /// The power the forced charge should use, W.
    pub force_charge_w: Option<u32>,
    /// When `blocked`/`force_charge` last changed.
    pub since: DateTime<Utc>,
    /// The last reason the latch was touched.
    pub reason: LatchReason,
    /// When `reason` was recorded.
    pub reason_at: DateTime<Utc>,
    /// The last voltage seen (fresh or not), V.
    pub voltage_v: Option<f64>,
    /// The last fresh reading, V.
    pub last_fresh_at: Option<DateTime<Utc>>,
}

impl CellLatch {
    /// The state before any reading: nothing blocked, nothing forced.
    pub fn idle(pack_id: &str, at: DateTime<Utc>) -> Self {
        Self {
            pack_id: pack_id.to_owned(),
            blocked: false,
            force_charge: false,
            force_charge_w: None,
            since: at,
            reason: LatchReason::MissingReading,
            reason_at: at,
            voltage_v: None,
            last_fresh_at: None,
        }
    }

    /// Whether the dispatcher must refuse discharge for this pack.
    pub const fn blocks_discharge(&self) -> bool {
        self.blocked
    }

    /// Whether the dispatcher must charge this pack even without a plan.
    pub const fn forces_charge(&self) -> bool {
        self.force_charge
    }
}
