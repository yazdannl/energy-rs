//! One plan through the real [`LivePlanSource`] over the current data, with no writes anywhere.
//!
//! The probe opens the price store and the ledger read-only, builds the pack states from live
//! reports (or the last ledger samples), and calls the same `plan` the engine calls. It never
//! changes the control mode and never constructs a [`DeviceController`](energy_battery::control::DeviceController)
//! write path.

use std::sync::Arc;

use energy_battery::fleet::DeviceSnapshot;
use energy_core::engine::PlanSource;
use energy_core::plan::{Plan, PlanError};
use energy_core::plan_source::{HaPlanSupply, LivePlanSource, LoadProfiles, SourceConfig};
use energy_ha::Connection;
use energy_ledger::Ledger;
use energy_price::{Contract, PriceRepository, Settlement, SqlitePriceRepository};

use crate::bootstrap::options::Options;
use crate::diag::pack_states;

/// The outcome of one plan attempt, plus what the attempt had to work with.
#[derive(Debug)]
pub struct PlanProbe {
    pub plan: Result<Plan, PlanError>,
    /// Pack states the plan was built from.
    pub packs: usize,
    /// Days in the learned load profile (`0` when the fallback shape is used).
    pub profile_days: usize,
    /// Read-side notes (missing ledger, no profile) that did not stop the plan.
    pub notes: Vec<String>,
}

/// Build and solve one plan at `now`; any read failure becomes a typed refusal, never a panic.
pub async fn run(
    connection: Arc<Connection>,
    options: &Options,
    devices: &[DeviceSnapshot],
    now: i64,
) -> PlanProbe {
    let mut notes = Vec::new();
    let repository = match SqlitePriceRepository::open_read_only(options.database()) {
        Ok(repository) => repository,
        Err(error) => {
            return refused(format!("no readable price store: {error}"));
        }
    };
    let ledger = open_ledger(options, &mut notes);
    let profiles = ledger
        .as_ref()
        .map(|ledger| Arc::new(LoadProfiles::new(Arc::clone(ledger))));
    let profile_days = profiles
        .as_ref()
        .and_then(|profiles| profiles.current(now))
        .map_or(0, |profile| profile.days);
    if profiles.is_none() {
        notes.push("no learned load profile: residential fallback".to_owned());
    }
    let packs = pack_states::build(ledger.as_deref(), devices);
    if packs.is_empty() {
        return PlanProbe {
            plan: Err(PlanError::NoPacks),
            packs: 0,
            profile_days,
            notes,
        };
    }
    let count = packs.len();
    let mut supply = HaPlanSupply::new(
        connection,
        Arc::new(repository) as Arc<dyn PriceRepository>,
        Settlement::new(Contract::zonneplan_2026()),
    );
    if let Some(profiles) = profiles {
        supply = supply.with_load_profiles(profiles);
    }
    let source = LivePlanSource::new(supply, SourceConfig::default());
    let plan = source.plan(now, packs).await;
    PlanProbe {
        plan,
        packs: count,
        profile_days,
        notes,
    }
}

fn open_ledger(options: &Options, notes: &mut Vec<String>) -> Option<Arc<Ledger>> {
    let path = options.database();
    if !path.exists() {
        notes.push(format!("no ledger at {}", path.display()));
        return None;
    }
    match Ledger::open_read_only(&path) {
        Ok(ledger) => Some(Arc::new(ledger)),
        Err(error) => {
            notes.push(format!("ledger is unreadable: {error}"));
            None
        }
    }
}

/// A probe refused before any read (no transport, no connection), for the CLI.
pub fn refused(reason: String) -> PlanProbe {
    PlanProbe {
        plan: Err(PlanError::Supply { reason }),
        packs: 0,
        profile_days: 0,
        notes: Vec::new(),
    }
}
