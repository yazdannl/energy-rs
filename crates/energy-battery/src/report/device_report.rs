//! One `GET /properties/report` document.

use crate::report::pack::PackReport;
use crate::report::properties::Properties;

/// A device report: identity, device clock and the decoded property/pack state.
///
/// `product` is a display string only — gating behaviour on a model table would reject hardware
/// that works (the vendor library is already behind the 3000 Mix and the 800 Pro 2).
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Device clock, unix seconds (`timestamp`, falling back to `ts`).
    pub timestamp: i64,
    /// Counts in 8 bits; a lower value than the previous report is a restart hint.
    pub message_id: u32,
    /// Authoritative identity.
    pub sn: String,
    pub version: Option<u32>,
    pub product: Option<String>,
    pub properties: Properties,
    pub packs: Vec<PackReport>,
}

impl Report {
    /// Average pack State of Charge in percent (the device averages across packs).
    pub fn soc_pct(&self) -> f64 {
        self.properties.electric_level_pct
    }

    /// AC power at the meter: positive while discharging, negative while charging.
    pub fn ac_power_w(&self) -> i64 {
        self.properties.output_home_w - self.properties.grid_input_w
    }

    /// Sum of the pack capacity labels, or `None` when no pack could be labelled.
    pub fn total_capacity_kwh(&self) -> Option<f64> {
        let known: Vec<f64> = self
            .packs
            .iter()
            .filter_map(|pack| pack.label.map(|label| label.capacity_kwh))
            .collect();
        if known.is_empty() {
            None
        } else {
            Some(known.iter().sum())
        }
    }

    /// Energy between the current SoC and the configured floor, from the labels only.
    pub fn available_kwh(&self) -> Option<f64> {
        let nominal = self.total_capacity_kwh()?;
        let usable_pct = (self.soc_pct() - self.properties.min_soc_pct).max(0.0);
        Some(nominal * usable_pct / 100.0)
    }
}
