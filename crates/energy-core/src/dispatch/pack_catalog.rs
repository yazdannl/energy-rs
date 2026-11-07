//! One ledger pack per zenSDK device: identity, registration and per-pack learning.
//!
//! A SolarFlow device is one controllable unit: it has one serial, one AC setpoint, one report and
//! its own `packData`. The ledger therefore gets **one pack row per device**, keyed by the device
//! serial, never one "hub" row with the fleet's summed capacity: a summed capacity hides which
//! pack is old, which is learning, and which refused a command. The fleet aggregate exists only for
//! the grid calculation, where the site sees four inverters as one battery.
//!
//! Capacity and efficiency come from the pack's own learning profile when one is published, and
//! fall back to its own `packData` label (`AB2000` = 1.92 kWh) and the typed round trip otherwise.
//! Nothing here reads a clock: the caller passes the registration instant.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use energy_battery::learning::{DEFAULT_ROUND_TRIP, LearningProfile, LearningStore};
use energy_ledger::{Ledger, Pack};

use crate::plan::PackSpec;

/// The SN ↔ ledger-id map and the per-pack physical limits.
pub struct PackCatalog {
    ledger: Arc<Ledger>,
    learning: Option<Arc<dyn LearningStore>>,
    ids: Mutex<BTreeMap<String, i64>>,
}

impl PackCatalog {
    /// A catalog over the ledger, with the optional per-pack learning store.
    pub fn new(ledger: Arc<Ledger>, learning: Option<Arc<dyn LearningStore>>) -> Self {
        Self {
            ledger,
            learning,
            ids: Mutex::new(BTreeMap::new()),
        }
    }

    /// Register a device on first sight and return its ledger id.
    ///
    /// A ledger failure is logged and yields `None`; the fleet read continues, because a database
    /// hiccup must not turn four reachable packs into an empty fleet. The next tick retries.
    pub fn register(
        &self,
        sn: &str,
        label_capacity_kwh: f64,
        max_charge_w: f64,
        max_discharge_w: f64,
        now: i64,
    ) -> Option<i64> {
        if let Some(id) = self.ledger_id(sn) {
            return Some(id);
        }
        let row = Pack::new(sn, label_capacity_kwh, now).with_limits(max_charge_w, max_discharge_w);
        match self.ledger.packs().and_then(|packs| packs.upsert(&row)) {
            Ok(stored) => {
                if let Ok(mut ids) = self.ids.lock() {
                    ids.insert(sn.to_owned(), stored.id);
                }
                Some(stored.id)
            }
            Err(error) => {
                tracing::warn!(pack = sn, %error, "pack registration failed");
                None
            }
        }
    }

    /// The ledger id a device was registered under, if any.
    pub fn ledger_id(&self, sn: &str) -> Option<i64> {
        self.ids.lock().ok().and_then(|ids| ids.get(sn).copied())
    }

    /// One device's physical limits, refined by its own learning profile.
    ///
    /// The learned capacity is used only when the learner published it
    /// ([`LearningProfile::effective_capacity_kwh`]); otherwise the device's own label stands. The
    /// efficiency split is the profile's when it is usable, else the typed default, exactly as an
    /// unlearned pack behaves today.
    pub fn spec(
        &self,
        sn: &str,
        label_capacity_kwh: f64,
        max_charge_w: f64,
        max_discharge_w: f64,
    ) -> PackSpec {
        let profile = self.profile(sn);
        let default_eta = DEFAULT_ROUND_TRIP.sqrt();
        let capacity_kwh = profile
            .as_ref()
            .and_then(LearningProfile::effective_capacity_kwh)
            .unwrap_or(label_capacity_kwh);
        let eta_charge = profile
            .as_ref()
            .map_or(default_eta, |profile| split(profile.eta_charge));
        let eta_discharge = profile
            .as_ref()
            .map_or(default_eta, |profile| split(profile.eta_discharge));
        PackSpec {
            pack_id: sn.to_owned(),
            capacity_kwh,
            min_soc_pct: 0.0,
            max_soc_pct: 100.0,
            max_charge_w,
            max_discharge_w,
            eta_charge,
            eta_discharge,
        }
    }

    /// The device's learning profile, or `None` when it was never learned or cannot be read.
    fn profile(&self, sn: &str) -> Option<LearningProfile> {
        let store = self.learning.as_ref()?;
        match store.load(sn) {
            Ok(profile) => profile,
            Err(error) => {
                tracing::warn!(pack = sn, %error, "learning profile read failed; using the label");
                None
            }
        }
    }
}

/// A usable split efficiency; anything else falls back to the typed default.
fn split(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 && value <= 1.0 {
        value
    } else {
        DEFAULT_ROUND_TRIP.sqrt()
    }
}
