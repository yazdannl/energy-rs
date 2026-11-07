//! A sample to write into the 1-minute tier.

use crate::measurement::SampleKind;

/// One pack measurement as read from a device report (or derived during a replay).
///
/// `power_w` is signed AC power: positive charges the pack, negative discharges it. `soc_pct` is
/// optional because a report can omit the state of charge; the gap is preserved rather than
/// interpolated. Writing the same `(pack_id, ts)` twice replaces the row, which makes ingestion
/// idempotent.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub pack_id: i64,
    pub ts: i64,
    pub power_w: f64,
    pub soc_pct: Option<f64>,
    pub kind: SampleKind,
}

impl Sample {
    pub fn new(pack_id: i64, ts: i64, power_w: f64, soc_pct: Option<f64>) -> Self {
        Self {
            pack_id,
            ts,
            power_w,
            soc_pct,
            kind: SampleKind::Report,
        }
    }

    #[must_use]
    pub fn with_kind(mut self, kind: SampleKind) -> Self {
        self.kind = kind;
        self
    }
}
