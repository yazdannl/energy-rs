#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::manual_range_contains,
    clippy::panic
)]

//! S10-S13, S18, S71-S72, S120 and the degenerate/hostile-input barriers.

mod m2_support;

use energy_decide::optimizer::{
    OptimizerConfig, OptimizerInput, OptimizerRefusal, Slot, allocate_pack_powers, solve,
};
use m2_support::{assert_physical, curtail, live, refused, slot, solved};

/// Review B item 4: hardware charge and discharge limits are asymmetric; the allocator logic
/// was already direction-aware, only its frozen defaults and this regression test were wrong.
#[test]
fn s10_hardware_asymmetric_pack_and_fleet_limits() {
    let config = live();
    assert_eq!(config.pack_charge_w, 1000.0);
    assert_eq!(config.pack_discharge_w, 800.0);
    assert_eq!(config.charge_power_w, 4000.0);
    assert_eq!(config.discharge_power_w, 3200.0);

    let charge = allocate_pack_powers(&[1000, 1000, 1000, 1000], &config);
    assert_eq!(charge.powers_w, vec![1000, 1000, 1000, 1000]);
    assert_eq!(charge.binding_bounds.len(), 0);
    assert_eq!(charge.powers_w.iter().sum::<i32>(), 4000);

    let discharge = allocate_pack_powers(&[-800, -800, -800, -800], &config);
    assert_eq!(discharge.powers_w, vec![-800, -800, -800, -800]);
    assert_eq!(discharge.binding_bounds.len(), 0);
    assert_eq!(discharge.powers_w.iter().sum::<i32>(), -3200);

    let over_charge = allocate_pack_powers(&[-1000, -800, -800, -800], &config);
    assert_eq!(over_charge.powers_w, vec![-800, -800, -800, -800]);
    assert!(
        over_charge
            .binding_bounds
            .iter()
            .any(|bound| bound.reason == "pack_power")
    );

    assert_one_pack_limit(&config);
}

fn assert_one_pack_limit(config: &energy_decide::optimizer::OptimizerConfig) {
    let allocation = allocate_pack_powers(&[1500, 500, 500, 500], config);
    assert_eq!(allocation.powers_w, vec![1000, 500, 500, 500]);
    assert_eq!(allocation.powers_w.iter().sum::<i32>(), 2500);
    assert!(
        allocation
            .binding_bounds
            .iter()
            .any(|bound| bound.reason == "pack_power")
    );
}

/// S10: a request beyond the aggregate cap is clipped with `power_clamp`.
#[test]
fn s10_fleet_power_clamp() {
    let config = OptimizerConfig {
        charge_power_w: 3000.0,
        ..live()
    };
    let allocation = allocate_pack_powers(&[1000, 1000, 1000, 1000], &config);
    assert_eq!(allocation.powers_w, vec![1000, 1000, 1000, 0]);
    assert_eq!(allocation.powers_w.iter().sum::<i32>(), 3000);
    assert!(
        allocation
            .binding_bounds
            .iter()
            .any(|bound| bound.reason == "power_clamp")
    );
}

/// MIN-SETPOINT AFTER FLEET CLAMP: a 20 W remainder is never emitted; serve one pack.
#[test]
fn fleet_cap_never_leaves_a_subminimum_pack_setpoint() {
    let min = live().min_pack_power_w;
    assert_eq!(min, 50.0);
    let min_u = u32::try_from(min as i32).unwrap();
    let charge_config = OptimizerConfig {
        charge_power_w: 1020.0,
        ..live()
    };
    let charge = allocate_pack_powers(&[1000, 1000], &charge_config);
    assert_eq!(charge.powers_w, vec![1000, 0]);
    assert_eq!(charge.powers_w.iter().sum::<i32>(), 1000);
    assert!(
        charge
            .binding_bounds
            .iter()
            .any(|bound| bound.reason == "power_clamp")
    );

    let discharge_config = OptimizerConfig {
        discharge_power_w: 820.0,
        ..live()
    };
    let discharge = allocate_pack_powers(&[-800, -800], &discharge_config);
    assert_eq!(discharge.powers_w, vec![-800, 0]);
    assert_eq!(discharge.powers_w.iter().sum::<i32>(), -800);
    assert!(
        discharge
            .binding_bounds
            .iter()
            .any(|bound| bound.reason == "power_clamp")
    );

    for allocation in [charge.powers_w, discharge.powers_w].iter().flatten() {
        assert!(*allocation == 0 || allocation.unsigned_abs() >= min_u);
    }
}

