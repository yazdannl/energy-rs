//! Parse the Zonneplan integration's HA sensor: the `forecast` attribute in either the
//! quarter-hourly shape (`price_tax_included.amount`) or the legacy hourly shape
//! (`electricity_price`).  Both shapes also publish the price without the energy tax
//! (`price_tax_excluded.amount`, `electricity_price_excl_tax`), which is the feed-in price after
//! net metering ends.  The gas sensor publishes the same shape, so the unit and the grid are the
//! decoy guards (prices.md §1.3).

use serde_json::Value;

use crate::error::PriceError;
use crate::provider::parse::{amount_eur, parse_timestamp};
use crate::provider::{ParsedSeries, RawSlot};

/// One forecast entry: start, optional end, all-in price, optional price without the energy tax.
type Entry = (i64, Option<i64>, f64, Option<f64>);

/// Parse `GET /api/states/<sensor>` for a Zonneplan electricity tariff sensor.
pub fn parse_state(payload: &Value) -> Result<ParsedSeries, PriceError> {
    let state = payload
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(state, "unavailable" | "unknown") {
        return Err(PriceError::Payload(
            "price sensor is unavailable".to_owned(),
        ));
    }
    let attributes = payload
        .get("attributes")
        .ok_or_else(|| PriceError::Payload("sensor has no attributes".to_owned()))?;
    check_unit(
        attributes
            .get("unit_of_measurement")
            .and_then(Value::as_str),
    )?;
    let forecast = attributes
        .get("forecast")
        .and_then(Value::as_array)
        .ok_or_else(|| PriceError::Payload("sensor has no forecast attribute".to_owned()))?;
    if forecast.len() < 2 {
        return Err(PriceError::GasDecoy);
    }
    let mut entries: Vec<Entry> = Vec::with_capacity(forecast.len());
    for entry in forecast {
        if let Some(parsed) = parse_entry(entry)? {
            entries.push(parsed);
        }
    }
    if entries.len() < 2 {
        return Err(PriceError::Payload(
            "forecast entries lack start timestamps".to_owned(),
        ));
    }
    entries.sort_by_key(|entry| entry.0);
    let step = infer_step(&entries).ok_or(PriceError::GasDecoy)?;
    if !matches!(step, 900 | 3600) {
        return Err(PriceError::GasDecoy);
    }
    let raw = entries
        .into_iter()
        .map(|(t_start, _, buy_all_in, buy_ex_tax)| RawSlot {
            t_start,
            buy_all_in,
            buy_ex_tax,
        })
        .collect();
    Ok(ParsedSeries { raw, step })
}

fn check_unit(unit: Option<&str>) -> Result<(), PriceError> {
    let Some(unit) = unit else {
        return Ok(());
    };
    let lower = unit.to_lowercase();
    if lower.contains('m') && (lower.contains('³') || lower.contains('3')) {
        return Err(PriceError::GasDecoy);
    }
    if !lower.contains("kwh") {
        return Err(PriceError::Payload(format!(
            "sensor unit {unit} is not EUR/kWh"
        )));
    }
    Ok(())
}

fn parse_entry(entry: &Value) -> Result<Option<Entry>, PriceError> {
    let start_text = entry
        .get("start_date")
        .and_then(Value::as_str)
        .or_else(|| entry.get("datetime").and_then(Value::as_str));
    let Some(start_text) = start_text else {
        return Ok(None);
    };
    let Some(t_start) = parse_timestamp(start_text) else {
        return Ok(None);
    };
    let t_end = entry
        .get("end_date")
        .and_then(Value::as_str)
        .and_then(parse_timestamp);
    let buy_all_in = entry
        .pointer("/price_tax_included/amount")
        .and_then(amount_eur)
        .or_else(|| entry.get("electricity_price").and_then(amount_eur))
        .ok_or_else(|| PriceError::Payload(format!("forecast entry at {t_start} has no price")))?;
    let buy_ex_tax = entry
        .pointer("/price_tax_excluded/amount")
        .and_then(amount_eur)
        .or_else(|| entry.get("electricity_price_excl_tax").and_then(amount_eur));
    Ok(Some((t_start, t_end, buy_all_in, buy_ex_tax)))
}

fn infer_step(entries: &[Entry]) -> Option<i64> {
    for (start, end, _, _) in entries {
        if let Some(end) = end
            && end > start
        {
            return Some(end - start);
        }
    }
    entries.windows(2).find_map(|pair| match pair {
        [first, second] if second.0 > first.0 => Some(second.0 - first.0),
        _ => None,
    })
}
