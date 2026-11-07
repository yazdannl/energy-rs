//! Query API round trips: latest pack state, plans and slots, dispatch, savings and as-of settings.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod support;

use energy_ledger::{DispatchEntry, Event, EventLevel, Plan, PlanSlot, SavingsDaily, Tier};
use support::{TempDatabase, register_pack, sample, write};

#[test]
fn latest_state_is_the_newest_sample_of_each_pack() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let first = register_pack(&ledger, "ABC123");
    let second = register_pack(&ledger, "ABC456");
    write(
        &ledger,
        &[
            sample(first, 60, 100.0, Some(40.0)),
            sample(first, 120, 200.0, Some(41.0)),
            sample(second, 60, -100.0, Some(80.0)),
        ],
    );
    let states = ledger.packs().unwrap().states().unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].pack.id, first);
    assert_eq!(states[0].last.as_ref().unwrap().ts, 120);
    assert_eq!(states[0].last.as_ref().unwrap().power_w, Some(200.0));
    assert_eq!(states[1].last.as_ref().unwrap().ts, 60);
    assert_eq!(
        ledger
            .measurements()
            .unwrap()
            .latest(first)
            .unwrap()
            .unwrap()
            .ts,
        120
    );
    assert!(
        ledger
            .measurements()
            .unwrap()
            .latest(999)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_pack_upsert_keeps_the_first_seen_and_known_limits() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let packs = ledger.packs().unwrap();
    let mut pack = energy_ledger::Pack::new("ABC123", 1.92, 100);
    pack.last_seen = 100;
    let stored = packs.upsert(&pack).unwrap();
    assert_eq!(stored.first_seen, 100);
    let mut later = energy_ledger::Pack::new("ABC123", 1.92, 900);
    later.last_seen = 900;
    let updated = packs.upsert(&later).unwrap();
    assert_eq!(updated.id, stored.id);
    assert_eq!(updated.first_seen, 100);
    assert_eq!(updated.last_seen, 900);
    let limited = energy_ledger::Pack {
        id: 0,
        sn: "ABC123".to_owned(),
        name: Some("garage".to_owned()),
        label_capacity_kwh: 1.92,
        reported_capacity_kwh: None,
        max_charge_w: Some(800.0),
        max_discharge_w: Some(800.0),
        first_seen: 900,
        last_seen: 1_000,
        ..energy_ledger::Pack::new("ABC123", 1.92, 900)
    };
    packs.upsert(&limited).unwrap();
    let mut blank = energy_ledger::Pack::new("ABC123", 1.92, 1_000);
    blank.last_seen = 1_100;
    let stored = packs.upsert(&blank).unwrap();
    assert_eq!(stored.max_charge_w, Some(800.0));
    assert_eq!(stored.name.as_deref(), Some("garage"));
    assert_eq!(stored.last_seen, 1_100);
    assert_eq!(packs.by_sn("ABC123").unwrap().unwrap().id, stored.id);
    assert_eq!(packs.get(stored.id).unwrap().unwrap().sn, "ABC123");
    assert!(packs.get(999).unwrap().is_none());
}

#[test]
fn a_plan_round_trips_with_its_slots_and_becomes_current() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let mut first = PlanSlot::at(0, 1_000);
    first.buy_all_in = 0.25;
    first.feed_in = 0.06;
    first.pv_kw = 1.5;
    first.load_kw = 0.4;
    first.battery_kw = 0.8;
    first.soc_kwh = 3.0;
    first.target_soc_kwh = 3.8;
    first.reason = "cheap".to_owned();
    let second = PlanSlot::at(1, 1_900);
    let plan = Plan::new(900, "dry_run", 1_000, 2_800, "{\"cost\":1.2}");
    let stored = ledger
        .plans()
        .unwrap()
        .insert(&plan, &[first, second])
        .unwrap();
    assert!(stored.id > 0);
    assert_eq!(
        ledger.plans().unwrap().current().unwrap().unwrap().id,
        stored.id
    );
    let slots = ledger.plans().unwrap().slots(stored.id).unwrap();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].plan_id, stored.id);
    assert_eq!(slots[0].buy_all_in, 0.25);
    assert_eq!(slots[0].reason, "cheap");
    assert_eq!(slots[1].idx, 1);
    assert_eq!(
        ledger
            .plans()
            .unwrap()
            .current()
            .unwrap()
            .unwrap()
            .summary_json,
        "{\"cost\":1.2}"
    );
}

#[test]
fn plans_are_ordered_by_creation_and_the_newest_wins() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let plans = ledger.plans().unwrap();
    let older = plans
        .insert(&Plan::new(1_000, "advisory", 1_000, 2_000, "{}"), &[])
        .unwrap();
    let newer = plans
        .insert(&Plan::new(2_000, "auto", 2_000, 3_000, "{}"), &[])
        .unwrap();
    assert_eq!(plans.current().unwrap().unwrap().id, newer.id);
    let range = plans.range(0, 3_000).unwrap();
    assert_eq!(range.len(), 2);
    assert_eq!(range[0].id, older.id);
    assert_eq!(plans.range(2_000, 3_000).unwrap().len(), 1);
}

