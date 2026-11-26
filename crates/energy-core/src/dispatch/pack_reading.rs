//! One pack as the fleet port reports it: limits, live state and reading age.

use energy_battery::SocLimit;

use crate::allocation::{CHARGE_RELEASE_HYSTERESIS_PCT, ChargeSkip, FULL_CHARGE_FAULT_BAND_PCT};
use crate::dispatch::pack_flow::PackFlow;
use crate::plan::{PackSpec, PackState};
use crate::safety::Reason;

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
    /// Reported state of charge, percent. The real fleet fills this from the device's own
    /// `electricLevel`; it is not a learned or fleet-averaged value.
    pub soc_pct: Option<f64>,
    /// Explicit device-reported `electricLevel`, the authority for the charge ceiling.
    pub device_soc_pct: Option<f64>,
    /// Device-reported `socLimit`: `Upper` means charge is closed.
    pub soc_limit: SocLimit,
    /// Device-reported `faultLevel`; 2 near the ceiling is normal full-charge protection.
    pub fault_level: Option<i32>,
    /// Stored energy, kWh.
    pub energy_kwh: Option<f64>,
    /// Measured AC power, W (positive charging).
    pub power_w: f64,
    /// Direction-specific flow evidence. The real fleet always fills this; scripted fakes may
    /// leave it `None` and fall back to `power_w` for legacy tests.
    pub flow: Option<PackFlow>,
    /// Sustained discharge power with a contradictory status: an alarm, not a fault.
    pub flow_alarm: Option<String>,
    /// Active, unresolved control episode from this pack's controller; cleared by a verified hold.
    pub active_fault: Option<String>,
    /// Historical control fault from the device controller, even after the report looks idle.
    pub historical_fault: Option<String>,
    /// Lowest cell voltage the device reported, V.
    pub min_cell_v: Option<f64>,
    /// Highest cell voltage the device reported, V.
    pub max_cell_v: Option<f64>,
    /// Total pack voltage the device reported, V.
    pub total_voltage_v: Option<f64>,
    /// Highest reported pack temperature, °C.
    pub max_temp_c: Option<f64>,
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
        let soc_pct = self.reported_soc().filter(|soc| soc.is_finite())?;
        Some(PackState {
            spec: self.spec.clone(),
            soc_pct,
        })
    }

    /// Whether this pack may take part in allocation and receive a command.
    pub fn usable(&self) -> bool {
        self.available && !self.stale && self.state().is_some()
    }

    /// The device's own SoC, never a learned or fleet-average value.
    fn reported_soc(&self) -> Option<f64> {
        self.device_soc_pct.or(self.soc_pct)
    }

    /// Whether this device's own report closes charge right now.
    ///
    /// `faultLevel` 2 is only a charge block within [`FULL_CHARGE_FAULT_BAND_PCT`] of the ceiling:
    /// at 100 % with `is_error` 0 it is the BMS protecting a full pack, not a fault to escalate.
    pub fn device_charge_closed(&self) -> bool {
        let Some(soc) = self.reported_soc().filter(|soc| soc.is_finite()) else {
            return false;
        };
        let ceiling = self.spec.max_soc_pct;
        soc >= ceiling
            || self.soc_limit.blocks_charge()
            || (self.fault_level.is_some_and(|level| level >= 2)
                && soc >= ceiling - FULL_CHARGE_FAULT_BAND_PCT)
    }

    /// Whether fresh device SoC has fallen past the charge-lockout hysteresis.
    pub fn device_charge_released(&self) -> bool {
        self.available
            && !self.stale
            && self.reported_soc().is_some_and(|soc| {
                soc.is_finite() && soc <= self.spec.max_soc_pct - CHARGE_RELEASE_HYSTERESIS_PCT
            })
    }

    /// Why this pack must not receive charge this tick, if at all.
    ///
    /// Only a pack whose own fresh report closes charge is skipped. A stale or missing sample is
    /// left to the 0.15.12 ladder: no new skip, no target change, no reservation. An existing
    /// ceiling latch survives staleness, so a previously closed pack is not re-authorised by a
    /// bad read.
    pub fn charge_skip(&self, locked: bool) -> Option<ChargeSkip> {
        // An existing latch is authority on its own: neither missing SoC nor staleness may
        // re-authorise charge. Only the fresh release threshold in the dispatcher clears it.
        if locked {
            return Some(ChargeSkip::new(
                self.pack_id(),
                Reason::ChargeAtCeiling,
                "ceiling latch retained until a fresh report falls past the release margin",
            ));
        }
        if !self.available || self.stale {
            return None;
        }
        let soc = self.reported_soc().filter(|soc| soc.is_finite())?;
        self.device_charge_closed().then(|| {
            ChargeSkip::new(
                self.pack_id(),
                Reason::ChargeAtCeiling,
                format!(
                    "device report: electricLevel={soc:.0}%, socLimit={:?}, faultLevel={:?}, ceiling={:.0}%",
                    self.soc_limit,
                    self.fault_level,
                    self.spec.max_soc_pct
                ),
            )
        })
    }
}
