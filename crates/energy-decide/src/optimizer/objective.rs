//! Exact objective arithmetic: terms, units and canonical quantisation.

/// Quantise EUR to the canonical 1e-6 key resolution.
pub fn round_eur(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

/// Quantise `Σ dt·p²` to the canonical 1e-3 key resolution.
pub fn round_p2(value: f64) -> f64 {
    (value * 1e3).round() / 1e3
}

/// Quantise stored energy to 1 Wh for canonical signature keys.
pub fn quantise_wh(energy_kwh: f64) -> i32 {
    (energy_kwh * 1000.0).round() as i32
}

/// Wear on stored kWh actually discharged, EUR.
pub fn wear_eur(energy_from_kwh: f64, energy_to_kwh: f64, wear_rate: f64) -> f64 {
    wear_rate * (energy_from_kwh - energy_to_kwh).max(0.0)
}

/// Energy transition through one leg's efficiency, kWh.
pub fn next_energy_kwh(energy_kwh: f64, power_kw: f64, dt_h: f64, eta_c: f64, eta_d: f64) -> f64 {
    if power_kw >= 0.0 {
        energy_kwh + dt_h * power_kw * eta_c
    } else {
        energy_kwh + dt_h * power_kw / eta_d
    }
}

/// Terminal salvage minus the one-sided, deductible shortfall penalty, EUR.
pub fn terminal_eur(
    terminal_kwh: f64,
    target_kwh: f64,
    deadband_kwh: f64,
    penalty_eur_per_kwh: f64,
    salvage_eur_per_stored_kwh: f64,
) -> f64 {
    -salvage_eur_per_stored_kwh * terminal_kwh
        + penalty_eur_per_kwh * (target_kwh - terminal_kwh - deadband_kwh).max(0.0)
}
