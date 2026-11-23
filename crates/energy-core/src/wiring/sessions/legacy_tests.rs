#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;

use super::*;
use crate::wiring::LedgerLearningStore;
use energy_battery::learning::{CapacityBounds, LearningProfile, LearningReport};
use energy_ledger::{Pack, Sample};

#[test]
fn legacy_incomplete_runs_repair_into_multisegment_cycles() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let id = ledger
        .packs()
        .expect("packs")
        .upsert(&Pack::new("SN-LEGACY", 1.92, 0))
        .expect("upsert")
        .id;
    let store = LedgerLearningStore::new(Arc::clone(&ledger));
    let base = 1_700_000_000;
    let report = LearningReport::learn(
        "SN-LEGACY",
        &[],
        &CapacityBounds::new(1.92, None),
        None,
        instant(base - 24 * 3600),
    );
    // The stored profile predates attempt timestamps: the first pass must repair old rows.
    let mut profile = LearningProfile::from_report(&report);
    profile.efficiency_diagnostics.last_attempt_at = None;
    profile.efficiency_diagnostics.last_learning_at = None;
    store.save(&profile).expect("legacy profile");

    let policy = SessionPolicy::new(300, 7 * 24);
    let now = seed_legacy_cycles(&ledger, id, base, &policy);
    let reports = run(&ledger, &store, now, &policy).expect("scheduled relearn");
    assert_eq!(reports.len(), 1, "the repaired cycle must be learned");

    let diagnostics = &reports[0].efficiency.diagnostics;
    assert_eq!(diagnostics.pairs, 3, "{diagnostics:?}");
    assert_eq!(diagnostics.accepted, 3, "{diagnostics:?}");
    assert_eq!(diagnostics.block_splits, 0, "{diagnostics:?}");
    assert!(!diagnostics.reasons.contains_key("session_incomplete"));
    assert!(!diagnostics.reasons.contains_key("session_short"));
    assert_eq!(diagnostics.attempts.len(), 3);
    assert!(
        diagnostics
            .attempts
            .iter()
            .all(|attempt| attempt.charge_segments == 3),
        "each charge must merge all three cheap-window segments: {:?}",
        diagnostics.attempts
    );
    assert!(
        diagnostics.attempts.iter().all(|attempt| {
            attempt
                .round_trip
                .is_some_and(|round_trip| (round_trip - 0.81).abs() < 1e-9)
                && (attempt.soc_from_pct - 20.0).abs() < 1e-9
                && (attempt.soc_to_pct - 20.0).abs() < 1e-9
        }),
        "closed integer-percent cycles must measure 0.81: {:?}",
        diagnostics.attempts
    );
}

/// Three real-shaped days: a three-segment cheap-window charge, a three-hour 100 % hold, one
/// evening discharge, integer-percent SoC. Only the charge part is extracted first, which leaves
/// its last segment `incomplete` exactly as the old one-hour extraction bug did.
fn seed_legacy_cycles(ledger: &Ledger, pack_id: i64, base: i64, policy: &SessionPolicy) -> i64 {
    let mut charge_parts = Vec::new();
    let mut tails = Vec::new();
    let mut charge_ends = Vec::new();
    for day in 0_i64..3 {
        let start = base + day * 24 * 3600;
        let (samples, charge_end) = multisegment_cycle(pack_id, start);
        let split = usize::try_from(charge_end + 1).expect("segment length");
        charge_parts.extend(samples.iter().take(split).cloned());
        tails.extend(samples.into_iter().skip(split));
        charge_ends.push((start, start + (charge_end + 1) * 60));
    }
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&charge_parts)
        .expect("seed charge parts");
    for (start, to) in charge_ends {
        ledger
            .sessions()
            .expect("sessions")
            .extract(pack_id, Tier::OneMinute, start, to, &policy.rules)
            .expect("legacy extraction");
    }
    ledger
        .measurements()
        .expect("measurements")
        .write_many(&tails)
        .expect("seed holds and discharges");
    base + 3 * 24 * 3600 + 6 * 3600
}

fn multisegment_cycle(pack_id: i64, start: i64) -> (Vec<Sample>, i64) {
    let mut samples = Vec::new();
    let mut minute: i64 = 0;
    let mut push = |power_w: f64, soc_pct: i64| {
        samples.push(Sample::new(
            pack_id,
            start + minute * 60,
            power_w,
            Some(soc_pct as f64),
        ));
        minute += 1;
    };
    push(0.0, 20);
    for soc in 20..50 {
        push(1275.0, soc);
    }
    push(0.0, 50);
    for soc in 50..80 {
        push(1275.0, soc);
    }
    push(0.0, 80);
    for soc in 80..=100 {
        push(1275.0, soc);
    }
    // 1 initial hold + 30 + 30 + 21 charge samples + 2 inter-segment holds.
    let charge_end = 83_i64;
    for _ in 0..180 {
        push(0.0, 100);
    }
    for soc in (20..=100).rev() {
        push(-1032.75, soc);
    }
    for _ in 0..10 {
        push(0.0, 20);
    }
    (samples, charge_end)
}
