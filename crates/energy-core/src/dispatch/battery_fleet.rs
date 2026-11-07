//! The real fleet port: `energy-battery`'s cached fleet read, mapped to pack readings.

use std::sync::Arc;

use energy_battery::fleet::DeviceSnapshot;
use energy_battery::fleet::FleetClient;
use energy_battery::report::Report;
use energy_battery::transport::DeviceIo;
use tokio::time::Instant;

use crate::dispatch::pack_catalog::PackCatalog;
use crate::dispatch::{BoxFuture, DeviceFleet, FleetError, FleetView, PackReading};

/// AC ceiling assumed when a device reports none, W.
const DEFAULT_MAX_W: f64 = 800.0;
/// Configured floor when the device reports none, percent (PLAN.md: 10-100 %).
const DEFAULT_MIN_SOC_PCT: f64 = 10.0;

/// A [`DeviceFleet`] over the fleet client.
///
/// Every reading is derived from the device's own report: the capacity label, the live charge and
/// discharge maxima and the reported SoC, refined only by that pack's own learning profile (never a
/// model table and never the fleet's summed capacity). The device sign convention (positive
/// discharging) is flipped once here, so the rest of the engine uses one convention: positive watts
/// charge the pack.
pub struct BatteryFleet<T: DeviceIo> {
    client: Arc<FleetClient<T>>,
    catalog: Arc<PackCatalog>,
    stale_after_s: f64,
}

impl<T: DeviceIo> BatteryFleet<T> {
    /// A port over the fleet client, with the catalog that registers one pack per device and the
    /// reading-age bound that marks a pack stale.
    pub fn new(client: Arc<FleetClient<T>>, catalog: Arc<PackCatalog>, stale_after_s: f64) -> Self {
        Self {
            client,
            catalog,
            stale_after_s,
        }
    }

    /// The per-device catalog (SN ↔ ledger id and per-pack learning).
    pub fn catalog(&self) -> &Arc<PackCatalog> {
        &self.catalog
    }

    /// One device as a pack reading, as seen at `now`.
    fn reading(&self, device: &DeviceSnapshot, now: i64) -> PackReading {
        let age_s = device
            .age(Instant::now())
            .map_or(f64::INFINITY, |age| age.as_secs_f64());
        let observed_at = now.saturating_sub(age_of(age_s));
        match device.report.as_ref() {
            Some(report) => self.mapped(device, report, observed_at, age_s, now),
            None => self.missing(device, now),
        }
    }

    fn mapped(
        &self,
        device: &DeviceSnapshot,
        report: &Report,
        observed_at: i64,
        age_s: f64,
        now: i64,
    ) -> PackReading {
        // The device's own `packData` labels, never a fleet/hub sum: one device is one ledger pack.
        let label_capacity_kwh = report.total_capacity_kwh().unwrap_or(0.0);
        let charge_w = limit(report.properties.charge_max_w);
        let discharge_w = limit(report.properties.inverse_max_w);
        let ledger_id =
            self.catalog
                .register(&device.sn, label_capacity_kwh, charge_w, discharge_w, now);
        let mut spec = self
            .catalog
            .spec(&device.sn, label_capacity_kwh, charge_w, discharge_w);
        spec.min_soc_pct = min_soc(report);
        spec.max_soc_pct = 100.0;
        let capacity_kwh = spec.capacity_kwh;
        let soc_pct = report.soc_pct();
        PackReading {
            ledger_id,
            spec,
            soc_pct: Some(soc_pct),
            energy_kwh: (capacity_kwh > 0.0).then(|| capacity_kwh * soc_pct / 100.0),
            power_w: -(report.ac_power_w() as f64),
            min_cell_v: min_cell_v(report),
            observed_at,
            available: device.error.is_none(),
            stale: age_s > self.stale_after_s,
        }
    }

    /// A device that has never answered: unusable, but still listed so it is logged and held.
    fn missing(&self, device: &DeviceSnapshot, now: i64) -> PackReading {
        let spec = self
            .catalog
            .spec(&device.sn, 0.0, DEFAULT_MAX_W, DEFAULT_MAX_W);
        PackReading {
            ledger_id: self.catalog.ledger_id(&device.sn),
            spec,
            soc_pct: None,
            energy_kwh: None,
            power_w: 0.0,
            min_cell_v: None,
            observed_at: now,
            available: false,
            stale: true,
        }
    }
}

impl<T: DeviceIo> DeviceFleet for BatteryFleet<T> {
    fn view(&self, now: i64) -> BoxFuture<'_, Result<FleetView, FleetError>> {
        Box::pin(async move {
            let snapshot = self.client.refresh().await;
            Ok(FleetView::new(
                snapshot
                    .devices
                    .iter()
                    .map(|device| self.reading(device, now))
                    .collect(),
            ))
        })
    }
}

/// Whole seconds an age represents, saturating instead of overflowing.
fn age_of(age_s: f64) -> i64 {
    if !age_s.is_finite() || age_s <= 0.0 {
        return 0;
    }
    let seconds = age_s.round();
    if seconds >= i64::MAX as f64 {
        i64::MAX
    } else {
        seconds as i64
    }
}

/// The device's own floor, with the plan's floor when it does not report one.
fn min_soc(report: &Report) -> f64 {
    let reported = report.properties.min_soc_pct;
    if reported.is_finite() && reported > 0.0 {
        reported
    } else {
        DEFAULT_MIN_SOC_PCT
    }
}

/// A live device limit, with the shipped ceiling when the device reports none.
fn limit(reported: u32) -> f64 {
    if reported > 0 {
        f64::from(reported)
    } else {
        DEFAULT_MAX_W
    }
}

/// The lowest cell voltage any pack reported, or `None` when none did.
fn min_cell_v(report: &Report) -> Option<f64> {
    report
        .packs
        .iter()
        .map(|pack| pack.min_cell_v)
        .filter(|volt| volt.is_finite() && *volt > 0.0)
        .fold(None, |lowest, volt| {
            Some(lowest.map_or(volt, |low: f64| low.min(volt)))
        })
}
