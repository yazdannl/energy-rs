#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::manual_range_contains,
    clippy::panic,
    clippy::assertions_on_constants
)]

//! S15-S17, S67-S74, S117-S121, S179-S180, S184: every objective term and unit.

mod m2_support;

use energy_decide::optimizer::objective::terminal_eur;
use energy_decide::optimizer::{OptimizerInput, Slot, max_charge_w_at_soc, solve};
use m2_support::{assert_physical, curtail, live, slot, solved};

/// S67/S74: the economics contract and the two distinct wear units.
#[test]
fn s67_s74_wear_and_cycle_profit() {
    let wear_cycle: f64 = 0.9 * 0.02;
    assert!((wear_cycle - 0.018).abs() < 1e-12);
    let profit: f64 = 0.81 * 0.5303 - 0.3498 - wear_cycle;
    assert!((profit - 0.061743).abs() < 1e-9);
    assert!((0.02_f64 - 0.018 / 0.9).abs() < 1e-12);
    let config = live();
    let input = OptimizerInput {
        slots: vec![slot(0.3498, 0.5303, 900)],
        config: config.clone(),
    };
    let result = solved(&input);
    assert_physical(&input, &result);
    assert!(
        (result.objective.wear_eur - 0.02 * (config.start_kwh - result.terminal_kwh)).abs() < 1e-12
    );
    let mut discharge = slot(0.3498, 0.5303, 900);
    discharge.pv_price_eur_per_kwh = Some(0.10);
    let preference = energy_decide::optimizer::solve_curtailment(
        &discharge,
        0,
        -1.0,
        config.import_limit_kw,
        0.0,
    )
    .unwrap()
    .policy_preference_eur;
    assert!(preference.abs() < 1e-12);
}

/// S68/S73: scaling arithmetic and the unprofitable-cycle hold.
#[test]
fn s68_s73_scaling_and_unprofitable_hold() {
    let charge_cost: f64 = 6.4 / 0.81 * 0.3498;
    let wear: f64 = 6.4 / 0.9 * 0.02;
    assert!((charge_cost - 2.76385185).abs() < 1e-8);
    assert!((wear - 0.14222222).abs() < 1e-8);
    let today: f64 = 6.4 * 0.4188 - charge_cost - wear;
    let tomorrow: f64 = 6.4 * 0.5303 - charge_cost - wear;
    assert!((today + 0.22575407).abs() < 1e-8);
    assert!((tomorrow - 0.48784593).abs() < 1e-8);
    assert!((tomorrow - today - 0.7136).abs() < 1e-8);
    let loss: f64 = 0.81 * 0.35 - 0.53 - 0.018;
    assert!((loss + 0.2645).abs() < 1e-9);
    let input = OptimizerInput {
        slots: vec![slot(0.53, 0.35, 900), slot(0.53, 0.35, 900)],
        config: live(),
    };
    let result = solved(&input);
    assert!(result.schedule.iter().all(|row| row.power_w >= 0));
}

/// S15/S16/S17/S118: learned basis, export gate and consistent terminal salvage.
#[test]
fn s15_s16_s17_s118_values() {
    let config = live();
    assert!((config.effective_target_kwh() - 1.178625).abs() < 1e-12);
    assert!((config.soft_guideline_kwh - 0.78575).abs() < 1e-12);
    assert!((0.10_f64 * 7.68 - 0.768).abs() < 1e-12);
    assert!((config.solar_headroom_kwh() - 1.5715).abs() < 1e-12);
    assert!((0.35_f64 / 0.81 + 0.02 - 0.452_098_77).abs() < 1e-8);
    assert!(0.53 > 0.35 / 0.81 + 0.02);
    assert!(0.44 < 0.35 / 0.81 + 0.02);
    let salvage: f64 = 0.3498 / 0.9;
    assert!((salvage - 0.388_666_67).abs() < 1e-8);
    assert!((salvage - 0.451_851_85).abs() > 0.06);
    let input = OptimizerInput {
        slots: vec![slot(0.3498, 0.3498, 900)],
        config,
    };
    let result = solved(&input);
    assert!((result.salvage_eur_per_stored_kwh - salvage).abs() < 1e-12);
    assert!((result.objective.salvage_eur + salvage * result.terminal_kwh).abs() < 1e-12);
}

