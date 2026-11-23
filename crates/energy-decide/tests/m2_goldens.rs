#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::manual_range_contains,
    clippy::panic
)]

//! S158 and S164: independent optimizer oracles, not hand-picked schedules.

mod m2_support;

use energy_decide::optimizer::{OptimizerConfig, OptimizerInput, solve};
use m2_support::{
    assert_physical, brute_force, dp_objective, fixture_prices, live, slot, solved, tiny,
};

/// S158: two 15-minute slots at 0.10 then 0.30, unit efficiency, zero wear.
#[test]
fn s158_two_slot_optimizer_golden() {
    let config = tiny(0.0, 1.0, 1000.0, 1.0, 0.0);
    let input = OptimizerInput {
        slots: vec![slot(0.10, 0.30, 900), slot(0.10, 0.30, 900)],
        config,
    };
    let result = solved(&input);
    let powers: Vec<i32> = result.schedule.iter().map(|row| row.power_w).collect();
    let energies: Vec<(f64, f64)> = result
        .schedule
        .iter()
        .map(|row| (row.energy_start_kwh, row.energy_end_kwh))
        .collect();
    assert_eq!(powers, vec![1000, -1000]);
    assert_eq!(energies, vec![(0.0, 0.25), (0.25, 0.0)]);
    assert!((result.objective.total_eur + 0.05).abs() < 1e-12);
    assert!((0.25 * 0.10 - 0.25 * 0.30 + 0.0 - result.objective.total_eur).abs() < 1e-12);
    let (oracle_objective, oracle_powers) = brute_force(&input);
    assert!((oracle_objective - result.objective.total_eur).abs() <= 0.01);
    assert_eq!(oracle_powers, powers);
    assert_physical(&input, &result);
}

/// S164: the complete 120-slot timestamped oracle fixture.
#[test]
fn s164_full_fixture_oracle() {
    let input = OptimizerInput {
        slots: fixture_prices()
            .into_iter()
            .map(|price| slot(price, price, 900))
            .collect(),
        config: live(),
    };
    assert_eq!(input.slots.len(), 120);
    let result = solved(&input);
    assert_physical(&input, &result);

    // Independent backward DP over a 20 Wh grid (1 Wh is intractable in debug on this host).
    let oracle = dp_objective(&input, 0.02);
    assert!(
        (oracle - result.objective.total_eur).abs() <= 0.01,
        "oracle {oracle:.9} vs implementation {:.9}",
        result.objective.total_eur
    );

    // Reviewer-fixed golden value of the canonical optimum for this fixture.
    assert!((result.objective.total_eur + 3.085_210_965_741).abs() < 1e-6);
    assert!((result.terminal_kwh - 1.179_461_111_111_108_6).abs() < 1e-9);
    assert!(
        result
            .schedule
            .iter()
            .all(|row| row.power_w >= 0 || row.slot >= 11)
    );
    // All cheap slots carry the same 0.3498 price; the canonical energy-signature key prefers
    // the later equal-price charges, so assert the count before the peak rather than "tonight".
    assert!(
        result.schedule[12..96]
            .iter()
            .filter(|row| row.power_w > 0)
            .count()
            >= 4
    );
    assert!(result.schedule[96..104].iter().any(|row| row.power_w < 0));
    assert_eq!(result.tie_break.first_discharge_slot_index, 96);
}

/// S123/S116 replay: stored-energy conservation for the two live strategies.
#[test]
fn s116_s123_fixture_revenue_and_energy() {
    let sell_prices = [0.4968, 0.5098, 0.5114, 0.5303, 0.5188, 0.5156, 0.5125];
    let revenue = 0.8 * sell_prices[..6].iter().sum::<f64>() + 0.2 * sell_prices[6];
    assert!((revenue - 2.56866).abs() < 1e-9);
    assert!((revenue - 2.6515).abs() > 0.08);
    let net_b = revenue - 2.159259 - 0.111111;
    assert!((net_b - 0.29829).abs() < 2e-5);
    let mut energy: f64 = 6.995;
    for _ in 0..6 {
        energy -= 0.8 / 0.9;
    }
    energy -= 0.2 / 0.9;
    for _ in 0..7 {
        energy += 0.8 * 0.9;
    }
    energy += 0.57284 * 0.9;
    assert!((energy - 6.995).abs() < 1e-6);
    assert!(energy >= 1.178625 && energy <= 7.8575);
}

