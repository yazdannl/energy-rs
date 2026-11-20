//! The plan setpoint corrected towards the measured grid, once per tick.
//!
//! The follow only ever holds the site at **zero** exchange. A slot that is deliberately exchanging
//! energy — charging from the grid, or selling — has already decided its battery rate, and that
//! rate is the decision: following the grid there would trade the plan's charge (or sale) for a
//! net-import target, which is not what the plan said. So those slots are left alone, and every
//! other slot is self-consumption: absorb the surplus, cover the house, export or import only what
//! the fleet physically cannot avoid.

use crate::dispatch::{FleetView, MeterReading};
use crate::safety::{FollowRequest, GridFollow, SafetyLimits};
use energy_ledger::plan::PlanSlot;

const NEGLIGIBLE_KW: f64 = 1e-9;
/// 0.01 cent/kWh. Prices are published at 1e-6 resolution, so a netted pair can differ by a
/// micro-euro after rounding; a real spread this small cannot pay for a cycle (wear alone is
/// 0.02 EUR/kWh), so treating it as netting cannot hide a profitable hold.
const PRICE_EPSILON: f64 = 1e-4;

/// The target the fleet is actually asked for: the plan's setpoint, plus the grid-follow move.
///
/// The loop is a pure correction, not a plan change: the plan says whether this hour charges or
/// serves and how much, and the correction closes whatever the meter says the house is doing right
/// now. It is skipped when there is nothing to follow — a stale tick, no slot, or no grid reading —
/// because a follow without an intent is a guess.
pub(crate) fn target(
    plan_power_w: f64,
    slot: Option<&PlanSlot>,
    meter: &MeterReading,
    fleet: &FleetView,
    limits: &SafetyLimits,
    stale: bool,
) -> f64 {
    if stale {
        return plan_power_w;
    }
    let (Some(slot), Some(grid_w)) = (slot, meter.grid_w) else {
        return plan_power_w;
    };
    let Some(desired_grid_w) = desired_grid_w(slot) else {
        return plan_power_w;
    };
    let follow = GridFollow {
        deadband_w: limits.grid_follow_deadband_w,
        min_budget_w: limits.grid_follow_min_budget_w,
    };
    follow.correct(&FollowRequest {
        plan_w: plan_power_w,
        battery_w: fleet.power_w(),
        desired_grid_w,
        measured_grid_w: grid_w,
        capability_w: capability_w(fleet),
    })
}

/// The grid this slot should hold, W; `None` when the slot is deliberately exchanging energy.
///
/// A slot whose battery and grid flow point the same way is buying to charge or selling on purpose.
/// Its number is a *battery rate*, not a grid target, so the follow leaves it alone. Every other
/// slot's grid is an artefact of the load and PV the plan assumed, and the site holds the meter at
/// zero instead — absorb what the sun delivers, cover what the house draws, rather than importing
/// to match a forecast that turned out wrong. A zero-rate hold whose *published* buy and feed-in
/// prices match is the exception: buying and selling have the same price there, so following only
/// loses wear and round-trip energy. The per-slot prices are the signal, not the operator's
/// `net_metering` flag: a provider can publish equal prices with the flag off, and a netted regime
/// can still publish a lower feed-in in a slot (e.g. after the saldering budget runs out). What
/// settles this slot's energy is the price pair, so that is what dispatch must judge.
///
/// The planner's convention is `battery_kw > 0` charges and `battery_kw < 0` discharges, with
/// `planned_grid = load_kw - pv_kw + battery_kw` (positive imports). A discharging slot with
/// negative planned grid is therefore exporting on purpose, not idle.
fn desired_grid_w(slot: &PlanSlot) -> Option<f64> {
    if ![slot.load_kw, slot.pv_kw, slot.battery_kw]
        .iter()
        .all(|value| value.is_finite())
    {
        // A slot that cannot say what it expected is not a target: hold the plan and let the
        // ladder's missing-telemetry rule do its job.
        return None;
    }
    // `buy_all_in`/`feed_in` are the settlement prices with taxes and fees included, so their
    // equality is the economic test here, not the operator's regime flag.
    // A zero-setpoint slot whose prices net 1:1 is a hold, not a self-consumption slot: buying the
    // house's energy now and selling the same kWh from the pack later have the same price, so
    // following the meter to zero only pays wear and round-trip loss. The plan left that energy in
    // the pack on purpose (reserve_hold); keep its setpoint instead of draining it. A zero-price
    // slot is not netting: both prices are then just missing, so it must keep the old behaviour.
    let netted = slot.buy_all_in > 0.0 && (slot.buy_all_in - slot.feed_in).abs() <= PRICE_EPSILON;
    if netted && slot.battery_kw.abs() <= NEGLIGIBLE_KW {
        return None;
    }
    let planned_grid = slot.load_kw - slot.pv_kw + slot.battery_kw;
    let exchanging = slot.battery_kw * planned_grid > 0.0;
    (!exchanging).then_some(0.0)
}

