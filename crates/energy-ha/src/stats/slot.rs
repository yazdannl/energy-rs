//! Aligned energy slots built from statistics `change` rows.

use std::collections::BTreeMap;

use super::{Granularity, StatRow};

/// One aligned slot; `kwh: None` is an explicit gap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub start_ms: i64,
    pub end_ms: i64,
    pub kwh: Option<f64>,
}

impl Slot {
    pub fn is_gap(&self) -> bool {
        self.kwh.is_none()
    }
}

/// Build aligned slots from raw statistics rows.
///
/// The first row is always discarded: the caller must have requested one warm-up bucket before
/// `from_ms`. A missing, `None` or negative `change` yields a gap. Quarter-hour slots need all
/// three 5-minute buckets; a partial slot is a gap, not a partial sum.
pub fn parse_slots(
    rows: &[StatRow],
    from_ms: i64,
    to_ms: i64,
    granularity: Granularity,
) -> Vec<Slot> {
    let from = align_down(from_ms, granularity.millis());
    let to = align_down(to_ms, granularity.millis());
    if to <= from {
        return Vec::new();
    }

    let mut trusted = trusted_changes(rows, from, to);
    let step = granularity.millis();
    let mut slots = Vec::new();
    let mut start = from;
    while start < to {
        let kwh = match granularity {
            Granularity::FiveMinute | Granularity::Hour => trusted.remove(&start),
            Granularity::QuarterHour => sum_quarter_hour(&mut trusted, start),
        };
        slots.push(Slot {
            start_ms: start,
            end_ms: start + step,
            kwh,
        });
        start += step;
    }
    slots
}

fn trusted_changes(rows: &[StatRow], from: i64, to: i64) -> BTreeMap<i64, f64> {
    let mut trusted = BTreeMap::new();
    for row in rows.iter().skip(1) {
        if row.start < from || row.start >= to {
            continue;
        }
        if let Some(change) = row.change
            && change.is_finite()
            && change >= 0.0
        {
            trusted.insert(row.start, change);
        }
    }
    trusted
}

fn sum_quarter_hour(trusted: &mut BTreeMap<i64, f64>, start: i64) -> Option<f64> {
    let mut sum = 0.0;
    for index in 0..3 {
        sum += trusted.remove(&(start + index * 5 * 60 * 1_000))?;
    }
    Some(sum)
}

pub(crate) fn align_down(millis: i64, step: i64) -> i64 {
    millis.div_euclid(step) * step
}
