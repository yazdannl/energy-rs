//! The cached state of one device.

use std::time::Duration;

use tokio::time::Instant;

use crate::control::command::Command;
use crate::control::fault::ControlFault;
use crate::control::observation::Observation;
use crate::report::Report;

/// Last known state of one device: the report, when it was read, and whatever went wrong.
///
/// A failed poll keeps the previous report (with its old `fetched_at`) so the UI can show "stale"
/// instead of vanishing, and the error stays visible next to it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSnapshot {
    pub sn: String,
    pub report: Option<Report>,
    pub fetched_at: Option<Instant>,
    pub error: Option<String>,
    pub fault: Option<ControlFault>,
    pub confirmed: Option<Command>,
}

impl DeviceSnapshot {
    pub fn new(sn: impl Into<String>) -> Self {
        Self {
            sn: sn.into(),
            report: None,
            fetched_at: None,
            error: None,
            fault: None,
            confirmed: None,
        }
    }

    /// Fold one successful observation into the cache.
    pub fn from_observation(observation: Observation, at: Instant) -> Self {
        Self {
            sn: observation.report.sn.clone(),
            report: Some(observation.report),
            fetched_at: Some(at),
            error: None,
            fault: observation.fault,
            confirmed: observation.confirmed,
        }
    }

    /// Record a failed poll, keeping the last good report.
    pub fn record_failure(&mut self, error: &str, at: Instant) {
        self.error = Some(error.to_owned());
        self.fetched_at = Some(at);
    }

    /// Age of the cached report, `None` when nothing was ever read.
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.fetched_at.map(|at| now.saturating_duration_since(at))
    }

    pub fn is_fresh(&self, now: Instant, max_age: Duration) -> bool {
        self.age(now).is_some_and(|age| age <= max_age)
    }

    pub fn is_online(&self) -> bool {
        self.report.is_some() && self.error.is_none()
    }

    pub fn soc_pct(&self) -> Option<f64> {
        self.report.as_ref().map(Report::soc_pct)
    }

    pub fn capacity_kwh(&self) -> Option<f64> {
        self.report.as_ref().and_then(Report::total_capacity_kwh)
    }

    /// Signed AC power, positive while discharging.
    pub fn power_w(&self) -> i64 {
        self.report.as_ref().map_or(0, Report::ac_power_w)
    }
}
