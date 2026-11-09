//! Reads and writes the single selection row.

use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::LedgerError;
use crate::selection::SelectionRow;

/// Repository for `pack_selection`.
pub struct Selection<'a> {
    conn: MutexGuard<'a, Connection>,
}

impl<'a> Selection<'a> {
    pub(crate) fn new(conn: MutexGuard<'a, Connection>) -> Self {
        Self { conn }
    }

    /// The latched selection, `None` when the fleet is idle.
    pub fn get(&self) -> Result<Option<SelectionRow>, LedgerError> {
        let row = self
            .conn
            .query_row(
                "SELECT direction, holder, selected_at, target_w FROM pack_selection WHERE id = 1",
                [],
                |row| {
                    Ok(SelectionRow {
                        direction: row.get(0)?,
                        holder: row.get(1)?,
                        selected_at: row.get(2)?,
                        target_w: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Latch a selection.
    pub fn set(&self, selection: &SelectionRow) -> Result<(), LedgerError> {
        self.conn.execute(
            "INSERT INTO pack_selection (id, direction, holder, selected_at, target_w)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                direction = excluded.direction,
                holder = excluded.holder,
                selected_at = excluded.selected_at,
                target_w = excluded.target_w",
            params![
                selection.direction,
                selection.holder,
                selection.selected_at,
                selection.target_w
            ],
        )?;
        Ok(())
    }

    /// Release the hold.
    pub fn clear(&self) -> Result<(), LedgerError> {
        self.conn
            .execute("DELETE FROM pack_selection WHERE id = 1", [])?;
        Ok(())
    }
}
