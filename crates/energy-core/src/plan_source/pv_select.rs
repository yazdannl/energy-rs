//! Choosing the PV series one plan is served: the model, the provider, or the measured fallback.

use std::sync::Arc;

use energy_ha::Connection;

use super::load_serve::Weather;
use super::{PvHandle, PvMode};
use crate::plan::PlanError;
use crate::plan_source::MeasuredHistory;
use crate::plan_source::ha_series;

/// Everything the PV choice needs from the supply.
pub(crate) struct PvRequest<'a> {
    /// The Home Assistant socket the provider forecast is read over.
    pub connection: &'a Connection,
    /// The operator's array and mode, when the settings are wired in.
    pub handle: Option<&'a Arc<PvHandle>>,
    /// The energy dashboard's solar-forecast config entries.
    pub entry_ids: &'a [String],
    /// The first slot's UTC epoch second.
    pub start: i64,
    /// Slots in the horizon.
    pub slots: usize,
    /// The weather fetched for this plan.
    pub weather: &'a Weather,
    /// The measured history the EWMA fallback reads.
    pub history: &'a MeasuredHistory,
}

/// The PV series the plan uses: the learned model, Home Assistant's provider, or the measured
/// EWMA — whichever `pv_mode` allows first.
///
/// The model is compared against the provider over the same horizon and the totals are logged;
/// that ratio is how the model's `scale` is calibrated against the real array.
pub(crate) async fn select(
    request: PvRequest<'_>,
    notes: &mut Vec<String>,
) -> Result<Vec<f64>, PlanError> {
    let PvRequest {
        connection,
        handle,
        entry_ids,
        start,
        slots,
        weather,
        history,
    } = request;
    let config = handle.map(|pv| pv.get()).unwrap_or_default();
    let provider = ha_series::solar_forecast(connection, entry_ids)
        .await?
        .map(|series| ha_series::sample(&series, start, slots));
    let model = match (config.mode, weather.location) {
        (PvMode::Provider, _) | (_, None) => None,
        (_, Some((latitude, longitude))) => {
            super::pv_model::model_pv(&config, latitude, longitude, &weather.hours, start, slots)
        }
    };
    if let (Some(model), Some(provider)) = (model.as_ref(), provider.as_ref()) {
        let model_kwh: f64 = model.iter().sum::<f64>() * 0.25;
        let provider_kwh: f64 = provider.iter().sum::<f64>() * 0.25;
        if provider_kwh > 0.05 {
            tracing::info!(
                model_kwh,
                provider_kwh,
                ratio = model_kwh / provider_kwh,
                "PV model against the provider over the horizon"
            );
        }
    }
    match (config.mode, model, provider) {
        (PvMode::Model | PvMode::Auto, Some(model), _) => Ok(model),
        (PvMode::Model, None, _) => Err(PlanError::ForecastUnavailable {
            name: "pv_kw",
            reason: "pv_mode is model but no model forecast is available: set the array \
                         capacity, tilt and azimuth, and check that the weather forecast answers"
                .to_owned(),
        }),
        (PvMode::Provider, _, Some(provider)) => Ok(provider),
        (PvMode::Auto, None, Some(provider)) => {
            // Only worth a note when an array *is* configured and the model could not run: an
            // unconfigured array is the documented default, not a degraded forecast.
            if config.model_ready() {
                notes.push(
                    "PV model unavailable (no weather); the provider forecast is used".to_string(),
                );
            }
            Ok(provider)
        }
        (PvMode::Provider, _, None) | (PvMode::Auto, None, None) => {
            measured_pv(history, start, slots, notes)
        }
    }
}

/// The measured EWMA fallback, with the note that says why it is serving.
fn measured_pv(
    history: &MeasuredHistory,
    start: i64,
    slots: usize,
    notes: &mut Vec<String>,
) -> Result<Vec<f64>, PlanError> {
    notes.push("solar forecast missing; EWMA fallback from measured PV".to_string());
    history
        .fallback(start, slots, false)
        .ok_or(PlanError::ForecastUnavailable {
            name: "pv_kw",
            reason: "no solar forecast and no full day of measured history".to_owned(),
        })
}
