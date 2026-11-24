//! M5 safety ladder: decisions only. M4 obeys them; M6 owns write authority.
//!
//! The override order is `RULE_ORDER` in `rule.rs`; solar headroom is deliberately
//! outside it because it never commands or constrains a charge.

pub mod decision;
pub mod escalation;
pub mod input;
pub mod ladder;
pub mod outcome;
pub mod receipt;
pub mod report;
pub mod rule;
pub mod solar;
pub mod verification;

pub(crate) mod rules;

pub use decision::SafetyDecision;
pub use escalation::{Escalation, SAFETY_RETRY_OFFSETS_MS};
pub use input::{STALE_SAMPLE_MS, SafetyInput};
pub use ladder::{
    CELL_RESUME_V, CELL_TRIGGER_V, EMERGENCY_CHARGE_W, RESTART_GRACE_MS, SafetyLadder,
};
pub use outcome::DecisionOutcome;
pub use receipt::SafetyReceipt;
pub use report::{SafetyReport, from_row};
pub use rule::{RULE_ORDER, Rule};
pub use solar::{SolarHeadroom, TELEMETRY_FRESH_MS};
pub use verification::{HoldProof, NEUTRAL_REPORT_FRESH_MS};
