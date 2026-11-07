//! A meter that answers with scripted values.

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

use energy_core::dispatch::{BoxFuture, MeterReading, MeterSource};

/// A meter that answers with scripted values.
pub struct FakeMeter {
    reading: Mutex<MeterReading>,
}

impl Default for FakeMeter {
    fn default() -> Self {
        Self {
            reading: Mutex::new(MeterReading::default()),
        }
    }
}

impl FakeMeter {
    pub fn new(reading: MeterReading) -> Self {
        Self {
            reading: Mutex::new(reading),
        }
    }

    pub fn set(&self, reading: MeterReading) {
        *self.reading.lock().expect("meter reading") = reading;
    }
}

impl MeterSource for FakeMeter {
    fn reading(&self) -> BoxFuture<'_, MeterReading> {
        Box::pin(async move { *self.reading.lock().expect("meter reading") })
    }
}
