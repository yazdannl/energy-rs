//! Greedy, deterministic allocation: proportional to remaining headroom, one redistribution pass.

use crate::allocation::{ALLOCATION_DEADBAND_W, PackAllocation};
use crate::plan::PackState;

/// The fleet setpoint split into per-pack commands.
#[derive(Debug, Clone, PartialEq)]
pub struct Allocation {
    /// The fleet demand that was split, W.
    pub requested_w: f64,
    /// Sum of the per-pack commands, W.
    pub allocated_w: f64,
    /// The part that did not fit any pack's limits, W (same sign as the demand).
    pub unmet_w: f64,
    /// One entry per pack, in stable id order.
    pub packs: Vec<PackAllocation>,
}

impl Allocation {
    /// Split `target_w` across `packs`, never exceeding a pack's own ceiling or floor.
    ///
    /// The split is proportional to each pack's remaining headroom (which is SoC-balanced: a fuller
    /// pack can discharge more and a emptier one can charge more), then one redistribution pass
    /// fills what the proportional split left because of a per-pack ceiling.
    pub fn split(
        target_w: f64,
        packs: &[PackState],
        reserve_soc_pct: f64,
        slot_hours: f64,
        block_discharge: &[&str],
    ) -> Self {
        let mut ordered: Vec<&PackState> = packs.iter().collect();
        ordered.sort_by(|a, b| a.spec.pack_id.cmp(&b.spec.pack_id));
        if ordered.is_empty() {
            return Self::empty(target_w);
        }
        if !target_w.is_finite() || target_w.abs() < ALLOCATION_DEADBAND_W {
            return Self::fixed(&ordered, target_w);
        }
        let charging = target_w > 0.0;
        let demand = target_w.abs();
        let rooms = headrooms(
            &ordered,
            charging,
            slot_hours,
            reserve_soc_pct,
            block_discharge,
        );
        let total: f64 = rooms.iter().sum();
        let mut allocated = vec![0.0; ordered.len()];
        if total > 0.0 {
            for (index, room) in rooms.iter().enumerate() {
                allocated[index] = (demand * room / total).min(*room);
            }
        }
        redistribute(&ordered, &rooms, &mut allocated, demand, charging);
        Self::assemble(target_w, &ordered, allocated)
    }

    /// The allocation for one pack, if it took part.
    pub fn watts_for(&self, pack_id: &str) -> Option<f64> {
        self.packs
            .iter()
            .find(|entry| entry.pack_id == pack_id)
            .map(|entry| entry.watts)
    }

    fn empty(target_w: f64) -> Self {
        Self {
            requested_w: target_w,
            allocated_w: 0.0,
            unmet_w: target_w,
            packs: Vec::new(),
        }
    }

    fn fixed(packs: &[&PackState], target_w: f64) -> Self {
        let packs = packs
            .iter()
            .map(|state| PackAllocation {
                pack_id: state.spec.pack_id.clone(),
                watts: 0.0,
            })
            .collect();
        Self {
            requested_w: target_w,
            allocated_w: 0.0,
            unmet_w: 0.0,
            packs,
        }
    }

    fn assemble(target_w: f64, packs: &[&PackState], allocated: Vec<f64>) -> Self {
        let sign = if target_w >= 0.0 { 1.0 } else { -1.0 };
        let entries: Vec<PackAllocation> = packs
            .iter()
            .zip(allocated)
            .map(|(state, watts)| PackAllocation {
                pack_id: state.spec.pack_id.clone(),
                watts: sign * watts,
            })
            .collect();
        let allocated_w: f64 = entries.iter().map(|entry| entry.watts).sum();
        let unmet = (target_w - allocated_w).abs();
        Self {
            requested_w: target_w,
            allocated_w,
            unmet_w: if unmet <= ALLOCATION_DEADBAND_W {
                0.0
            } else {
                target_w - allocated_w
            },
            packs: entries,
        }
    }
}

fn headrooms(
    packs: &[&PackState],
    charging: bool,
    slot_hours: f64,
    reserve_soc_pct: f64,
    block_discharge: &[&str],
) -> Vec<f64> {
    packs
        .iter()
        .map(|state| {
            if !charging && block_discharge.contains(&state.spec.pack_id.as_str()) {
                return 0.0;
            }
            state
                .headroom_w(charging, slot_hours, reserve_soc_pct)
                .max(0.0)
        })
        .collect()
}

fn redistribute(
    packs: &[&PackState],
    rooms: &[f64],
    allocated: &mut [f64],
    demand: f64,
    charging: bool,
) {
    let mut shortfall = demand - allocated.iter().sum::<f64>();
    if shortfall <= 1e-9 {
        return;
    }
    let mut order: Vec<usize> = (0..packs.len()).collect();
    order.sort_by(|a, b| {
        efficiency(packs[*b], charging)
            .total_cmp(&efficiency(packs[*a], charging))
            .then_with(|| packs[*a].spec.pack_id.cmp(&packs[*b].spec.pack_id))
    });
    for index in order {
        let spare = rooms[index] - allocated[index];
        if spare <= 1e-9 {
            continue;
        }
        let take = spare.min(shortfall);
        allocated[index] += take;
        shortfall -= take;
        if shortfall <= 1e-9 {
            break;
        }
    }
}

fn efficiency(pack: &PackState, charging: bool) -> f64 {
    if charging {
        pack.spec.eta_charge
    } else {
        pack.spec.eta_discharge
    }
}
