//! Required grid categories need one authoritative source; alternatives never double-count.

use crate::horizon::{
    EmptyReason, GridSeries, GridSource, GridSourceCoverage, HorizonRefusal, NoticeLog,
    SourceOutcome,
};

/// Required statistics categories for a plan.
pub const REQUIRED_CATEGORIES: [&str; 3] = ["grid_import", "grid_export", "pv"];

/// Sum configured histories over the covered slots, recording every source's typed outcome.
pub fn sum_grid_sources(
    sources: &[GridSource],
    slots: usize,
    notices: &mut NoticeLog,
    coverage: &mut Vec<GridSourceCoverage>,
) -> Result<GridSeries, HorizonRefusal> {
    let mut rows: Vec<GridSourceCoverage> = sources
        .iter()
        .map(|source| source_row(source, slots, notices))
        .collect();
    let mut partial = Vec::new();
    let checked: Result<Vec<usize>, HorizonRefusal> = REQUIRED_CATEGORIES
        .iter()
        .map(|category| check_category(sources, &mut rows, category))
        .collect();
    // Both rows and notices reach the caller even when a category refuses.
    coverage.clone_from(&rows);
    let selected = checked?;
    mark_redundant(sources, &mut rows, &selected, notices, &mut partial);
    coverage.clone_from(&rows);
    Ok(GridSeries {
        values: series_values(sources, &rows, slots),
        coverage: rows,
        partial_sources: partial,
    })
}

fn source_row(source: &GridSource, slots: usize, notices: &mut NoticeLog) -> GridSourceCoverage {
    let present = if source.available {
        source
            .slots
            .iter()
            .take(slots)
            .filter(|value| matches!(value, Some(number) if number.is_finite()))
            .count()
    } else {
        notices.emit_once(&format!("source_unavailable:{}", source.source_id));
        0
    };
    let missing = slots.saturating_sub(present);
    if missing > 0 {
        notices.emit_once(&format!("source_missing:{}", source.source_id));
    }
    let outcome = if missing == 0 {
        SourceOutcome::Ok
    } else if source.required {
        SourceOutcome::PartialRequired
    } else {
        SourceOutcome::OptionalZero
    };
    let empty_reason = if missing == 0 {
        None
    } else if present == 0 {
        Some(EmptyReason::AllEmpty)
    } else {
        Some(EmptyReason::MissingSlots)
    };
    GridSourceCoverage {
        source_id: source.source_id.clone(),
        required: source.required,
        slots_present: present,
        slots_missing: missing,
        empty_reason,
        outcome,
    }
}

fn check_category(
    sources: &[GridSource],
    rows: &mut [GridSourceCoverage],
    category: &str,
) -> Result<usize, HorizonRefusal> {
    let mut chosen = None;
    let mut fallback = None;
    for (index, source) in sources.iter().enumerate() {
        if source.category != category || !source.required {
            continue;
        }
        let full = source.available && rows[index].slots_missing == 0;
        if full && chosen.is_none() {
            chosen = Some(index);
        } else if !full
            && fallback.is_none_or(|(_, best): (usize, usize)| rows[index].slots_missing < best)
        {
            fallback = Some((index, rows[index].slots_missing));
        }
    }
    if let Some(index) = chosen {
        return Ok(index);
    }
    let (index, missing) = fallback.unwrap_or((0, 0));
    let source_id = if fallback.is_some() {
        sources[index].source_id.clone()
    } else {
        category.to_owned()
    };
    let empty_reason = if fallback.is_some() {
        rows[index].empty_reason
    } else {
        Some(EmptyReason::AllEmpty)
    };
    for (row, source) in rows.iter_mut().zip(sources) {
        if source.category == category && source.required {
            row.outcome = SourceOutcome::InputsUnavailable;
        }
    }
    Err(HorizonRefusal::InputsUnavailable {
        missing_slots: missing,
        source_id,
        empty_reason,
    })
}

fn mark_redundant(
    sources: &[GridSource],
    rows: &mut [GridSourceCoverage],
    selected: &[usize],
    notices: &mut NoticeLog,
    partial: &mut Vec<String>,
) {
    for (index, source) in sources.iter().enumerate() {
        if !source.required {
            continue;
        }
        if rows[index].outcome == SourceOutcome::PartialRequired {
            partial.push(source.source_id.clone());
        }
        if selected.contains(&index) || rows[index].slots_missing > 0 {
            continue;
        }
        if rows[index].outcome == SourceOutcome::Ok {
            rows[index].outcome = SourceOutcome::Redundant;
            notices.emit_once(&format!("source_redundant:{}", source.source_id));
            partial.push(source.source_id.clone());
        }
    }
}

fn series_values(sources: &[GridSource], rows: &[GridSourceCoverage], slots: usize) -> Vec<f64> {
    let mut values = vec![0.0; slots];
    for (source, row) in sources.iter().zip(rows) {
        if row.outcome != SourceOutcome::Ok || !source.available {
            continue;
        }
        for (index, value) in values.iter_mut().enumerate() {
            if let Some(measurement) = source.slots.get(index)
                && let Some(measurement) = measurement
                && measurement.is_finite()
            {
                *value += *measurement;
            }
        }
    }
    values
}
