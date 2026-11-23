//! M1 scenarios S04-S05, S63-S65 and S124/S127: per-slot net metering and export prices.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

mod support;

use energy_decide::horizon::settlement::settle_slot;
use energy_decide::horizon::{
    AcknowledgedRegime, BudgetState, HorizonRefusal, NetMeteringRegime, OperatorOverride,
    PriceSlot, SettlementReason, build_schedule, netting_boundary_utc,
};
use support::{price, ts};

#[test]
fn s04_no_budget_defaults_to_feed_in_unless_untracked_is_acknowledged() {
    let slot = priced("2026-11-23T16:45:00Z", 0.5303, 0.18);
    let regime = NetMeteringRegime::DateDriven;
    let unknown = settle_slot(&slot, &regime, &BudgetState::Unknown, slot.t_start).unwrap();
    assert_eq!(unknown.export_eur_per_kwh, 0.18);
    assert_eq!(unknown.reason, SettlementReason::SalderingBudgetUnknown);
    assert_eq!(unknown.reason.slug(), "saldering_budget_unknown");
    assert!(unknown.netting);
    let tracked = settle_slot(
        &slot,
        &regime,
        &BudgetState::UntrackedAcknowledged,
        slot.t_start,
    )
    .unwrap();
    assert_eq!(tracked.export_eur_per_kwh, 0.5303);
    assert_eq!(tracked.budget_source, "untracked");
}

#[test]
fn s05_metering_off_prices_export_at_feed_in() {
    let slot = priced("2026-11-23T16:45:00Z", 0.5303, 0.19);
    let regime = override_regime("off", slot.t_start);
    let settled = settle_slot(&slot, &regime, &BudgetState::Unknown, slot.t_start).unwrap();
    assert_eq!(settled.buy_eur_per_kwh, 0.5303);
    assert_eq!(settled.export_eur_per_kwh, 0.19);
    let spread = settled.buy_eur_per_kwh - settled.export_eur_per_kwh;
    assert!((spread - 0.3403).abs() < 1e-12, "spread {spread}");
}

#[test]
fn s63_boundary_slot_22_45_is_netted_and_23_00_is_not() {
    let last = priced("2026-12-31T22:45:00Z", 0.412, 0.19);
    let first = priced("2026-12-31T23:00:00Z", 0.412, 0.19);
    let regime = NetMeteringRegime::DateDriven;
    let budget = BudgetState::Active {
        remaining_kwh: 100.0,
    };
    let before = settle_slot(&last, &regime, &budget, last.t_start).unwrap();
    let after = settle_slot(&first, &regime, &budget, first.t_start).unwrap();
    assert!(before.netting);
    assert_eq!(before.export_eur_per_kwh, 0.412);
    assert!(!after.netting);
    assert_eq!(after.export_eur_per_kwh, 0.19);
}

#[test]
fn s124_boundary_is_exclusive_and_no_slot_straddles_it() {
    let times = [
        "2026-12-31T22:30:00Z",
        "2026-12-31T22:45:00Z",
        "2026-12-31T23:00:00Z",
        "2026-12-31T23:15:00Z",
    ];
    let expected = [true, true, false, false];
    let regime = NetMeteringRegime::DateDriven;
    for (time, netted) in times.iter().zip(expected) {
        let slot = priced(time, 0.412, 0.19);
        let settled = settle_slot(&slot, &regime, &BudgetState::Unknown, slot.t_start).unwrap();
        assert_eq!(settled.netting, netted, "slot {time}");
    }
    let boundary = ts("2026-12-31T23:00:00Z");
    let amsterdam = chrono::DateTime::from_timestamp(boundary, 0)
        .unwrap()
        .with_timezone(&chrono_tz::Europe::Amsterdam);
    assert_eq!(
        amsterdam.format("%Y-%m-%dT%H:%M").to_string(),
        "2027-01-01T00:00"
    );
}

#[test]
fn s64_unknown_regime_refuses_without_keeping_raw_value() {
    let refusal = NetMeteringRegime::from_config("maybe", ts("2026-11-22T15:47:00Z"), None)
        .err()
        .unwrap();
    assert_eq!(refusal, HorizonRefusal::UnknownRegime);
    assert!(!format!("{refusal:?}").contains("maybe"));
    assert_eq!(refusal.reason_slug(), "UnknownRegime");
}

#[test]
fn s65_budget_states_have_distinct_reasons_and_signed_prices() {
    let active = priced("2026-11-23T16:45:00Z", 0.5303, 0.19);
    let cases = [
        (
            BudgetState::Unknown,
            SettlementReason::SalderingBudgetUnknown,
        ),
        (BudgetState::Zero, SettlementReason::SalderingBudgetZero),
        (
            BudgetState::Exhausted,
            SettlementReason::SalderingBudgetExhausted,
        ),
    ];
    for (budget, reason) in cases {
        let settled = settle_slot(
            &active,
            &NetMeteringRegime::DateDriven,
            &budget,
            active.t_start,
        )
        .unwrap();
        assert_eq!(settled.export_eur_per_kwh, 0.19);
        assert_eq!(settled.reason, reason);
    }
    let tracked = settle_slot(
        &active,
        &NetMeteringRegime::DateDriven,
        &BudgetState::UntrackedAcknowledged,
        active.t_start,
    )
    .unwrap();
    assert_eq!(tracked.export_eur_per_kwh, 0.5303);
    assert_eq!(tracked.budget_source, "untracked");
}

