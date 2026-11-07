//! Select the exact 120 quarter-hour slots a plan covers, refusing any hole.

use energy_price::Slot;

use crate::plan::{PlanError, PlanInput, SLOT_SECONDS};

/// The horizon's price slots, starting with the one containing `now`.
pub(crate) fn slots(input: &PlanInput) -> Result<Vec<Slot>, PlanError> {
    let first = input
        .prices
        .slot_at(input.now)
        .ok_or(PlanError::PriceGap { t_start: input.now })?;
    let resolution = first.source.slot_seconds();
    if resolution != SLOT_SECONDS {
        return Err(PlanError::PriceResolution {
            expected_s: SLOT_SECONDS,
            got_s: resolution,
        });
    }
    let start = first.t_start;
    let mut slots = Vec::with_capacity(input.horizon_slots);
    for index in 0..input.horizon_slots {
        // At most 120 slots, far inside i64.
        #[allow(clippy::cast_possible_wrap)]
        let offset = index as i64;
        let t_start = start + offset * SLOT_SECONDS;
        let slot = input
            .prices
            .slot_at(t_start)
            .ok_or(PlanError::PriceGap { t_start })?;
        if slot.t_start != t_start {
            return Err(PlanError::PriceGap { t_start });
        }
        slots.push(*slot);
    }
    Ok(slots)
}
