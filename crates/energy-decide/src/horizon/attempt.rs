//! The full M1 attempt: coverage and refusal evidence exist even when no horizon is emitted.

use crate::horizon::{Coverage, GridSourceCoverage, Horizon, HorizonOutcome, HorizonRefusal};

#[derive(Debug, Clone, PartialEq)]
pub struct HorizonAttempt {
    /// Slots asked for; never altered by a fallback.
    pub requested_slots: usize,
    /// Requested and actual coverage.
    pub coverage: Coverage,
    /// Per-source coverage rows, also present on refusal.
    pub grid_source_coverage: Vec<GridSourceCoverage>,
    /// One-time notice keys emitted while summing sources.
    pub notices: Vec<String>,
    /// Provisional tail state before the mandatory refusal checks.
    pub provisional_outcome: HorizonOutcome,
    /// Successful horizon or the typed refusal.
    pub result: Result<Horizon, HorizonRefusal>,
}

impl HorizonAttempt {
    /// Build an attempt whose evidence is filled in as checks run.
    pub fn new(requested_slots: usize, minimum_slots: usize) -> Self {
        Self {
            requested_slots,
            coverage: Coverage {
                requested_slots,
                actual_slots: 0,
                minimum_slots,
                coverage_end: None,
                missing_slots: 0,
                first_missing_t_start: None,
            },
            grid_source_coverage: Vec::new(),
            notices: Vec::new(),
            provisional_outcome: HorizonOutcome::Healthy,
            result: Err(HorizonRefusal::HorizonSlots {
                slots: requested_slots,
            }),
        }
    }

    /// The emitted horizon, if any.
    pub fn horizon(&self) -> Option<&Horizon> {
        self.result.as_ref().ok()
    }

    /// The typed refusal, if any.
    pub fn refusal(&self) -> Option<&HorizonRefusal> {
        self.result.as_ref().err()
    }
}
