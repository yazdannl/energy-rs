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

/// A constant kWh series published only in the last `slots` quarters of the window.
fn tail_series(slots: i64, kwh: f64) -> BTreeMap<i64, f64> {
    (192 - slots..192)
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
fn an_empty_grid_source_does_not_poison_a_48h_history_window() {
    // Live shape: two tariff counters publish only the last 11 quarters of a 48 h window, a legacy
    // grid source has no rows in it at all, and PV covers the whole window.
    let legacy = BTreeMap::new();
    let import = sum_series(&[tail_series(11, 0.10), tail_series(11, 0.26), legacy.clone()]);
    let export = sum_series(&[tail_series(11, 0.05), tail_series(11, 0.02), legacy]);
    let pv = series_except(&[]);

    assert_eq!(import.len(), 11, "an absent source must not erase the tail");
    assert_eq!(import.get(&(START + 181 * SLOT_SECONDS)), Some(&0.36));
    let (window, gap) = gap_free_window(&import, &export, &pv, WINDOW)
        .expect("a three-hour gap-free tail is enough");
    assert_eq!(window.0, START + 181 * SLOT_SECONDS);
    assert_eq!(window.1, WINDOW.1);
    assert_eq!(gap, Some(START + 180 * SLOT_SECONDS));

    let history = timeline(
        &import,
        &export,
        &pv,
        &BTreeMap::new(),
        &BTreeMap::new(),
        window,
    )
    .expect("the shortened window is a valid measured history");
    assert_eq!(history.len(), 11);
}

#[test]
fn an_absent_source_does_not_hide_a_hole_in_a_publishing_source() {
    let at = START + 185 * SLOT_SECONDS;
    let mut tariff_2 = tail_series(11, 0.26);
    tariff_2.remove(&at);
    let import = sum_series(&[tail_series(11, 0.10), tariff_2, BTreeMap::new()]);
    let export = sum_series(&[tail_series(11, 0.05), tail_series(11, 0.02)]);

    assert!(
        !import.contains_key(&at),
        "a hole in a publishing tariff is still a hole"
    );
    assert_ne!(import.get(&at).copied(), Some(0.10));
    let (window, gap) = gap_free_window(&import, &export, &series_except(&[]), WINDOW)
        .expect("a shorter gap-free tail is still usable");
    assert_eq!(window.0, at + SLOT_SECONDS);
    assert_eq!(gap, Some(at));
}

#[test]
fn one_grid_source_keeps_its_existing_series() {
    let tail = tail_series(11, 0.10);
    assert_eq!(sum_series(std::slice::from_ref(&tail)), tail);
    assert!(sum_series(&[BTreeMap::new()]).is_empty());
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
