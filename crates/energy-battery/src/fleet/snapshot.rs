//! One fleet poll result.

use tokio::time::Instant;

use crate::fleet::aggregate::FleetAggregate;
use crate::fleet::device_snapshot::DeviceSnapshot;

/// A consistent view of every configured device plus the aggregate.
#[derive(Debug, Clone, PartialEq)]
pub struct FleetSnapshot {
    pub devices: Vec<DeviceSnapshot>,
    pub aggregate: FleetAggregate,
    /// Newest `fetched_at` among the devices, `None` when nothing was ever read.
    pub fetched_at: Option<Instant>,
}

impl FleetSnapshot {
    pub fn from_devices(devices: Vec<DeviceSnapshot>) -> Self {
        let aggregate = FleetAggregate::from_devices(&devices);
        let fetched_at = devices.iter().filter_map(|device| device.fetched_at).max();
        Self {
            devices,
            aggregate,
            fetched_at,
        }
    }

    pub fn device(&self, sn: &str) -> Option<&DeviceSnapshot> {
        self.devices
            .iter()
            .find(|device| device.sn.eq_ignore_ascii_case(sn))
    }

    pub fn failures(&self) -> impl Iterator<Item = &DeviceSnapshot> {
        self.devices.iter().filter(|device| device.error.is_some())
    }

    pub fn is_complete(&self) -> bool {
        self.devices.iter().all(DeviceSnapshot::is_online)
    }
}