/// S11: the minimum pack setpoint zeroes 49 W and keeps 51 W.
#[test]
fn s11_min_pack_power() {
    let config = live();
    let allocation = allocate_pack_powers(&[49, 51], &config);
    assert_eq!(allocation.powers_w, vec![0, 51]);
    assert!(
        allocation
            .binding_bounds
            .iter()
            .any(|bound| bound.reason == "min_pack_power")
    );
}

/// S12/S13: hard reserve floor, soft guideline and separate solar headroom.
#[test]
fn s12_s13_floor_and_guideline() {
    let config = OptimizerConfig {
        start_kwh: 1.6,
        ceiling_kwh: 7.68,
        hard_floor_kwh: 0.384,
        soft_guideline_kwh: 1.536,
        ..live()
    };
    assert!((config.effective_target_kwh() - 1.536).abs() < 1e-12);
    let input = OptimizerInput {
        slots: vec![slot(0.4, 0.5, 900)],
        config,
    };
    let result = solved(&input);
    assert!(result.schedule[0].power_w >= 0);
    assert_physical(&input, &result);
    let floor: f64 = 0.15 * 7.68;
    assert!((floor - 1.152).abs() < 1e-12);
    assert!((0.2_f64 * 7.8575 - 1.5715).abs() < 1e-12);
}

/// S18: zero export forbids export legs but not self-consumption discharge.
#[test]
fn s18_zero_export_forbids_only_export() {
    let mut config = live();
    config.start_kwh = 4.0;
    let zero_slot = Slot {
        export_limit_kw: Some(0.0),
        sell_eur_per_kwh: 2.0,
        ..slot(0.1, 2.0, 900)
    };
    let input = OptimizerInput {
        slots: vec![zero_slot],
        config,
    };
    let result = solved(&input);
    assert!(result.schedule[0].power_w >= 0 || (result.schedule[0].grid_kw).abs() < 1e-9);
    assert!(result.schedule[0].grid_kw >= -1e-9);
    let mut self_consumption = slot(0.1, 2.0, 900);
    self_consumption.export_limit_kw = Some(0.0);
    self_consumption.load_kw = 1.0;
    let input = OptimizerInput {
        slots: vec![self_consumption],
        config: live(),
    };
    let solved_slot = curtail(&input, 0, -1.0).unwrap();
    assert!(solved_slot.grid_kw.abs() < 1e-9);
}

/// S71: a 780 s first slot only trades 0.693333 kWh at 3.2 kW.
#[test]
fn s71_partial_first_slot() {
    let input = OptimizerInput {
        slots: vec![slot(0.35, 0.5, 780)],
        config: live(),
    };
    let solved_slot = curtail(&input, 0, 3.2).unwrap();
    let traded: f64 = 780.0 / 3600.0 * 3.2;
    assert!((traded - 0.693_333_33).abs() < 1e-8);
    assert!((solved_slot.grid_cost_eur - 0.35 * traded).abs() < 1e-12);
}

/// S72/S120: the 7.5 kW import ceiling is a hard inequality on the planned grid power.
#[test]
fn s72_s120_site_inequalities() {
    let mut load = slot(0.4, 0.4, 900);
    load.load_kw = 5.0;
    let input = OptimizerInput {
        slots: vec![load],
        config: live(),
    };
    assert!(curtail(&input, 0, 2.5).is_ok());
    let refused_limits = curtail(&input, 0, 3.2).unwrap_err();
    assert!(
        refused_limits
            .iter()
            .any(|bound| bound.reason == "import_limit")
    );

    let mut import = slot(0.4, 0.4, 900);
    import.load_kw = 11.0;
    let import_input = OptimizerInput {
        slots: vec![import],
        config: live(),
    };
    let bounds = curtail(&import_input, 0, -3.2).unwrap_err();
    assert!(bounds.iter().any(|bound| bound.reason == "import_limit"));
    assert!(bounds.iter().any(|bound| bound.reason == "pack_power"));

    let mut export = slot(0.4, 0.4, 900);
    export.load_kw = 1.0;
    export.pv_kw = 3.0;
    export.export_limit_kw = Some(0.5);
    let export_input = OptimizerInput {
        slots: vec![export],
        config: live(),
    };
    // g0 = -2 kW, so c=0 would export 2 kW and violate the 0.5 kW ceiling.
    let solved_export = curtail(&export_input, 0, 0.0).unwrap();
    assert!((solved_export.curtail_kw - 1.5).abs() < 1e-12);
    assert!((solved_export.grid_kw + 0.5).abs() < 1e-12);
}

