//! One generated plan.

/// A plan header. The timeline lives in [`PlanSlot`](crate::plan::PlanSlot) rows keyed by this
/// plan's id. `summary_json` is an opaque JSON object for the UI (costs, savings estimate,
/// warnings); the ledger stores it verbatim. `mode` is the control mode the plan was produced in
/// (`advisory`, `dry_run`, `auto`).
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub id: i64,
    pub created_at: i64,
    pub mode: String,
    pub horizon_start: i64,
    pub horizon_end: i64,
    pub summary_json: String,
}

impl Plan {
    /// A plan header with no id yet (assigned by [`Plans::insert`](crate::plan::Plans::insert)).
    pub fn new(
        created_at: i64,
        mode: impl Into<String>,
        horizon_start: i64,
        horizon_end: i64,
        summary_json: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            created_at,
            mode: mode.into(),
            horizon_start,
            horizon_end,
            summary_json: summary_json.into(),
        }
    }

    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            created_at: row.get(1)?,
            mode: row.get(2)?,
            horizon_start: row.get(3)?,
            horizon_end: row.get(4)?,
            summary_json: row.get(5)?,
        })
    }
}
