//! What the server discovered, plus the faults only it can see.
//!
//! The engine publishes the control truth; this cache is the discovery/fault half of the same
//! snapshot. It is written by the long-running loops (dashboard discovery, prices, the load profile,
//! the HA link) and read by every snapshot publish, so those loops never block a tick.

use std::collections::BTreeMap;
use std::sync::Mutex;

use energy_core::stream::{FaultSnapshot, RuntimeView, SiteSnapshot, SiteSource};
use energy_ha::SiteModel;

/// The server's half of [`RuntimeView`].
#[derive(Debug, Default)]
pub struct SiteCache {
    site: Mutex<SiteSnapshot>,
    faults: Mutex<BTreeMap<String, FaultSnapshot>>,
}

impl SiteCache {
    /// An empty cache: nothing discovered, nothing wrong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the discovered dashboard.
    pub fn set_site(&self, site: SiteSnapshot) {
        if let Ok(mut current) = self.site.lock() {
            *current = site;
        }
    }

    /// Open or close one fault; `None` clears it.
    pub fn set_fault(&self, kind: &str, message: Option<String>, since: i64) {
        let Ok(mut faults) = self.faults.lock() else {
            return;
        };
        match message {
            Some(message) => {
                faults.insert(
                    kind.to_owned(),
                    FaultSnapshot {
                        kind: kind.to_owned(),
                        message,
                        since,
                    },
                );
            }
            None => {
                faults.remove(kind);
            }
        }
    }
}

impl RuntimeView for SiteCache {
    fn site(&self) -> SiteSnapshot {
        self.site
            .lock()
            .map(|site| site.clone())
            .unwrap_or_default()
    }

    fn faults(&self) -> Vec<FaultSnapshot> {
        self.faults
            .lock()
            .map(|faults| faults.values().cloned().collect())
            .unwrap_or_default()
    }
}

/// Map the dashboard model onto the API's source list.
pub fn site_snapshot(site: &SiteModel) -> SiteSnapshot {
    let mut sources = Vec::new();
    push(&mut sources, "grid", site.grid.import.as_ref(), "import");
    push(&mut sources, "grid", site.grid.export.as_ref(), "export");
    push(
        &mut sources,
        "solar",
        site.solar.energy.as_ref(),
        "production",
    );
    for battery in &site.batteries {
        push(&mut sources, "battery", battery.charge.as_ref(), "charge");
        push(&mut sources, "battery", battery.soc.as_ref(), "level");
    }
    let mut capabilities = Vec::new();
    if site.configured {
        capabilities.push("battery_control".to_owned());
        capabilities.push("savings".to_owned());
    } else {
        capabilities.push("no_dashboard".to_owned());
    }
    SiteSnapshot {
        configured: site.configured,
        sources,
        capabilities,
    }
}

fn push(sources: &mut Vec<SiteSource>, kind: &str, entity: Option<&String>, label: &str) {
    if let Some(entity) = entity.filter(|id| !id.trim().is_empty()) {
        sources.push(SiteSource {
            kind: kind.to_owned(),
            entity_id: entity.clone(),
            label: Some(label.to_owned()),
        });
    }
}
