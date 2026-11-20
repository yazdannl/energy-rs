//! Unit tests for the bounded confirmation loop.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use serde_json::json;

use super::*;
use crate::control::command::Command;
use crate::control::limits::ControlLimits;
use crate::parse_report;
use crate::report::Report;
use crate::transport::{DeviceIo, WriteAck, WriteRequest};

struct NoReport;

impl DeviceIo for NoReport {
    fn sn(&self) -> &'static str {
        "NO-REPORT"
    }

    fn report(&self) -> crate::transport::BoxFuture<'_, Result<Report, BatteryError>> {
        Box::pin(std::future::ready(Err(BatteryError::Invalid {
            reason: "no cached report".to_owned(),
        })))
    }

    fn write(
        &self,
        _: WriteRequest,
    ) -> crate::transport::BoxFuture<'_, Result<WriteAck, BatteryError>> {
        Box::pin(std::future::ready(Ok(WriteAck {
            timestamp: None,
            message_id: None,
            code: 200,
            sn: Some(self.sn().to_owned()),
        })))
    }
}

fn report() -> Report {
    let document = json!({
        "timestamp": 1_791_959_470,
        "messageId": 4211,
        "sn": "NO-REPORT",
        "version": 3,
        "product": "solarFlow800Plus",
        "properties": {"acMode": 2, "inputLimit": 0, "outputLimit": 0},
        "packData": []
    });
    parse_report(&serde_json::to_vec(&document).expect("report json")).expect("parse")
}

#[test]
fn a_read_miss_releases_the_command_even_with_no_cached_report() {
    let controller = DeviceController::new(NoReport, ControlLimits::unlimited());
    let mut inner = controller.lock().expect("inner");
    let pending = controller.install(&mut inner, Command::Charge { watts: 300 }, &report(), false);
    drop(inner);
    assert_eq!(
        controller.pending().map(|p| p.request_id),
        Some(pending.request_id)
    );

    assert!(
        read_miss(
            &controller,
            pending.request_id,
            Some("read timeout".to_owned())
        )
        .is_err()
    );
    assert_eq!(
        controller.pending(),
        None,
        "no Unconfirmed storm on the next write"
    );

    // The same cleanup guarantee applies to a timed-out expiry with no cached report.
    let mut inner = controller.lock().expect("inner");
    let pending = controller.install(&mut inner, Command::Charge { watts: 300 }, &report(), false);
    drop(inner);
    assert!(expire(&controller, pending.request_id, "deadline").is_err());
    assert_eq!(
        controller.pending(),
        None,
        "expiry must also release the command"
    );
}
