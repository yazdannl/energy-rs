//! Parse the raw Zonneplan chart payload (`chart.series.prices`) into a series.  The gas tariff is
//! the same fixed-point shape, so only a regular 15/60-minute grid with at least two entries is
//! accepted; everything else is refused as a decoy.

use serde_json::Value;

use crate::error::PriceError;
use crate::provider::parse::{ParsedSeries, RawSlot, amount_eur, parse_timestamp};

/// Chart names (prices.md §1.1); the quarter-hourly series is the settlement grid.
pub(crate) const CHART_QUARTER: &str = "electricity-quarter-hourly";
/// Legacy hourly chart used when the quarter-hourly series is unavailable.
pub(crate) const CHART_HOUR: &str = "electricity-hourly";

/// Parse one chart payload, refusing non-electricity grids.
pub fn parse_chart(payload: &Value) -> Result<ParsedSeries, PriceError> {
    let prices = payload
        .pointer("/chart/series/prices")
        .and_then(Value::as_array)
        .ok_or_else(|| PriceError::Payload("chart.series.prices is missing".to_owned()))?;
    if prices.len() < 2 {
        return Err(PriceError::GasDecoy);
    }
    let mut raw = Vec::with_capacity(prices.len());
    let mut step: Option<i64> = None;
    for entry in prices {
        let (start, end, buy_all_in) = parse_entry(entry)?;
        let duration = end - start;
        if duration <= 0 {
            return Err(PriceError::Payload(format!(
                "price entry {start} has a non-positive duration"
            )));
        }
        match step {
            None => step = Some(duration),
            Some(expected) if expected != duration => {
                return Err(PriceError::Payload(format!(
                    "mixed price grid: {expected}s and {duration}s"
                )));
            }
            Some(_) => {}
        }
        raw.push(RawSlot {
            t_start: start,
            buy_all_in,
        });
    }
    let step = step.ok_or(PriceError::GasDecoy)?;
    if !matches!(step, 900 | 3600) {
        return Err(PriceError::GasDecoy);
    }
    Ok(ParsedSeries { raw, step })
}

fn parse_entry(entry: &Value) -> Result<(i64, i64, f64), PriceError> {
    let start_text = entry
        .get("start_date")
        .and_then(Value::as_str)
        .ok_or_else(|| PriceError::Payload("price entry has no start_date".to_owned()))?;
    let end_text = entry
        .get("end_date")
        .and_then(Value::as_str)
        .ok_or_else(|| PriceError::Payload("price entry has no end_date".to_owned()))?;
    let start = parse_timestamp(start_text)
        .ok_or_else(|| PriceError::Payload(format!("bad start_date {start_text}")))?;
    let end = parse_timestamp(end_text)
        .ok_or_else(|| PriceError::Payload(format!("bad end_date {end_text}")))?;
    let buy_all_in = entry
        .pointer("/price_tax_included/amount")
        .and_then(amount_eur)
        .ok_or_else(|| {
            PriceError::Payload("price entry has no price_tax_included.amount".to_owned())
        })?;
    Ok((start, end, buy_all_in))
}
