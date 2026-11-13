//! The live supply: Home Assistant statistics, the solar forecast and the price store.

use std::collections::BTreeMap;
use std::sync::Arc;

use energy_forecast::load::{LoadConfig, LoadDevice, LoadProfile};

use chrono::{DateTime, Utc};
use energy_ha::{Connection, discover};
use energy_price::Settlement;
use energy_price::store::PriceRepository;

use crate::dispatch::BoxFuture;
use crate::plan::PlanError;
use crate::plan_source::PvHandle;
use crate::plan_source::ha_series;
use crate::plan_source::load_profiles::LoadProfiles;
use crate::plan_source::load_serve::{self, Served};
use crate::plan_source::{Forecast, HISTORY_HOURS, MeasuredHistory, PlanSupply, Supply};

/// The real supply: `energy/get_prefs` and the energy dashboard's statistics.
///
/// Every required statistic is read as `change` per complete quarter-hour; a hole is refused. The
/// load forecast is the learned house profile (see [`LoadProfiles`]) plus whatever the operator
/// configured for it — named loads, holiday mode and the outdoor temperature — and PV comes from
/// `energy/solar_forecast` when a provider is configured. A missing input degrades the
/// [`Forecast`] with a note instead of being fabricated, and the plan summary shows it.
pub struct HaPlanSupply {
    connection: Arc<Connection>,
    prices: Arc<dyn PriceRepository>,
    settlement: Settlement,
    profiles: Option<Arc<LoadProfiles>>,
    pv: Option<Arc<PvHandle>>,
}

impl HaPlanSupply {
    /// A supply over one HA connection and the price store.
    pub fn new(
        connection: Arc<Connection>,
        prices: Arc<dyn PriceRepository>,
        settlement: Settlement,
    ) -> Self {
        Self {
            connection,
            prices,
            settlement,
            profiles: None,
            pv: None,
        }
    }

    /// Use the learned house profile for the load forecast (a fallback when it is absent/stale).
    #[must_use]
    pub fn with_load_profiles(mut self, profiles: Arc<LoadProfiles>) -> Self {
        self.profiles = Some(profiles);
        self
    }

    /// Use the operator's PV array and mode (the learned model is the default in `auto`).
    #[must_use]
    pub fn with_pv(mut self, pv: Arc<PvHandle>) -> Self {
        self.pv = Some(pv);
        self
    }

    /// The measured history from the energy dashboard statistics.
    async fn history(&self, now: i64) -> Result<MeasuredHistory, PlanError> {
        let site = discover(&self.connection)
            .await
            .map_err(|error| PlanError::Supply {
                reason: error.to_string(),
            })?;
        if !site.configured {
            return Err(PlanError::SiteNotConfigured);
        }
        let window = ha_series::history_window(now, HISTORY_HOURS);
        let import = self
            .statistic(site.grid.import.as_ref(), "grid_import", window)
            .await?;
        let export = self
            .statistic(site.grid.export.as_ref(), "grid_export", window)
            .await?;
        let pv = self
            .statistic(site.solar.energy.as_ref(), "solar_energy", window)
            .await?;
        let charge = self.battery(&site, window, true).await?;
        let discharge = self.battery(&site, window, false).await?;
        ha_series::timeline(&import, &export, &pv, &charge, &discharge, window)
    }

    /// One configured statistic, refused when the dashboard does not name it.
    async fn statistic(
        &self,
        entity: Option<&String>,
        name: &'static str,
        window: (i64, i64),
    ) -> Result<BTreeMap<i64, f64>, PlanError> {
        let entity = entity
            .filter(|id| !id.trim().is_empty())
            .ok_or(PlanError::StatsMissing { series: name })?;
        ha_series::fetch(&self.connection, entity, window).await
    }

    /// The summed battery statistics of one direction; empty when no battery configures one.
    async fn battery(
        &self,
        site: &energy_ha::SiteModel,
        window: (i64, i64),
        charging: bool,
    ) -> Result<BTreeMap<i64, f64>, PlanError> {
        let (charge, discharge) = ha_series::battery_entities(site);
        let entities = if charging { charge } else { discharge };
        let mut total = BTreeMap::new();
        for entity in entities {
            let series = ha_series::fetch(&self.connection, &entity, window).await?;
            for (t_start, kwh) in series {
                *total.entry(t_start).or_insert(0.0) += kwh;
            }
        }
        Ok(total)
    }

