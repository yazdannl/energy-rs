//! Long-term statistics coverage for every dashboard source the planner reads.
//!
//! The planner reads 15-minute `change` buckets over 48 hours and refuses a hole. Recorder
//! statistics are the one input a user can silently lose by disabling an entity, so each source is
//! checked at 5-minute (last 24 h) and hourly (last 30 d) resolution, including the warm-up row the
//! parser discards and the unit override Home Assistant applies.

use std::collections::BTreeMap;

use energy_ha::{
    Connection, Period, SiteModel, StatRow, StatType, StatisticMetadata, StatisticsQuery,
    statistics_during_period, statistics_metadata,
};

use crate::diag::check::Check;
use crate::diag::status::Status;

const DAY_S: i64 = 86_400;
const MONTH_S: i64 = 30 * DAY_S;
const WARMUP_RISK: &str = "no warm-up row: the first bucket is discarded";

/// One dashboard statistic to probe: label, entity and whether it is a level (`mean`) series.
struct Series {
    label: String,
    entity: String,
    level: bool,
}

/// Every configured source statistic, deduplicated in reading order.
pub async fn checks(connection: &Connection, site: &SiteModel, now: i64) -> Vec<Check> {
    let series = series(site);
    if series.is_empty() {
        return vec![Check::fail(
            "stats",
            "the dashboard names no source statistic",
        )];
    }
    let ids: Vec<String> = series.iter().map(|series| series.entity.clone()).collect();
    let metadata = match statistics_metadata(connection, &ids).await {
        Ok(metadata) => metadata,
        Err(error) => {
            return vec![Check::warn(
                "stats metadata",
                format!("recorder/get_statistics_metadata failed: {error}"),
            )];
        }
    };
    let metadata = by_id(metadata);
    let mut checks = Vec::new();
    for series in series {
        entity(&mut checks, connection, &metadata, &series, now).await;
    }
    checks
}

/// The two resolutions for one entity, plus its unit note.
async fn entity(
    checks: &mut Vec<Check>,
    connection: &Connection,
    metadata: &BTreeMap<String, StatisticMetadata>,
    series: &Series,
    now: i64,
) {
    if !metadata.contains_key(&series.entity) {
        checks.push(Check::fail(
            format!("stats {}", series.label),
            format!("{} is not a recorder statistic", series.entity),
        ));
        return;
    }
    let unit = unit_note(metadata.get(&series.entity));
    for (resolution, from, step) in [
        ("5min/24h", now - DAY_S, 300_i64),
        ("hour/30d", now - MONTH_S, 3_600_i64),
    ] {
        let period = if step == 300 {
            Period::FiveMinute
        } else {
            Period::Hour
        };
        let window = (from, now, step, period);
        checks.push(window_check(connection, series, window, &unit, resolution).await);
    }
}

/// One window: row count, coverage, warm-up row and unit.
async fn window_check(
    connection: &Connection,
    series: &Series,
    (from, to, step, period): (i64, i64, i64, Period),
    unit: &(bool, String),
    resolution: &str,
) -> Check {
    let name = format!("stats {} {resolution}", series.label);
    let kind = if series.level {
        StatType::Mean
    } else {
        StatType::Change
    };
    let rows = match fetch(connection, &series.entity, from, to, period, kind).await {
        Ok(rows) => rows,
        Err(error) => return Check::fail(name, format!("statistics query failed: {error}")),
    };
    let expected = usize::try_from(((to - from) / step).max(1)).unwrap_or(1);
    let (from_ms, to_ms) = (from.saturating_mul(1_000), to.saturating_mul(1_000));
    let present = rows
        .iter()
        .filter(|row| {
            row.start >= from_ms && row.start < to_ms && value(row, series.level).is_some()
        })
        .count();
    let warmup = rows.first().is_some_and(|row| row.start < from_ms);
    let ratio = present as f64 / expected as f64;
    let status = verdict(ratio, warmup, unit.0);
    let detail = format!(
        "{:.1}% ({present}/{expected}), unit {}, {}",
        ratio * 100.0,
        unit.1,
        if warmup {
            "warm-up row ok"
        } else {
            WARMUP_RISK
        },
    );
    Check {
        name,
        status,
        detail,
    }
}

