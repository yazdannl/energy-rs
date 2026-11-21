//! Does the pack actually do what it was told?

use crate::dispatch::{HOLD_DEADBAND_W, PackReading, PackReport};

/// How far a pack's reported power may sit from its command before it counts as ignoring it.
///
/// A pack reports its own measurement a few seconds behind the command, and a ramp moves in steps,
/// so a tight bound would flag every legitimate transition.
pub const TOLERANCE_W: f64 = 250.0;

/// Consecutive non-compliant ticks before the pack is treated as not following.
const STRIKES: u8 = 3;

/// The per-pack compliance streak.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Readback {
    /// Consecutive ticks whose measured power contradicted the command.
    pub strikes: u8,
    /// When the run started, UTC epoch seconds; `None` while compliant.
    pub since: Option<i64>,
    /// The last command we asked the pack to follow, used to judge recovery during backoff.
    pub expected_w: Option<f64>,
}

impl Readback {
    /// Fold one tick's command and measurement in; returns the new streak.
    ///
    /// A command inside the hold deadband expects silence, so a pack that keeps delivering is as
    /// non-compliant as one that refuses to deliver: the point is that the device and the engine
    /// disagree about what it is doing.
    #[must_use]
    pub fn fold(
        mut self,
        report: &PackReport,
        reading: Option<&PackReading>,
        now: i64,
        backed_off: bool,
    ) -> Self {
        let Some(reading) = reading else {
            // The pack vanished from the read: availability/staleness rules handle that case.
            return Self::default();
        };
        if !report.written {
            if !backed_off {
                return Self::default();
            }
            // A suppressed tick carries no new command, so it must not add strikes; but a pack
            // that has come back into compliance is real recovery evidence and ends the streak.
            if self
                .expected_w
                .is_some_and(|expected| complies(reading, expected))
            {
                return Self::default();
            }
            return self;
        }
        let commanded = if report.fail_safe {
            0.0
        } else {
            report.intended_w
        };
        self.expected_w = Some(commanded);
        if complies(reading, commanded) {
            return Self {
                expected_w: self.expected_w,
                ..Self::default()
            };
        }
        self.strikes = self.strikes.saturating_add(1);
        self.since.get_or_insert(now);
        self
    }

    /// Whether the pack has ignored its command long enough to be treated as not following.
    pub const fn ignoring(&self) -> bool {
        self.strikes >= STRIKES
    }
}

/// The real fleet supplies direction-specific evidence; scripted fakes fall back to signed power.
fn complies(reading: &PackReading, commanded: f64) -> bool {
    if let Some(flow) = reading.flow {
        return flow.follows(commanded, TOLERANCE_W);
    }
    let measured = reading.power_w;
    (measured - commanded).abs() <= TOLERANCE_W
        || (commanded.abs() <= HOLD_DEADBAND_W && measured.abs() <= TOLERANCE_W)
}
