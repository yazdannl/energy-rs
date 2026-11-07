//! Battery fleet assembly: manual entries plus mDNS, one controller per device.
//!
//! Discovery is deliberately boring: the options' manual `address`/`serial` entries always win, and
//! an mDNS browse adds whatever answers `_zendure._tcp` on the LAN. The registry dedupes by serial,
//! so a device found both ways is registered once. No cloud, no token, no TLS.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use energy_battery::control::{ControlLimits, DeviceController};
use energy_battery::discovery::{DiscoveredDevice, DiscoveryRegistry, browse_mdns};
use energy_battery::fleet::FleetClient;
use energy_battery::transport::HttpTransport;

use crate::bootstrap::options::Options;

/// How long mDNS gets to answer; startup must not hang on a silent LAN.
const MDNS_WINDOW: Duration = Duration::from_secs(2);

/// The assembled fleet: one client for reads, one controller per device for writes.
pub struct Fleet {
    pub client: Arc<FleetClient<HttpTransport>>,
    pub controllers: Vec<Arc<DeviceController<HttpTransport>>>,
    pub discovered: usize,
    pub manual: usize,
}

/// Discover the devices and build their transports and controllers.
pub async fn build(options: &Options, limits: ControlLimits) -> Result<Fleet> {
    let manual = options.manual_batteries();
    let (registry, discovered) = discover(options, &manual).await;
    if registry.is_empty() {
        tracing::warn!(
            "no batteries configured and none discovered; the engine will run with an empty fleet. \
             Add manual address/serial entries or enable mDNS"
        );
    }
    let mut controllers = Vec::new();
    for device in registry.devices() {
        let transport = HttpTransport::new(device.endpoint()).with_context(|| {
            format!(
                "battery {} at {} is not a usable endpoint",
                device.sn,
                device.endpoint().authority()
            )
        })?;
        controllers.push(Arc::new(DeviceController::new(transport, limits)));
    }
    let client = Arc::new(FleetClient::new(controllers.clone()));
    Ok(Fleet {
        client,
        controllers,
        discovered,
        manual: manual.len(),
    })
}

/// Merge the manual entries with whatever mDNS answers.
async fn discover(
    options: &Options,
    manual: &[(String, String, u16)],
) -> (DiscoveryRegistry, usize) {
    let mut registry = manual_registry(manual);
    let Some(found) = browse(options).await else {
        return (registry, 0);
    };
    let discovered = found.len();
    let added = registry.merge_all(found);
    if added < discovered {
        tracing::info!(
            found = discovered,
            added,
            "mDNS found batteries; duplicates kept the manual entry"
        );
    }
    (registry, discovered)
}

/// The operator's entries, always first.
fn manual_registry(manual: &[(String, String, u16)]) -> DiscoveryRegistry {
    let mut registry = DiscoveryRegistry::new();
    for (address, serial, port) in manual {
        registry.merge(DiscoveredDevice::manual(address, serial, *port));
    }
    registry
}

/// One mDNS browse; `None` when it is disabled or unavailable.
async fn browse(options: &Options) -> Option<Vec<DiscoveredDevice>> {
    if !options.mdns() {
        return None;
    }
    match browse_mdns(MDNS_WINDOW).await {
        Ok(found) => Some(found),
        Err(error) => {
            tracing::warn!(%error, "mDNS discovery is unavailable; using manual entries only");
            None
        }
    }
}
