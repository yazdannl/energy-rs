//! One local day of measured slots and the coverage it proves.

use chrono::NaiveDate;

use crate::plan::SLOT_SECONDS;

use super::{FlowTotals, SlotFlow};

/// Every quarter-hour of one `Europe/Amsterdam` day, measurements or gaps.
///
/// The slot grid is the UTC quarter-hour grid from local midnight to the next local midnight, so a
/// spring-forward day has 92 slots and an autumn day 100. Coverage counts slots with a usable
/// import, export, PV and load measurement; completeness needs **every** slot.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredDay {
    pub day: NaiveDate,
    pub slots: Vec<SlotFlow>,
}

impl MeasuredDay {
    /// A day over its slot grid.
    pub fn new(day: NaiveDate, slots: Vec<SlotFlow>) -> Self {
        Self { day, slots }
    }

    /// Hours of the day that carried a full measurement.
    pub fn coverage_hours(&self) -> f64 {
        let slots = SLOT_SECONDS as f64 / 3600.0;
        self.slots.iter().filter(|slot| slot.is_measured()).count() as f64 * slots
    }

    /// Whether every slot of the local day is measured; an empty grid is not complete.
    pub fn is_complete(&self) -> bool {
        !self.slots.is_empty() && self.slots.iter().all(SlotFlow::is_measured)
    }

    /// Measured energy sums over the day (partial data sums what is there).
    pub fn totals(&self) -> FlowTotals {
        let mut totals = FlowTotals::default();
        for slot in &self.slots {
            totals.add(slot);
        }
        totals
    }
}
