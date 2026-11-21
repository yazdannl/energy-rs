// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::unreadable_literal
)]

mod support;

use energy_battery::{
    CapacityBounds, CapacityPool, CapacitySource, Direction, LearningProfile, LearningReport,
    SessionRecord,
};
use support::learning::{LABEL_KWH, SPLIT, at, bounds, session};

/// The four serials and session counts from the owner's report.
const PACKS: [&str; 4] = [
    "EOC1NLN9N464770",
    "EOC1NLN9N465216",
    "EOC1NLNAN528963",
    "EOC1NLN9N465846",
];
const COUNTS: [usize; 4] = [28, 24, 26, 26];
const NOW_HOURS: i64 = 400;

/// Deterministic noise in `[-1, 1]`: no rand dependency and identical runs for every gate.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64 / (f64::from(u32::MAX) / 2.0)) - 1.0
    }
}

/// Sessions whose true endpoints are fractional but whose BMS endpoints are whole percent, with a
/// small independent AC-energy error — the two biases called out in the diagnosis.
fn sessions(index: usize, count: usize, capacity: f64, long_every: usize) -> Vec<SessionRecord> {
    let mut noise =
        Noise(0x9E37_79B9_7F4A_7C15 ^ (index as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
    let mut result = Vec::with_capacity(count);
    for run in 0..count {
        let long = long_every > 0 && run % long_every == 0;
        let (centre_start, centre_end) = if long { (12.0, 78.0) } else { (31.0, 40.0) };
        let true_start = centre_start + 0.45 * noise.next();
        let true_end = centre_end + 0.45 * noise.next();
        let ac_kwh =
            (true_end - true_start) / 100.0 * capacity / SPLIT * (1.0 + 0.006 * noise.next());
        let mut record = session(
            Direction::Charge,
            true_start.round(),
            true_end.round(),
            ac_kwh,
            at((index * 100 + run * 2) as i64),
        );
        PACKS[index % PACKS.len()].clone_into(&mut record.pack_id);
        result.push(record);
    }
    result
}

/// Pool the four observed packs and learn each one against that shared prior.
fn learn_fleet(packs: &[(&str, Vec<SessionRecord>)]) -> Vec<LearningReport> {
    let peers: Vec<(f64, &[SessionRecord])> = packs
        .iter()
        .map(|(_, sessions)| (LABEL_KWH, sessions.as_slice()))
        .collect();
    let pool = CapacityPool::from_sessions(&peers, at(NOW_HOURS));
    packs
        .iter()
        .map(|(id, sessions)| {
            LearningReport::learn_with_pool(
                id,
                sessions,
                &bounds(None),
                None,
                Some(&pool),
                at(NOW_HOURS),
            )
        })
        .collect()
}

#[test]
fn four_identical_packs_agree_and_stay_near_the_label() {
    let packs: Vec<(String, Vec<SessionRecord>)> = (0..4)
        .map(|index| {
            (
                PACKS[index].to_owned(),
                sessions(index, COUNTS[index], LABEL_KWH, 4),
            )
        })
        .collect();
    let reports = learn_fleet(
        &packs
            .iter()
            .map(|(id, sessions)| (id.as_str(), sessions.clone()))
            .collect::<Vec<_>>(),
    );
    let values: Vec<f64> = reports
        .iter()
        .map(|report| report.capacity.applied_kwh.unwrap())
        .collect();
    let lowest = values.iter().copied().reduce(f64::min).unwrap();
    let highest = values.iter().copied().reduce(f64::max).unwrap();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    for (index, (report, value)) in reports.iter().zip(&values).enumerate() {
        println!(
            "{}: published {:.6}, raw {:.6}, confidence {:.3}, cycles {}, points {:.0}, source {}",
            PACKS[index],
            value,
            report.capacity.raw_kwh.unwrap(),
            report.capacity.confidence,
            report.capacity.cycles_used,
            report.capacity.soc_points,
            report.capacity.source.slug(),
        );
        assert!((value - LABEL_KWH).abs() / LABEL_KWH < 0.05, "{report}");
        assert!(report.capacity.used <= COUNTS[index]);
        assert_eq!(report.capacity.cycles_used, report.capacity.used);
        assert!(report.capacity.soc_points > 300.0, "{report}");
        let profile = LearningProfile::from_report(report);
        assert_eq!(profile.cycles_used, report.capacity.used);
        assert!((profile.soc_points_covered - report.capacity.soc_points).abs() < 1e-9);
        let decoded: LearningProfile =
            serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
        assert_eq!(decoded.capacity_source, profile.capacity_source);
        assert_eq!(decoded.cycles_used, profile.cycles_used);
        assert!((decoded.soc_points_covered - profile.soc_points_covered).abs() < 1e-9);
        assert!((decoded.capacity_kwh.unwrap() - profile.capacity_kwh.unwrap()).abs() < 1e-12);
    }
    assert!(
        (highest - lowest) / mean < 0.02,
        "spread {lowest}..{highest}"
    );
}

#[test]
fn genuine_fifteen_percent_aging_still_publishes_near_1_63() {
    let mut packs: Vec<(String, Vec<SessionRecord>)> = (0..3)
        .map(|index| {
            (
                PACKS[index].to_owned(),
                sessions(index, COUNTS[index], LABEL_KWH, 4),
            )
        })
        .collect();
    let aged_kwh = LABEL_KWH * 0.85;
    packs.push((
        PACKS[3].to_owned(),
        sessions(3, 26, aged_kwh, 1), // long spans: strong evidence of real aging
    ));
    let reports = learn_fleet(
        &packs
            .iter()
            .map(|(id, sessions)| (id.as_str(), sessions.clone()))
            .collect::<Vec<_>>(),
    );
    let aged = reports.last().unwrap();
    let published = aged.capacity.applied_kwh.unwrap();
    println!(
        "aged: published {published:.6}, raw {:.6}, confidence {:.3}, band {}..{}, source {}",
        aged.capacity.raw_kwh.unwrap(),
        aged.capacity.confidence,
        aged.capacity.band_low_kwh,
        aged.capacity.band_high_kwh,
        aged.capacity.source.slug(),
    );
    assert!((published - aged_kwh).abs() < 0.05, "{aged}");
    assert!(published < LABEL_KWH * 0.9, "{aged}");
    assert!(aged.capacity.band_low_kwh < LABEL_KWH * 0.9, "{aged}");
    assert_eq!(aged.capacity.source, CapacitySource::Own);
    assert!(aged.capacity.confidence > 0.8, "{aged}");
}

#[test]
fn confidence_tracks_covered_soc_not_session_count() {
    let short = LearningReport::learn(
        PACKS[0],
        &sessions(0, 4, LABEL_KWH, 0),
        &bounds(None),
        None,
        at(NOW_HOURS),
    );
    let long = LearningReport::learn(
        PACKS[1],
        &sessions(1, 6, LABEL_KWH, 1),
        &bounds(None),
        None,
        at(NOW_HOURS),
    );
    assert!(short.capacity.confidence < 0.15, "{short}");
    assert!(
        long.capacity.confidence > short.capacity.confidence * 3.0,
        "{long}"
    );
    assert!(long.capacity.soc_points > short.capacity.soc_points * 3.0);
    assert_eq!(short.capacity.source, CapacitySource::Label);
    assert_eq!(long.capacity.source, CapacitySource::Own);
    // More of the same short cycles raises confidence a little, never to certainty.
    let many_short = LearningReport::learn(
        PACKS[2],
        &sessions(2, 28, LABEL_KWH, 0),
        &bounds(None),
        None,
        at(NOW_HOURS),
    );
    assert!(many_short.capacity.confidence < 0.4, "{many_short}");
    assert!(many_short.capacity.confidence > short.capacity.confidence);
    assert!(many_short.capacity.soc_points > short.capacity.soc_points);
}

#[test]
fn a_reported_capacity_is_never_exceeded() {
    let reported = CapacityBounds::new(LABEL_KWH, Some(1.80));
    let report = LearningReport::learn_with_pool(
        PACKS[0],
        &sessions(0, 26, 2.2, 1),
        &reported,
        None,
        None,
        at(NOW_HOURS),
    );
    let published = report.capacity.applied_kwh.unwrap();
    assert!(published <= 1.80, "{report}");
    assert_eq!(report.capacity.band_high_kwh, 1.80);
}

#[test]
fn a_low_information_pack_uses_the_pooled_prior() {
    let peers: Vec<(String, Vec<SessionRecord>)> = (0..4)
        .map(|index| {
            (
                PACKS[index].to_owned(),
                sessions(index, COUNTS[index], LABEL_KWH, 4),
            )
        })
        .collect();
    let pool = CapacityPool::from_sessions(
        &peers
            .iter()
            .map(|(_, rows)| (LABEL_KWH, rows.as_slice()))
            .collect::<Vec<_>>(),
        at(NOW_HOURS),
    );
    assert_eq!(pool.packs_for_label(LABEL_KWH), 4);
    assert!(pool.for_label(1.5).is_none());
    let thin = LearningReport::learn_with_pool(
        PACKS[0],
        &sessions(0, 3, 2.3, 0),
        &bounds(None),
        None,
        Some(&pool),
        at(NOW_HOURS),
    );
    assert_eq!(thin.capacity.source, CapacitySource::Pooled);
    assert!(
        (thin.capacity.applied_kwh.unwrap() - pool.for_label(LABEL_KWH).unwrap().0).abs() < 0.01
    );
}
