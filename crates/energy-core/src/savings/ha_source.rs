//! The real savings source: the energy dashboard's statistics, the price store and the annual
//! saldering counters.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{Datelike, NaiveDate};
use energy_ha::{Connection, HaClient, SiteModel, discover};
use energy_price::PriceRepository;
use energy_price::book::query::local_midnight;

use crate::dispatch::BoxFuture;

use super::annual_seeds::SeedState;
use super::statistic_series::{day_slots, year_totals};
use super::{DayReading, MeasuredDay, SavingsError, SavingsSource, SettlementSeed};

/// Savings reads over one Home Assistant connection.
///
/// Every day is measured from the dashboard statistics as 15-minute `change` values; a missing row
/// is a gap. Prices come from the price store's published slots. The year-to-date saldering
/// counters are rebuilt from the year's hourly history once per calendar year and then extended
/// one day at a time, so a catch-up does not re-read January.
pub struct HaSavingsSource {
    client: Arc<HaClient>,
    prices: Arc<dyn PriceRepository>,
    seeds: Mutex<Option<SeedState>>,
}

impl std::fmt::Debug for HaSavingsSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HaSavingsSource")
            .finish_non_exhaustive()
    }
}

impl HaSavingsSource {
    /// A source over one supervised Home Assistant link and the price store.
    pub fn new(client: Arc<HaClient>, prices: Arc<dyn PriceRepository>) -> Self {
        Self {
            client,
            prices,
            seeds: Mutex::new(None),
        }
    }

    /// The start-of-day seed, reading and caching the annual history when needed.
    async fn seed(
        &self,
        connection: &Connection,
        site: &SiteModel,
        day: NaiveDate,
    ) -> Result<SettlementSeed, SavingsError> {
        if let Some(seed) = self.cached_seed(day) {
            return Ok(seed);
        }
        let from = self.next_seed_day(day);
        let totals = year_totals(connection, site, from, day).await?;
        self.extend_seeds(&totals, day)
    }

    /// The seed for `day` if the counters have already reached it.
    fn cached_seed(&self, day: NaiveDate) -> Option<SettlementSeed> {
        let guard = self.seeds.lock().ok()?;
        guard
            .as_ref()
            .filter(|state| state.year() == day.year())
            .and_then(|state| state.seed(day))
    }

    /// First day whose annual totals still have to be applied.
    fn next_seed_day(&self, day: NaiveDate) -> NaiveDate {
        let guard = self.seeds.lock().ok();
        match guard.as_ref().and_then(|state| state.as_ref()) {
            Some(state) if state.year() == day.year() => state.next_day().min(day),
            _ => NaiveDate::from_ymd_opt(day.year(), 1, 1).unwrap_or(day),
        }
    }

    /// Apply the measured daily totals in day order and return the requested day's seed.
    fn extend_seeds(
        &self,
        totals: &BTreeMap<NaiveDate, (f64, f64)>,
        through: NaiveDate,
    ) -> Result<SettlementSeed, SavingsError> {
        let mut guard = self.seeds.lock().map_err(|_| SavingsError::Measurement {
            series: "saldering",
            reason: "the seed cache is poisoned".to_owned(),
        })?;
        let state = guard.get_or_insert_with(|| SeedState::new(through.year()));
        if state.year() != through.year() {
            *state = SeedState::new(through.year());
        }
        let mut cursor = state.next_day();
        while cursor <= through {
            let (import_kwh, export_kwh) = totals.get(&cursor).copied().unwrap_or_default();
            state.extend(cursor, import_kwh, export_kwh);
            let Some(next) = cursor.succ_opt() else {
                break;
            };
            cursor = next;
        }
        state
            .seed(through)
            .ok_or_else(|| SavingsError::Measurement {
                series: "saldering",
                reason: format!("no seed could be derived for {through}"),
            })
    }
}

impl SavingsSource for HaSavingsSource {
    fn read_day(&self, day: NaiveDate) -> BoxFuture<'_, Result<DayReading, SavingsError>> {
        Box::pin(async move {
            let from = local_midnight(day).ok_or(SavingsError::Clock)?;
            let to = day
                .succ_opt()
                .and_then(local_midnight)
                .ok_or(SavingsError::Clock)?;
            let connection =
                self.client
                    .connection()
                    .await
                    .map_err(|error| SavingsError::Measurement {
                        series: "connection",
                        reason: error.to_string(),
                    })?;
            let site = discover(&connection)
                .await
                .map_err(|error| SavingsError::Measurement {
                    series: "energy_prefs",
                    reason: error.to_string(),
                })?;
            if !site.configured {
                return Err(SavingsError::SiteNotConfigured);
            }
            let slots = day_slots(&connection, &site, from, to).await?;
            let prices = self
                .prices
                .load(from, to)
                .map_err(|error| SavingsError::Prices(error.to_string()))?;
            let seed = self.seed(&connection, &site, day).await?;
            Ok(DayReading {
                measured: MeasuredDay::new(day, slots),
                prices: prices
                    .into_iter()
                    .map(|slot| (slot.t_start, slot))
                    .collect(),
                seed,
            })
        })
    }
}
