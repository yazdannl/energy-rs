//! One ledger pack per zenSDK device: identity, registration and per-pack learning.
//!
//! A SolarFlow device is one controllable unit: it has one serial, one AC setpoint, one report and
//! its own `packData`. The ledger therefore gets **one pack row per device**, keyed by the device
//! serial, never one "hub" row with the fleet's summed capacity: a summed capacity hides which
//! pack is old, which is learning, and which refused a command. The fleet aggregate exists only for
//! the grid calculation, where the site sees four inverters as one battery.
//!
//! Capacity comes from the pack's own learning profile when one is published, and falls back to
//! its own `packData` label (`AB2000` = 1.92 kWh). Efficiency uses the shared learned > manual >
//! default rule from `round_trip`, so an operator's pre-learning entry survives an unmeasured
//! profile. Nothing here reads a clock: the caller passes the registration instant.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use energy_battery::learning::round_trip::{published_round_trip, resolve};
use energy_battery::learning::{LearningProfile, LearningStore};
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
    /// efficiency follows the shared learned > manual > default precedence: a profile that has not
    /// measured a round trip cannot hide the operator's per-pack entry.
    pub fn spec(
        &self,
        sn: &str,
        label_capacity_kwh: f64,
        max_charge_w: f64,
        max_discharge_w: f64,
    ) -> PackSpec {
        let profile = self.profile(sn);
        let capacity_kwh = profile
            .as_ref()
            .and_then(LearningProfile::effective_capacity_kwh)
            .unwrap_or(label_capacity_kwh);
        let (stored_learned, manual) = self.stored_round_trip(sn);
        let learned = profile
            .as_ref()
            .filter(|profile| profile.efficiency_applied)
            .map(|profile| profile.round_trip)
            .or(stored_learned);
        let (round_trip, _) = resolve(learned, manual);
        let eta = round_trip.sqrt();
        PackSpec {
            pack_id: sn.to_owned(),
            capacity_kwh,
            min_soc_pct: 0.0,
            max_soc_pct: 100.0,
            max_charge_w,
            max_discharge_w,
            eta_charge: eta,
            eta_discharge: eta,
        }
    }

    /// The row's published measurement and the operator's entry, in that order.
    ///
    /// A ledger read failure leaves both absent, so the typed default stays in charge.
    fn stored_round_trip(&self, sn: &str) -> (Option<f64>, Option<f64>) {
        let stored = self.ledger.packs().and_then(|packs| {
            packs.by_sn(sn).map(|pack| {
                pack.map(|pack| {
                    (
                        published_round_trip(pack.learned_round_trip, pack.learned_at),
                        pack.manual_round_trip,
                    )
                })
                .unwrap_or_default()
            })
        });
        match stored {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(pack = sn, %error, "pack round trip read failed; using the default");
                (None, None)
            }
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
