//! One dispatch attempt.

/// One write attempt (or a dry-run/rejected decision).
///
/// `requested_w` is what the engine wanted, `applied_w` what the device confirmed (equal to the
/// request when verified, `0` when nothing was sent). `verified` is the echo check, not the HTTP
/// result; `error` carries the refusal or mismatch when `verified` is false. `pack_id` is `None`
/// only for a fleet-level decision that never reached a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct DispatchEntry {
    pub id: i64,
    pub ts: i64,
    pub pack_id: Option<i64>,
    pub requested_w: f64,
    pub applied_w: f64,
    pub reason: String,
    pub mode: String,
    pub verified: bool,
    pub error: Option<String>,
}

impl DispatchEntry {
    pub fn new(
        ts: i64,
        pack_id: Option<i64>,
        requested_w: f64,
        applied_w: f64,
        reason: impl Into<String>,
        mode: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            ts,
            pack_id,
            requested_w,
            applied_w,
            reason: reason.into(),
            mode: mode.into(),
            verified: false,
            error: None,
        }
    }

    #[must_use]
    pub fn verified(mut self, applied_w: f64) -> Self {
        self.applied_w = applied_w;
        self.verified = true;
        self.error = None;
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
        Ok(Self {
            id: row.get(0)?,
            ts: row.get(1)?,
            pack_id: row.get(2)?,
            requested_w: row.get(3)?,
            applied_w: row.get(4)?,
            reason: row.get(5)?,
            mode: row.get(6)?,
            verified: verified != 0,
            error: row.get(8)?,
        })
    }
}
