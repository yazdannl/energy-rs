//! A flow wait spans several engine ticks: the next tick must not treat the pending write as a
//! fault and cancel the very ramp it is waiting for.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::unreadable_literal
)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use energy_battery::{
    BatteryError, BoxFuture, ControlLimits, DeviceController, DeviceIo, Report, WriteAck,
    WriteRequest, parse_report,
};
use energy_core::dispatch::{BatterySink, CommandSink};
use serde_json::json;

const SN: &str = "SN-A";

struct Reports {
    queued: Mutex<VecDeque<Report>>,
    next_id: Mutex<u32>,
    writes: AtomicUsize,
}

impl DeviceIo for Reports {
    fn sn(&self) -> &str {
        SN
    }

    fn report(&self) -> BoxFuture<'_, Result<Report, BatteryError>> {
        let queued = self.queued.lock().unwrap().pop_front();
        let report = queued.unwrap_or_else(|| {
            let id = {
                let mut next = self.next_id.lock().unwrap();
                *next += 1;
                *next
            };
            report(id, true)
        });
        Box::pin(async move { Ok(report) })
    }

    fn write(&self, _request: WriteRequest) -> BoxFuture<'_, Result<WriteAck, BatteryError>> {
        self.writes.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {
            Ok(WriteAck {
                timestamp: None,
                message_id: None,
                code: 200,
                sn: Some(SN.to_owned()),
            })
        })
    }
}

fn report(id: u32, flowing: bool) -> Report {
    parse_report(
        json!({
            "sn": SN,
            "timestamp": 1_800_000_000 + i64::from(id),
            "messageId": id,
            "properties": {
                "electricLevel": 12,
                "acMode": if flowing { 1 } else { 2 },
                "acStatus": i32::from(flowing) * 2,
                "dcStatus": i32::from(flowing),
                "inputLimit": if flowing { 300 } else { 0 },
                "outputLimit": 0,
                "gridInputPower": i32::from(flowing) * 300,
                "outputHomePower": 0,
                "chargeMaxLimit": 1000,
                "inverseMaxPower": 800
            }
        })
        .to_string()
        .as_bytes(),
    )
    .expect("report")
}

#[tokio::test]
async fn a_tick_during_the_flow_wait_keeps_the_charge_alive() {
    let io = Arc::new(Reports {
        queued: Mutex::new(VecDeque::from(vec![report(1, false), report(2, true)])),
        next_id: Mutex::new(2),
        writes: AtomicUsize::new(0),
    });
    let controller = Arc::new(
        DeviceController::new(io.clone(), ControlLimits::unlimited()).with_spacing(Duration::ZERO),
    );
    controller.poll().await.expect("baseline");
    let sink = BatterySink::new(vec![Arc::clone(&controller)], Duration::MAX)
        .with_confirm_window(Duration::from_secs(2));

    // The second tick asks for the same command while the first is still in the flow window.
    let (first, second) = tokio::join!(sink.write(SN, 300.0), sink.write(SN, 300.0));
    let first = first.expect("no device fault for the in-flight write");
    let second = second.expect("no device fault for the in-flight write");
    assert!(
        first.verified || second.verified,
        "the shared flow wait must confirm once: {first:?} {second:?}"
    );
    assert!(
        !(first.superseded && second.superseded),
        "a pending write must not cancel itself"
    );
    assert_eq!(
        io.writes.load(Ordering::Relaxed),
        1,
        "only one write went out"
    );
}
