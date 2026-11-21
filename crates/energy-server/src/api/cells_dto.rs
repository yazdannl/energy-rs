//! The cell-voltage object inside a pack's live state.

use serde::Serialize;

use energy_battery::protection::CellConfig;
use energy_core::stream::CellVoltages;

/// The device's highest/lowest cell, their spread and the pack context, in V/mV/°C.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CellsDto {
    min_cell_v: Option<f64>,
    max_cell_v: Option<f64>,
    spread_mv: Option<f64>,
    pack_voltage_v: Option<f64>,
    max_temp_c: Option<f64>,
    /// `below_cutoff`, `below_resume` or `ok`: the lowest cell against the guard's own config.
    level: Option<&'static str>,
}

/// Render the cell readings, judging the level with the same config the guard uses.
pub(crate) fn cells_dto(cells: &CellVoltages) -> CellsDto {
    let guard = CellConfig::default();
    let level = cells.min_cell_v.map(|voltage| {
        if voltage < guard.cutoff_v {
            "below_cutoff"
        } else if voltage < guard.resume_v {
            "below_resume"
        } else {
            "ok"
        }
    });
    CellsDto {
        min_cell_v: cells.min_cell_v,
        max_cell_v: cells.max_cell_v,
        spread_mv: cells.spread_mv,
        pack_voltage_v: cells.pack_voltage_v,
        max_temp_c: cells.max_temp_c,
        level,
    }
}