/// Empty, single-slot, 96-slot degraded and over-long horizons.
#[test]
fn degenerate_horizons() {
    assert_eq!(
        refused(&OptimizerInput {
            slots: Vec::new(),
            config: live(),
        })
        .reason_slug(),
        "EmptyHorizon"
    );
    let single = OptimizerInput {
        slots: vec![slot(0.4, 0.5, 900)],
        config: live(),
    };
    let result = solved(&single);
    assert_eq!(result.schedule.len(), 1);
    let degraded = OptimizerInput {
        slots: (0..96).map(|_| slot(0.4, 0.5, 900)).collect(),
        config: live(),
    };
    let degraded_result = solved(&degraded);
    assert_eq!(degraded_result.schedule.len(), 96);
    assert_eq!(degraded_result.tie_break.power_signature_w.len(), 96);
    assert_eq!(
        refused(&OptimizerInput {
            slots: (0..121).map(|_| slot(0.4, 0.5, 900)).collect(),
            config: live(),
        })
        .reason_slug(),
        "HorizonSlots"
    );
}

/// NaN/infinite prices or PV refuse; finite negative prices are allowed.
#[test]
fn hostile_numeric_inputs() {
    for field in ["buy", "sell", "pv"] {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut hostile = slot(0.4, 0.5, 900);
            match field {
                "buy" => hostile.buy_eur_per_kwh = value,
                "sell" => hostile.sell_eur_per_kwh = value,
                _ => hostile.pv_kw = value,
            }
            let input = OptimizerInput {
                slots: vec![hostile],
                config: live(),
            };
            assert!(matches!(
                refused(&input),
                OptimizerRefusal::NonFiniteInput { field: actual, .. } if actual == field
            ));
        }
    }
    let negative_pv = OptimizerInput {
        slots: vec![Slot {
            pv_kw: -1.0,
            ..slot(0.4, 0.5, 900)
        }],
        config: live(),
    };
    assert!(matches!(
        refused(&negative_pv),
        OptimizerRefusal::InvalidPv { .. }
    ));
    let negative_prices = OptimizerInput {
        slots: vec![slot(-0.1, -0.05, 900)],
        config: live(),
    };
    assert!(solve(&negative_prices).is_ok());
}

/// Review A item 3: an unreachable terminal target stays a soft, priced shortfall in M2.
#[test]
fn unreachable_terminal_is_soft_and_invalid_bounds_still_refuse() {
    let config = OptimizerConfig {
        start_kwh: 1.2,
        hard_floor_kwh: 1.178625,
        soft_guideline_kwh: 7.0,
        charge_power_w: 100.0,
        ..live()
    };
    let input = OptimizerInput {
        slots: vec![slot(0.4, 0.5, 900)],
        config,
    };
    let result = solved(&input);
    assert_eq!(result.schedule[0].power_w, 100);
    assert!((result.terminal_kwh - 1.2225).abs() < 1e-12);
    assert!((result.objective.terminal_penalty_eur - 0.859125).abs() < 1e-12);
    assert!((result.objective.total_eur - 0.3257916666666667).abs() < 1e-12);
    let invalid = OptimizerConfig {
        hard_floor_kwh: 9.0,
        ..live()
    };
    let input = OptimizerInput {
        slots: vec![slot(0.4, 0.5, 900)],
        config: invalid,
    };
    assert!(matches!(
        refused(&input),
        OptimizerRefusal::InvalidBounds { .. }
    ));
}
