//! The learned load profile: persisted through the forecast store, fresh for a week.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use energy_core::plan_source::LoadProfiles;
use energy_forecast::load::LoadSample;
use energy_ledger::Ledger;

const NOW: i64 = 1_790_000_000;

fn samples(now: i64, days: i64) -> Vec<LoadSample> {
    let end = DateTime::<Utc>::from_timestamp(now - now.rem_euclid(3600), 0).unwrap();
    let mut samples = Vec::new();
    for index in 0..days * 24 {
        let moment = end - Duration::hours(days * 24 - index);
        let hour = index % 24;
        samples.push(LoadSample {
            moment,
            load_kw: if (7..19).contains(&hour) { 2.0 } else { 0.5 },
        });
    }
    samples
}

#[test]
fn a_profile_is_learned_persisted_and_expires() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let profiles = LoadProfiles::new(Arc::clone(&ledger));
    let now = DateTime::<Utc>::from_timestamp(NOW, 0).unwrap();
    let profile = profiles.refresh(&samples(NOW, 10), now).expect("learn");
    assert!(profile.days >= 9, "the profile saw {} days", profile.days);

    let loaded = profiles.current(NOW).expect("the stored profile is served");
    assert_eq!(loaded.days, profile.days);
    assert!(loaded.median(10) > 0.0);

    // Seven days of tolerance, then the planner stops trusting it.
    assert!(profiles.current(NOW + 6 * 86_400).is_some());
    assert!(
        profiles
            .current(NOW + LoadProfiles::MAX_AGE_S + 60)
            .is_none()
    );

    // A fresh instance reads the same ledger (a restart, not a lost profile).
    let reopened = LoadProfiles::new(Arc::clone(&ledger));
    assert!(reopened.current(NOW).is_some());
}

#[test]
fn a_thin_profile_is_not_used() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    let profiles = LoadProfiles::new(ledger);
    let now = DateTime::<Utc>::from_timestamp(NOW, 0).unwrap();
    profiles.refresh(&samples(NOW, 1), now).expect("learn");
    assert!(
        profiles.current(NOW).is_none(),
        "one day of samples must not steer the battery"
    );
}