/// The fleet's AC capability, W: what the packs together could take or deliver.
fn capability_w(fleet: &FleetView) -> f64 {
    fleet
        .packs
        .iter()
        .map(|pack| pack.spec.max_charge_w.max(pack.spec.max_discharge_w))
        .filter(|watts| watts.is_finite() && *watts > 0.0)
        .sum()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{desired_grid_w, target};
    use crate::dispatch::{FleetView, MeterReading, PackReading};
    use crate::plan::PackSpec;
    use crate::safety::SafetyLimits;
    use energy_ledger::plan::PlanSlot;

    fn slot(load_kw: f64, pv_kw: f64, battery_kw: f64) -> PlanSlot {
        priced_slot(load_kw, pv_kw, battery_kw, 0.0, 0.0)
    }

    fn priced_slot(load_kw: f64, pv_kw: f64, battery_kw: f64, buy: f64, feed: f64) -> PlanSlot {
        let mut slot = PlanSlot::at(0, 0);
        slot.load_kw = load_kw;
        slot.pv_kw = pv_kw;
        slot.battery_kw = battery_kw;
        slot.buy_all_in = buy;
        slot.feed_in = feed;
        slot
    }

    fn follow(slot: &PlanSlot, plan_w: f64, grid_w: f64, fleet: &FleetView) -> f64 {
        target(
            plan_w,
            Some(slot),
            &MeterReading::grid(grid_w),
            fleet,
            &SafetyLimits::default(),
            false,
        )
    }

    #[test]
    fn peak_discharge_keeps_its_export_setpoint() {
        let slot = slot(0.4646, 0.0, -3.2);
        assert_eq!(desired_grid_w(&slot), None, "a deliberate export");
        assert_eq!(
            follow(&slot, -3_200.0, 0.0, &FleetView::default()),
            -3_200.0
        );
    }

    #[test]
    fn self_consumption_follows_the_grid_to_zero() {
        let slot = slot(0.5, 0.0, -0.2);
        assert_eq!(desired_grid_w(&slot), Some(0.0), "grid is an artefact");
        assert_eq!(follow(&slot, -200.0, 500.0, &FleetView::default()), -500.0);
    }

    #[test]
    fn net_metered_reserve_hold_keeps_the_plan_setpoint() {
        let slot = priced_slot(0.8, 0.0, 0.0, 0.415_838, 0.415_838);
        assert_eq!(desired_grid_w(&slot), None, "self-consumption pays nothing");
        assert_eq!(follow(&slot, 0.0, 800.0, &FleetView::default()), 0.0);
    }

    #[test]
    fn a_zero_price_hold_is_not_treated_as_netting() {
        let slot = priced_slot(0.5, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(desired_grid_w(&slot), Some(0.0), "prices are missing");
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), -500.0);
    }

    #[test]
    fn net_metered_hold_stops_a_discharging_pack_before_the_cell_guard() {
        let slot = priced_slot(0.8, 0.0, 0.0, 0.415_838, 0.415_838);
        // An almost-empty pack still discharging at 3.15 V must not be pushed into the guard.
        let fleet = FleetView::new(vec![PackReading {
            ledger_id: None,
            spec: PackSpec {
                pack_id: "SN-LOW".to_owned(),
                capacity_kwh: 1.92,
                min_soc_pct: 0.0,
                max_soc_pct: 100.0,
                max_charge_w: 1000.0,
                max_discharge_w: 800.0,
                eta_charge: 0.9,
                eta_discharge: 0.9,
            },
            soc_pct: Some(11.0),
            energy_kwh: Some(0.2112),
            power_w: -799.0,
            min_cell_v: Some(3.15),
            observed_at: 0,
            available: true,
            stale: false,
        }]);
        assert_eq!(follow(&slot, 0.0, 800.0, &fleet), 0.0);
    }

    #[test]
    fn a_small_nonzero_setpoint_is_not_a_hold_under_netting() {
        let slot = priced_slot(0.8, 0.0, -0.05, 0.32, 0.32);
        assert_eq!(desired_grid_w(&slot), Some(0.0), "0.05 kW is real");
        assert_eq!(follow(&slot, -50.0, 500.0, &FleetView::default()), -500.0);
    }

    #[test]
    fn net_metered_hold_does_not_store_pv_surplus() {
        let slot = priced_slot(0.2, 0.7, 0.0, 0.32, 0.32);
        assert_eq!(desired_grid_w(&slot), None, "export pays the same");
        assert_eq!(follow(&slot, 0.0, -500.0, &FleetView::default()), 0.0);
    }

    #[test]
    fn metered_hold_still_follows_when_buy_and_feed_differ() {
        let slot = priced_slot(0.5, 0.0, 0.0, 0.45, 0.08);
        assert_eq!(desired_grid_w(&slot), Some(0.0), "import is dearer");
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), -500.0);
    }

    #[test]
    fn deliberate_charge_still_ignores_grid_follow_under_netting() {
        let slot = priced_slot(0.8, 0.0, 0.5, 0.32, 0.32);
        assert_eq!(desired_grid_w(&slot), None, "a deliberate grid charge");
        assert_eq!(follow(&slot, 500.0, 1300.0, &FleetView::default()), 500.0);
    }

    #[test]
    fn a_positive_spread_inside_tolerance_is_netting() {
        let slot = priced_slot(0.5, 0.0, 0.0, 0.315_505, 0.315_555);
        assert_eq!(desired_grid_w(&slot), None, "50 micro-euro is rounding");
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), 0.0);
    }

    #[test]
    fn a_positive_spread_outside_tolerance_still_follows() {
        let slot = priced_slot(0.5, 0.0, 0.0, 0.315_505, 0.315_705);
        assert_eq!(
            desired_grid_w(&slot),
            Some(0.0),
            "0.0002 EUR is a real spread"
        );
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), -500.0);
    }

    #[test]
    fn a_negative_spread_inside_tolerance_is_netting() {
        let slot = priced_slot(0.5, 0.0, 0.0, 0.315_505, 0.315_455);
        assert_eq!(desired_grid_w(&slot), None, "50 micro-euro either way");
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), 0.0);
    }

    #[test]
    fn a_negative_spread_outside_tolerance_still_follows() {
        let slot = priced_slot(0.5, 0.0, 0.0, 0.315_505, 0.315_305);
        assert_eq!(desired_grid_w(&slot), Some(0.0), "dearer import is real");
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), -500.0);
    }

    #[test]
    fn an_equal_negative_price_pair_is_not_netting() {
        let slot = priced_slot(0.5, 0.0, 0.0, -0.05, -0.05);
        assert_eq!(
            desired_grid_w(&slot),
            Some(0.0),
            "negative prices are a signal"
        );
        assert_eq!(follow(&slot, 0.0, 500.0, &FleetView::default()), -500.0);
    }
}
