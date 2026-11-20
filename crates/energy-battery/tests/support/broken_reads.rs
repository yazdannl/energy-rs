//! A device whose report reads can be switched from healthy to stalled.

// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(clippy::unwrap_used, clippy::panic, dead_code)]

use std::sync::Mutex;

use energy_battery::{BatteryError, BoxFuture, DeviceIo, Report, WriteAck, WriteRequest};

use super::report;

pub struct BrokenReads {
    sn: String,
    broken: Mutex<bool>,
}

impl BrokenReads {
    pub fn new(sn: &str) -> Self {
        Self {
            sn: sn.to_owned(),
            broken: Mutex::new(true),
        }
    }

    /// Serve one healthy baseline report, then fail every later read.
    pub fn start_healthy(&self) {
        *self.broken.lock().unwrap() = false;
    }

    pub fn break_reads(&self) {
        *self.broken.lock().unwrap() = true;
    }
}

impl DeviceIo for BrokenReads {
    fn sn(&self) -> &str {
        &self.sn
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        let result = if *self.broken.lock().unwrap() {
            Err(BatteryError::Invalid {
                reason: "stalled transport".to_owned(),
            })
        } else {
            Ok(report(&self.sn))
        };
        Box::pin(std::future::ready(result))
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
