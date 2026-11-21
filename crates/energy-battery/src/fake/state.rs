//! The fake device's programmable state.

use tokio::time::Instant;

use crate::fake::behaviour::FakeBehaviour;
use crate::fake::pack::FakePack;

/// A whole zenSDK device in memory: what it reports, what it accepted and how it misbehaves.
#[derive(Debug, Clone)]
pub struct FakeState {
    pub sn: String,
    pub product: String,
    pub version: u32,
    /// Counts like the real thing: one per report or answered write, wrapping in 8 bits.
    pub message_id: u32,
    /// Device wall-clock seconds. Reports advance `message_id`, never the clock.
    pub timestamp: i64,
    pub ac_mode: i32,
    /// `acStatus` in the report: 2 = grid charging, 1 = AC output, 0 = idle.
    pub ac_status: i32,
    /// `dcStatus` in the report: 1 = the DC/DC path is running.
    pub dc_status: i32,
    pub input_limit_w: u32,
    pub output_limit_w: u32,
    pub smart_mode: i32,
    pub soc_pct: f64,
    /// `minSoc` in deci-percent.
    pub min_soc_deci: i64,
    /// `socSet` in deci-percent.
    pub soc_set_deci: i64,
    pub charge_max_w: u32,
    pub inverse_max_w: u32,
    pub grid_input_w: i64,
    pub output_home_w: i64,
    pub packs: Vec<FakePack>,
    pub behaviour: FakeBehaviour,
    /// Every write body received, in order.
    pub writes: Vec<serde_json::Value>,
    /// Parallel to `writes`: whether that write changed the device state.
    pub applied: Vec<bool>,
    /// Writes acknowledged but not applied.
    pub dropped: u32,
    /// Properties of a write whose limits are applied only after a few stale reports.
    pub(crate) deferred_limits: Option<serde_json::Value>,
    /// How many more reports must still show the pre-write limits.
    pub(crate) deferred_reports: u32,
    pub(crate) last_write_at: Option<Instant>,
}

impl FakeState {
    /// An 800 Plus class device at rest: one AB2000 pack, 45 %, AC limits 0, RAM mode.
    pub fn new(sn: impl Into<String>) -> Self {
        let sn = sn.into();
        Self {
            sn,
            product: "solarFlow800Plus".to_owned(),
            version: 3,
            message_id: 4211,
            timestamp: 1_791_959_470,
            ac_mode: 2,
            ac_status: 0,
            dc_status: 0,
            input_limit_w: 0,
            output_limit_w: 0,
            smart_mode: 1,
            soc_pct: 45.0,
            min_soc_deci: 100,
            soc_set_deci: 1000,
            charge_max_w: 1000,
            inverse_max_w: 800,
            grid_input_w: 0,
            output_home_w: 0,
            packs: vec![FakePack::ab2000("CO4A000000000001")],
            behaviour: FakeBehaviour::default(),
            writes: Vec::new(),
            applied: Vec::new(),
            dropped: 0,
            deferred_limits: None,
            deferred_reports: 0,
            last_write_at: None,
        }
    }

    #[must_use]
    pub fn with_behaviour(mut self, behaviour: FakeBehaviour) -> Self {
        self.behaviour = behaviour;
        self
    }

    #[must_use]
    pub fn with_packs(mut self, packs: Vec<FakePack>) -> Self {
        self.packs = packs;
        self
    }

    #[must_use]
    pub fn with_soc(mut self, soc_pct: f64) -> Self {
        self.soc_pct = soc_pct;
        self
    }

    /// Set the AC power the next report shows (positive = discharge).
    pub fn set_power(&mut self, grid_input_w: i64, output_home_w: i64) {
        self.grid_input_w = grid_input_w;
        self.output_home_w = output_home_w;
    }

    /// Set both limits directly (a device that is already running).
    pub fn set_limits(&mut self, input_w: u32, output_w: u32) {
        self.input_limit_w = input_w;
        self.output_limit_w = output_w;
    }

    pub fn write_count(&self) -> usize {
        self.writes.len()
    }

    pub fn applied_count(&self) -> usize {
        self.applied.iter().filter(|applied| **applied).count()
    }

    pub fn last_body(&self) -> Option<&serde_json::Value> {
        self.writes.last()
    }

    /// Advance the report counter like the device does, before this report is built.
    ///
    /// The device emits fresh-ID reports while applying a write, but its clock is wall time:
    /// bumping the timestamp here too would put the fake's clock in the future and look skewed.
    pub(crate) fn advance_report_id(&mut self) {
        self.message_id = self.message_id.wrapping_add(1);
    }

    /// Apply a deferred write *after* the report with the stale limits was emitted.
    pub(crate) fn settle_deferred_write(&mut self) {
        if self.deferred_reports > 0 {
            self.deferred_reports -= 1;
            if self.deferred_reports == 0 {
                let properties = self.deferred_limits.take();
                self.apply_properties(properties.as_ref());
            }
        }
    }
}
