//! One pack as the fleet port reports it: limits, live state and reading age.

use crate::plan::{PackSpec, PackState};

/// The live state of one controllable pack (a device, keyed by its hub serial).
///
/// `power_w` is signed AC power in the engine's convention -- positive **charging** -- which is the
/// opposite of the zenSDK report, so the fleet adapter negates it once and the ladder never has to.
/// `observed_at` is the instant of the underlying report, not of the read: the cell-voltage guard
/// judges freshness from it and a cached report must not look new.
#[derive(Debug, Clone, PartialEq)]
pub struct PackReading {
    /// The ledger id of this pack, when it is already registered.
    pub ledger_id: Option<i64>,
    /// Configured limits and efficiency for this pack.
    pub spec: PackSpec,
    /// Reported state of charge, percent.
    pub soc_pct: Option<f64>,
    /// Stored energy, kWh.
    pub energy_kwh: Option<f64>,
    /// Measured AC power, W (positive charging).
    pub power_w: f64,
    /// Lowest cell voltage the device reported, V.
    pub min_cell_v: Option<f64>,
    /// When the report was taken, UTC epoch seconds.
    pub observed_at: i64,
    /// Whether the device answered.
    pub available: bool,
    /// Whether the report is older than the configured freshness bound.
    pub stale: bool,
}

impl PackReading {
    /// The stable pack id.
    pub fn pack_id(&self) -> &str {
        &self.spec.pack_id
    }

    /// The pack as an allocation/plan input, or `None` when its SoC is unknown.
    pub fn state(&self) -> Option<PackState> {
        let soc_pct = self.soc_pct.filter(|soc| soc.is_finite())?;
        Some(PackState {
            spec: self.spec.clone(),
            soc_pct,
        })
    }

    /// Whether this pack may take part in allocation and receive a command.
    pub fn usable(&self) -> bool {
        self.available && !self.stale && self.state().is_some()
    }
}
