//! `estimate_slot`: the serving path around the walk, including the night gate.

use crate::ForecastError;
use crate::features::{BuiltFeatures, FeatureInputs, Features, build, index, is_night};
use crate::model::PvModel;
use crate::physics::{SolarGeometry, solar_geometry};

use super::slot_estimate::SlotEstimate;
use super::slot_inputs::SlotInputs;

/// One slot's learned PV estimate, in kW.
///
/// Below the horizon the answer is zero without consulting the model: the training corpus kept no
/// rows below five degrees, and production is physically zero at or below zero anyway. The residual
/// clamp is applied inside [`PvModel::ratio`], before the physics multiplication, and the product is
/// floored at zero.
pub fn estimate_slot(model: &PvModel, inputs: &SlotInputs) -> Result<SlotEstimate, ForecastError> {
    let geometry = solar_geometry(&inputs.site, inputs.moment);
    if is_night(geometry.elevation_deg) {
        return Ok(SlotEstimate::night(
            geometry.elevation_deg,
            inputs.t_ambient_c,
        ));
    }
    let built = build_features(inputs, &geometry);
    let ratio = model.ratio(&built.features)?;
    Ok(SlotEstimate {
        pv_kw: (built.physics_w * ratio).max(0.0) / 1000.0,
        ratio,
        physics_w: built.physics_w,
        poa_wm2: built.poa_wm2,
        ghi_wm2: inputs.ghi_wm2,
        clearness_index: built.features.get(index::CLEARNESS_INDEX),
        t_cell_c: built.features.get(index::T_CELL_C),
        elevation_deg: geometry.elevation_deg,
    })
}

/// The feature row a daytime slot builds, for tests and diagnostics: features, physics, POA and
/// elevation. `None` below the horizon, where the serving path builds no features at all.
pub fn slot_features(inputs: &SlotInputs) -> Option<(Features, f64, f64, f64)> {
    let geometry = solar_geometry(&inputs.site, inputs.moment);
    if is_night(geometry.elevation_deg) {
        return None;
    }
    let built = build_features(inputs, &geometry);
    Some((
        built.features,
        built.physics_w,
        built.poa_wm2,
        geometry.elevation_deg,
    ))
}

fn build_features(inputs: &SlotInputs, geometry: &SolarGeometry) -> BuiltFeatures {
    let mut feature_inputs = FeatureInputs::new(
        geometry,
        inputs.ghi_wm2,
        inputs.t_ambient_c,
        inputs.dc_capacity_wp,
        inputs.ac_capacity_wp,
        inputs.moment,
    );
    feature_inputs.cloud_pct = inputs.cloud_pct;
    feature_inputs.attenuation = inputs.attenuation;
    feature_inputs.wind_ms = inputs.wind_ms;
    feature_inputs.rh_pct = inputs.rh_pct;
    feature_inputs.rain_mm = inputs.rain_mm;
    build(&feature_inputs)
}
