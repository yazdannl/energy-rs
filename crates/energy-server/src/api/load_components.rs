//! The components the served load forecast is made of: the house base and every named load.

use energy_core::plan_source::LoadProfiles;
use energy_forecast::load::LoadProfile;
use serde::Serialize;

/// One component of the served load forecast, as its own profile shapes it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Component {
    label: String,
    kind: String,
    /// Mean bucket median per local hour (weekday and weekend averaged), scaled, kW.
    curve_kw: Vec<f64>,
    /// Temperature slope, kW per degree C.
    fit_b: f64,
    /// How much of the variance that slope explains.
    r2: f64,
    /// The level scale the profile carries.
    scale: f64,
    /// Whether the temperature term is applied for this kind.
    temperature: bool,
}

/// The 24-hour shape one profile serves: weekday and weekend buckets averaged, scaled.
fn curve(profile: &LoadProfile) -> Vec<f64> {
    (0..24)
        .map(|hour| profile.median(hour).midpoint(profile.median(hour + 24)) * profile.scale)
        .collect()
}

pub(crate) fn component(
    label: &str,
    kind: &str,
    profile: &LoadProfile,
    temperature: bool,
) -> Component {
    Component {
        label: label.to_string(),
        kind: kind.to_string(),
        curve_kw: curve(profile),
        fit_b: profile.fit.b,
        r2: profile.fit.r2,
        scale: profile.scale,
        temperature,
    }
}

/// The house base plus every named load that has a profile.
pub(crate) fn component_list(
    base: &energy_forecast::load::LoadProfile,
    profiles: &LoadProfiles,
    devices: &[energy_forecast::load::LoadDevice],
) -> Vec<Component> {
    let mut out = vec![component("House base", "base", base, false)];
    for device in devices {
        if let Some(profile) = profiles.device(&device.key) {
            out.push(component(
                &device.name,
                device.kind.as_str(),
                &profile,
                device.temperature,
            ));
        }
    }
    out
}
