//! The live state object of one pack in `GET /packs`.

use chrono::Utc;
use serde::Serialize;

use crate::api::cells_dto::{CellsDto, cells_dto};
use crate::api::dto::Timestamp;
use crate::api::dto::units::watts;

use energy_core::stream::PackSnapshot;
use energy_ledger::PackState;

/// A reading older than this is stale (five 1-minute samples).
const STALE_AFTER_S: i64 = 300;

/// Live state of one pack. `cells` inherits `sampledAt`/`stale` from the same reading.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StateDto {
    soc_pct: Option<f64>,
    power_w: Option<f64>,
    cells: Option<CellsDto>,
    sampled_at: Option<Timestamp>,
    stale: bool,
    /// Active control fault (a messageId reset or a pack that never drew power).
    control_fault: Option<String>,
    /// Historical control fault, kept after the pack recovers or goes idle.
    last_control_fault: Option<String>,
    /// A sustained power/status contradiction worth showing even though control continues.
    flow_alarm: Option<String>,
}

/// The engine's live values, falling back to the newest ledger sample before the first tick.
pub(crate) fn state_dto(live: Option<&PackSnapshot>, stored: &PackState) -> StateDto {
    let now = Utc::now().timestamp();
    if let Some(live) = live {
        return StateDto {
            soc_pct: live.soc_pct,
            power_w: live.power_w.map(watts),
            cells: Some(cells_dto(&live.cells)),
            sampled_at: Timestamp::lenient(live.sampled_at),
            stale: live.stale,
            control_fault: live.control_fault.clone(),
            last_control_fault: live.last_control_fault.clone(),
            flow_alarm: live.flow_alarm.clone(),
        };
    }
    let last = stored.last.as_ref();
    let sampled_at = last.map(|sample| sample.ts);
    StateDto {
        soc_pct: last.and_then(|sample| sample.soc_pct),
        power_w: last.and_then(|sample| sample.power_w).map(watts),
        cells: None,
        sampled_at: Timestamp::lenient(sampled_at),
        stale: sampled_at.is_none_or(|at| now - at > STALE_AFTER_S),
        control_fault: None,
        last_control_fault: None,
        flow_alarm: None,
    }
}