    /// The forward forecast: provider PV and either the learned load profile or the fallback.
    ///
    /// A learned profile is the point of P12: when one exists, the load series is a model output and
    /// the plan is not marked degraded for it. Only a genuinely degraded input (no solar forecast, or
    /// a residential fallback shape) still sets the note.
    async fn forecast(
        &self,
        now: i64,
        history: &MeasuredHistory,
        entry_ids: &[String],
    ) -> Result<Forecast, PlanError> {
        let start = ha_series::slot_floor(now);
        let slots = crate::plan::HORIZON_SLOTS;
        let mut notes: Vec<String> = Vec::new();
        let config = self
            .profiles
            .as_ref()
            .map(|profiles| profiles.config())
            .unwrap_or_default();
        let weather = load_serve::weather(&self.connection, &config, now, slots).await;
        let pv_kw = super::pv_select::select(
            super::pv_select::PvRequest {
                connection: &self.connection,
                handle: self.pv.as_ref(),
                entry_ids,
                start,
                slots,
                weather: &weather,
                history,
            },
            &mut notes,
        )
        .await?;
        let profile = self
            .profiles
            .as_ref()
            .and_then(|profiles| profiles.current(now));
        let prepared = self.prepare(&config, &weather.temperatures);
        notes.extend(prepared.notes.iter().cloned());
        let served = Served {
            devices: &prepared.devices,
            device_profiles: &prepared.device_profiles,
            temperatures: &prepared.temperatures,
            holiday: prepared.holiday,
        };
        let base = history.load_kwh_over(24);
        let load_kw = match (&profile, base) {
            (Some(profile), _) => {
                load_serve::serve(Some(profile), 0.0, now, slots, &served, &mut notes)?
            }
            (None, base) if base > 0.0 => {
                notes.push("load profile missing; residential fallback shape".to_string());
                load_serve::serve(None, base, now, slots, &served, &mut notes)?
            }
            (None, _) => {
                notes.push("load profile missing; EWMA fallback from measured load".to_string());
                history
                    .fallback(start, slots, true)
                    .ok_or(PlanError::ForecastUnavailable {
                        name: "load_kw",
                        reason: "no measured load level and no full day of history".to_owned(),
                    })?
            }
        };
        Ok(if notes.is_empty() {
            Forecast::fresh(pv_kw, load_kw, now)
        } else {
            Forecast::degraded(pv_kw, load_kw, now, notes.join("; "))
        })
    }

    /// Everything the load forecast needs beside the house profile and the measured level.
    fn prepare(
        &self,
        config: &LoadConfig,
        temperatures: &BTreeMap<DateTime<Utc>, f64>,
    ) -> Prepared {
        let (devices, problems) = config.parsed_devices();
        let device_profiles = self
            .profiles
            .as_ref()
            .map(|profiles| profiles.device_profiles(&devices))
            .unwrap_or_default();
        let mut notes: Vec<String> = problems
            .iter()
            .map(|problem| format!("load.devices: {problem}"))
            .collect();
        let configured = config.temperature_source().is_some() || config.weather_source().is_some();
        if temperatures.is_empty() && configured {
            notes.push(
                "no outdoor temperature available; the temperature term is off for this plan"
                    .to_owned(),
            );
        }
        Prepared {
            devices,
            device_profiles,
            temperatures: temperatures.clone(),
            holiday: config.holiday,
            notes,
        }
    }
}

impl PlanSupply for HaPlanSupply {
    fn supply(&self, now: i64) -> BoxFuture<'_, Result<Supply, PlanError>> {
        Box::pin(async move {
            let history = self.history(now).await?;
            let site = discover(&self.connection)
                .await
                .map_err(|error| PlanError::Supply {
                    reason: error.to_string(),
                })?;
            let forecast = self
                .forecast(now, &history, &site.solar.forecast_entry_ids)
                .await?;
            let from = ha_series::slot_floor(now);
            // Load the slots that exist and validate them as one gap-free book; a shorter book is
            // the normal case before the ~13:00 CET publication, and the source shortens its
            // horizon. `load_book` would instead refuse the (legitimately) missing tail.
            let prices = energy_price::PriceBook::new(
                self.prices
                    .load(from, ha_series::price_end(from))
                    .map_err(PlanError::Price)?,
            )
            .map_err(PlanError::Price)?;
            let fetched_at = self
                .prices
                .latest_publication()
                .map_err(PlanError::Price)?
                .map_or(0, |publication| publication.fetched_at);
            Ok(Supply {
                history,
                forecast: Some(forecast),
                prices,
                price_fetched_at: fetched_at,
                settlement: self.settlement.clone(),
            })
        })
    }
}

/// Everything one load forecast needs beyond the house profile and the measured level.
struct Prepared {
    devices: Vec<LoadDevice>,
    device_profiles: BTreeMap<String, LoadProfile>,
    temperatures: BTreeMap<DateTime<Utc>, f64>,
    holiday: bool,
    notes: Vec<String>,
}