#[test]
fn a_dispatch_entry_round_trips_verified_and_failed() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    let dispatch = ledger.dispatch().unwrap();
    let verified =
        DispatchEntry::new(100, Some(pack), 800.0, 800.0, "charge", "auto").verified(795.0);
    let failed = DispatchEntry::new(200, Some(pack), -500.0, 0.0, "discharge", "auto")
        .failed("serial mismatch")
        .with_note("balance current during the maintenance hold");
    let verified_id = dispatch.insert(&verified).unwrap();
    let failed_id = dispatch.insert(&failed).unwrap();
    assert!(verified_id < failed_id);
    let rows = dispatch.range(0, 300).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].verified);
    assert_eq!(rows[0].applied_w, 795.0);
    assert_eq!(rows[0].error, None);
    assert!(!rows[1].verified);
    assert_eq!(rows[1].error.as_deref(), Some("serial mismatch"));
    assert_eq!(
        rows[1].note.as_deref(),
        Some("balance current during the maintenance hold")
    );
}

#[test]
fn savings_days_round_trip_rounded_to_a_micro_euro() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let mut day = SavingsDaily::new("2026-06-15", 0.1 + 0.2, 1.0);
    day.import_kwh = 2.5;
    day.export_kwh = 1.5;
    day.pv_kwh = 4.0;
    day.battery_charge_kwh = 1.5;
    day.battery_discharge_kwh = 1.2;
    day.coverage_hours = 23.5;
    day.complete = false;
    ledger.savings().unwrap().upsert(&day).unwrap();
    let stored = ledger
        .savings()
        .unwrap()
        .get("2026-06-15")
        .unwrap()
        .unwrap();
    assert_eq!(stored.actual_eur, 0.3);
    assert!((stored.saving_eur - 0.7).abs() < 1e-12);
    assert_eq!(stored.import_kwh, 2.5);
    assert_eq!(stored.coverage_hours, 23.5);
    assert!(!stored.complete);
    assert_eq!(
        ledger
            .savings()
            .unwrap()
            .range("2026-06-01", "2026-06-30")
            .unwrap()
            .len(),
        1
    );
    assert!(
        ledger
            .savings()
            .unwrap()
            .get("2026-06-16")
            .unwrap()
            .is_none()
    );
}

#[test]
fn settings_are_as_of_readable() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let settings = ledger.settings().unwrap();
    assert_eq!(settings.get("mode").unwrap(), None);
    assert_eq!(settings.as_of("mode", 1_000).unwrap(), None);
    settings.set("mode", "advisory", 1_000).unwrap();
    settings.set("mode", "dry_run", 2_000).unwrap();
    assert_eq!(settings.get("mode").unwrap().as_deref(), Some("dry_run"));
    assert_eq!(
        settings.as_of("mode", 1_500).unwrap().as_deref(),
        Some("advisory")
    );
    assert_eq!(
        settings.as_of("mode", 2_000).unwrap().as_deref(),
        Some("dry_run")
    );
    assert_eq!(settings.as_of("mode", 999).unwrap(), None);
    settings.set("mode", "dry_run", 3_000).unwrap();
    assert_eq!(
        settings.as_of("mode", 2_500).unwrap().as_deref(),
        Some("dry_run")
    );
    assert_eq!(
        settings.as_of("mode", 3_500).unwrap().as_deref(),
        Some("dry_run")
    );
    settings.set("mode", "auto", 4_000).unwrap();
    assert_eq!(
        settings.as_of("mode", 3_500).unwrap().as_deref(),
        Some("dry_run")
    );
    assert_eq!(
        settings.as_of("mode", 4_000).unwrap().as_deref(),
        Some("auto")
    );
}

#[test]
fn events_round_trip_with_level_and_payload() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let events = ledger.events().unwrap();
    let info = Event::new(100, EventLevel::Info, "mode", "dry run started");
    let error = Event::new(200, EventLevel::Error, "ha.offline", "no connection")
        .with_data("{\"retries\":3}");
    events.insert(&info).unwrap();
    events.insert(&error).unwrap();
    let recent = events.recent(10).unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].level, EventLevel::Error);
    assert_eq!(recent[0].data_json.as_deref(), Some("{\"retries\":3}"));
    assert_eq!(events.range(0, 150).unwrap()[0].level, EventLevel::Info);
}

#[test]
fn measurement_range_reads_only_the_requested_window() {
    let db = TempDatabase::new();
    let ledger = db.ledger();
    let pack = register_pack(&ledger, "ABC123");
    write(
        &ledger,
        &[
            sample(pack, 60, 100.0, Some(40.0)),
            sample(pack, 120, 200.0, Some(41.0)),
            sample(pack, 180, 300.0, Some(42.0)),
        ],
    );
    let rows = ledger
        .measurements()
        .unwrap()
        .range(Tier::OneMinute, pack, 120, 180)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ts, 120);
    assert_eq!(rows[0].power_w, Some(200.0));
    assert_eq!(ledger.retention().plans_kept, 200);
    assert_eq!(ledger.retention().dispatch_kept, 20_000);
    assert!(ledger.path().ends_with("ledger.db"));
}
