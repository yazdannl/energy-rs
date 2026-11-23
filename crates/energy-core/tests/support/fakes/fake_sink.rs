//! A sink that records commands and can refuse or fail to echo them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::cast_possible_wrap
)]
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use energy_battery::ControlFault;
use energy_core::dispatch::{BoxFuture, CommandSink, Progress, SinkError, WriteReceipt};

use super::NOW;
use super::sent::Sent;

/// A hook that runs at the instant of the transport write.
type Observer = Arc<dyn Fn(&str) + Send + Sync>;

/// A sink that records commands and can refuse or fail to echo them.
pub struct FakeSink {
    commands: Mutex<Vec<(String, Sent)>>,
    reject_writes: AtomicBool,
    supersede_writes: AtomicBool,
    unverified_writes: AtomicBool,
    unverified_holds: AtomicBool,
    /// Positive W: writes confirm with real but lower flow instead of the requested magnitude.
    curtailed_w: AtomicI64,
    read_miss: AtomicBool,
    observer: Mutex<Option<Observer>>,
    hold_delays: Mutex<BTreeMap<String, Duration>>,
    stuck_holds: Mutex<BTreeSet<String>>,
    fenced_holds: Mutex<BTreeSet<String>>,
    hold_errors: Mutex<BTreeMap<String, SinkError>>,
    hold_observer: Mutex<Option<Observer>>,
    /// Return this typed fault for the pack's next write (then clear it).
    faults: Mutex<BTreeMap<String, ControlFault>>,
}

impl Default for FakeSink {
    fn default() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
            reject_writes: AtomicBool::new(false),
            supersede_writes: AtomicBool::new(false),
            unverified_writes: AtomicBool::new(false),
            unverified_holds: AtomicBool::new(false),
            curtailed_w: AtomicI64::new(0),
            read_miss: AtomicBool::new(false),
            observer: Mutex::new(None),
            hold_delays: Mutex::new(BTreeMap::new()),
            stuck_holds: Mutex::new(BTreeSet::new()),
            fenced_holds: Mutex::new(BTreeSet::new()),
            hold_errors: Mutex::new(BTreeMap::new()),
            hold_observer: Mutex::new(None),
            faults: Mutex::new(BTreeMap::new()),
        }
    }
}

impl FakeSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reject_writes(&self, reject: bool) {
        self.reject_writes.store(reject, Ordering::Relaxed);
    }

    pub fn supersede_writes(&self, supersede: bool) {
        self.supersede_writes.store(supersede, Ordering::Relaxed);
    }

    pub fn unverified_writes(&self, unverified: bool) {
        self.unverified_writes.store(unverified, Ordering::Relaxed);
    }

    pub fn unverified_holds(&self, unverified: bool) {
        self.unverified_holds.store(unverified, Ordering::Relaxed);
    }

    /// Confirm writes with direction-correct flow capped at `watts` (0 disables curtailment).
    pub fn curtail(&self, watts: i64) {
        self.curtailed_w.store(watts, Ordering::Relaxed);
    }

    /// Make this pack's hold wait `delay` before its echo is available.
    pub fn delay_hold(&self, pack_id: &str, delay: Duration) {
        self.hold_delays
            .lock()
            .expect("sink hold delays")
            .insert(pack_id.to_owned(), delay);
    }

    /// This pack's hold never echoes, so its verification budget expires.
    pub fn never_confirm_hold(&self, pack_id: &str) {
        self.stuck_holds
            .lock()
            .expect("sink stuck holds")
            .insert(pack_id.to_owned());
    }

    /// Return a typed error for this pack's hold, to distinguish a dead transport from a refusal.
    pub fn fail_hold(&self, pack_id: &str, error: SinkError) {
        self.hold_errors
            .lock()
            .expect("sink hold errors")
            .insert(pack_id.to_owned(), error);
    }

    /// This pack's hold is refused by the transport's generation check: no write, no echo.
    pub fn fence_hold(&self, pack_id: &str) {
        self.fenced_holds
            .lock()
            .expect("sink fenced holds")
            .insert(pack_id.to_owned());
    }

    /// Script a read miss during echo confirmation: the write lands, but no echo is read.
    pub fn read_miss(&self, miss: bool) {
        self.read_miss.store(miss, Ordering::Relaxed);
    }

    /// Make the next write to `pack_id` return a typed device fault.
    pub fn fault_with(&self, pack_id: &str, fault: ControlFault) {
        self.faults
            .lock()
            .expect("sink faults")
            .insert(pack_id.to_owned(), fault);
    }

    /// Run `hook` at the instant of the transport write, i.e. after the intent was journalled.
    pub fn observe(&self, hook: Observer) {
        *self.observer.lock().expect("sink observer") = Some(hook);
    }

    /// Run `hook` at the instant a hold reaches the transport.
    pub fn observe_holds(&self, hook: Observer) {
        *self.hold_observer.lock().expect("sink hold observer") = Some(hook);
    }

    fn notify(&self, pack_id: &str) {
        let hook = self
            .observer
            .lock()
            .expect("sink observer")
            .as_ref()
            .map(Arc::clone);
        if let Some(hook) = hook {
            hook(pack_id);
        }
    }

    fn notify_hold(&self, pack_id: &str) {
        let hook = self
            .hold_observer
            .lock()
            .expect("sink hold observer")
            .as_ref()
            .map(Arc::clone);
        if let Some(hook) = hook {
            hook(pack_id);
        }
    }

    pub fn commands(&self) -> Vec<(String, Sent)> {
        self.commands.lock().expect("sink commands").clone()
    }

    pub fn writes(&self) -> Vec<f64> {
        self.commands()
            .into_iter()
            .filter_map(|(_, sent)| match sent {
                Sent::Write(watts) => Some(watts),
                Sent::Hold => None,
            })
            .collect()
    }

    pub fn holds(&self) -> usize {
        self.commands()
            .into_iter()
            .filter(|(_, sent)| *sent == Sent::Hold)
            .count()
    }

    pub fn len(&self) -> usize {
        self.commands.lock().expect("sink commands").len()
    }

    pub fn last(&self) -> Option<(String, Sent)> {
        self.commands().last().cloned()
    }

    fn record(&self, pack_id: &str, sent: Sent) {
        self.commands
            .lock()
            .expect("sink commands")
            .push((pack_id.to_owned(), sent));
    }
}

