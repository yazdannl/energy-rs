//! Fetch per-slot kWh series from recorder statistics.

use chrono::{DateTime, Utc};

use crate::client::{Connection, HaError};

use super::{
    Granularity, Slot, StatType, StatisticsQuery, parse_slots, slot::align_down,
    statistics_during_period,
};

/// Fetch a per-slot kWh series for one energy counter.
///
/// `from`/`to` are floored to slot boundaries; only complete slots are returned. The recorder is
/// queried from one bucket before `from`, and that warm-up row is discarded by [`parse_slots`].
pub async fn slot_series(
    connection: &Connection,
    entity_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    granularity: Granularity,
) -> Result<Vec<Slot>, HaError> {
    let from_ms = align_down(from.timestamp_millis(), granularity.millis());
    let to_ms = align_down(to.timestamp_millis(), granularity.millis());
    if to_ms <= from_ms {
        return Ok(Vec::new());
    }
    let source_period = granularity.source_period();
    let start = timestamp(from_ms - source_period.millis())?;
    let end = timestamp(to_ms)?;
    let query = StatisticsQuery::new(start, Some(end), vec![entity_id.to_owned()], source_period)
        .with_types(vec![StatType::Change])
        .with_unit("energy", "kWh");
    let response = statistics_during_period(connection, &query).await?;
    let rows = response.get(entity_id).map(Vec::as_slice).unwrap_or(&[]);
    Ok(parse_slots(rows, from_ms, to_ms, granularity))
}

fn timestamp(millis: i64) -> Result<DateTime<Utc>, HaError> {
    DateTime::from_timestamp_millis(millis)
        .ok_or_else(|| HaError::Config("slot_series timestamp is out of range".to_owned()))
}

pub async fn slot_series_5min(
    connection: &Connection,
    entity_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Slot>, HaError> {
    slot_series(connection, entity_id, from, to, Granularity::FiveMinute).await
}

pub async fn slot_series_15min(
    connection: &Connection,
    entity_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Slot>, HaError> {
    slot_series(connection, entity_id, from, to, Granularity::QuarterHour).await
}

pub async fn slot_series_hourly(
    connection: &Connection,
    entity_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Slot>, HaError> {
    slot_series(connection, entity_id, from, to, Granularity::Hour).await
}
