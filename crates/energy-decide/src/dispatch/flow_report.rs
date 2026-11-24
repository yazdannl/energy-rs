//! One fresh pack observation with the data needed for honest evidence.

use crate::dispatch::Echo;

/// Signed measured flow and the status/fault/SoC context around it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowReport {
    /// Stable pack identifier.
    pub pack_id: String,
    /// Observation instant, UTC epoch milliseconds.
    pub at_ms: i64,
    /// Measured signed power, charge positive, W.
    pub measured_w: f64,
    /// Reported AC status code.
    pub ac_status: i32,
    /// Reported DC status code.
    pub dc_status: i32,
    /// Reported fault level; zero for a derating/refusal pack.
    pub fault_level: i32,
    /// Reported state of charge, percent.
    pub soc_pct: f64,
    /// Configured device minimum SoC, percent.
    pub device_min_soc_pct: f64,
    /// Echo attached to this reading, when the device still reports one.
    pub echo: Option<Echo>,
    /// Ordered report sequence for freshness checks.
    pub message_id: u32,
    /// Whether the device generated its own timestamp for this report.
    pub device_clock_ok: bool,
    /// Whether that device clock is coherent with the pack's command timeline.
    pub device_clock_coherent: bool,
}

impl FlowReport {
    /// Whether this pack reports a fault.
    pub fn faulted(&self) -> bool {
        self.fault_level != 0
    }

    /// Whether a refusal above the device minimum is an observed capability shortfall.
    pub fn above_device_min_soc(&self) -> bool {
        self.soc_pct > self.device_min_soc_pct
    }
}
