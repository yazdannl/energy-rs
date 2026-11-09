//! Greedy, deterministic allocation: the fewest packs that can take the request, best SoC first.

use crate::allocation::{ALLOCATION_DEADBAND_W, PackAllocation, SplitInput};
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
    /// With `staggering` the fleet runs as few packs as the request needs: packs are taken in SoC
    /// order (the emptiest charges first, the fullest discharges first) and each one is filled to
    /// its own headroom before the next starts. A pack is only started when it can take at least
    /// `min_active_w`, so a sliver is left to the grid instead of waking a pack for 20 W.
    ///
    /// Without `staggering` the target is spread proportionally over every pack's headroom, with
    /// one redistribution pass for per-pack ceilings — the pre-0.4 behaviour, kept as a fallback.
    pub fn split(target_w: f64, packs: &[PackState], input: &SplitInput<'_>) -> Self {
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
        let rooms = headrooms(&ordered, charging, input);
        let mut allocated = vec![0.0; ordered.len()];
        if input.staggering {
            fill(&ordered, &rooms, &mut allocated, demand, charging, input);
        } else {
            spread(&rooms, &mut allocated, demand);
            redistribute(&ordered, &rooms, &mut allocated, demand, charging);
        }
        let deadband = input.min_active_w.max(ALLOCATION_DEADBAND_W);
        Self::assemble(target_w, &ordered, allocated, deadband)
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

    fn assemble(target_w: f64, packs: &[&PackState], allocated: Vec<f64>, deadband: f64) -> Self {
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
            unmet_w: if unmet <= deadband {
                0.0
            } else {
                target_w - allocated_w
            },
            packs: entries,
        }
    }
}

/// Take packs in SoC order, filling each before the next: the emptiest charges, the fullest serves.
fn fill(
    packs: &[&PackState],
    rooms: &[f64],
    allocated: &mut [f64],
    demand: f64,
    charging: bool,
    input: &SplitInput<'_>,
) {
    let floor = input.min_active_w.max(0.0);
    let mut remaining = demand;
    for index in rank(packs, rooms, charging, input) {
        if remaining < floor || remaining <= 0.0 {
            // Whatever is left is inside the allowed deviation: the grid covers it, and no pack is
            // woken for a sliver it could not run efficiently anyway.
            break;
        }
        let room = rooms[index];
        if room < floor {
            // A pack that can only take a sliver is left idle; the next one gets the whole share.
            continue;
        }
        let watts = remaining.min(room);
        allocated[index] = watts;
        remaining -= watts;
    }
}

/// The pack order for this direction: emptiest first while charging, fullest first while serving.
///
/// A live hold beats SoC order — that is the whole point of the latch, so a pack cannot rotate just
/// because its SoC crossed a neighbour's. SoC differences inside `soc_band_pct` are a tie, so the
/// order cannot flip on measurement noise either.
fn rank(
    packs: &[&PackState],
    rooms: &[f64],
    charging: bool,
    input: &SplitInput<'_>,
) -> Vec<usize> {
    let band = input.soc_band_pct.max(0.0);
    let key = |state: &PackState| {
        if band <= 0.0 {
            state.soc_pct
        } else {
            (state.soc_pct / band).floor()
        }
    };
    let holder = held(packs, rooms, charging, input);
    let mut order: Vec<usize> = (0..packs.len()).collect();
    order.sort_by(|a, b| {
        let held_first = hold_order(holder == Some(*a), holder == Some(*b));
        let lhs = key(packs[*a]);
        let rhs = key(packs[*b]);
        let by_soc = if charging {
            lhs.total_cmp(&rhs)
        } else {
            rhs.total_cmp(&lhs)
        };
        held_first
            .then(by_soc)
            .then_with(|| packs[*a].spec.pack_id.cmp(&packs[*b].spec.pack_id))
    });
    order
}

/// The held pack goes first; everything else keeps its SoC order.
fn hold_order(left: bool, right: bool) -> std::cmp::Ordering {
    match (left, right) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

/// The index of the held pack, if the hold is still valid for this tick.
fn held(
    packs: &[&PackState],
    rooms: &[f64],
    charging: bool,
    input: &SplitInput<'_>,
) -> Option<usize> {
    let hold = input.hold?;
    let wanted = if charging { "charge" } else { "discharge" };
    if hold.direction != wanted {
        return None;
    }
    // Zero disables the latch: the operator asked for a fresh SoC order every tick.
    if input.hold_seconds <= 0 || input.now.saturating_sub(hold.since) > input.hold_seconds {
        return None;
    }
    (0..packs.len()).find(|index| {
        packs[*index].spec.pack_id == hold.holder
            && rooms[*index] >= input.min_active_w.max(0.0)
    })
}

/// The pre-0.4 split: every pack in proportion to its headroom.
fn spread(rooms: &[f64], allocated: &mut [f64], demand: f64) {
    let total: f64 = rooms.iter().sum();
    if total <= 0.0 {
        return;
    }
    for (index, room) in rooms.iter().enumerate() {
        allocated[index] = (demand * room / total).min(*room);
    }
}

fn headrooms(packs: &[&PackState], charging: bool, input: &SplitInput<'_>) -> Vec<f64> {
    packs
        .iter()
        .map(|state| {
            if !charging && input.block_discharge.contains(&state.spec.pack_id.as_str()) {
                return 0.0;
            }
            state
                .headroom_w(charging, input.slot_hours, input.reserve_soc_pct)
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
