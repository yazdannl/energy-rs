//! Per-table deleted counts from one retention run.

/// What one trim deleted, one field per table. Re-running the job immediately returns all zeros,
/// which is what makes it safe to schedule.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub measurements_1m: usize,
    pub measurements_5m: usize,
    pub plans: usize,
    pub plan_slots: usize,
    pub dispatch_log: usize,
}

impl RetentionReport {
    pub fn total(self) -> usize {
        self.measurements_1m
            + self.measurements_5m
            + self.plans
            + self.plan_slots
            + self.dispatch_log
    }
}
