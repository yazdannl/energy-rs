//! Battery fleet assembly: manual entries plus mDNS, one controller per device.
//!
//! Discovery is deliberately boring: the options' manual `address`/`serial` entries always win, and
//! an mDNS browse adds whatever answers `_zendure._tcp` on the LAN. The registry dedupes by serial,
//! so a device found both ways is registered once. No cloud, no token, no TLS.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use energy_battery::control::{ControlLimits, DeviceController};
use energy_battery::discovery::{DiscoveredDevice, DiscoveryRegistry, browse_mdns, scan_lan};
use energy_battery::fleet::FleetClient;
use energy_battery::transport::HttpTransport;

use crate::bootstrap::options::Options;

/// How long mDNS gets to answer; startup must not hang on a silent LAN.
/// How long the LAN sweep may take: a /24 at 300 ms per host, 64 at a time.
const SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(6);
/// How long the LAN sweep may take.
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
    let mut discovered = 0;
    if let Some(found) = browse(options).await {
        discovered += found.len();
        report(&mut registry, &found, "mDNS");
    }
    if options.scan() {
        match scan_lan(SCAN_WINDOW, &seeds(manual)).await {
            Ok(found) => {
                discovered += found.len();
                report(&mut registry, &found, "LAN scan");
            }
            Err(error) => tracing::warn!(%error, "LAN scan is unavailable; using what was found"),
        }
    }
    (registry, discovered)
}

/// The subnets worth sweeping: the operator's own addresses.
fn seeds(manual: &[(String, String, u16)]) -> Vec<std::net::Ipv4Addr> {
    manual
        .iter()
        .filter_map(|(address, _, _)| address.parse().ok())
        .collect()
}

/// Merge a discovery round and say what it added.
fn report(registry: &mut DiscoveryRegistry, found: &[DiscoveredDevice], source: &str) {
    let added = registry.merge_all(found.iter().cloned());
    if !found.is_empty() {
        tracing::info!(source, found = found.len(), added, "batteries discovered");
    }
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
