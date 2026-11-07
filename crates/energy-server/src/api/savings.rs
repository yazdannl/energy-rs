//! `GET /api/v1/savings?period=day|month|year|lifetime` — measured savings per local day.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use chrono::{Datelike, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::api::blocking::with_ledger;
use crate::api::dto::Euro;
use crate::api::dto::units::{hours, kwh_to_wh, watt_hours};
use crate::api::error::ApiError;
use crate::api::extract::ApiQuery;
use crate::service::Service;

use energy_ledger::SavingsDaily;

/// Tariff timezone: local days are `Europe/Amsterdam` dates (PLAN.md §2).
const TARIFF_TZ: Tz = chrono_tz::Europe::Amsterdam;

/// Raw query strings.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavingsParams {
    period: Option<String>,
}

/// The rollup window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Period {
    Day,
    Month,
    Year,
    Lifetime,
}

impl Period {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "day" => Some(Self::Day),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            "lifetime" => Some(Self::Lifetime),
            _ => None,
        }
    }

    const fn slug(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
            Self::Lifetime => "lifetime",
        }
    }

    /// Inclusive local-day bounds.
    fn bounds(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let first_of_month = today.with_day(1).unwrap_or(today);
        let first_of_year = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today);
        match self {
            Self::Day => (today, today),
            Self::Month => (first_of_month, today),
            Self::Year => (first_of_year, today),
            Self::Lifetime => (NaiveDate::from_ymd_opt(1970, 1, 1).unwrap_or(today), today),
        }
    }
}

/// One local day of measured accounting.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DayDto {
    day: String,
    actual_eur: Euro,
    baseline_eur: Euro,
    saving_eur: Euro,
    import_wh: f64,
    export_wh: f64,
    pv_wh: f64,
    battery_charge_wh: f64,
    battery_discharge_wh: f64,
    coverage_hours: f64,
    complete: bool,
}

/// Sums over the selected period; energy in Wh.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotalsDto {
    actual_eur: Euro,
    baseline_eur: Euro,
    saving_eur: Euro,
    import_wh: f64,
    export_wh: f64,
    pv_wh: f64,
    battery_charge_wh: f64,
    battery_discharge_wh: f64,
    days: usize,
    complete_days: usize,
}

/// `GET /api/v1/savings` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Savings {
    period: &'static str,
    from_day: String,
    to_day: String,
    days: Vec<DayDto>,
    totals: TotalsDto,
}

fn day_dto(day: &SavingsDaily) -> DayDto {
    DayDto {
        day: day.day_local.clone(),
        actual_eur: Euro::new(day.actual_eur),
        baseline_eur: Euro::new(day.baseline_eur),
        saving_eur: Euro::new(day.saving_eur),
        import_wh: kwh_to_wh(day.import_kwh),
        export_wh: kwh_to_wh(day.export_kwh),
        pv_wh: kwh_to_wh(day.pv_kwh),
        battery_charge_wh: kwh_to_wh(day.battery_charge_kwh),
        battery_discharge_wh: kwh_to_wh(day.battery_discharge_kwh),
        coverage_hours: hours(day.coverage_hours),
        complete: day.complete,
    }
}

fn totals_dto(days: &[SavingsDaily]) -> TotalsDto {
    let mut actual = 0.0;
    let mut baseline = 0.0;
    let mut saving = 0.0;
    let mut import_wh = 0.0;
    let mut export_wh = 0.0;
    let mut pv_wh = 0.0;
    let mut charge_wh = 0.0;
    let mut discharge_wh = 0.0;
    let mut complete_days = 0;
    for day in days {
        actual += day.actual_eur;
        baseline += day.baseline_eur;
        saving += day.saving_eur;
        import_wh += kwh_to_wh(day.import_kwh);
        export_wh += kwh_to_wh(day.export_kwh);
        pv_wh += kwh_to_wh(day.pv_kwh);
        charge_wh += kwh_to_wh(day.battery_charge_kwh);
        discharge_wh += kwh_to_wh(day.battery_discharge_kwh);
        if day.complete {
            complete_days += 1;
        }
    }
    TotalsDto {
        actual_eur: Euro::new(actual),
        baseline_eur: Euro::new(baseline),
        saving_eur: Euro::new(saving),
        import_wh: watt_hours(import_wh),
        export_wh: watt_hours(export_wh),
        pv_wh: watt_hours(pv_wh),
        battery_charge_wh: watt_hours(charge_wh),
        battery_discharge_wh: watt_hours(discharge_wh),
        days: days.len(),
        complete_days,
    }
}

/// `GET /api/v1/savings`
pub(crate) async fn savings(
    State(service): State<Arc<Service>>,
    ApiQuery(params): ApiQuery<SavingsParams>,
) -> Result<Json<Savings>, ApiError> {
    let period = match params.period.as_deref() {
        None => Period::Day,
        Some(value) => Period::parse(value).ok_or_else(|| {
            ApiError::query("period must be one of day, month, year, lifetime")
                .with_details(serde_json::json!({ "period": value }))
        })?,
    };
    let today = Utc::now().with_timezone(&TARIFF_TZ).date_naive();
    let (from, to) = period.bounds(today);
    let (from_day, to_day) = (from.to_string(), to.to_string());
    let (query_from, query_to) = (from_day.clone(), to_day.clone());
    let days = with_ledger(&service, move |ledger| {
        ledger
            .savings()
            .map_err(|err| ApiError::internal("list savings", err))?
            .range(&query_from, &query_to)
            .map_err(|err| ApiError::internal("list savings", err))
    })
    .await?;
    let totals = totals_dto(&days);
    Ok(Json(Savings {
        period: period.slug(),
        from_day,
        to_day,
        days: days.iter().map(day_dto).collect(),
        totals,
    }))
}
