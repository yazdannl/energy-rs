//! UTC and local-clock helpers: slot grids, hour floors and the local bucket key.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use chrono_tz::Tz;

/// Slot length of the served forecast, minutes.
pub const SLOT_MINUTES: u32 = 15;

/// Floor an instant to the start of its UTC hour.
pub fn hour_floor(moment: DateTime<Utc>) -> DateTime<Utc> {
    let zeroed = moment.with_minute(0).and_then(|value| value.with_second(0));
    zeroed
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(moment)
}

/// Floor an instant down to the previous quarter-hour boundary.
pub fn quarter_floor(moment: DateTime<Utc>) -> DateTime<Utc> {
    let minutes = moment.minute();
    let floored = minutes - minutes % SLOT_MINUTES;
    let hour_aligned = moment
        .with_minute(floored)
        .and_then(|value| value.with_second(0));
    hour_aligned
        .and_then(|value| value.with_nanosecond(0))
        .unwrap_or(moment)
}

/// Every `step_minutes` slot in `[start, end]` (both ends inclusive, like `slot_range`).
pub fn slot_grid(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_minutes: i64,
) -> Vec<DateTime<Utc>> {
    let mut slots = Vec::new();
    let step = Duration::minutes(step_minutes.max(1));
    let mut cursor = start;
    while cursor <= end {
        slots.push(cursor);
        cursor += step;
    }
    slots
}

/// Every `step_minutes` slot in `[start, end)` (`start` floored to the slot boundary), matching
/// Python's `slot_range`.
pub fn slot_range(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_minutes: i64,
) -> Vec<DateTime<Utc>> {
    if step_minutes <= 0 || end <= start {
        return Vec::new();
    }
    let mut slots = Vec::new();
    let step = Duration::minutes(step_minutes);
    let mut cursor = quarter_floor(start);
    while cursor < end {
        slots.push(cursor);
        cursor += step;
    }
    slots
}

/// Every whole UTC hour in `[first, last]` (both ends inclusive).
pub fn hour_grid(first: DateTime<Utc>, last: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    slot_grid(first, last, 60)
}

/// Wall-clock hour in `zone` for one instant.
pub fn local_hour(moment: DateTime<Utc>, zone: Tz) -> u32 {
    moment.with_timezone(&zone).hour()
}

/// Local calendar date in `zone` for one instant.
pub fn local_date(moment: DateTime<Utc>, zone: Tz) -> NaiveDate {
    moment.with_timezone(&zone).date_naive()
}

/// The 48-bucket index: `0..24` weekday, `24..48` weekend, in local time.
pub fn weekday_hour_bucket(moment: DateTime<Utc>, zone: Tz) -> usize {
    let local = moment.with_timezone(&zone);
    let offset = if local.weekday().num_days_from_monday() >= 5 {
        24
    } else {
        0
    };
    offset + local.hour() as usize
}

/// Parse a RFC 3339 / ISO-8601 timestamp as UTC.
pub fn parse_iso_utc(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

/// UTC ISO-8601 with second precision, matching Python's `isoformat(timespec="seconds")`.
pub fn iso_seconds(moment: DateTime<Utc>) -> String {
    moment.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}
