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

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use energy_core::dispatch::{BoxFuture, CommandSink, SinkError, WriteReceipt};

use super::sent::Sent;

/// A sink that records commands and can refuse or fail to echo them.
pub struct FakeSink {
    commands: Mutex<Vec<(String, Sent)>>,
    reject_writes: AtomicBool,
    unverified_writes: AtomicBool,
    unverified_holds: AtomicBool,
}

impl Default for FakeSink {
    fn default() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
            reject_writes: AtomicBool::new(false),
            unverified_writes: AtomicBool::new(false),
            unverified_holds: AtomicBool::new(false),
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
        self.record(pack_id, Sent::Hold);
        let pack_id = pack_id.to_owned();
        Box::pin(async move {
            if pack_id == "SN-FAIL" {
                return Err(SinkError::UnknownPack { pack_id });
            }
            Ok(WriteReceipt {
                applied_w: 0.0,
                verified: !self.unverified_holds.load(Ordering::Relaxed),
            })
        })
    }
}