/// S115: verified prices make holding today correct; tomorrow's peak pays more.
#[test]
fn s115_today_peak_is_not_sold() {
    let today_max = [
        0.4037, 0.4044, 0.4072, 0.4111, 0.4130, 0.4064, 0.4074, 0.4061, 0.4188,
    ]
    .into_iter()
    .max_by(f64::total_cmp)
    .unwrap_or(0.0);
    assert!((today_max - 0.4188).abs() < 1e-12);
    let tomorrow = fixture_prices()[96..112].to_vec();
    let weighted: f64 = 2.56866 / 5.0;
    assert!((weighted - 0.513732).abs() < 1e-9);
    assert!(tomorrow.iter().any(|price| *price >= 0.5303));
    let input = OptimizerInput {
        slots: fixture_prices()
            .into_iter()
            .map(|price| slot(price, price, 900))
            .collect(),
        config: live(),
    };
    assert!(
        solved(&input)
            .schedule
            .iter()
            .all(|row| row.power_w >= 0 || row.slot >= 96)
    );
}

/// S157-style determinism on the full fixture: same input, byte-identical schedule.
#[test]
fn s157_deterministic_full_fixture() {
    let input = OptimizerInput {
        slots: fixture_prices()
            .into_iter()
            .map(|price| slot(price, price, 900))
            .collect(),
        config: live(),
    };
    let first = solve(&input).unwrap();
    let second = solve(&input).unwrap();
    assert_eq!(
        format!("{:?}", first.schedule),
        format!("{:?}", second.schedule)
    );
    assert_eq!(first.tie_break, second.tie_break);
    assert_eq!(first.objective, second.objective);
}

/// Review A item 1 errata (report only; the frozen SPEC is unchanged): its "- wear" is profit
/// notation. This minimiser ADDS wear to cost. A 0.25 kWh discharge at buy 1.00 / sell 1.04 earns
/// 0.0100 EUR of spread, so wear 0.02 EUR/stored kWh (0.005 EUR) is worth paying; wear 0.50
/// (0.125 EUR) is not, so the optimizer must not cycle.
#[test]
fn wear_errata_golden_discharge_only_when_spread_exceeds_wear() {
    let cheap = OptimizerInput {
        slots: vec![slot(1.0, 1.04, 900)],
        config: tiny(1.0, 1.0, 1000.0, 1.0, 0.02),
    };
    let discharged = solved(&cheap);
    assert_eq!(
        discharged
            .schedule
            .iter()
            .map(|row| row.power_w)
            .collect::<Vec<_>>(),
        vec![-1000]
    );
    let spread = 0.25 * (1.04 - 1.0);
    assert!(spread > 0.005 && spread < 0.125);
    assert!((discharged.objective.grid_eur + 0.26).abs() < 1e-12);
    assert!((discharged.objective.wear_eur - 0.005).abs() < 1e-12);
    assert!((discharged.objective.salvage_eur + 0.75).abs() < 1e-12);
    assert!((discharged.objective.total_eur + 1.005).abs() < 1e-12);

    let dear = OptimizerInput {
        slots: vec![slot(1.0, 1.04, 900)],
        config: tiny(1.0, 1.0, 1000.0, 1.0, 0.50),
    };
    let held = solved(&dear);
    assert_eq!(held.schedule[0].power_w, 0);
    assert_eq!(held.objective.wear_eur, 0.0);
    assert!((held.objective.total_eur + 1.0).abs() < 1e-12);
}