#[test]
fn s127_negative_feed_in_is_never_clamped_and_acknowledgement_is_regime_scoped() {
    let at_night = priced("2026-12-31T23:00:00Z", 0.5303, -0.01);
    let budgets = [
        BudgetState::Unknown,
        BudgetState::Zero,
        BudgetState::Exhausted,
    ];
    for budget in budgets {
        let settled = settle_slot(
            &at_night,
            &NetMeteringRegime::DateDriven,
            &budget,
            at_night.t_start,
        )
        .unwrap();
        assert_eq!(settled.export_eur_per_kwh, -0.01);
    }
    let tracked = settle_slot(
        &at_night,
        &NetMeteringRegime::DateDriven,
        &BudgetState::UntrackedAcknowledged,
        at_night.t_start,
    )
    .unwrap();
    assert_eq!(tracked.export_eur_per_kwh, -0.01);
    assert!(!tracked.netting);
    let at_day = priced("2026-11-23T16:45:00Z", 0.5303, -0.01);
    let tracked = settle_slot(
        &at_day,
        &NetMeteringRegime::DateDriven,
        &BudgetState::UntrackedAcknowledged,
        at_day.t_start,
    )
    .unwrap();
    assert_eq!(tracked.export_eur_per_kwh, 0.5303);
}

#[test]
fn s63_explicit_override_needs_full_operator_contract() {
    let now = ts("2026-11-22T15:47:00Z");
    assert_eq!(
        NetMeteringRegime::from_config("on", now, None),
        Err(HorizonRefusal::OverrideMissing)
    );
    let expired = OperatorOverride {
        regime: AcknowledgedRegime::On,
        accepted_by: "operator".to_owned(),
        expires_at: now - 1,
        event: "override-on".to_owned(),
    };
    assert!(matches!(
        NetMeteringRegime::from_config("on", now, Some(expired)),
        Err(HorizonRefusal::OverrideExpired { .. })
    ));
}

#[test]
fn m1b_acknowledgement_must_be_complete_and_regime_bound() {
    let now = ts("2026-11-22T15:47:00Z");
    let base = OperatorOverride {
        regime: AcknowledgedRegime::On,
        accepted_by: "operator".to_owned(),
        expires_at: now + 900,
        event: "override-on".to_owned(),
    };
    let mut empty_by = base.clone();
    empty_by.accepted_by = "  ".to_owned();
    let mut empty_event = base.clone();
    empty_event.event = String::new();
    let mismatched = OperatorOverride {
        regime: AcknowledgedRegime::Off,
        ..base.clone()
    };
    let expired = OperatorOverride {
        expires_at: now - 1,
        ..base.clone()
    };
    assert_eq!(
        NetMeteringRegime::from_config("on", now, None),
        Err(HorizonRefusal::OverrideMissing)
    );
    assert_eq!(
        NetMeteringRegime::from_config("on", now, Some(expired)),
        Err(HorizonRefusal::OverrideExpired {
            expires_at: now - 1
        })
    );
    assert_eq!(
        NetMeteringRegime::from_config("on", now, Some(empty_by)),
        Err(HorizonRefusal::OverrideIncomplete)
    );
    assert_eq!(
        NetMeteringRegime::from_config("on", now, Some(empty_event)),
        Err(HorizonRefusal::OverrideIncomplete)
    );
    assert_eq!(
        NetMeteringRegime::from_config("on", now, Some(mismatched)),
        Err(HorizonRefusal::OverrideMismatch)
    );
    assert!(matches!(
        NetMeteringRegime::from_config("on", now, Some(base)),
        Ok(NetMeteringRegime::On(_))
    ));
}

#[test]
fn m1b_crossing_the_regime_boundary_is_refused() {
    let crossing = price("2026-12-31T22:52:30Z", 0.412);
    let regime = NetMeteringRegime::DateDriven;
    let refusal = build_schedule(
        &[crossing],
        &regime,
        &BudgetState::Unknown,
        crossing.t_start,
    )
    .err()
    .unwrap();
    let boundary = netting_boundary_utc();
    assert_eq!(boundary, ts("2026-12-31T23:00:00Z"));
    assert_eq!(
        refusal,
        HorizonRefusal::RegimeBoundaryCrossing {
            t_start: ts("2026-12-31T22:52:30Z"),
            t_end: ts("2026-12-31T23:07:30Z"),
            boundary,
        }
    );
}

fn priced(start: &str, buy: f64, feed_in: f64) -> PriceSlot {
    let mut slot = price(start, buy);
    slot.feed_in_post = feed_in;
    slot
}

fn override_regime(raw: &str, now: i64) -> NetMeteringRegime {
    let operator = OperatorOverride {
        regime: if raw.eq_ignore_ascii_case("off") {
            AcknowledgedRegime::Off
        } else {
            AcknowledgedRegime::On
        },
        accepted_by: "operator".to_owned(),
        expires_at: now + 86_400,
        event: "override".to_owned(),
    };
    NetMeteringRegime::from_config(raw, now + 1, Some(operator)).unwrap()
}
