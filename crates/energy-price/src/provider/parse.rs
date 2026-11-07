//! Shared ingest helpers: fixed-point amounts (/1e7), tolerant timestamp parsing, range/past
//! filtering and the one place that turns a parsed series into a validated [`PriceBook`] with the
//! market price derived exactly once.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde_json::Value;

use super::ParsedSeries;
use crate::book::query::{local_naive_to_utc, local_year_of};
use crate::book::{PriceBook, PriceQuery, Slot, SlotSource};
use crate::error::PriceError;
use crate::settlement::Contract;

/// Zonneplan/HA fixed-point scale: raw int / 1e7 = EUR/kWh (prices.md §1.2).
pub(crate) const AMOUNT_SCALE: f64 = 1e7;

/// Below this magnitude an amount is already EUR/kWh, not fixed point: a raw amount of 1 000 is
/// 0.0001 EUR/kWh, so the two shapes cannot be confused in practice.
const SCALED_BELOW: f64 = 1_000.0;

/// Which provider parsed a series; determines the stored [`SlotSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Zonneplan app API (`app-api.zonneplan.nl`).
    Zonneplan,
    /// Home Assistant sensor forecast.
    Ha,
}

/// Raw fixed-point amount (or an already-scaled float) to EUR/kWh.
///
/// Both published shapes are accepted: the fixed-point `amount` (raw integer, divide by 1e7) and a
/// sensor that republishes the same value already scaled.  A NaN/infinite amount is refused.
pub(crate) fn amount_eur(value: &Value) -> Option<f64> {
    let amount = value.as_f64()?;
    if !amount.is_finite() {
        return None;
    }
    Some(if amount.abs() < SCALED_BELOW {
        amount
    } else {
        amount / AMOUNT_SCALE
    })
}

/// Parse RFC3339 offsets first, then fall back to naive Amsterdam local time for HA sensors.
pub(crate) fn parse_timestamp(text: &str) -> Option<i64> {
    if let Ok(with_offset) = DateTime::parse_from_rfc3339(text) {
        return Some(with_offset.timestamp());
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
            return local_naive_to_utc(naive);
        }
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .and_then(local_naive_to_utc)
}

/// The resolution of a payload, mapped to its slot source; anything but 15/60 minutes is a decoy.
fn slot_source(provider: ProviderKind, step: i64) -> Result<SlotSource, PriceError> {
    match (provider, step) {
        (ProviderKind::Zonneplan, 900) => Ok(SlotSource::ZonneplanQuarterHourly),
        (ProviderKind::Zonneplan, 3600) => Ok(SlotSource::ZonneplanHourly),
        (ProviderKind::Ha, 900) => Ok(SlotSource::HaQuarterHourly),
        (ProviderKind::Ha, 3600) => Ok(SlotSource::HaHourly),
        _ => Err(PriceError::GasDecoy),
    }
}

/// Keep only slots inside the requested local dates that are not fully past.
pub(crate) fn filter_series(series: ParsedSeries, query: &PriceQuery) -> ParsedSeries {
    let raw = series
        .raw
        .into_iter()
        .filter(|raw| {
            query.contains(raw.t_start) && raw.t_start.saturating_add(series.step) > query.now()
        })
        .collect();
    ParsedSeries {
        raw,
        step: series.step,
    }
}

/// Derive `market` per slot with the contract constants, round to 1e-6 and validate the grid.
pub(crate) fn build_book(
    series: &ParsedSeries,
    provider: ProviderKind,
    contract: &Contract,
) -> Result<PriceBook, PriceError> {
    let source = slot_source(provider, series.step)?;
    let mut slots = Vec::with_capacity(series.raw.len());
    for raw in &series.raw {
        let year = local_year_of(raw.t_start).ok_or_else(|| {
            PriceError::Payload(format!("slot start {} has no local year", raw.t_start))
        })?;
        let market = contract.market_from_buy(raw.buy_all_in, year)?;
        slots.push(Slot::new(raw.t_start, raw.buy_all_in, market, source).money_rounded());
    }
    PriceBook::new(slots)
}

/// Filter, derive and validate one payload for a query.  An empty result is a refusal, never an
/// empty book: exact-or-refuse on coverage.
///
/// This is also the recorded-fixture entry point: a stored payload is ingested exactly like a live
/// one, so parser and grid tests need no network.
pub fn ingest(
    series: ParsedSeries,
    provider: ProviderKind,
    contract: &Contract,
    query: &PriceQuery,
) -> Result<PriceBook, PriceError> {
    let filtered = filter_series(series, query);
    if filtered.raw.is_empty() {
        return Err(PriceError::NotPublished {
            from: query.from(),
            to: query.to(),
        });
    }
    build_book(&filtered, provider, contract)
}
