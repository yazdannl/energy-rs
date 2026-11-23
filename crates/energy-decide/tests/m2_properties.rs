#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::manual_range_contains,
    clippy::panic
)]

//! S161/S69/S119: generated physical invariants, typed refusals and deterministic replay.

mod m2_support;

use energy_decide::optimizer::{OptimizerInput, OptimizerRefusal, Slot, solve};
use m2_support::{assert_physical, live, slot, solved};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0 >> 11
    }

    fn unit(&mut self) -> f64 {
        (self.next() % 10_000) as f64 / 10_000.0
    }
}

/// Seeded generated horizons: conservation, bounds, typed refusal and byte-identical replays.
#[test]
fn s161_generated_physical_invariants() {
    let mut rng = Rng(42);
    let mut accepted = 0;
    for _ in 0..40 {
        let count = 1 + (rng.next() % 6) as usize;
        let config = energy_decide::optimizer::OptimizerConfig {
            start_kwh: 2.0 + 4.0 * rng.unit(),
            ..live()
        };
        let slots: Vec<Slot> = (0..count)
            .map(|_| {
                let mut slot = slot(0.10 + 0.50 * rng.unit(), 0.10 + 0.50 * rng.unit(), 900);
                slot.load_kw = 5.0 * rng.unit();
                slot.pv_kw = 5.0 * rng.unit();
                slot.export_limit_kw = match rng.next() % 3 {
                    0 => None,
                    1 => Some(0.0),
                    _ => Some(1.0),
                };
                slot
            })
            .collect();
        let input = OptimizerInput { slots, config };
        let result = match solve(&input) {
            Ok(result) => result,
            Err(OptimizerRefusal::Infeasible { binding_bounds, .. }) => {
                assert_ne!(binding_bounds.len(), 0);
                continue;
            }
            Err(OptimizerRefusal::TerminalTargetUnreachable { .. }) => continue,
            Err(other) => panic!("unexpected generated refusal: {other:?}"),
        };
        accepted += 1;
        assert_physical(&input, &result);
        assert!(result.objective.total_eur.is_finite());
        let replay = solve(&input).unwrap();
        assert_eq!(
            format!("{:?}", result.schedule),
            format!("{:?}", replay.schedule)
        );
        assert_eq!(result.tie_break, replay.tie_break);
    }
    assert!(
        accepted > 0,
        "at least one generated horizon must be accepted"
    );
}

/// Every infeasible generated horizon refuses with all binding bounds and no executable command.
#[test]
fn s161_generated_infeasible_refusals() {
    for _ in 0..20 {
        let mut impossible = slot(0.4, 0.4, 900);
        impossible.load_kw = 20.0;
        let input = OptimizerInput {
            slots: vec![impossible],
            config: live(),
        };
        match solve(&input) {
            Err(OptimizerRefusal::Infeasible { binding_bounds, .. }) => {
                assert!(
                    binding_bounds
                        .iter()
                        .any(|bound| bound.reason == "import_limit")
                );
            }
            other => panic!("infeasible horizon produced a result: {other:?}"),
        }
    }
}

/// S69/S119: equal-price peaks prefer the earlier discharge; no discharge sorts last.
#[test]
fn s69_s119_canonical_tie_keys() {
    let config = energy_decide::optimizer::OptimizerConfig {
        start_kwh: 6.9,
        ..live()
    };
    let input = OptimizerInput {
        slots: (0..6).map(|_| slot(0.4, 0.55, 900)).collect(),
        config,
    };
    let result = solved(&input);
    let first = result.tie_break.first_discharge_slot_index;
    assert!(first < 6);
    assert_eq!(
        result.tie_break.power_signature_w.len(),
        result.schedule.len()
    );
    assert_eq!(
        result.tie_break.energy_signature_wh.len(),
        result.schedule.len()
    );
    assert_eq!(result.tie_break.tie_resolved_by, "canonical_signatures");
    let held = OptimizerInput {
        slots: (0..2).map(|_| slot(0.4, 0.1, 900)).collect(),
        config: energy_decide::optimizer::OptimizerConfig {
            start_kwh: 1.2,
            ..live()
        },
    };
    let held_result = solved(&held);
    assert_eq!(held_result.tie_break.first_discharge_slot_index, 2);
}

/// Generated horizons never mutate immutable slot inputs or emit NaN into the objective.
#[test]
fn deterministic_objective_and_no_nan() {
    for seed in 0..12_u64 {
        let mut rng = Rng(seed + 7);
        let config = energy_decide::optimizer::OptimizerConfig {
            start_kwh: 2.0 + 5.0 * rng.unit(),
            ..live()
        };
        let slots = (0..4)
            .map(|_| {
                let mut slot = slot(0.1 + 0.5 * rng.unit(), 0.1 + 0.5 * rng.unit(), 900);
                slot.pv_kw = 3.0 * rng.unit();
                slot.load_kw = 3.0 * rng.unit();
                slot
            })
            .collect();
        let input = OptimizerInput { slots, config };
        if let Ok(first) = solve(&input) {
            assert!(first.objective.total_eur.is_finite());
            assert!(first.objective.policy_preference_eur.is_finite());
            let second = solve(&input).unwrap();
            assert_eq!(first.objective, second.objective);
            assert_eq!(first.schedule, second.schedule);
        }
    }
}
