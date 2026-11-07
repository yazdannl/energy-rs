//! Pure run detection: turn a window of samples into candidate sessions.
//!
//! The scan is deterministic and independent of the database, so the rules are unit-testable.
//! Coverage never claims the interval before a run's first sample; a run's trailing edge extends to
//! the next contiguous sample when that sample is a hold or the opposite direction, and otherwise
//! to its last sample plus one nominal interval (an unbounded edge, which makes the run
//! incomplete). A one-sample opposite-direction blip between two runs of the same direction flags
//! both neighbours as a `direction_reversal` instead of silently booking two truncated sessions.

use crate::session::{Direction, SessionRules};

/// One sample as read for extraction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScanSample {
    pub(crate) ts: i64,
    pub(crate) power_w: f64,
    pub(crate) soc_pct: Option<f64>,
}

/// Result of scanning a whole window.
pub(crate) struct Scan {
    pub(crate) runs: Vec<Run>,
    pub(crate) skipped: usize,
}

/// A candidate session (before it is stored).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Run {
    pub(crate) direction: Direction,
    pub(crate) start_ts: i64,
    pub(crate) end_ts: i64,
    pub(crate) sample_count: usize,
    pub(crate) energy_ac_kwh: f64,
    pub(crate) soc_start: Option<f64>,
    pub(crate) soc_end: Option<f64>,
    pub(crate) complete: bool,
    pub(crate) invalidated_reason: Option<&'static str>,
    pub(crate) first: usize,
    pub(crate) last: usize,
}

/// Scan `samples` (inside the window) with one sample of context on each side.
pub(crate) fn scan(
    samples: &[ScanSample],
    before: Option<&ScanSample>,
    after: Option<&ScanSample>,
    rules: &SessionRules,
    interval_s: i64,
) -> Scan {
    let max_gap = rules.max_gap_s(interval_s);
    let mut runs = Vec::new();
    let mut skipped = 0_usize;
    let mut start = 0_usize;
    while start < samples.len() {
        let mut end = start + 1;
        while end < samples.len() && samples[end].ts - samples[end - 1].ts <= max_gap {
            end += 1;
        }
        let left = if start == 0 {
            before.filter(|sample| samples[0].ts - sample.ts <= max_gap)
        } else {
            None
        };
        let right = if end == samples.len() {
            after.filter(|sample| sample.ts - samples[end - 1].ts <= max_gap)
        } else {
            None
        };
        let mut segment = segment_runs(
            &samples[start..end],
            left,
            right,
            rules,
            interval_s,
            max_gap,
        );
        mark_reversals(&mut segment, rules.min_samples);
        skipped += segment
            .iter()
            .filter(|run| run.sample_count < rules.min_samples)
            .count();
        segment.retain(|run| run.sample_count >= rules.min_samples);
        runs.append(&mut segment);
        start = end;
    }
    Scan { runs, skipped }
}

fn segment_runs(
    segment: &[ScanSample],
    left: Option<&ScanSample>,
    right: Option<&ScanSample>,
    rules: &SessionRules,
    interval_s: i64,
    max_gap: i64,
) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut index = 0_usize;
    while index < segment.len() {
        let Some(direction) = Direction::of(segment[index].power_w, rules.deadband_w) else {
            index += 1;
            continue;
        };
        let mut end = index + 1;
        while end < segment.len()
            && Direction::of(segment[end].power_w, rules.deadband_w) == Some(direction)
        {
            end += 1;
        }
        let left_neighbor = if index == 0 {
            left
        } else {
            Some(&segment[index - 1])
        };
        let right_neighbor = if end == segment.len() {
            right
        } else {
            Some(&segment[end])
        };
        let left_bounded = left_neighbor.is_some_and(|sample| {
            segment[index].ts - sample.ts <= max_gap
                && Direction::of(sample.power_w, rules.deadband_w) != Some(direction)
        });
        let (end_ts, right_bounded) = trailing_edge(
            &segment[index..end],
            right_neighbor,
            direction,
            rules.deadband_w,
            interval_s,
            max_gap,
        );
        runs.push(build_run(
            &segment[index..end],
            direction,
            end_ts,
            left_bounded,
            right_bounded,
            (index, end - 1),
        ));
        index = end;
    }
    runs
}

fn trailing_edge(
    run: &[ScanSample],
    right: Option<&ScanSample>,
    direction: Direction,
    deadband_w: f64,
    interval_s: i64,
    max_gap: i64,
) -> (i64, bool) {
    let last_ts = run[run.len() - 1].ts;
    match right {
        Some(sample)
            if sample.ts - last_ts <= max_gap
                && Direction::of(sample.power_w, deadband_w) != Some(direction) =>
        {
            (sample.ts, true)
        }
        _ => (last_ts + interval_s, false),
    }
}

/// Build one run; coverage runs from the first sample to `end_ts` (exclusive).
fn build_run(
    run: &[ScanSample],
    direction: Direction,
    end_ts: i64,
    left_bounded: bool,
    right_bounded: bool,
    span: (usize, usize),
) -> Run {
    let start_ts = run[0].ts;
    let duration_s = end_ts - start_ts;
    let mean_power_w = run.iter().map(|sample| sample.power_w).sum::<f64>() / run.len() as f64;
    let energy_ac_kwh = mean_power_w.abs() * duration_s as f64 / 3_600_000.0;
    let soc_gap = run.iter().any(|sample| sample.soc_pct.is_none());
    Run {
        direction,
        start_ts,
        end_ts,
        sample_count: run.len(),
        energy_ac_kwh,
        soc_start: run[0].soc_pct,
        soc_end: run[run.len() - 1].soc_pct,
        complete: left_bounded && right_bounded && !soc_gap,
        invalidated_reason: soc_gap.then_some("soc_gap"),
        first: span.0,
        last: span.1,
    }
}

/// Flag the two neighbours of a one-sample opposite-direction blip.
fn mark_reversals(runs: &mut [Run], min_samples: usize) {
    let mut flagged = Vec::new();
    for index in 1..runs.len().saturating_sub(1) {
        let blip = runs[index].sample_count < min_samples
            && runs[index].first == runs[index - 1].last + 1
            && runs[index + 1].first == runs[index].last + 1
            && runs[index - 1].direction == runs[index + 1].direction
            && runs[index].direction != runs[index - 1].direction;
        if blip {
            flagged.push(index - 1);
            flagged.push(index + 1);
        }
    }
    for index in flagged {
        runs[index].complete = false;
        if runs[index].invalidated_reason.is_none() {
            runs[index].invalidated_reason = Some("direction_reversal");
        }
    }
}
