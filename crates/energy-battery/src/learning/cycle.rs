//! A closed charge → discharge cycle: the only unit that can measure a round trip.

use chrono::{DateTime, Utc};

/// One charge immediately followed by the discharge that takes the pack back down.
///
/// The split efficiency `x = eta_charge = eta_discharge` is the positive root of the energy balance
/// `dSOC · C = I · x − O / x`, where `I` and `O` are the two legs' AC energies, `dSOC` the cycle's
/// net SoC change and `C` the assumed pack capacity. A cycle that closes exactly (`dSOC = 0`)
/// reduces to `x = sqrt(O / I)` and does not touch `C` at all; a deeper or shallower one is corrected
/// by the energy the pack really gave up instead of being extrapolated from whole-percent readings.
#[derive(Debug, Clone, PartialEq)]
pub struct CyclePair {
    /// First sample of the charge leg.
    pub charge_at: DateTime<Utc>,
    /// First sample of the discharge leg.
    pub discharge_at: DateTime<Utc>,
    /// AC energy in, kWh.
    pub charge_kwh: f64,
    /// AC energy out, kWh.
    pub discharge_kwh: f64,
    /// SoC the charge moved, points.
    pub charge_span_pct: f64,
    /// SoC the discharge moved, points.
    pub discharge_span_pct: f64,
    /// `(end SoC of the discharge − start SoC of the charge) / 100 · C`: the cycle's net stored
    /// energy, the only place the assumed capacity enters.
    pub net_kwh: f64,
    /// `x`, the per-leg efficiency both legs share.
    pub split_eta: f64,
    /// `x²`, this cycle's AC-to-AC round trip.
    pub round_trip: f64,
    /// `min(dSOC) · I / (I + |net|)`: a deeper cycle is a sharper measurement, and one that leans on
    /// the capacity guess counts for less.
    pub weight: f64,
}

impl CyclePair {
    /// The capacity this cycle implies, kWh, for the plausibility envelope only.
    ///
    /// `sqrt(I · O / (dSOC_c · dSOC_d))` with both spans as fractions: the same number either leg
    /// yields once the cycle's own efficiency is applied, and independent of the capacity guess.
    pub fn implied_capacity_kwh(&self) -> Option<f64> {
        let charge_span = self.charge_span_pct / 100.0;
        let discharge_span = self.discharge_span_pct / 100.0;
        let product = self.charge_kwh * self.discharge_kwh / (charge_span * discharge_span);
        if product.is_finite() && product > 0.0 {
            Some(product.sqrt())
        } else {
            None
        }
    }
}
