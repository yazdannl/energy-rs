//! The per-slot net-metering schedule attached to a typed horizon.

use crate::horizon::{
    BudgetState, HorizonRefusal, NetMeteringRegime, PriceSlot, SlotSettlement, settle_slot,
};

#[derive(Debug, Clone, PartialEq)]
pub struct NetMeteringSchedule {
    /// One settlement per covered slot, in horizon order.
    pub slots: Vec<SlotSettlement>,
}

impl NetMeteringSchedule {
    /// Settlement for an exact slot start.
    pub fn at(&self, t_start: i64) -> Option<&SlotSettlement> {
        self.slots.iter().find(|slot| slot.t_start == t_start)
    }
}

/// Build the schedule for all covered slots.
pub fn build_schedule(
    slots: &[PriceSlot],
    regime: &NetMeteringRegime,
    budget: &BudgetState,
    now: i64,
) -> Result<NetMeteringSchedule, HorizonRefusal> {
    let mut priced = Vec::with_capacity(slots.len());
    for slot in slots {
        priced.push(settle_slot(slot, regime, budget, now)?);
    }
    Ok(NetMeteringSchedule { slots: priced })
}
