//! The learned PV model, served per quarter-hour slot from the weather forecast.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Timelike, Utc};
use energy_forecast::model::PvModel;
use energy_forecast::physics::Site;
use energy_forecast::pv::{SlotInputs, estimate_slot};
use energy_ha::OpenMeteoHour;

use super::PvConfig;
use crate::plan::SLOT_SECONDS;

/// The C artifact committed with the binary, parsed once per process.
static MODEL: std::sync::OnceLock<Option<PvModel>> = std::sync::OnceLock::new();

/// The model this build ships, or `None` when the artifact cannot be parsed (logged once).
fn model() -> Option<&'static PvModel> {
    MODEL
        .get_or_init(|| match PvModel::embedded() {
            Ok(model) => Some(model),
            Err(error) => {
                tracing::error!(%error, "the embedded PV model is unusable; the plan falls back");
                None
            }
        })
        .as_ref()
}

/// The model's PV series over the horizon, kW, or `None` when it cannot serve this plan.
///
/// One weather hour per slot, taken at the slot's own hour; a missing hour is carried forward so a
/// short forecast degrades the tail instead of zeroing it. A model that answers non-finite is
/// treated as a refusal for that whole plan, never as a zero: a silently zero PV forecast would
/// plan a day without sun.
pub(crate) fn model_pv(
    config: &PvConfig,
    latitude: f64,
    longitude: f64,
    weather: &BTreeMap<DateTime<Utc>, OpenMeteoHour>,
    start: i64,
    slots: usize,
) -> Option<Vec<f64>> {
    if !config.model_ready() {
        return None;
    }
    let model = model()?;
    let site = Site::new(latitude, longitude, config.tilt_deg, config.azimuth_deg);
    let dc_wp = config.dc_kwp * 1000.0;
    let ac_wp = config.ac_kwp * 1000.0;
    let scale = config.scale();
    let mut series = Vec::with_capacity(slots);
    let mut last: Option<OpenMeteoHour> = None;
    for index in 0..slots {
        let offset = i64::try_from(index)
            .unwrap_or(i64::MAX)
            .saturating_mul(SLOT_SECONDS);
        let moment = DateTime::<Utc>::from_timestamp(start.saturating_add(offset), 0)?;
        let hour = moment - Duration::minutes(i64::from(moment.minute()));
        let sample = weather
            .get(&hour)
            .copied()
            .or(last)
            .or_else(|| nearest(weather, moment))?;
        last = Some(sample);
        let inputs = SlotInputs {
            moment,
            site,
            dc_capacity_wp: dc_wp,
            ac_capacity_wp: ac_wp,
            ghi_wm2: sample.ghi_wm2,
            t_ambient_c: sample.temperature_c,
            cloud_pct: sample.cloud_pct,
            // Open-Meteo's GHI is already the attenuated irradiance: nothing further is applied.
            attenuation: 1.0,
            wind_ms: sample.wind_ms,
            rh_pct: sample.rh_pct,
            rain_mm: sample.rain_mm,
        };
        let estimate = estimate_slot(model, &inputs).ok()?;
        if !estimate.pv_kw.is_finite() {
            return None;
        }
        series.push((estimate.pv_kw * scale).max(0.0));
    }
    Some(series)
}

/// The weather hour nearest `moment` (the forecast can start after the plan does).
fn nearest(
    weather: &BTreeMap<DateTime<Utc>, OpenMeteoHour>,
    moment: DateTime<Utc>,
) -> Option<OpenMeteoHour> {
    weather
        .range(..=moment)
        .next_back()
        .or_else(|| weather.iter().next())
        .map(|(_, hour)| *hour)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::TimeZone;
    use energy_ha::OpenMeteoHour;

    use super::{PvConfig, model_pv};

    fn config() -> PvConfig {
        PvConfig {
            dc_kwp: 2.16,
            ac_kwp: 2.4,
            tilt_deg: 25.0,
            azimuth_deg: 135.0,
            ..PvConfig::default()
        }
    }

    fn noon(hour: u32, ghi: f64) -> BTreeMap<chrono::DateTime<chrono::Utc>, OpenMeteoHour> {
        let moment = chrono::Utc
            .with_ymd_and_hms(2026, 6, 21, hour, 0, 0)
            .unwrap();
        let mut weather = BTreeMap::new();
        weather.insert(
            moment,
            OpenMeteoHour {
                temperature_c: 20.0,
                cloud_pct: 10.0,
                wind_ms: 2.0,
                rh_pct: 55.0,
                rain_mm: 0.0,
                ghi_wm2: ghi,
            },
        );
        weather
    }

    #[test]
    fn the_embedded_model_serves_a_plausible_summer_noon() {
        let start = chrono::Utc
            .with_ymd_and_hms(2026, 6, 21, 11, 0, 0)
            .unwrap()
            .timestamp();
        let series =
            model_pv(&config(), 52.08, 4.51, &noon(12, 800.0), start, 8).expect("the model serves");
        assert_eq!(series.len(), 8);
        let peak = series.iter().copied().fold(0.0_f64, f64::max);
        assert!(
            (0.2..=2.4).contains(&peak),
            "a 2.16 kWp array at noon should be between 0.2 and 2.4 kW, got {peak}"
        );
        assert!(series.iter().all(|kw| kw.is_finite() && *kw >= 0.0));
    }

    #[test]
    fn an_unconfigured_array_is_refused_rather_than_guessed() {
        let start = chrono::Utc
            .with_ymd_and_hms(2026, 6, 21, 11, 0, 0)
            .unwrap()
            .timestamp();
        let mut unconfigured = config();
        unconfigured.dc_kwp = 0.0;
        assert!(model_pv(&unconfigured, 52.08, 4.51, &noon(12, 800.0), start, 4).is_none());
        // No weather at all: also a refusal, never a zero-PV plan.
        assert!(model_pv(&config(), 52.08, 4.51, &BTreeMap::new(), start, 4).is_none());
    }

    #[test]
    fn the_night_is_zero_without_consulting_the_model() {
        let start = chrono::Utc
            .with_ymd_and_hms(2026, 6, 21, 0, 0, 0)
            .unwrap()
            .timestamp();
        let series = model_pv(&config(), 52.08, 4.51, &noon(0, 0.0), start, 4).expect("serves");
        assert!(series.iter().all(|kw| *kw == 0.0));
    }
}
