//! The operator's reserve is the hard SoC floor of both the plan and the clamp ladder.
//!
//! The two soft reserves in `Policy` (peak guideline, solar headroom) are deliberately different
//! and must not move this floor.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use std::sync::Mutex;

use energy_core::dispatch::BoxFuture;
use energy_core::engine::PlanSource;
use energy_core::plan::{ExportPolicy, HORIZON_SLOTS, PackState, Plan, PlanError, Policy};
use energy_core::plan_source::{
    Forecast, LivePlanSource, MeasuredHistory, PlanSupply, SourceConfig, Supply,
};
use energy_core::safety::{Authority, CommandRequest, Measured, Reason, SafetyLimits, check};
use energy_price::PriceBook;
use energy_price::settlement::NetMetering;
use support::{NOW, book, settlement, state};

/// Two 1.92 kWh packs with a 5 % hardware floor, as reported by Zendure (`minSoc` 50).
const CAPACITY_KWH: f64 = 3.84;
const PACK_CAPACITY_KWH: f64 = 1.92;
const PACK_MIN_SOC_PCT: f64 = 5.0;
/// The live four-pack fleet: 4 × 1.92 kWh label capacity.
const FLEET_CAPACITY_KWH: f64 = 4.0 * PACK_CAPACITY_KWH;

struct FakeSupply {
    supply: Mutex<Supply>,
}

impl PlanSupply for FakeSupply {
    fn supply(&self, _now: i64) -> BoxFuture<'_, Result<Supply, PlanError>> {
        Box::pin(async move { Ok(self.supply.lock().expect("supply").clone()) })
    }
}

fn supply(prices: PriceBook) -> Supply {
    let starts: Vec<i64> = (0..96).map(|index| NOW - 900 * (96 - index)).collect();
    Supply {
        history: MeasuredHistory::new(starts, vec![0.0; 96], vec![0.6; 96]).expect("history"),
        forecast: Some(Forecast::fresh(
            vec![0.0; HORIZON_SLOTS],
            vec![0.6; HORIZON_SLOTS],
            NOW,
        )),
        prices,
        price_fetched_at: NOW,
        settlement: settlement(),
    }
}

fn packs() -> Vec<PackState> {
    vec![
        state("a", PACK_CAPACITY_KWH, 500.0, 60.0),
        state("b", PACK_CAPACITY_KWH, 500.0, 60.0),
    ]
}

/// Expensive first half, cheap second half, no sun: the solver drains the packs towards their
/// floor while import is dear. Feed-in is far below the buy price too. A non-zero penalty makes
/// the solver actually finish at the terminal target instead of optimising past it.
fn source(reserve: f64, peak_reserve: f64, peak_penalty: f64) -> LivePlanSource<FakeSupply> {
    let policy = Policy {
        export: ExportPolicy::Legacy,
        net_metering: NetMetering::Off,
        solar_reserve_pct: 0.0,
        peak_reserve_pct: peak_reserve,
        peak_reserve_value_eur_per_kwh: peak_penalty,
        wear_eur_per_kwh: 0.0,
        cost_basis_eur_per_kwh: None,
        zero_export: false,
    };
    let config = SourceConfig {
        reserve_soc_pct: reserve,
        policy,
        ..SourceConfig::default()
    };
    LivePlanSource::new(
        FakeSupply {
            supply: Mutex::new(supply(book(NOW, HORIZON_SLOTS, |index| {
                if index < HORIZON_SLOTS / 2 {
                    0.80
                } else {
                    0.01
                }
            }))),
        },
        config,
    )
}

async fn plan(reserve: f64, peak_reserve: f64) -> Plan {
    plan_for(packs(), reserve, peak_reserve, 0.0).await
}

async fn plan_for(
    fleet: Vec<PackState>,
    reserve: f64,
    peak_reserve: f64,
    peak_penalty: f64,
) -> Plan {
    source(reserve, peak_reserve, peak_penalty)
        .plan(NOW, fleet)
        .await
        .expect("plan")
}

fn four_packs() -> Vec<PackState> {
    (0..4)
        .map(|index| state(&format!("pack-{index}"), PACK_CAPACITY_KWH, 500.0, 60.0))
        .collect()
}

fn floor_kwh(reserve: f64) -> f64 {
    CAPACITY_KWH * reserve.max(PACK_MIN_SOC_PCT) / 100.0
}

fn min_slot_kwh(plan: &Plan) -> f64 {
    plan.slots
        .iter()
        .map(|slot| slot.soc_kwh)
        .fold(f64::INFINITY, f64::min)
}

/// The live contradiction: 10 % peak guideline, 15 % hard reserve, four packs.
#[tokio::test]
async fn a_soft_guideline_below_the_floor_is_raised_and_the_plan_is_feasible() {
    let plan = plan_for(four_packs(), 15.0, 10.0, 0.15).await;
    let floor = FLEET_CAPACITY_KWH * 0.15;
    let soft = FLEET_CAPACITY_KWH * 0.10;

    assert!(
        (plan.summary.terminal_target_kwh.expect("effective target") - floor).abs() < 1e-6,
        "terminal target must be the 15 % floor: {:?}",
        plan.summary.terminal_target_kwh
    );
    assert!(
        (plan.summary.peak_reserve_kwh.expect("soft guideline") - soft).abs() < 1e-6,
        "the soft 10 % guideline must stay distinct: {:?}",
        plan.summary.peak_reserve_kwh
    );
    assert!(plan.feasible, "the floor-clamped target must be reachable");
    assert_eq!(plan.summary.infeasible_reason, None);
    // `soc_kwh` is the state at a slot's start; `target_soc_kwh` is the end state (the DP's
    // final step is reconstructed there), so that is the horizon's terminal SoC.
    let end = plan.slots.last().expect("a final slot").target_soc_kwh;
    assert!(
        end + 1e-6 >= floor && end <= floor + 0.11,
        "the SoC path must still end at the 15 % floor, not below it: {end} vs {floor}"
    );
    assert!(
        min_slot_kwh(&plan) + 1e-6 >= floor,
        "no slot may dip below the 15 % floor"
    );
}

