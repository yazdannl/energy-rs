//! A typed horizon: only valid, contiguous 900 s slots and their settlement schedule.

use crate::horizon::{
    Coverage, GridSourceCoverage, HorizonOutcome, NetMeteringSchedule, PriceSlot,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Horizon {
    /// Start of the slot containing the plan instant, UTC epoch seconds.
    pub t0: i64,
    /// Slots originally requested.
    pub requested_slots: usize,
    /// Requested and actual coverage.
    pub coverage: Coverage,
    /// Valid covered price slots, without holes or resampling.
    pub slots: Vec<PriceSlot>,
    /// Per-slot net-metering and export price.
    pub net_metering: NetMeteringSchedule,
    /// Summed required grid series, one value per covered slot.
    pub grid_series: Vec<f64>,
    /// Per-source coverage evidence.
    pub grid_source_coverage: Vec<GridSourceCoverage>,
    /// Final successful outcome.
    pub outcome: HorizonOutcome,
}

impl Horizon {
    /// Number of covered slots.
    pub fn actual_slots(&self) -> usize {
        self.slots.len()
    }

    /// Exclusive end of coverage, UTC epoch seconds.
    pub fn coverage_end(&self) -> i64 {
        self.t0 + i64::try_from(self.actual_slots()).unwrap_or(i64::MAX) * crate::SLOT_SECONDS
    }

    /// The slot containing an instant, when covered.
    pub fn slot_at(&self, t: i64) -> Option<&PriceSlot> {
        self.slots
            .iter()
            .find(|slot| slot.t_start <= t && t < slot.t_start.saturating_add(crate::SLOT_SECONDS))
    }
}
