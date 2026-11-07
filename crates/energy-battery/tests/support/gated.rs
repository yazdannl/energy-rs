//! A device whose write blocks until the test releases it.

// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal,
    dead_code
)]

use std::collections::VecDeque;
use std::sync::Mutex;

use energy_battery::{BatteryError, BoxFuture, DeviceIo, Report, WriteAck, WriteRequest};
use tokio::sync::Notify;

use crate::support::report;

/// A scripted device whose write blocks until the test lets it go.
///
/// It exists so a test can look at the controller *while* a write is in flight: the pending command
/// has to be installed before the request leaves, otherwise a report arriving during the write has
/// nothing to verify.
pub struct GatedDevice {
    sn: String,
    reports: Mutex<VecDeque<Report>>,
    writes: Mutex<Vec<WriteRequest>>,
    started: Notify,
    release: Notify,
}

impl GatedDevice {
    pub fn new(sn: &str) -> Self {
        Self {
            sn: sn.to_owned(),
            reports: Mutex::new(VecDeque::new()),
            writes: Mutex::new(Vec::new()),
            started: Notify::new(),
            release: Notify::new(),
        }
    }

    pub fn push_report(&self, report: Report) {
        self.reports.lock().unwrap().push_back(report);
    }

    /// Wait until a write has been transmitted and is waiting for the release.
    pub async fn write_started(&self) {
        self.started.notified().await;
    }

    /// Let the in-flight write return its ack.
    pub fn release(&self) {
        self.release.notify_one();
    }

    pub fn writes(&self) -> usize {
        self.writes.lock().unwrap().len()
    }
}

impl DeviceIo for GatedDevice {
    fn sn(&self) -> &str {
        &self.sn
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        let next = self
            .reports
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| report(&self.sn));
        Box::pin(std::future::ready(Ok(next)))
    }

    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        self.writes.lock().unwrap().push(request);
        Box::pin(async move {
            self.started.notify_one();
            self.release.notified().await;
            Ok(WriteAck {
                timestamp: None,
                message_id: None,
                code: 200,
                sn: Some(self.sn.clone()),
            })
        })
    }
}