/// Review A item 4: the first four S164 slots with a hand breakdown and a full 1 Wh oracle.
#[test]
fn s164_short_fixture_hand_breakdown_and_exhaustive_oracle() {
    let prices = fixture_prices()[..4].to_vec();
    assert_eq!(prices, vec![0.3980, 0.4037, 0.4044, 0.4072]);
    let input = OptimizerInput {
        slots: prices
            .iter()
            .map(|price| slot(*price, *price, 900))
            .collect(),
        config: live(),
    };
    let result = solved(&input);
    assert_eq!(
        result
            .schedule
            .iter()
            .map(|row| row.power_w)
            .collect::<Vec<_>>(),
        vec![0, 0, 0, 0]
    );
    assert!((result.terminal_kwh - 6.995).abs() < 1e-12);

    // Hand computation: no grid import/export, no discharge, no shortfall (6.995 kWh is above
    // the 1.178625 kWh target). Cheapest buy is 0.3980 EUR/kWh, so salvage is 0.3980/0.9 =
    // 0.442222222222 EUR/stored kWh: 0.442222222222 * 6.995 = 3.093344444444 EUR credit.
    let cheapest_buy = 0.3980;
    let salvage_rate = cheapest_buy / 0.9;
    assert!((result.objective.salvage_eur + salvage_rate * 6.995).abs() < 1e-12);
    assert_eq!(result.objective.grid_eur, 0.0);
    assert_eq!(result.objective.wear_eur, 0.0);
    assert_eq!(result.objective.terminal_penalty_eur, 0.0);
    assert!((result.objective.total_eur + 3.0933444444444445).abs() < 1e-12);

    // Optimality: brute_force enumerates every one of the 25^4 = 390625 action paths on exact
    // 1 Wh / 1 W arithmetic. It finds the same euro value (the single-charge path [417,0,0,0]
    // ties because charging at the cheapest price costs p*E_AC and salvage returns
    // (p/eta)*eta*E_AC = p*E_AC); the canonical nonzero-slot key selects the flat path.
    let (oracle, oracle_powers) = brute_force(&input);
    assert!((oracle - result.objective.total_eur).abs() <= 1e-12);
    assert!(oracle_powers.iter().filter(|power| **power != 0).count() <= 1);
}

/// Review B item 2: exhaustive 1 Wh search at states where interpolation used to invent
/// continuations. Each replayed continuation must be physically executable from the exact energy.
#[test]
fn s161_off_lattice_states_match_exhaustive_search() {
    let near_floor = OptimizerConfig {
        start_kwh: 1.188625,
        ceiling_kwh: 3.0,
        hard_floor_kwh: 1.178625,
        soft_guideline_kwh: 1.178625,
        charge_power_w: 1000.0,
        discharge_power_w: 1000.0,
        ..live()
    };
    assert_exhaustive(&OptimizerInput {
        slots: vec![slot(0.4, 0.5, 900), slot(0.5, 0.6, 900)],
        config: near_floor,
    });

    let near_ceiling = OptimizerConfig {
        start_kwh: 2.98,
        ..live()
    };
    assert_exhaustive(&OptimizerInput {
        slots: vec![slot(0.3, 0.5, 900), slot(0.4, 0.6, 900)],
        config: near_ceiling,
    });

    let discontinuity = OptimizerConfig {
        start_kwh: 2.97,
        ceiling_kwh: 3.0,
        charge_power_w: 1000.0,
        discharge_power_w: 1000.0,
        ..live()
    };
    assert_exhaustive(&OptimizerInput {
        slots: vec![slot(0.2, 0.4, 900), slot(0.25, 0.45, 900)],
        config: discontinuity,
    });
}

fn assert_exhaustive(input: &OptimizerInput) {
    let result = solved(input);
    assert_physical(input, &result);
    let (oracle, _) = brute_force(input);
    assert!(
        (oracle - result.objective.total_eur).abs() <= 1e-9,
        "oracle {oracle:.12} vs implementation {:.12}",
        result.objective.total_eur
    );
}

/// Review B item 5: the p² key is the exact sum in W² and has no slot-duration factor.
#[test]
fn canonical_key_uses_power_squared_in_watts() {
    let input = OptimizerInput {
        slots: vec![slot(1.0, 1.2, 1800)],
        config: tiny(1.0, 1.0, 1000.0, 1.0, 0.0),
    };
    let result = solved(&input);
    assert_eq!(result.schedule[0].power_w, -1000);
    assert_eq!(result.tie_break.sum_power_squared, 1_000_000.0);
    assert_ne!(result.tie_break.sum_power_squared, 0.5);
}

/// Review B item 5: neutral-price paths tie on euros; only the canonical key selects zero.
#[test]
fn canonical_key_decides_a_pure_tie() {
    let input = OptimizerInput {
        slots: vec![slot(1.0, 1.0, 900), slot(1.0, 1.0, 900)],
        config: tiny(0.5, 1.0, 1000.0, 1.0, 0.0),
    };
    let result = solved(&input);
    assert_eq!(
        result
            .schedule
            .iter()
            .map(|row| row.power_w)
            .collect::<Vec<_>>(),
        vec![-43, 0]
    );
    assert_eq!(result.tie_break.first_discharge_slot_index, 0);
    assert_eq!(result.tie_break.sum_power_squared, 1849.0);
    assert_eq!(result.tie_break.nonzero_slot_count, 1);
    assert_eq!(result.tie_break.objective_rounded_eur, -0.5);
}
