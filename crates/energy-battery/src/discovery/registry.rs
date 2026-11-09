//! Merge discovery results into one device per serial.

use std::collections::BTreeMap;

use crate::discovery::found::DiscoveredDevice;

/// The set of devices worth talking to, keyed by serial.
///
/// mDNS can announce the same device repeatedly and an operator can add an address that mDNS also
/// finds, so every insert merges: the serial decides identity, a manual address outranks the mDNS
/// one, and a model hint from either source is kept.
#[derive(Debug, Default, Clone)]
pub struct DiscoveryRegistry {
    devices: BTreeMap<String, DiscoveredDevice>,
}

impl DiscoveryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or merge one device. Returns true when the registry changed (new serial or a better
    /// address), false for a repeat announcement of the same fact.
    pub fn merge(&mut self, device: DiscoveredDevice) -> bool {
        match self.devices.get_mut(&device.sn) {
            None => {
                self.devices.insert(device.sn.clone(), device);
                true
            }
            Some(existing) => {
                if device.source.priority() > existing.source.priority() {
                    // A more authoritative source wins the address; the product hint is kept.
                    let product = existing.product.clone();
                    *existing = device;
                    existing.product = existing.product.clone().or(product);
                    return true;
                }
                // Same or lower authority: follow a moved address, keep the hint. A manual entry is
                // an operator decision, so it only ever gains a product hint.
                let moved = existing.host != device.host || existing.port != device.port;
                let same_source = device.source.priority() == existing.source.priority();
                if moved && same_source {
                    existing.host.clone_from(&device.host);
                    existing.port = device.port;
                }
                fill_product(existing, &device) || (moved && same_source)
            }
        }
    }

    /// Merge many results; returns how many changed the registry.
    pub fn merge_all(&mut self, devices: impl IntoIterator<Item = DiscoveredDevice>) -> usize {
        devices
            .into_iter()
            .filter(|device| self.merge(device.clone()))
            .count()
    }

    pub fn devices(&self) -> impl Iterator<Item = &DiscoveredDevice> {
        self.devices.values()
    }

    pub fn get(&self, sn: &str) -> Option<&DiscoveredDevice> {
        self.devices.get(&sn.trim().to_ascii_uppercase())
    }

    pub fn into_devices(self) -> Vec<DiscoveredDevice> {
        self.devices.into_values().collect()
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

fn fill_product(existing: &mut DiscoveredDevice, incoming: &DiscoveredDevice) -> bool {
    if existing.product.is_none() && incoming.product.is_some() {
        existing.product.clone_from(&incoming.product);
        return true;
    }
    false
}
