//! Self-consumption export guard with finite evidence.

use crate::safety::rules::{Candidate, hold};
use crate::safety::{Rule, SafetyInput};

pub(crate) fn export_guard(input: &SafetyInput) -> Option<Candidate> {
    let grid = input.grid_w?;
    let battery = input.battery_w?;
    if !grid.is_finite() || !battery.is_finite() || !input.plan_w.is_finite() {
        return Some(hold(Rule::ExportGuard));
    }
    let projected = grid - battery + input.plan_w;
    if input.plan_w < 0.0 && projected < -input.export_ceiling_w {
        return Some(hold(Rule::ExportGuard));
    }
    None
}