/// S70/S117/S121: curtailment is one physical variable priced once by the grid term.
#[test]
fn s70_s117_s121_curtailment_algebra() {
    let mut base = slot(0.5303, 0.5303, 900);
    base.load_kw = 1.0;
    base.pv_kw = 3.0;
    let input = OptimizerInput {
        slots: vec![base.clone()],
        config: live(),
    };
    let unlimited = curtail(&input, 0, 0.0).unwrap();
    assert!((unlimited.curtail_kw).abs() < 1e-12);
    assert!((unlimited.grid_kw + 2.0).abs() < 1e-12);
    assert!((unlimited.grid_cost_eur + 0.26515).abs() < 1e-12);

    let mut capped = base.clone();
    capped.export_limit_kw = Some(1.0);
    let capped_input = OptimizerInput {
        slots: vec![capped],
        config: live(),
    };
    let capped_result = curtail(&capped_input, 0, 0.0).unwrap();
    assert!((capped_result.curtail_kw - 1.0).abs() < 1e-12);
    assert!((capped_result.grid_kw + 1.0).abs() < 1e-12);
    assert!((capped_result.grid_cost_eur + 0.132575).abs() < 1e-12);

    let mut zero = base;
    zero.export_limit_kw = Some(0.0);
    let zero_input = OptimizerInput {
        slots: vec![zero],
        config: live(),
    };
    let zero_result = curtail(&zero_input, 0, 0.0).unwrap();
    assert!((zero_result.curtail_kw - 2.0).abs() < 1e-12);
    assert!(zero_result.grid_kw.abs() < 1e-12);
    assert!(zero_result.grid_cost_eur.abs() < 1e-12);
}

/// S121: sweep PV, load, export caps and power; grid cost is priced exactly once.
#[test]
fn s121_curtailment_property_sweep() {
    for pv in [0.0, 1.0, 3.0, 5.0] {
        for load in [0.0, 1.0, 5.0] {
            for limit in [None, Some(1.0), Some(0.0)] {
                for power in [-3.2, 0.0, 2.4] {
                    let mut slot = Slot {
                        pv_kw: pv,
                        load_kw: load,
                        export_limit_kw: limit,
                        ..slot(0.21, 0.53, 900)
                    };
                    slot.buy_eur_per_kwh = 0.40;
                    let input = OptimizerInput {
                        slots: vec![slot],
                        config: live(),
                    };
                    if let Ok(solved_slot) = curtail(&input, 0, power) {
                        let grid = load - pv + solved_slot.curtail_kw + power;
                        let expected = 0.25 * (0.40 * grid.max(0.0) - 0.53 * (-grid).max(0.0));
                        assert!((solved_slot.grid_cost_eur - expected).abs() < 1e-12);
                        assert!(
                            solved_slot.curtail_kw >= 0.0 && solved_slot.curtail_kw <= pv + 1e-12
                        );
                        assert!(solved_slot.policy_preference_eur.abs() < 1e-12);
                    }
                }
            }
        }
    }
}

/// S179/S180: charging-only policy preference and the deductible terminal deadband.
#[test]
fn s179_s180_policy_and_deadband() {
    let config = live();
    let target = config.effective_target_kwh();
    let short_004 = terminal_eur(target - 0.04, target, 0.05, 0.15, 0.388_666_67);
    let short_010 = terminal_eur(target - 0.10, target, 0.05, 0.15, 0.388_666_67);
    let base = -0.388_666_67 * (target - 0.04);
    assert!((short_004 - base).abs() < 1e-12);
    assert!((short_010 - (-0.388_666_67 * (target - 0.10) + 0.0075)).abs() < 1e-12);

    let mut discharge = slot(0.30, 0.30, 900);
    discharge.pv_price_eur_per_kwh = Some(0.10);
    discharge.export_limit_kw = Some(0.0);
    discharge.load_kw = 1.0;
    let preference = energy_decide::optimizer::solve_curtailment(
        &discharge,
        0,
        -1.0,
        config.import_limit_kw,
        0.0,
    )
    .unwrap()
    .policy_preference_eur;
    assert!(preference.abs() < 1e-12);
    let result = solved(&OptimizerInput {
        slots: vec![discharge],
        config,
    });
    assert!((result.objective.policy_preference_eur).abs() < 1e-12);
}

