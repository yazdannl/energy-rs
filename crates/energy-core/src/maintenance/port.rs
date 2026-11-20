//! Shared maintenance port: settings, history and one-shot manual forcing.

use std::sync::{Arc, Mutex};

use energy_battery::protection::full_charge::{ChargeCandidate, DuePack, FullChargeSettings};

use crate::maintenance::FullChargeHistory;
use crate::plan::PackState;

/// The maintenance policy shared by the plan source and the engine.
///
/// The plan source uses it while solving; the engine uses the same instance for the dwell writer
/// and for `POST /protection`. A manual request is a pending flag, not a write: if the solve that
/// follows is refused, the flag survives for the next accepted plan and no history is stamped.
pub struct MaintenancePort {
    history: Arc<dyn FullChargeHistory>,
    settings: FullChargeSettings,
    /// `None`: no manual request. `Some([])`: fleet-wide. Otherwise the requested serials.
    forced: Mutex<Option<Vec<String>>>,
}

impl MaintenancePort {
    /// A port over the durable history and the resolved settings.
    pub fn new(history: Arc<dyn FullChargeHistory>, settings: FullChargeSettings) -> Self {
        Self {
            history,
            settings,
            forced: Mutex::new(None),
        }
    }

    /// The resolved settings (the API and the tick both read this one copy).
    pub fn settings(&self) -> &FullChargeSettings {
        &self.settings
    }

    /// Force a maintenance charge for one pack, or the whole fleet when `pack` is `None`.
    pub fn force(&self, pack: Option<&str>) {
        let Ok(mut forced) = self.forced.lock() else {
            return;
        };
        match (pack, forced.as_mut()) {
            (None, _) => *forced = Some(Vec::new()),
            (Some(sn), Some(list)) if !list.iter().any(|item| item == sn) => {
                list.push(sn.to_owned());
            }
            (Some(sn), None) => *forced = Some(vec![sn.to_owned()]),
            _ => {}
        }
    }

    /// Choose this plan's maintenance charges and clear the manual flags they satisfied.
    pub fn assignments(
        &self,
        now: i64,
        packs: &[PackState],
        slots: &[ChargeCandidate],
        budget_w: f64,
    ) -> Vec<energy_battery::protection::full_charge::FullChargeAssignment> {
        let forced = self.forced.lock().ok().and_then(|guard| guard.clone());
        let due: Vec<DuePack> = packs
            .iter()
            .filter(|pack| pack.spec.invalid().is_none())
            .map(|pack| DuePack {
                pack_id: pack.spec.pack_id.clone(),
                capacity_kwh: pack.spec.capacity_kwh,
                soc_pct: pack.soc_pct,
                eta_charge: pack.spec.eta_charge,
                max_charge_w: pack.spec.max_charge_w,
                last_full_at: self.history.last_full_at(&pack.spec.pack_id),
                forced: forced.as_ref().is_some_and(|list| {
                    list.is_empty() || list.iter().any(|sn| sn == &pack.spec.pack_id)
                }),
            })
            .collect();
        let assignments = energy_battery::protection::full_charge::interval::plan(
            now,
            &self.settings,
            slots,
            &due,
            budget_w,
        );
        if !assignments.is_empty()
            && let Ok(mut pending) = self.forced.lock()
        {
            *pending = None;
        }
        assignments
    }

    /// The durable history, for the dwell writer.
    pub fn history(&self) -> &Arc<dyn FullChargeHistory> {
        &self.history
    }
}
