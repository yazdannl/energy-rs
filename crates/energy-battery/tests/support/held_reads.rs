//! A device whose next report read blocks until the test releases it.

// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(clippy::unwrap_used, clippy::panic, dead_code)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use energy_battery::{BatteryError, BoxFuture, DeviceIo, Report, WriteAck, WriteRequest};
use tokio::sync::Notify;

use super::report;

pub struct HeldReads {
    sn: String,
    held: Mutex<Option<Report>>,
    blocked: AtomicBool,
    started: Notify,
    release: Notify,
}

impl HeldReads {
    pub fn new(sn: &str) -> Self {
        Self {
            sn: sn.to_owned(),
            held: Mutex::new(None),
            blocked: AtomicBool::new(false),
            started: Notify::new(),
            release: Notify::new(),
        }
    }

    /// Make the next read block until [`release`](Self::release), then answer with `report`.
    pub fn hold_next(&self, report: Report) {
        *self.held.lock().unwrap() = Some(report);
        self.blocked.store(true, Ordering::Relaxed);
    }

    pub async fn read_started(&self) {
        self.started.notified().await;
    }

    pub fn release(&self) {
        self.release.notify_one();
    }
}

impl DeviceIo for HeldReads {
    fn sn(&self) -> &str {
        &self.sn
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        let sn = self.sn.clone();
        let held = self.held.lock().unwrap().take();
        let blocked = self.blocked.swap(false, Ordering::Relaxed);
        Box::pin(async move {
            if blocked {
                self.started.notify_one();
                self.release.notified().await;
            }
            Ok(held.unwrap_or_else(|| report(&sn)))
        })
    }

    fn write(&self, _request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        Box::pin(std::future::ready(Ok(WriteAck {
            timestamp: None,
            message_id: None,
            code: 200,
            sn: Some(self.sn.clone()),
        })))
    }
}
