//! A fleet whose reading the test can replace.

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

use energy_core::dispatch::{BoxFuture, DeviceFleet, FleetError, FleetView, PackReading};

use super::fixtures::pack;

/// A fleet whose reading the test can replace.
pub struct FakeFleet {
    view: Mutex<FleetView>,
    fail: AtomicBool,
}

impl FakeFleet {
    pub fn new(packs: Vec<PackReading>) -> Self {
        Self {
            view: Mutex::new(FleetView::new(packs)),
            fail: AtomicBool::new(false),
        }
    }

    pub fn one(soc_pct: f64) -> Self {
        Self::new(vec![pack("SN-A", soc_pct)])
    }

    pub fn set(&self, packs: Vec<PackReading>) {
        *self.view.lock().expect("fleet view") = FleetView::new(packs);
    }

    pub fn fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::Relaxed);
    }
}

impl DeviceFleet for FakeFleet {
    fn view(&self, _now: i64) -> BoxFuture<'_, Result<FleetView, FleetError>> {
        Box::pin(async move {
            if self.fail.load(Ordering::Relaxed) {
                return Err(FleetError::Unavailable {
                    reason: "scripted failure".to_owned(),
                });
            }
            Ok(self.view.lock().expect("fleet view").clone())
        })
    }
}
