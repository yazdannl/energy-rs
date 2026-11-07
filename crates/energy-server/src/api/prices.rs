//! `GET /api/v1/prices?from&to` — the published buy, market and feed-in price per slot.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::blocking::with_prices;
use crate::api::dto::{Euro, Timestamp, parse_rfc3339};
use crate::api::error::ApiError;
use crate::api::extract::ApiQuery;
use crate::service::Service;

use energy_price::{ExportSource, Slot};

/// The default display window (one day of quarter-hour slots plus tomorrow).
const DEFAULT_WINDOW_S: i64 = 48 * 3_600;
/// The widest range one request may ask for.
const MAX_WINDOW_S: i64 = 8 * 24 * 3_600;
/// What `from` snaps to when the caller omits it (quarter-hour grid).
const SLOT_S: i64 = 900;

/// Raw query strings; every value is parsed here so a bad one is a typed 400.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PriceParams {
    from: Option<String>,
    to: Option<String>,
}

/// One price slot. Money is EUR/kWh, rounded to the ledger's 1e-6 resolution.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SlotDto {
    start_at: Timestamp,
    end_at: Timestamp,
    buy_eur_per_kwh: Euro,
    market_eur_per_kwh: Euro,
    feed_in_eur_per_kwh: Euro,
    source: &'static str,
}

/// `GET /api/v1/prices` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Prices {
    from: Timestamp,
    to: Timestamp,
    currency: &'static str,
    count: usize,
    slots: Vec<SlotDto>,
}

fn resolve_window(params: &PriceParams) -> Result<(i64, i64), ApiError> {
    let default_from = Utc::now().timestamp().div_euclid(SLOT_S) * SLOT_S;
    let from = match &params.from {
        Some(value) => parse_rfc3339(value)?.timestamp(),
        None => default_from,
    };
    let to = match &params.to {
        Some(value) => parse_rfc3339(value)?.timestamp(),
        None => from.saturating_add(DEFAULT_WINDOW_S),
    };
    if to <= from {
        return Err(ApiError::query("`to` must be later than `from`"));
    }
    if to - from > MAX_WINDOW_S {
        return Err(
            ApiError::query("the price window may span at most 8 days").with_details(
                serde_json::json!({ "maxWindowSeconds": MAX_WINDOW_S, "requestedSeconds": to - from }),
            ),
        );
    }
    Ok((from, to))
}

fn slot_dto(slot: &Slot, feed_in: f64) -> Result<SlotDto, ApiError> {
    Ok(SlotDto {
        start_at: Timestamp::from_epoch(slot.t_start)?,
        end_at: Timestamp::from_epoch(slot.t_end())?,
        buy_eur_per_kwh: Euro::new(slot.buy_all_in),
        market_eur_per_kwh: Euro::new(slot.market),
        feed_in_eur_per_kwh: Euro::new(feed_in),
        source: slot.source.as_str(),
    })
}

fn slots_dto(slots: &[Slot], service: &Service) -> Result<Vec<SlotDto>, ApiError> {
    let settlement = service.settlement();
    slots
        .iter()
        .map(|slot| {
            let feed_in = settlement.export_price(slot, ExportSource::Pv);
            slot_dto(slot, feed_in)
        })
        .collect()
}

/// `GET /api/v1/prices`
pub(crate) async fn prices(
    State(service): State<Arc<Service>>,
    ApiQuery(params): ApiQuery<PriceParams>,
) -> Result<Json<Prices>, ApiError> {
    let (from, to) = resolve_window(&params)?;
    let slots = with_prices(&service, move |store| {
        store
            .load(from, to)
            .map_err(|err| ApiError::internal("load prices", err))
    })
    .await?;
    let count = slots.len();
    let slots = slots_dto(&slots, &service)?;
    Ok(Json(Prices {
        from: Timestamp::from_epoch(from)?,
        to: Timestamp::from_epoch(to)?,
        currency: "EUR",
        count,
        slots,
    }))
}
