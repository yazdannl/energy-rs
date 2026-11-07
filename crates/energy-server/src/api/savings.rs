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
///
/// Money is `null` for a closed day whose slots were not all measured; the energy sums and
/// `coverageHours` are still reported so a partial day is visible instead of dropped. The current
/// day is provisional (money present, `complete: false`) until it closes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DayDto {
    day: String,
    actual_eur: Option<Euro>,
    baseline_eur: Option<Euro>,
    saving_eur: Option<Euro>,
    arbitrage_eur: Option<Euro>,
    self_consumption_eur: Option<Euro>,
    feed_in_eur: Option<Euro>,
    battery_balance_eur: Option<Euro>,
    import_wh: f64,
    export_wh: f64,
    pv_wh: f64,
    battery_charge_wh: f64,
    battery_discharge_wh: f64,
    coverage_hours: f64,
    complete: bool,
}

/// Sums over the selected period; energy in Wh.
///
/// `lifetime` sums complete days only, so a partially measured day never leaks into the number the
/// user quotes; shorter periods include the provisional current day (its money exists, it is simply
/// not complete yet). A closed day with `null` money is excluded from every total.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotalsDto {
    actual_eur: Euro,
    baseline_eur: Euro,
    saving_eur: Euro,
    arbitrage_eur: Euro,
    self_consumption_eur: Euro,
    feed_in_eur: Euro,
    battery_balance_eur: Euro,
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
        actual_eur: day.actual_eur.map(Euro::new),
        baseline_eur: day.baseline_eur.map(Euro::new),
        saving_eur: day.saving_eur.map(Euro::new),
        arbitrage_eur: day.arbitrage_eur.map(Euro::new),
        self_consumption_eur: day.self_consumption_eur.map(Euro::new),
        feed_in_eur: day.feed_in_eur.map(Euro::new),
        battery_balance_eur: day.battery_balance_eur.map(Euro::new),
        import_wh: kwh_to_wh(day.import_kwh),
        export_wh: kwh_to_wh(day.export_kwh),
        pv_wh: kwh_to_wh(day.pv_kwh),
        battery_charge_wh: kwh_to_wh(day.battery_charge_kwh),
        battery_discharge_wh: kwh_to_wh(day.battery_discharge_kwh),
        coverage_hours: hours(day.coverage_hours),
        complete: day.complete,
    }
}

/// The running sums of one totals row.
#[derive(Debug, Default)]
struct Totals {
    actual: f64,
    baseline: f64,
    saving: f64,
    arbitrage: f64,
    self_consumption: f64,
    feed_in: f64,
    balance: f64,
    import_wh: f64,
    export_wh: f64,
    pv_wh: f64,
    charge_wh: f64,
    discharge_wh: f64,
    complete_days: usize,
}

impl Totals {
    /// Add one day when it belongs in the totals.
    fn add(&mut self, day: &SavingsDaily, complete_only: bool) {
        if !counts(day, complete_only) {
            return;
        }
        self.actual += day.actual_eur.unwrap_or_default();
        self.baseline += day.baseline_eur.unwrap_or_default();
        self.saving += day.saving_eur.unwrap_or_default();
        self.arbitrage += day.arbitrage_eur.unwrap_or_default();
        self.self_consumption += day.self_consumption_eur.unwrap_or_default();
        self.feed_in += day.feed_in_eur.unwrap_or_default();
        self.balance += day.battery_balance_eur.unwrap_or_default();
        self.import_wh += kwh_to_wh(day.import_kwh);
        self.export_wh += kwh_to_wh(day.export_kwh);
        self.pv_wh += kwh_to_wh(day.pv_kwh);
        self.charge_wh += kwh_to_wh(day.battery_charge_kwh);
        self.discharge_wh += kwh_to_wh(day.battery_discharge_kwh);
        if day.complete {
            self.complete_days += 1;
        }
    }

    fn dto(&self, days: usize) -> TotalsDto {
        TotalsDto {
            actual_eur: Euro::new(self.actual),
            baseline_eur: Euro::new(self.baseline),
            saving_eur: Euro::new(self.saving),
            arbitrage_eur: Euro::new(self.arbitrage),
            self_consumption_eur: Euro::new(self.self_consumption),
            feed_in_eur: Euro::new(self.feed_in),
            battery_balance_eur: Euro::new(self.balance),
            import_wh: watt_hours(self.import_wh),
            export_wh: watt_hours(self.export_wh),
            pv_wh: watt_hours(self.pv_wh),
            battery_charge_wh: watt_hours(self.charge_wh),
            battery_discharge_wh: watt_hours(self.discharge_wh),
            days,
            complete_days: self.complete_days,
        }
    }
}

/// Whether a day contributes to the totals: a complete day always, the provisional current day in
/// a shorter period, never a closed day whose money is `NULL`.
fn counts(day: &SavingsDaily, complete_only: bool) -> bool {
    if complete_only {
        return day.complete;
    }
    day.has_money()
}

fn totals_dto(days: &[SavingsDaily], complete_only: bool) -> TotalsDto {
    let mut totals = Totals::default();
    for day in days {
        totals.add(day, complete_only);
    }
    totals.dto(days.len())
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
    let totals = totals_dto(&days, period == Period::Lifetime);
    Ok(Json(Savings {
        period: period.slug(),
        from_day,
        to_day,
        days: days.iter().map(day_dto).collect(),
        totals,
    }))
}
