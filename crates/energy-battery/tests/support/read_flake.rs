//! A device whose report reads can miss, for read-retry and read-miss policy tests.
//
// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};

use energy_battery::{BatteryError, BoxFuture, DeviceIo, Report, WriteAck, WriteRequest};

/// A device that fails the next `misses` report reads, then behaves like its inner device.
pub struct ReadFlake<T> {
    inner: T,
    misses: AtomicUsize,
}

impl<T: DeviceIo> ReadFlake<T> {
    pub fn new(inner: T, misses: usize) -> Self {
        Self {
            inner,
            misses: AtomicUsize::new(misses),
        }
    }

    pub fn set_misses(&self, misses: usize) {
        self.misses.store(misses, Ordering::Relaxed);
    }
}

impl<T: DeviceIo> DeviceIo for ReadFlake<T> {
    fn sn(&self) -> &str {
        self.inner.sn()
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        if self
            .misses
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return Box::pin(std::future::ready(Err(BatteryError::Transport {
                host: "device".to_owned(),
                port: 80,
                reason: "scripted read timeout".to_owned(),
            })));
        }
        self.inner.report()
    }

    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        self.inner.write(request)
    }
}
