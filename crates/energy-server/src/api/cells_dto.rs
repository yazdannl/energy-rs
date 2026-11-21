//! The cell-voltage object inside a pack's live state.

use serde::Serialize;

use energy_core::stream::CellVoltages;

/// Raw device readings for one pack: highest/lowest cell, spread and pack context, in V/mV/°C.
///
/// There is deliberately no guard verdict here. Blocking is the protection latch's statement, and
/// the latch carries hysteresis that a snapshot of the voltage alone cannot reproduce.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CellsDto {
    min_cell_v: Option<f64>,
    max_cell_v: Option<f64>,
    spread_mv: Option<f64>,
    pack_voltage_v: Option<f64>,
    max_temp_c: Option<f64>,
}

/// Render the readings exactly as the engine published them.
pub(crate) fn cells_dto(cells: &CellVoltages) -> CellsDto {
    CellsDto {
        min_cell_v: cells.min_cell_v,
        max_cell_v: cells.max_cell_v,
        spread_mv: cells.spread_mv,
        pack_voltage_v: cells.pack_voltage_v,
        max_temp_c: cells.max_temp_c,
    }
}
