//! `GET`/`PATCH /api/v1/loads` — the load-prediction settings: holiday mode, the weather sources
//! and the named loads that are modelled above the house profile.
//!
//! A write is persisted first and then asks the load-profile loop to relearn, because a device is
//! only added back above the base when the base that is being served was learned with that device
//! subtracted. Until the relearn lands, the stored config is visible but not yet modelled, and the
//! answer says which devices are modelled `now`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use energy_core::plan_source::LoadProfiles;
use energy_forecast::load::LoadConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::error::ApiError;
use crate::api::extract::ApiJson;
use crate::service::Service;

/// One configured named load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Device {
    entity: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    keep_while_away: bool,
}

impl Device {
    fn payload(&self) -> Value {
        json!({
            "entity": self.entity.trim(),
            "kind": self.kind.trim(),
            "name": self.name.trim(),
            "keep_while_away": self.keep_while_away,
        })
    }
}

/// One named load the base profile could not subtract, and why.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Excluded {
    key: String,
    reason: String,
}

/// What the base profile did with the named loads when it was learned.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Accounting {
    subtracted: Vec<String>,
    excluded: Vec<Excluded>,
    dropped_hours: u32,
    clamped_hours: u32,
}

/// What the profile the planner is using says.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LearnedProfile {
    days: usize,
    samples: usize,
    base_kw: f64,
    /// The house's own standing load: the floor of the served total.
    house_base_kw: f64,
    updated_at: String,
    temperature_hours: usize,
    accounting: Accounting,
    /// The house base and every named load, with the curve each one serves.
    components: Vec<super::load_components::Component>,
}

/// One device as the forecast would use it right now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Modelled {
    entity: String,
    modelled: bool,
}

/// `GET`/`PATCH /api/v1/loads` payload.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Loads {
    holiday: bool,
    temperature_entity: String,
    weather_entity: String,
    devices: Vec<Device>,
    /// Entries that had to be dropped or noted before they could be modelled.
    problems: Vec<String>,
    /// The learned base profile the planner is using, when one is fresh enough.
    profile: Option<LearnedProfile>,
    /// Whether each configured device is modelled above the base right now.
    modelled: Vec<Modelled>,
}

/// The `PATCH` body: the whole configuration, replaced as a unit.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoadsRequest {
    #[serde(default)]
    holiday: bool,
    #[serde(default)]
    temperature_entity: String,
    #[serde(default)]
    weather_entity: String,
    #[serde(default)]
    devices: Vec<Device>,
}

/// Run `work` against the profile store on the blocking pool (the ledger is synchronous).
async fn with_profiles<T: Send + 'static>(
    service: &Service,
    work: impl FnOnce(&LoadProfiles) -> T + Send + 'static,
) -> Result<T, ApiError> {
    let ledger = Arc::clone(service.ledger());
    tokio::task::spawn_blocking(move || work(&LoadProfiles::new(ledger)))
        .await
        .map_err(|err| ApiError::internal("load profile task", err))
}

fn config_from_request(body: &LoadsRequest) -> Result<LoadConfig, ApiError> {
    for device in &body.devices {
        if device.entity.trim().is_empty() {
            return Err(ApiError::validation("every named load needs an entity id")
                .with_details(json!({"devices": body.devices})));
        }
    }
    let mut entities: Vec<String> = body
        .devices
        .iter()
        .map(|device| device.entity.trim().to_string())
        .collect();
    entities.sort();
    let total = entities.len();
    entities.dedup();
    if entities.len() != total {
        return Err(
            ApiError::validation("a named load is listed more than once")
                .with_details(json!({"entities": entities})),
        );
    }
    Ok(LoadConfig {
        holiday: body.holiday,
        temperature_entity: body.temperature_entity.trim().to_string(),
        weather_entity: body.weather_entity.trim().to_string(),
        devices: body.devices.iter().map(Device::payload).collect(),
    })
}

fn render(config: &LoadConfig, profiles: &LoadProfiles, now: i64) -> Loads {
    let (configured, problems) = config.parsed_devices();
    let modelled = configured
        .iter()
        .map(|device| Modelled {
            entity: device.entity.clone(),
            modelled: profiles.device(&device.key).is_some(),
        })
        .collect();
    let devices = config
        .devices
        .iter()
        .map(|entry| Device {
            entity: entry
                .get("entity")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            kind: entry
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            keep_while_away: entry
                .get("keep_while_away")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect();
    let base = profiles.current(now);
    let parts = base
        .as_ref()
        .map(|base| super::load_components::component_list(base, profiles, &configured))
        .unwrap_or_default();
    let profile = base.map(|base| {
        let mut learned = learned(base);
        learned.components = parts;
        learned
    });
    Loads {
        holiday: config.holiday,
        temperature_entity: config.temperature_entity.clone(),
        weather_entity: config.weather_entity.clone(),
        devices,
        problems,
        profile,
        modelled,
    }
}

/// The learned profile as the API shows it, including what the split did with the meters.
fn learned(profile: energy_forecast::load::LoadProfile) -> LearnedProfile {
    LearnedProfile {
        days: profile.days,
        samples: profile.samples,
        base_kw: profile.base_kw,
        house_base_kw: profile.house_base_kw,
        updated_at: profile.updated_at,
        temperature_hours: profile.temperature_hours,
        components: Vec::new(),
        accounting: Accounting {
            subtracted: profile.devices_subtracted,
            excluded: profile
                .devices_excluded
                .into_iter()
                .map(|(key, reason)| Excluded { key, reason })
                .collect(),
            dropped_hours: profile.devices_dropped_hours,
            clamped_hours: profile.devices_clamped_hours,
        },
    }
}

/// `GET /api/v1/loads`
pub(crate) async fn get_loads(
    State(service): State<Arc<Service>>,
) -> Result<Json<Loads>, ApiError> {
    let now = Utc::now().timestamp();
    let loads = with_profiles(&service, move |profiles| {
        render(&profiles.config(), profiles, now)
    })
    .await?;
    Ok(Json(loads))
}

/// `PATCH /api/v1/loads`: store the whole configuration and ask for a relearn.
pub(crate) async fn patch_loads(
    State(service): State<Arc<Service>>,
    ApiJson(body): ApiJson<LoadsRequest>,
) -> Result<Json<Loads>, ApiError> {
    let config = config_from_request(&body)?;
    let stored = config.clone();
    let now = Utc::now().timestamp();
    let loads = with_profiles(&service, move |profiles| {
        profiles
            .set_config(&stored)
            .map_err(|err| ApiError::internal("write load config", err))?;
        Ok(render(&stored, profiles, now))
    })
    .await??;
    service.request_profile_refresh();
    Ok(Json(loads))
}