impl CommandSink for FakeSink {
    fn write(&self, pack_id: &str, watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        self.record(pack_id, Sent::Write(watts));
        self.notify(pack_id);
        let pack_id = pack_id.to_owned();
        Box::pin(async move {
            if self.reject_writes.load(Ordering::Relaxed) {
                return Err(SinkError::Device {
                    pack_id,
                    reason: "scripted rejection".to_owned(),
                });
            }
            let verified = !self.unverified_writes.load(Ordering::Relaxed);
            let fault = self.faults.lock().expect("sink faults").remove(&pack_id);
            if let Some(fault) = fault {
                return Err(SinkError::Fault {
                    pack_id,
                    fault: Box::new(fault),
                });
            }
            if self.supersede_writes.load(Ordering::Relaxed) {
                return Ok(WriteReceipt::superseded());
            }
            if self.read_miss.load(Ordering::Relaxed) {
                return Err(SinkError::ReadUnverified {
                    pack_id,
                    reason: "scripted read miss during echo".to_owned(),
                });
            }
            let curtail = self.curtailed_w.load(Ordering::Relaxed);
            if curtail > 0 {
                return Ok(WriteReceipt::partial(
                    curtail.min(watts.abs() as i64) as f64 * watts.signum(),
                )
                .with_measurement(NOW));
            }
            Ok(WriteReceipt {
                applied_w: watts,
                verified,
                partial: false,
                superseded: false,
                flow_measured: true,
                measured_at: Some(NOW),
            })
        })
    }

    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        self.hold_with_progress(pack_id, None, 0)
    }

    fn hold_with_progress<'a>(
        &'a self,
        pack_id: &str,
        _progress: Option<&'a dyn Progress>,
        _now: i64,
    ) -> BoxFuture<'a, Result<WriteReceipt, SinkError>> {
        self.record(pack_id, Sent::Hold);
        self.notify_hold(pack_id);
        let delay = self
            .hold_delays
            .lock()
            .expect("sink hold delays")
            .get(pack_id)
            .copied()
            .unwrap_or_default();
        let stuck = self
            .stuck_holds
            .lock()
            .expect("sink stuck holds")
            .contains(pack_id);
        let fenced = self
            .fenced_holds
            .lock()
            .expect("sink fenced holds")
            .contains(pack_id);
        let unverified = self.unverified_holds.load(Ordering::Relaxed);
        let hold_error = self
            .hold_errors
            .lock()
            .expect("sink hold errors")
            .get(pack_id)
            .cloned();
        let pack_id = pack_id.to_owned();
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if stuck {
                std::future::pending::<()>().await;
            }
            if fenced {
                return Err(SinkError::Fenced { pack_id });
            }
            if let Some(error) = hold_error {
                return Err(error);
            }
            if pack_id == "SN-FAIL" {
                return Err(SinkError::UnknownPack { pack_id });
            }
            Ok(WriteReceipt {
                applied_w: 0.0,
                verified: !unverified,
                partial: false,
                superseded: false,
                flow_measured: true,
                measured_at: Some(NOW),
            })
        })
    }
}