/// The guideline above the floor is unchanged: the clamp is one-way.
#[tokio::test]
async fn a_soft_guideline_above_the_floor_is_unchanged() {
    let plan = plan(10.0, 20.0).await;
    let soft = CAPACITY_KWH * 0.20;

    assert!(
        (plan.summary.terminal_target_kwh.expect("effective target") - soft).abs() < 1e-6,
        "a guideline above the floor must pass through unchanged: {:?}",
        plan.summary.terminal_target_kwh
    );
    assert_eq!(
        plan.summary.terminal_target_kwh,
        plan.summary.peak_reserve_kwh
    );
}

/// The reported terminal/peak figure follows the policy (10 % here), while the hard floor follows
/// the reserve setting (20 % here): they are not the same number.
#[tokio::test]
async fn the_hard_reserve_floor_and_the_soft_peak_reserve_stay_distinct() {
    let plan = plan(20.0, 10.0).await;
    let floor = floor_kwh(20.0);
    let min = min_slot_kwh(&plan);
    assert!(
        min + 1e-6 >= floor,
        "planned below the 20 % reserve: {min} < {floor}"
    );
    assert!(
        min <= floor + 0.11,
        "the plan never reached its floor: {min} > {floor}"
    );
    assert!(
        (plan.peak_reserve_kwh.expect("peak guideline") - CAPACITY_KWH * 0.10).abs() < 1e-6,
        "the soft peak reserve must still report 10 % of the fleet: {:?}",
        plan.peak_reserve_kwh
    );
}

#[tokio::test]
async fn the_plan_source_plans_to_the_reserve_floor_for_every_setting() {
    for reserve in [0.0, 10.0, 15.0, 20.0] {
        let plan = plan(reserve, 0.0).await;
        let floor = floor_kwh(reserve);
        let min = min_slot_kwh(&plan);
        assert!(
            min + 1e-6 >= floor,
            "reserve {reserve} %: planned below the floor {min} < {floor}"
        );
        assert!(
            min <= floor + 0.11,
            "reserve {reserve} %: the plan stopped above the floor {min} > {floor}"
        );
    }
}

#[tokio::test]
async fn a_15_percent_reserve_makes_both_the_floor_and_a_15_percent_peak_guideline_15_percent() {
    let plan = plan(15.0, 15.0).await;
    let expected = CAPACITY_KWH * 0.15;
    let min = min_slot_kwh(&plan);
    assert!(
        (min - expected).abs() <= 0.11,
        "the plan's floor must be 15 % of the fleet: {min} vs {expected}"
    );
    assert!(
        min + 1e-6 >= expected,
        "the plan dipped below 15 %: {min} < {expected}"
    );
    assert!(
        (plan.peak_reserve_kwh.expect("peak guideline") - expected).abs() < 1e-6,
        "the 15 % peak guideline should report 15 % of the fleet: {:?}",
        plan.peak_reserve_kwh
    );
}

/// The planning floor and the ladder's floor are one number: a slot just above it can still be
/// delivered, and a slot at or below it cannot be discharged any further.
#[tokio::test]
async fn the_plan_floor_is_the_floor_the_ladder_enforces() {
    let reserve = 15.0;
    let plan = plan(reserve, 0.0).await;
    let planned_floor = min_slot_kwh(&plan);
    let pack = packs()[0].spec.clone();
    let ladder_floor = pack.e_min_kwh(reserve);
    let fleet_ladder_floor: f64 = packs()
        .iter()
        .map(|state| state.spec.e_min_kwh(reserve))
        .sum();
    assert!(
        (planned_floor - fleet_ladder_floor).abs() <= 0.11,
        "planner floor {planned_floor} disagrees with fleet ladder floor {fleet_ladder_floor}"
    );

    let limits = SafetyLimits {
        mode: Authority::Auto,
        reserve_soc_pct: reserve,
        tick_seconds: 5.0,
        fleet_capacity_kwh: CAPACITY_KWH,
        ..SafetyLimits::default()
    };
    let request = |energy_kwh: f64| {
        CommandRequest::new(
            "a",
            -300.0,
            None,
            NOW,
            Measured {
                energy_kwh: Some(energy_kwh),
                soc_pct: Some(energy_kwh / PACK_CAPACITY_KWH * 100.0),
                ..Measured::default()
            },
        )
    };

    // A plan slot just above the floor is deliverable: the ladder leaves the discharge alone.
    let deliverable = check(&request(ladder_floor + 0.05), &pack, &limits, None);
    assert!(
        deliverable.writes() && (deliverable.allowed_w + 300.0).abs() < 1e-6,
        "a slot above the floor was clamped: {deliverable:?}"
    );
    // At the floor and below it, the ladder stops the discharge with the same reason.
    for energy in [ladder_floor, ladder_floor - 0.01] {
        let held = check(&request(energy), &pack, &limits, None);
        assert!(!held.writes(), "discharged below the floor: {held:?}");
        assert!(held.has(Reason::SocFloor), "wrong clamp reason: {held:?}");
    }
}
