//! The result of one command fanned out across the fleet.

use crate::fleet::outcome::DeviceOutcome;

/// Every device's outcome, plus how many were skipped because no command was planned for them.
///
/// A partial failure is the normal case (one device off the network, one wedged), so this is a
/// report, not an error: the caller sees exactly which serials accepted, which refused and why.
#[derive(Debug)]
pub struct FanOutReport {
    pub outcomes: Vec<DeviceOutcome>,
    pub skipped: usize,
}

impl FanOutReport {
    pub fn new(outcomes: Vec<DeviceOutcome>, skipped: usize) -> Self {
        Self { outcomes, skipped }
    }

    pub fn accepted(&self) -> impl Iterator<Item = &DeviceOutcome> {
        self.outcomes.iter().filter(|outcome| outcome.is_ok())
    }

    pub fn failed(&self) -> impl Iterator<Item = &DeviceOutcome> {
        self.outcomes.iter().filter(|outcome| !outcome.is_ok())
    }

    pub fn accepted_count(&self) -> usize {
        self.accepted().count()
    }

    pub fn failed_count(&self) -> usize {
        self.failed().count()
    }

    pub fn is_complete(&self) -> bool {
        self.skipped == 0 && self.failed_count() == 0
    }
}