/// PASS at 100 %, WARN for a partial window, FAIL below half.
fn verdict(ratio: f64, warmup: bool, unit_ok: bool) -> Status {
    if ratio < 0.5 {
        Status::Fail
    } else if ratio < 0.98 || !warmup || !unit_ok {
        Status::Warn
    } else {
        Status::Pass
    }
}

/// The unit of a statistic; a missing unit is a warning, never a silent default.
fn unit_note(meta: Option<&StatisticMetadata>) -> (bool, String) {
    let Some(meta) = meta else {
        return (false, "-".to_owned());
    };
    let unit = meta
        .statistics_unit_of_measurement
        .clone()
        .or_else(|| meta.display_unit_of_measurement.clone());
    let class = meta.unit_class.clone();
    match (unit, class) {
        (Some(unit), Some(class)) => (true, format!("{unit} ({class})")),
        (Some(unit), None) => (true, unit),
        (None, Some(class)) => (false, format!("- ({class})")),
        (None, None) => (false, "-".to_owned()),
    }
}

fn value(row: &StatRow, level: bool) -> Option<f64> {
    let value = if level { row.mean } else { row.change }?;
    value.is_finite().then_some(value)
}

async fn fetch(
    connection: &Connection,
    entity: &str,
    from: i64,
    to: i64,
    period: Period,
    kind: StatType,
) -> Result<Vec<StatRow>, energy_ha::HaError> {
    let start = from.saturating_sub(period.millis());
    let query = StatisticsQuery::new(
        stamp(start)?,
        Some(stamp(to)?),
        vec![entity.to_owned()],
        period,
    )
    .with_types(vec![kind]);
    let response = statistics_during_period(connection, &query).await?;
    Ok(response.get(entity).cloned().unwrap_or_default())
}

fn stamp(epoch: i64) -> Result<chrono::DateTime<chrono::Utc>, energy_ha::HaError> {
    chrono::DateTime::from_timestamp(epoch, 0).ok_or_else(|| {
        energy_ha::HaError::Config("statistics timestamp is out of range".to_owned())
    })
}

fn by_id(metadata: Vec<StatisticMetadata>) -> BTreeMap<String, StatisticMetadata> {
    metadata
        .into_iter()
        .map(|meta| (meta.statistic_id.clone(), meta))
        .collect()
}

/// Grid/solar/battery energy counters and the battery level.
fn series(site: &SiteModel) -> Vec<Series> {
    let mut out: Vec<Series> = Vec::new();
    push(
        &mut out,
        "grid_import".to_owned(),
        site.grid.import.as_deref(),
        false,
    );
    push(
        &mut out,
        "grid_export".to_owned(),
        site.grid.export.as_deref(),
        false,
    );
    push(
        &mut out,
        "solar".to_owned(),
        site.solar.energy.as_deref(),
        false,
    );
    let multiple = site.batteries.len() > 1;
    for (index, battery) in site.batteries.iter().enumerate() {
        let suffix = if multiple {
            format!("_{}", index + 1)
        } else {
            String::new()
        };
        push(
            &mut out,
            format!("battery_charge{suffix}"),
            battery.charge.as_deref(),
            false,
        );
        push(
            &mut out,
            format!("battery_discharge{suffix}"),
            battery.discharge.as_deref(),
            false,
        );
        push(
            &mut out,
            format!("battery_soc{suffix}"),
            battery.soc.as_deref(),
            true,
        );
    }
    out
}

fn push(out: &mut Vec<Series>, label: String, entity: Option<&str>, level: bool) {
    let Some(entity) = entity.filter(|id| !id.trim().is_empty()) else {
        return;
    };
    if out.iter().any(|series| series.entity == entity) {
        return;
    }
    out.push(Series {
        label,
        entity: entity.to_owned(),
        level,
    });
}
