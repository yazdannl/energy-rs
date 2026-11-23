#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::manual_range_contains,
    clippy::panic
)]

//! Real-shape arbitrage: one cheap buy slot, eight verified peak sell slots, learned basis.

mod m2_support;

use energy_decide::optimizer::{OptimizerConfig, OptimizerInput, solve};
use m2_support::{assert_physical, slot};

/// Sell at the verified 15:45-17:30Z peak array, buy at 0.3827, 92 % initial SoC.
#[test]
fn real_shape_arbitrage_matches_hand_computed_euros() {
    let prices = [
        0.3827, 0.5099, 0.4968, 0.5098, 0.5114, 0.5303, 0.5188, 0.5156, 0.5125,
    ];
    let input = OptimizerInput {
        slots: prices
            .iter()
            .map(|price| slot(*price, *price, 900))
            .collect(),
        config: OptimizerConfig {
            start_kwh: 0.92 * 7.8575,
            ..OptimizerConfig::default()
        },
    };
    let result = solve(&input).unwrap();
    assert_physical(&input, &result);

    let expected_powers = [2435, -3200, -1322, -3200, -3200, -3200, -3200, -3200, -3200];
    let expected_energies = [
        7.776775,
        6.887886111111111,
        6.5206638888888895,
        5.631775,
        4.742886111111111,
        3.853997222222222,
        2.9651083333333332,
        2.0762194444444444,
        1.1873305555555556,
    ];
    let powers: Vec<i32> = result.schedule.iter().map(|row| row.power_w).collect();
    let energies: Vec<f64> = result
        .schedule
        .iter()
        .map(|row| row.energy_end_kwh)
        .collect();
    assert_eq!(powers, expected_powers);
    assert_energies(&energies, &expected_energies);

    // Independent hand computation (dt 0.25 h, eta 0.9/0.9, hardware caps 4.0 kW charge
    // and 3.2 kW discharge, so the 24-step grid is -3200..4000 W in 300 W steps):
    // charge 2.435 kW adds 2.435*0.25*0.9 = 0.547875 kWh -> 7.776775 kWh,
    //   paying 0.608750 kWh * 0.3827 = +0.232968625 EUR
    // sell slots (AC kWh): 0.8 @0.5099, 0.3305 @0.4968, six 0.8 slots at the verified peaks:
    //   -0.407920000 -0.164192400 -0.407840000 -0.409120000 -0.424240000
    //   -0.415040000 -0.412480000 -0.410000000 = -3.050832400 EUR
    // grid total = 0.232968625 - 3.050832400 = -2.817863775 EUR
    // wear = 0.02 * (7.776775 - 1.187330555556) = 0.131788888889 EUR (stored kWh)
    // salvage = -(0.3827/0.9) * 1.187330555556 = -0.504879337346 EUR
    // terminal penalty = 0 (1.1873306 >= 1.178625 - 0.05)
    // objective = -2.817863775 + 0.131788888889 - 0.504879337346 = -3.190954223457 EUR
    let grid: f64 = result.schedule.iter().map(|row| row.grid_eur).sum();
    let wear: f64 = result.schedule.iter().map(|row| row.wear_eur).sum();
    assert!((grid + 2.817_863_775).abs() < 1e-9);
    assert!((wear - 0.131_788_888_889).abs() < 1e-9);
    assert!((result.objective.salvage_eur + 0.504_879_337_346).abs() < 1e-9);
    assert!(result.objective.terminal_penalty_eur.abs() < 1e-12);
    assert!((result.objective.total_eur + 3.190_954_223_457).abs() < 1e-9);
    assert_eq!(result.tie_break.first_discharge_slot_index, 1);
    assert!(result.terminal_kwh >= 1.178625);
}

fn assert_energies(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected.iter()) {
        assert!((actual - expected).abs() < 1e-9);
    }
}
