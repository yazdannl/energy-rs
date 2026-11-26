//! One dispatch attempt.

/// One write attempt (or a dry-run/rejected decision).
///
/// `requested_w` is what the engine wanted. Numeric applied watts are a private typestate that
/// only [`MeasuredWatts`] can build, so an echo, a pending command or a stale read cannot carry a
/// number. `verified` is the echo check, not proof of flow; `error` carries the refusal or mismatch.
/// `pack_id` is `None` only for a fleet-level decision that never reached a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchEntry {
    pub id: i64,
    pub ts: i64,
    pub pack_id: Option<i64>,
    pub requested_w: f64,
    /// Private: the only path to `Measured` is [`MeasuredWatts`] with a sample timestamp.
    applied: AppliedWatts,
    pub reason: String,
    pub mode: String,
    pub verified: bool,
    /// Whether a command reached the device in this tick; `None` on pre-v10 rows.
    pub written: Option<bool>,
    /// Private: only a timestamped sample may set the measurement state.
    flow_measured: bool,
    /// Private: a measured row never has an unknown-watts reason.
    applied_w_reason: Option<String>,
    /// Sample clock judged by a no-flow hold, if one fired.
    judged_sample_at: Option<i64>,
    /// Write clock that verdict judged against, if one fired.
    judged_write_at: Option<i64>,
    pub error: Option<String>,
    /// Free-form note for the operator (why a maintenance hold happened, the balance current).
    pub note: Option<String>,
}

use crate::dispatch::MeasuredWatts;

#[derive(Debug, Clone, PartialEq)]
enum AppliedWatts {
    Unknown,
    Measured(MeasuredWatts),
}

impl DispatchEntry {
    pub fn new(
        ts: i64,
        pack_id: Option<i64>,
        requested_w: f64,
        reason: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            ts,
            pack_id,
            requested_w,
            applied: AppliedWatts::Unknown,
            reason: reason.into(),
            mode: mode.into(),
            verified: false,
            written: None,
            flow_measured: false,
            applied_w_reason: None,
            judged_sample_at: None,
            judged_write_at: None,
            error: None,
            note: None,
        }
    }

    /// Publish a timestamped flow sample. This is the only way numeric watts enter a row.
    #[must_use]
    pub fn measured(mut self, sample: MeasuredWatts) -> Self {
        self.applied = AppliedWatts::Measured(sample);
        self.flow_measured = true;
        // A measured row cannot simultaneously say why its watts are unknown.
        self.applied_w_reason = None;
        self
    }

    /// The numeric watts, only when a measurement actually exists.
    #[must_use]
    pub fn applied_w(&self) -> Option<f64> {
        match self.applied {
            AppliedWatts::Measured(sample) => Some(sample.watts),
            AppliedWatts::Unknown => None,
        }
    }

    /// Whether a fresh measurement exists. Never inferred from `verified`.
    #[must_use]
    pub const fn flow_measured(&self) -> bool {
        self.flow_measured
    }

    /// Why the watts are unknown, if any. Mutually exclusive with `flow_measured`.
    #[must_use]
    pub fn applied_w_reason(&self) -> Option<&str> {
        self.applied_w_reason.as_deref()
    }

    /// The release marker written only by a measuring writer; legacy rows return `None`.
    #[must_use]
    pub const fn measured_marker(&self) -> Option<i64> {
        match self.applied {
            AppliedWatts::Measured(_) => Some(1),
            AppliedWatts::Unknown => None,
        }
    }

    /// Clocks behind a no-flow verdict: judged sample and write, when present.
    #[must_use]
    pub const fn flow_verdict(&self) -> (Option<i64>, Option<i64>) {
        (self.judged_sample_at, self.judged_write_at)
    }

    #[must_use]
    pub fn with_flow_verdict(mut self, sample_at: i64, write_at: i64) -> Self {
        self.judged_sample_at = Some(sample_at);
        self.judged_write_at = Some(write_at);
        self
    }

    /// The echo check; never promotes an unknown flow to a measurement.
    #[must_use]
    pub fn verified(self) -> Self {
        self.echo_verified()
    }

    /// The device echoed the command, but no fresh sample proved the flow.
    ///
    /// Used when a sample was stale: it records the echo without claiming a measurement, so a
    /// row can never say `verified` and `flowMeasured` while carrying `appliedWReason=sample_stale`.
    #[must_use]
    pub fn echo_verified(mut self) -> Self {
        self.verified = true;
        self.error = None;
        self
    }

    #[must_use]
    pub fn written(mut self, written: bool) -> Self {
        self.written = Some(written);
        self
    }

    #[must_use]
    pub fn with_applied_w_reason(mut self, reason: impl Into<String>) -> Self {
        self.applied_w_reason = Some(reason.into());
        self.applied = AppliedWatts::Unknown;
        self.flow_measured = false;
        self
    }

    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    #[must_use]
    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.verified = false;
        self.error = Some(error.into());
        self
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let verified: i64 = row.get(7)?;
        // Only the v13 release marker proves a measurement. A legacy `flow_measured = verified`
        // flag or an echo can never promote a stored zero into sampled watts.
        let flow_measured = row.get::<_, Option<i64>>(13)?.unwrap_or_default() == 1;
        let applied = match row.get::<_, Option<f64>>(4)?.filter(|_| flow_measured) {
            Some(watts) => AppliedWatts::Measured(MeasuredWatts::new(watts, row.get(1)?)),
            None => AppliedWatts::Unknown,
        };
        // Normalise inconsistent historical rows rather than publishing both claims.
        let applied_w_reason = if flow_measured { None } else { row.get(12)? };
        Ok(Self {
            id: row.get(0)?,
            ts: row.get(1)?,
            pack_id: row.get(2)?,
            requested_w: row.get(3)?,
            applied,
            reason: row.get(5)?,
            mode: row.get(6)?,
            verified: verified != 0,
            error: row.get(8)?,
            note: row.get(9)?,
            written: row.get(10)?,
            flow_measured,
            applied_w_reason,
            judged_sample_at: row.get(14)?,
            judged_write_at: row.get(15)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_state_never_publishes_measured_watts_and_an_unknown_reason() {
        let measured = DispatchEntry::new(1, None, 900.0, "dispatch", "auto")
            .measured(MeasuredWatts::new(900.0, 1));
        let with_reason = measured.clone().with_applied_w_reason("sample_stale");
        assert_eq!(with_reason.applied_w(), None);
        assert!(!with_reason.flow_measured());
        assert_eq!(with_reason.applied_w_reason(), Some("sample_stale"));

        let cleared = with_reason.measured(MeasuredWatts::new(900.0, 2));
        assert_eq!(cleared.applied_w(), Some(900.0));
        assert!(cleared.flow_measured());
        assert_eq!(cleared.applied_w_reason(), None);
        assert_eq!(cleared.measured_marker(), Some(1));
    }
}
