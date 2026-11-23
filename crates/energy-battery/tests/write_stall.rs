//! One device write that never returns must not freeze polling or safety holds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;
use tokio::sync::Notify;

use energy_battery::control::{Command, ControlLimits, DeviceController};
use energy_battery::error::BatteryError;
use energy_battery::parse_report;
use energy_battery::report::Report;
use energy_battery::transport::{BoxFuture, DeviceIo, WriteAck, WriteRequest};

struct NeverWrites {
    started: Arc<Notify>,
    holds: Arc<AtomicUsize>,
}

impl DeviceIo for NeverWrites {
    fn sn(&self) -> &'static str {
        "STUCK"
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        Box::pin(async move { Ok(report()) })
    }

    fn write(&self, request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        let started = Arc::clone(&self.started);
        let holds = Arc::clone(&self.holds);
        Box::pin(async move {
            if request.properties().ac_mode().is_none() {
                holds.fetch_add(1, Ordering::Relaxed);
                return Ok(WriteAck {
                    timestamp: None,
                    message_id: None,
                    code: 200,
                    sn: Some(self.sn().to_owned()),
                });
            }
            started.notify_one();
            std::future::pending().await
        })
    }
}

fn report() -> Report {
    let document = json!({
        "timestamp": 1_791_959_470,
        "messageId": 4211,
        "sn": "STUCK",
        "version": 3,
        "product": "solarFlow800Plus",
        "properties": {"acMode": 2, "inputLimit": 0, "outputLimit": 800, "chargeMaxLimit": 1000, "inverseMaxLimit": 800},
        "packData": []
    });
    parse_report(&serde_json::to_vec(&document).expect("report json")).expect("parse")
}

#[tokio::test]
async fn a_never_acknowledged_write_cannot_block_polling_or_a_hold() {
    let started = Arc::new(Notify::new());
    let holds = Arc::new(AtomicUsize::new(0));
    let controller = Arc::new(
        DeviceController::new(
            NeverWrites {
                started: Arc::clone(&started),
                holds: Arc::clone(&holds),
            },
            ControlLimits::unlimited(),
        )
        .with_spacing(Duration::ZERO),
    );
    tokio::time::timeout(Duration::from_secs(2), controller.poll())
        .await
        .expect("prime poll must not hang")
        .expect("prime the report");
    eprintln!("primed");

    let writer = Arc::clone(&controller);
    let write = tokio::spawn(async move { writer.apply(Command::Charge { watts: 300 }).await });
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .expect("write must start");
    eprintln!("write started");

    eprintln!("polling during write");
    assert!(
        tokio::time::timeout(Duration::from_secs(1), controller.poll())
            .await
            .is_ok(),
        "a stalled write froze the sampler"
    );

    eprintln!("poll during write done");
    let holder = Arc::clone(&controller);
    let hold = tokio::spawn(async move { holder.apply(Command::Hold).await });
    eprintln!("issuing hold");
    let held = tokio::time::timeout(Duration::from_secs(8), hold)
        .await
        .expect("a safety hold must not wait behind a stalled write")
        .expect("hold task");
    assert!(held.is_ok(), "hold failed: {:?}", held.err());
    assert_eq!(
        holds.load(Ordering::Relaxed),
        1,
        "hold must actually be written"
    );

    eprintln!("hold done");
    write.abort();
}
