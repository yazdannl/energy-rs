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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use energy_core::dispatch::{BoxFuture, CommandSink, Progress, SinkError, WriteReceipt};

use super::sent::Sent;

/// A hook that runs at the instant of the transport write.
type Observer = Arc<dyn Fn(&str) + Send + Sync>;

/// A sink that records commands and can refuse or fail to echo them.
pub struct FakeSink {
    commands: Mutex<Vec<(String, Sent)>>,
    reject_writes: AtomicBool,
    unverified_writes: AtomicBool,
    unverified_holds: AtomicBool,
    observer: Mutex<Option<Observer>>,
    hold_delays: Mutex<BTreeMap<String, Duration>>,
    stuck_holds: Mutex<BTreeSet<String>>,
    hold_observer: Mutex<Option<Observer>>,
}

impl Default for FakeSink {
    fn default() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
            reject_writes: AtomicBool::new(false),
            unverified_writes: AtomicBool::new(false),
            unverified_holds: AtomicBool::new(false),
            observer: Mutex::new(None),
            hold_delays: Mutex::new(BTreeMap::new()),
            stuck_holds: Mutex::new(BTreeSet::new()),
            hold_observer: Mutex::new(None),
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

    pub fn unverified_writes(&self, unverified: bool) {
        self.unverified_writes.store(unverified, Ordering::Relaxed);
    }

    pub fn unverified_holds(&self, unverified: bool) {
        self.unverified_holds.store(unverified, Ordering::Relaxed);
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
            Ok(WriteReceipt {
                applied_w: watts,
                verified: !self.unverified_writes.load(Ordering::Relaxed),
            })
        })
    }

    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        self.hold_with_progress(pack_id, None)
    }

    fn hold_with_progress<'a>(
        &'a self,
        pack_id: &str,
        _progress: Option<&'a dyn Progress>,
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
        let unverified = self.unverified_holds.load(Ordering::Relaxed);
        let pack_id = pack_id.to_owned();
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if stuck {
                std::future::pending::<()>().await;
            }
            if pack_id == "SN-FAIL" {
                return Err(SinkError::UnknownPack { pack_id });
            }
            Ok(WriteReceipt {
                applied_w: 0.0,
                verified: !unverified,
            })
        })
    }
}
