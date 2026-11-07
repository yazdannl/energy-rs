//! NOCT cell temperature and the DC/AC power chain.

use super::constants::{ETA_INV, G_STC, GAMMA_PER_C, NOCT_C, T_REF_C};

/// NOCT module temperature model, degrees C.
pub fn cell_temperature(t_ambient_c: f64, poa: f64) -> f64 {
    t_ambient_c + (NOCT_C - 20.0) / 800.0 * poa
}

/// DC array power before the inverter, W.
pub fn dc_power(dc_capacity_wp: f64, poa: f64, t_cell_c: f64) -> f64 {
    dc_capacity_wp * (poa / G_STC) * (1.0 + GAMMA_PER_C * (t_cell_c - T_REF_C))
}

/// Expected AC power, W: DC derated by cell temperature and the inverter, then clipped.
///
/// `min(dc * eta, ac_capacity)` is what makes a DC/AC ratio above one observable. Past the
/// inverter's limit the curve flattens instead of following irradiance.
pub fn ac_power(dc_capacity_wp: f64, ac_capacity_wp: f64, poa: f64, t_cell_c: f64) -> f64 {
    let dc = dc_power(dc_capacity_wp, poa, t_cell_c).max(0.0) * ETA_INV;
    dc.min(ac_capacity_wp)
}
