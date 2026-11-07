//! Fleet-wide totals.

use crate::fleet::device_snapshot::DeviceSnapshot;

/// What the whole fleet is doing right now.
///
/// SoC is weighted by each device's labelled pack capacity, because averaging four packs of unequal
/// size would lie. Only online devices with a readable capacity contribute; `total` still counts
/// every configured device, so `online < total` is visible.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FleetAggregate {
    pub soc_weighted_pct: Option<f64>,
    pub power_w: i64,
    pub capacity_kwh: Option<f64>,
    pub online: usize,
    pub total: usize,
}

impl FleetAggregate {
    pub fn from_devices(devices: &[DeviceSnapshot]) -> Self {
        let mut energy = 0.0;
        let mut capacity = 0.0;
        let mut power_w = 0;
        let mut online = 0;
        for device in devices.iter().filter(|device| device.is_online()) {
            online += 1;
            power_w += device.power_w();
            if let (Some(soc), Some(kwh)) = (device.soc_pct(), device.capacity_kwh())
                && kwh > 0.0
            {
                energy += soc * kwh;
                capacity += kwh;
            }
        }
        Self {
            soc_weighted_pct: (capacity > 0.0).then(|| energy / capacity),
            power_w,
            capacity_kwh: (capacity > 0.0).then_some(capacity),
            online,
            total: devices.len(),
        }
    }
}
