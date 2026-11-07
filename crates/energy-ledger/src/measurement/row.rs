//! A measurement row read back from either tier.
//!
//! `samples` is `1` for a 1-minute row and the number of underlying samples for a 5-minute row; a
//! 5-minute bucket with no samples has `samples == 0` and `NULL` means. `power_w` is `None` only
//! for such an empty bucket.

/// One row of the 1-minute or 5-minute tier.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub pack_id: i64,
    pub ts: i64,
    pub power_w: Option<f64>,
    pub soc_pct: Option<f64>,
    pub samples: i64,
}

impl Measurement {
    /// Map `pack_id, ts, power_w, soc_pct, samples`.
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            pack_id: row.get(0)?,
            ts: row.get(1)?,
            power_w: row.get(2)?,
            soc_pct: row.get(3)?,
            samples: row.get(4)?,
        })
    }
}
