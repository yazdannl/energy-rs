//! The price store: save-once publications, idempotent saves, newest-publication loads, exact-or-
//! refuse ranges and the same semantics in memory and SQLite.  Money is canonicalised to 1e-6 so a
//! publication's identity does not depend on floating-point noise.

// Test code: a failed assertion is the report, so `unwrap`/`expect`/`panic!` are the intended
// API here, and fixture timestamps/money read better verbatim than with digit separators.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::assert_is_empty,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::unreadable_literal
)]

mod support;

use std::path::{Path, PathBuf};

use energy_price::{
    MemoryPriceRepository, PriceBook, PriceError, PriceRepository, SaveOutcome,
    SqlitePriceRepository,
};

const START: &str = "2026-11-20T00:00:00+01:00";

fn book(start: &str, buy_all_in: &[f64]) -> PriceBook {
    support::book(support::quarter_hourly(start, buy_all_in))
}

fn repositories() -> Vec<(&'static str, Box<dyn PriceRepository>)> {
    vec![
        (
            "memory",
            Box::new(MemoryPriceRepository::new()) as Box<dyn PriceRepository>,
        ),
        (
            "sqlite",
            Box::new(SqlitePriceRepository::open_in_memory().expect("in-memory sqlite opens")),
        ),
    ]
}

#[test]
fn the_published_no_tax_price_survives_a_round_trip() {
    for (name, repo) in repositories() {
        let slots: Vec<_> = support::quarter_hourly(START, &[0.20, 0.21])
            .into_iter()
            .enumerate()
            .map(|(index, slot)| {
                if index == 0 {
                    slot.with_ex_tax(Some(0.089152))
                } else {
                    slot
                }
            })
            .collect();
        let book = support::book(slots);
        repo.save_publication(&book).expect(name);
        let loaded = repo.load(0, i64::MAX).expect(name);
        assert_eq!(
            loaded.first().and_then(|slot| slot.buy_ex_tax),
            Some(0.089152),
            "{name}: the published price is stored, not re-derived"
        );
        assert_eq!(
            loaded.last().and_then(|slot| slot.buy_ex_tax),
            None,
            "{name}"
        );
    }
}

#[test]
fn saving_the_same_publication_twice_writes_once() {
    for (name, repo) in repositories() {
        let book = book(START, &[0.20, 0.21, 0.19, 0.18]);
        let first = repo.save_publication(&book).expect(name);
        assert!(first.is_new(), "{name}: the first save stores");
        let second = repo.save_publication(&book).expect(name);
        assert!(!second.is_new(), "{name}: the second save is a no-op");
        assert_eq!(first.publication().id, second.publication().id, "{name}");
        assert_eq!(first.publication().slot_count, 4, "{name}");
        assert_eq!(
            first.publication().content_hash,
            second.publication().content_hash
        );
        assert_eq!(repo.load(0, i64::MAX).expect(name).len(), 4, "{name}");
    }
}

#[test]
fn a_later_poll_of_the_same_publication_is_not_stored_again() {
    // Past slots are filtered on every poll, so the same publication arrives truncated.
    let full = book(START, &[0.20, 0.21, 0.19, 0.18]);
    let truncated = support::book(support::quarter_hourly(
        "2026-11-20T00:30:00+01:00",
        &[0.19, 0.18],
    ));
    for (name, repo) in repositories() {
        let stored = repo.save_publication(&full).expect(name);
        let again = repo.save_publication(&truncated).expect(name);
        assert!(
            !again.is_new(),
            "{name}: no new coverage, no new publication"
        );
        assert_eq!(again.publication().id, stored.publication().id, "{name}");
    }
}

#[test]
fn an_extended_publication_is_stored_beside_the_old_one() {
    for (name, repo) in repositories() {
        let first = repo
            .save_publication(&book(START, &[0.20, 0.21, 0.19, 0.18]))
            .expect(name);
        let extended = repo
            .save_publication(&book(START, &[0.20, 0.21, 0.19, 0.18, 0.25, 0.26]))
            .expect(name);
        assert!(
            extended.is_new(),
            "{name}: new coverage is a new publication"
        );
        assert!(extended.publication().id > first.publication().id, "{name}");
        assert_eq!(extended.publication().slot_count, 6, "{name}");
        let latest = repo.latest_publication().expect(name).expect(name);
        assert_eq!(latest.id, extended.publication().id, "{name}");
        assert_eq!(
            latest.first_t_start,
            support::instant(START),
            "{name}: publications are never rewritten"
        );

        let loaded = repo
            .load_book(0, i64::MAX)
            .expect("the stored range is gap-free");
        assert_eq!(loaded.len(), 6, "{name}");
        assert_eq!(
            loaded.last().map(|slot| slot.buy_all_in),
            Some(0.26),
            "{name}"
        );
    }
}

#[test]
fn a_revised_price_arrives_as_a_new_publication_and_wins_the_load() {
    for (name, repo) in repositories() {
        repo.save_publication(&book(START, &[0.20, 0.21]))
            .expect(name);
        let revised = support::book(vec![support::slot_from_instant(
            support::instant(START),
            0.30,
            energy_price::SlotSource::ZonneplanQuarterHourly,
        )]);
        let outcome = repo.save_publication(&revised).expect(name);
        assert!(
            outcome.is_new(),
            "{name}: a revised price is a new publication"
        );
        let loaded = repo
            .load(
                support::instant(START),
                support::instant("2026-11-20T00:15:00+01:00"),
            )
            .expect(name);
        assert_eq!(loaded.len(), 1, "{name}");
        assert_eq!(
            loaded[0].buy_all_in, 0.30,
            "{name}: newest publication per slot"
        );
    }
}

