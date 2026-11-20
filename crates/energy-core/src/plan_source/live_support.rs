//! Pure helpers shared by the live plan source.

use crate::plan::PackState;
use crate::plan_source::Supply;

/// Capacity-weighted charge efficiency for the plan's energy path.
pub(crate) fn weighted_charge_efficiency(packs: &[PackState]) -> f64 {
    let (weighted, capacity): (f64, f64) = packs.iter().fold((0.0, 0.0), |(energy, cap), pack| {
        (
            energy + pack.spec.eta_charge * pack.spec.capacity_kwh,
            cap + pack.spec.capacity_kwh,
        )
    });
    if capacity > 0.0 {
        weighted / capacity
    } else {
        1.0
    }
}

/// Why the provider forecast was not used.
pub(crate) fn fallback_note(supply: &Supply, slots: usize, now: i64, max_age_s: i64) -> String {
    match &supply.forecast {
        None => "no forecast available; EWMA fallback from measured history".to_owned(),
        Some(forecast) if forecast.len() < slots => format!(
            "forecast covers {} of {slots} slots; EWMA fallback",
            forecast.len()
        ),
        Some(forecast) => {
            let age = now.saturating_sub(forecast.as_of).max(0);
            format!("forecast is {age}s old (limit {max_age_s}s); EWMA fallback")
        }
    }
}
