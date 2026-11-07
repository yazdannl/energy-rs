//! Shared ingest helpers: fixed-point amounts (/1e7), tolerant timestamp parsing, range/past
//! filtering and the one place that turns a parsed series into a validated [`PriceBook`] with the
//! market price derived exactly once.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde_json::Value;

use crate::book::query::{local_naive_to_utc, local_year_of};
use crate::book::{PriceBook, PriceQuery, Slot, SlotSource};
use crate::error::PriceError;
use crate::settlement::Contract;

/// Zonneplan/HA fixed-point scale: raw int / 1e7 = EUR/kWh (prices.md §1.2).
pub(crate) const AMOUNT_SCALE: f64 = 1e7;

/// Which provider parsed a series; determines the stored [`SlotSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Zonneplan,
    Ha,
}

/// One parsed price before validation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawSlot {
    pub(crate) t_start: i64,
    pub(crate) buy_all_in: f64,
}

/// Parsed entries plus the resolution the payload advertises.
#[derive(Debug, Clone)]
pub(crate) struct ParsedSeries {
    pub(crate) raw: Vec<RawSlot>,
    pub(crate) step: i64,
}

/// Raw fixed-point amount (or an already-scaled float) to EUR/kWh.
pub(crate) fn amount_eur(value: &Value) -> Option<f64> {
    value.as_f64().map(|amount| amount / AMOUNT_SCALE)
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
    series: ParsedSeries,
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
pub(crate) fn ingest(
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
    build_book(filtered, provider, contract)
}