#[test]
fn a_hole_between_publications_is_refused_not_filled() {
    for (name, repo) in repositories() {
        repo.save_publication(&book("2026-11-20T00:00:00+01:00", &[0.20, 0.21]))
            .expect(name);
        // The 00:30 slot was never published: nothing may invent it.
        repo.save_publication(&book("2026-11-20T00:45:00+01:00", &[0.22, 0.23]))
            .expect(name);
        let error = repo
            .load_book(0, i64::MAX)
            .expect_err("the range has a hole");
        assert!(
            matches!(
                error,
                PriceError::Gap { t_start } if t_start == support::instant("2026-11-20T00:30:00+01:00")
            ),
            "{name}: {error}"
        );
        // The raw load still reports what is stored, so the UI can show the gap.
        assert_eq!(repo.load(0, i64::MAX).expect(name).len(), 4, "{name}");
    }
}

#[test]
fn money_is_canonicalised_before_it_is_hashed_and_stored() {
    let unrounded = PriceBook::new(vec![energy_price::Slot::new(
        support::instant(START),
        0.2,
        0.05715033057851243,
        energy_price::SlotSource::ZonneplanQuarterHourly,
    )])
    .expect("one slot is a valid book");
    for (name, repo) in repositories() {
        assert!(
            repo.save_publication(&unrounded).expect(name).is_new(),
            "{name}"
        );
        // The rounded twin is the same publication: the store canonicalises on write.
        let twin = book(START, &[0.2]);
        assert!(
            !repo.save_publication(&twin).expect(name).is_new(),
            "{name}"
        );
        let loaded = repo.load(0, i64::MAX).expect(name);
        assert_eq!(loaded[0].market, 0.057150, "{name}");
        assert_eq!(loaded[0], twin.slots()[0], "{name}");
    }
}

#[test]
fn a_provider_fallback_switch_mid_day_still_loads_as_one_book() {
    // Yesterday's slots came from the HA sensor, today's from the direct API: same 15-min grid.
    let ha = support::book(vec![
        support::market_slot(
            "2026-11-20T00:00:00+01:00",
            0.20,
            energy_price::SlotSource::HaQuarterHourly,
        ),
        support::market_slot(
            "2026-11-20T00:15:00+01:00",
            0.21,
            energy_price::SlotSource::HaQuarterHourly,
        ),
    ]);
    let direct = support::book(vec![support::slot_from_instant(
        support::instant("2026-11-20T00:30:00+01:00"),
        0.22,
        energy_price::SlotSource::ZonneplanQuarterHourly,
    )]);
    let repo = MemoryPriceRepository::new();
    repo.save_publication(&ha)
        .expect("the HA publication stores");
    repo.save_publication(&direct)
        .expect("the direct publication stores");
    let loaded = repo
        .load_book(0, i64::MAX)
        .expect("a same-resolution mix is one grid");
    assert_eq!(loaded.len(), 3);
    assert_eq!(
        loaded.last().map(|slot| slot.source),
        Some(energy_price::SlotSource::ZonneplanQuarterHourly)
    );

    // A 15-minute and an hourly publication do not form one book.
    let hourly = support::book(vec![support::market_slot(
        "2026-11-20T01:00:00+01:00",
        0.23,
        energy_price::SlotSource::ZonneplanHourly,
    )]);
    repo.save_publication(&hourly)
        .expect("the hourly publication stores");
    assert!(repo.load_book(0, i64::MAX).is_err());
}

#[test]
fn an_empty_publication_is_refused() {
    let empty = PriceBook::new(Vec::new()).expect("an empty book is constructible");
    for (name, repo) in repositories() {
        let error = repo.save_publication(&empty).expect_err("nothing to store");
        assert!(matches!(error, PriceError::Payload(_)), "{name}: {error}");
        assert!(repo.latest_publication().expect(name).is_none(), "{name}");
    }
}

#[test]
fn a_saved_publication_survives_reopening_the_database_file() {
    let path = temp_db_path();
    let expected = book(START, &[0.20, 0.21, 0.19]);
    {
        let repo = SqlitePriceRepository::open(&path).expect("the database opens");
        assert!(
            repo.save_publication(&expected)
                .expect("the save works")
                .is_new()
        );
    }
    {
        let repo = SqlitePriceRepository::open(&path).expect("the database reopens");
        let loaded = repo.load_book(0, i64::MAX).expect("the range is gap-free");
        assert_eq!(loaded, expected);
        let publication = repo
            .latest_publication()
            .expect("the query works")
            .expect("a publication is stored");
        assert_eq!(publication.slot_count, 3);
        assert_eq!(
            publication.source,
            energy_price::SlotSource::ZonneplanQuarterHourly
        );
        // Re-saving after a reopen is still idempotent.
        assert!(
            !repo
                .save_publication(&expected)
                .expect("the save works")
                .is_new()
        );
    }
    remove_db_files(&path);
}

#[test]
fn the_save_outcome_names_the_publication_it_touched() {
    let repo = MemoryPriceRepository::new();
    let stored = repo
        .save_publication(&book(START, &[0.20]))
        .expect("the save works");
    let SaveOutcome::Stored { publication } = &stored else {
        panic!("the first save stores a publication");
    };
    assert_eq!(publication.slot_count, 1);
    assert_eq!(publication.first_t_start, support::instant(START));
    assert_eq!(
        publication.last_t_end,
        support::instant("2026-11-20T00:15:00+01:00")
    );
    assert!(publication.covers(support::instant(START)));
    assert!(!publication.covers(publication.last_t_end));
    assert!(publication.fetched_at > 0);
}

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("energy-price-store-{}.db", std::process::id()))
}

fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let file = PathBuf::from(format!("{}{suffix}", path.display()));
        let _ = std::fs::remove_file(file);
    }
}
