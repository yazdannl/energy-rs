//! Plain-text columns, shared by the doctor table and the plan-once timeline.
//!
//! Deliberately not a table library: two aligned columns and a rule are enough, and the output has
//! to stay stable for scripts that read it.

/// Widest cell per column; at least `minimum` so a header never collapses.
pub fn widths(rows: &[Vec<String>], minimum: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = Vec::new();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let width = cell.chars().count().max(minimum);
            match widths.get_mut(index) {
                Some(current) => *current = (*current).max(width),
                None => widths.push(width),
            }
        }
    }
    widths
}

/// One aligned line; the last column is not padded (no trailing whitespace).
pub fn line(row: &[String], widths: &[usize]) -> String {
    let mut out = String::new();
    for (index, cell) in row.iter().enumerate() {
        if index + 1 == row.len() {
            out.push_str(cell);
            break;
        }
        let width = widths.get(index).copied().unwrap_or(0);
        out.push_str(cell);
        out.push_str(&" ".repeat(width.saturating_sub(cell.chars().count()) + 2));
    }
    out
}