/// S184: `e_N` is the only terminal state, whether N is 120 or 96.
#[test]
fn s184_terminal_index_consistency() {
    let config = live();
    for count in [96, 120] {
        let input = OptimizerInput {
            slots: (0..count).map(|_| slot(0.4, 0.4, 900)).collect(),
            config: OptimizerConfigWithStart {
                start: 1.2,
                base: config.clone(),
            }
            .build(),
        };
        let result = solve(&input).unwrap();
        assert_eq!(result.schedule.len(), count);
        assert!((result.terminal_kwh - 1.2).abs() < 1e-9);
        assert!((result.objective.salvage_eur + 1.2 * 0.4 / 0.9).abs() < 1e-6);
        assert!(result.objective.terminal_penalty_eur.abs() < 1e-12);
        assert!((result.effective_target_kwh - 1.178625).abs() < 1e-12);
    }
}

struct OptimizerConfigWithStart {
    start: f64,
    base: energy_decide::optimizer::OptimizerConfig,
}

impl OptimizerConfigWithStart {
    fn build(self) -> energy_decide::optimizer::OptimizerConfig {
        energy_decide::optimizer::OptimizerConfig {
            start_kwh: self.start,
            ..self.base
        }
    }
}

/// The taper injection point is flat today; no curve is invented.
#[test]
fn taper_injection_point_is_flat() {
    for soc in [0.5, 0.9, 0.96, 0.97, 0.98, 1.0] {
        assert!((max_charge_w_at_soc(800.0, soc) - 800.0).abs() < 1e-12);
    }
}

/// Review A item 2: the spread floor is a labelled policy preference, never settled money.
#[test]
fn min_spread_is_policy_only_and_never_changes_euros() {
    let config = energy_decide::optimizer::OptimizerConfig {
        start_kwh: 1.2,
        hard_floor_kwh: 1.178625,
        soft_guideline_kwh: 7.0,
        charge_power_w: 1000.0,
        min_spread_eur_per_kwh: 0.5,
        ..live()
    };
    let input = OptimizerInput {
        slots: vec![slot(0.4, 0.3, 900)],
        config: config.clone(),
    };
    let result = solved(&input);
    assert!(result.schedule[0].power_w > 0);
    assert!((result.objective.policy_min_spread_eur - 0.125).abs() < 1e-12);

    let baseline = OptimizerInput {
        slots: vec![slot(0.4, 0.3, 900)],
        config: energy_decide::optimizer::OptimizerConfig {
            min_spread_eur_per_kwh: 0.0,
            ..config
        },
    };
    let plain = solved(&baseline);
    assert_eq!(result.schedule[0].power_w, plain.schedule[0].power_w);
    assert_eq!(result.objective.grid_eur, plain.objective.grid_eur);
    assert_eq!(result.objective.total_eur, plain.objective.total_eur);
}

/// Review B item 1: normal prices make exporting PV cheaper than discarding it.
#[test]
fn curtailment_prefers_export_when_sell_is_positive() {
    let mut slot = slot(0.30, 0.10, 900);
    slot.pv_kw = 2.0;
    let solved = curtail(&input_with(&slot), 0, 0.0).unwrap();
    assert!((solved.curtail_kw - 0.0).abs() < 1e-12);
    assert!((solved.grid_kw + 2.0).abs() < 1e-12);
    assert!((solved.grid_cost_eur + 0.05).abs() < 1e-12);
}

/// Review B item 1: with negative export price and a load, the zero-grid kink wins.
#[test]
fn curtailment_prefers_zero_grid_kink() {
    let mut slot = slot(0.30, -0.10, 900);
    slot.pv_kw = 2.0;
    slot.load_kw = 1.0;
    let solved = curtail(&input_with(&slot), 0, 0.0).unwrap();
    assert!((solved.curtail_kw - 1.0).abs() < 1e-12);
    assert!(solved.grid_kw.abs() < 1e-12);
    assert!(solved.grid_cost_eur.abs() < 1e-12);
}

/// Review B item 1: with no load, all PV is curtailed rather than sold at a negative price.
#[test]
fn curtailment_prefers_full_curtailment_when_export_costs_money() {
    let mut slot = slot(0.30, -0.10, 900);
    slot.pv_kw = 2.0;
    let solved = curtail(&input_with(&slot), 0, 0.0).unwrap();
    assert!((solved.curtail_kw - 2.0).abs() < 1e-12);
    assert!(solved.grid_kw.abs() < 1e-12);
}

fn input_with(slot: &Slot) -> OptimizerInput {
    OptimizerInput {
        slots: vec![slot.clone()],
        config: live(),
    }
}
