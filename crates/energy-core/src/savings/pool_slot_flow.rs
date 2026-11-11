//! One measured slot for the cost pool.

/// One measured slot: AC kWh each way, with the efficiency that turns them into stored energy.
///
/// The pool is a *stored*-energy account (its basis is EUR per kWh in the pack), so the AC flows are
/// converted at the boundary. Mixing the two — adding AC charge and subtracting AC discharge from a
/// DC stock — drifts the quantity by the round-trip loss and quietly distorts the basis that
/// authorises a sale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotFlowAc {
    /// Energy charged into the pack, AC kWh.
    pub charge_ac: f64,
    /// The part of `charge_ac` that came from PV, AC kWh.
    pub pv_ac: f64,
    /// Energy discharged, AC kWh.
    pub discharge_ac: f64,
    /// The slot's buy price, EUR/kWh.
    pub buy: f64,
    /// The slot's feed-in, EUR/kWh.
    pub feed_in: f64,
    /// Charge efficiency (AC in → stored).
    pub eta_charge: f64,
    /// Discharge efficiency (stored → AC out).
    pub eta_discharge: f64,
}
