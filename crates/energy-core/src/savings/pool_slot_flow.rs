//! One measured slot for the cost pool.

/// One measured slot, AC kWh each way.
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
}
