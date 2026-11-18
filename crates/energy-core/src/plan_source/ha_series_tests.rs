//! Reading the energy statistics into a validated, gap-free history window.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use super::{SLOT_SECONDS, gap_free_window, published_end, sum_series, timeline};
use crate::plan::PlanError;

const START: i64 = 1_790_000_000 - 1_790_000_000 % SLOT_SECONDS;
const WINDOW: (i64, i64) = (START, START + 192 * SLOT_SECONDS);

/// A series of `slots` one-kWh quarter-hours from `START`.
fn series(slots: i64) -> BTreeMap<i64, f64> {
    (0..slots)
        .map(|index| (START + index * SLOT_SECONDS, 1.0))
        .collect()
}

/// A window of `slots` constant kWh values from `START`.
fn valued_series(slots: i64, kwh: f64) -> BTreeMap<i64, f64> {
    (0..slots)
        .map(|index| (START + index * SLOT_SECONDS, kwh))
        .collect()
}

/// A full window of one kWh per slot, minus the given missing starts.
fn series_except(missing: &[i64]) -> BTreeMap<i64, f64> {
    (0..192)
        .filter(|index| !missing.contains(&(START + *index * SLOT_SECONDS)))
        .map(|index| (START + index * SLOT_SECONDS, 1.0))
        .collect()
}

#[test]
fn the_unpublished_tail_is_trimmed_not_refused() {
    let window = (START, START + 4 * SLOT_SECONDS);
    let end = published_end(&series(3), &series(3), &series(3), window.0, window.1);
    assert_eq!(
        end,
        START + 3 * SLOT_SECONDS,
        "the last published slot wins"
    );
    let history = timeline(
        &series(3),
        &series(3),
        &series(3),
        &BTreeMap::new(),
        &BTreeMap::new(),
        window,
    )
    .expect("a lagging publication is not a refusal");
    assert_eq!(history.len(), 3);
}

#[test]
fn an_interior_hole_is_still_refused() {
    let window = (START, START + 3 * SLOT_SECONDS);
    let mut holed = series(3);
    holed.remove(&(START + SLOT_SECONDS));
    let error = timeline(
        &holed,
        &series(3),
        &series(3),
        &BTreeMap::new(),
        &BTreeMap::new(),
        window,
    )
    .expect_err("an interior gap must refuse");
    assert!(error.to_string().contains("gap"), "{error}");
}

#[test]
fn two_import_sources_sum_to_the_true_import_per_slot() {
    let tariff_1 = valued_series(192, 0.10);
    let tariff_2 = valued_series(192, 0.26);
    let import = sum_series(&[tariff_1, tariff_2]);

    assert_eq!(import.len(), 192);
    assert_eq!(import.get(&(START + SLOT_SECONDS)), Some(&0.36));
    let (window, gap) = gap_free_window(&import, &series_except(&[]), &series_except(&[]), WINDOW)
        .expect("a sum of complete sources has no gaps");
    assert_eq!(window, WINDOW);
    assert_eq!(gap, None);
}

#[test]
fn a_missing_slot_in_one_import_source_is_a_gap_not_a_partial_sum() {
    let at = START + 72 * SLOT_SECONDS;
    let mut tariff_2 = valued_series(192, 0.26);
    tariff_2.remove(&at);
    let import = sum_series(&[valued_series(192, 0.10), tariff_2]);

    assert!(
        !import.contains_key(&at),
        "a source's hole stays a hole in the sum"
    );
    assert_ne!(
        import.get(&at).copied(),
        Some(0.10),
        "the other tariff alone is not the import"
    );
    let (window, gap) = gap_free_window(&import, &series_except(&[]), &series_except(&[]), WINDOW)
        .expect("a long gap-free tail is still usable");
    assert_eq!(window.0, at + SLOT_SECONDS, "start after the gap");
    assert_eq!(window.1, WINDOW.1);
    assert_eq!(gap, Some(at));

    let full = series_except(&[]);
    let error = timeline(
        &import,
        &full,
        &full,
        &BTreeMap::new(),
        &BTreeMap::new(),
        WINDOW,
    )
    .expect_err("an unshrunk window still refuses the missing slot");
    assert!(error.to_string().contains("gap"), "{error}");
}

#[test]
fn no_gaps_keeps_the_window() {
    let full = series_except(&[]);
    let (window, gap) = gap_free_window(&full, &full, &full, WINDOW).expect("a full window");
    assert_eq!(window, WINDOW);
    assert_eq!(gap, None);
}

#[test]
fn a_gap_starts_the_window_at_the_next_slot() {
    let at = START + 72 * SLOT_SECONDS; // 18:00 on the first day of the window
    let holed = series_except(&[at]);
    let full = series_except(&[]);
    let (window, gap) =
        gap_free_window(&holed, &full, &full, WINDOW).expect("a long gap-free tail");
    assert_eq!(
        window.0,
        at + SLOT_SECONDS,
        "start after the statistics gap"
    );
    assert_eq!(window.1, WINDOW.1);
    assert_eq!(gap, Some(at));
}

#[test]
fn a_series_missing_everywhere_refuses() {
    let full = series_except(&[]);
    let error = gap_free_window(&full, &full, &BTreeMap::new(), WINDOW)
        .expect_err("no contiguous measurements exists");
    match error {
        PlanError::StatsGap { series, t_start } => {
            assert_eq!(series, "pv/load");
            assert_eq!(t_start, WINDOW.0);
        }
        other => panic!("expected a stats gap, got {other}"),
    }
}

#[test]
fn a_tail_shorter_than_one_hour_refuses() {
    let at = START + 189 * SLOT_SECONDS; // only three slots remain
    let holed = series_except(&[at]);
    let full = series_except(&[]);
    let error = gap_free_window(&holed, &full, &full, WINDOW)
        .expect_err("three slots are not enough of an anchor");
    match error {
        PlanError::StatsGap { series, t_start } => {
            assert_eq!(series, "pv/load");
            assert_eq!(t_start, at);
        }
        other => panic!("expected a stats gap, got {other}"),
    }
}
