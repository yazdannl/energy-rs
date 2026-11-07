//! How much history the ledger keeps.
//!
//! Defaults are the PLAN.md retention: 30 days of 1-minute rows, 1 year of 5-minute rows, the
//! newest 200 plans and 20 000 dispatch rows. Sessions, events and daily savings have no age limit.

/// Retention limits for the trim job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub one_minute_days: i64,
    pub five_minute_days: i64,
    pub plans_kept: i64,
    pub dispatch_kept: i64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            one_minute_days: 30,
            five_minute_days: 365,
            plans_kept: 200,
            dispatch_kept: 20_000,
        }
    }
}

impl RetentionPolicy {
    pub fn new(
        one_minute_days: i64,
        five_minute_days: i64,
        plans_kept: i64,
        dispatch_kept: i64,
    ) -> Self {
        Self {
            one_minute_days,
            five_minute_days,
            plans_kept,
            dispatch_kept,
        }
    }
}
