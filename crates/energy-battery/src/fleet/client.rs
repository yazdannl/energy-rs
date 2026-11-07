//! The fleet loop.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::control::command::Command;
use crate::control::controller::DeviceController;
use crate::fleet::device_snapshot::DeviceSnapshot;
use crate::fleet::fanout::FanOutReport;
use crate::fleet::outcome::DeviceOutcome;
use crate::fleet::snapshot::FleetSnapshot;
use crate::transport::device_io::DeviceIo;

/// Default poll interval per device. Control needs ~10 s freshness, telemetry 60 s.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// N devices behind one client.
///
/// Each device keeps its own controller (so its own pacing, echo and fault state). Polling is
/// staggered by `interval / n`; writes fan out concurrently, because the serial bottleneck is per
/// device, not per fleet.
pub struct FleetClient<T: DeviceIo> {
    devices: Vec<Arc<DeviceController<T>>>,
    interval: Duration,
    schedule: Mutex<Vec<Option<Instant>>>,
    states: Mutex<Vec<DeviceSnapshot>>,
}

impl<T: DeviceIo> FleetClient<T> {
    pub fn new(devices: Vec<Arc<DeviceController<T>>>) -> Self {
        Self::with_interval(devices, DEFAULT_POLL_INTERVAL)
    }

    /// Build with an explicit interval; the initial schedule is offset by `interval / n` per device.
    pub fn with_interval(devices: Vec<Arc<DeviceController<T>>>, interval: Duration) -> Self {
        let start = Instant::now();
        let offset = stagger(interval, devices.len());
        let schedule = (0..devices.len())
            .map(|index| Some(start + offset * index as u32))
            .collect();
        let states = devices
            .iter()
            .map(|device| DeviceSnapshot::new(device.sn()))
            .collect();
        Self {
            devices,
            interval,
            schedule: Mutex::new(schedule),
            states: Mutex::new(states),
        }
    }

    pub fn devices(&self) -> &[Arc<DeviceController<T>>] {
        &self.devices
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Delay between two devices' first polls.
    pub fn offset(&self) -> Duration {
        stagger(self.interval, self.devices.len())
    }

    /// Poll the devices whose slot is due and return the updated cache.
    pub async fn poll(&self) -> FleetSnapshot {
        let now = Instant::now();
        let due = {
            let mut schedule = self.schedule.lock().await;
            let mut due = Vec::new();
            for (index, slot) in schedule.iter_mut().enumerate() {
                if slot.is_none_or(|at| now >= at) {
                    *slot = Some(now + self.interval);
                    due.push(index);
                }
            }
            due
        };
        self.poll_indices(due).await
    }

    /// Poll every device now, regardless of the stagger schedule.
    pub async fn refresh(&self) -> FleetSnapshot {
        let indices = {
            let mut schedule = self.schedule.lock().await;
            let now = Instant::now();
            for slot in schedule.iter_mut() {
                *slot = Some(now + self.interval);
            }
            (0..self.devices.len()).collect()
        };
        self.poll_indices(indices).await
    }

    /// The cached view, without touching the network.
    pub async fn snapshot(&self) -> FleetSnapshot {
        let states = self.states.lock().await.clone();
        FleetSnapshot::from_devices(states)
    }

    /// Send one command per device, concurrently, and report every outcome.
    ///
    /// `plan` sees the cached state and returns `None` to skip a device (held, offline, no share).
    /// Each device still goes through its own pacer, so sequencing holds even under a fan-out.
    pub async fn fanout<F>(&self, plan: F) -> FanOutReport
    where
        F: Fn(&DeviceSnapshot) -> Option<Command>,
    {
        let snapshot = self.snapshot().await;
        let mut skipped = 0;
        let mut planned = Vec::new();
        for (index, device) in self.devices.iter().enumerate() {
            match snapshot.devices.get(index).and_then(&plan) {
                Some(command) => planned.push((Arc::clone(device), command)),
                None => skipped += 1,
            }
        }
        let outcomes = join_all(planned.into_iter().map(|(device, command)| async move {
            let sn = device.sn().to_owned();
            DeviceOutcome {
                sn,
                command,
                result: device.apply(command).await,
            }
        }))
        .await;
        FanOutReport::new(outcomes, skipped)
    }

    async fn poll_indices(&self, indices: Vec<usize>) -> FleetSnapshot {
        let mut results = Vec::with_capacity(indices.len());
        for index in indices {
            let observation = self.devices[index].poll().await;
            results.push((index, observation));
        }
        let mut states = self.states.lock().await;
        for (index, observation) in results {
            let at = Instant::now();
            match observation {
                Ok(observation) => {
                    states[index] = DeviceSnapshot::from_observation(observation, at);
                }
                Err(error) => states[index].record_failure(&error.to_string(), at),
            }
        }
        FleetSnapshot::from_devices(states.clone())
    }
}

fn stagger(interval: Duration, devices: usize) -> Duration {
    if devices == 0 {
        return Duration::ZERO;
    }
    interval / devices as u32
}
