//! One command request: the pack, the plan slot, the instant and the measurements.

use energy_ledger::plan::PlanSlot;

use crate::safety::Measured;

/// A command as the dispatcher wants to issue it after allocation.
///
/// `requested_w` is already the pack's share of the fleet setpoint, which is what makes the ramp
/// bound per pack instead of an aggregate budget (survey §7 trap 6). `exit_hold` marks the
/// boot/exit neutralisation write so the authority gate can refuse it explicitly in dry-run.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRequest {
    /// Pack the command targets.
    pub pack_id: String,
    /// Allocated AC setpoint, W (positive charging).
    pub requested_w: f64,
    /// The fleet plan slot in force, when the plan is still usable.
    pub plan_slot: Option<PlanSlot>,
    /// Command instant, UTC epoch seconds.
    pub now: i64,
    /// Whether this is the exit/boot hold.
    pub exit_hold: bool,
    /// Measurements for this tick.
    pub measured: Measured,
}

impl CommandRequest {
    /// A request with the pack's allocated watts and its measurements.
    pub fn new(
        pack_id: impl Into<String>,
        requested_w: f64,
        plan_slot: Option<PlanSlot>,
        now: i64,
        measured: Measured,
    ) -> Self {
        Self {
            pack_id: pack_id.into(),
            requested_w,
            plan_slot,
            now,
            exit_hold: false,
            measured,
        }
    }

    /// Mark this request as the exit/boot hold.
    #[must_use]
    pub fn as_exit_hold(mut self) -> Self {
        self.exit_hold = true;
        self
    }
}
