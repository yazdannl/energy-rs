//! Shared fixtures: a report document and a scripted (non-HTTP) device.
//
// Test code: a failed assertion is the report, so unwrap/expect/panic! are the intended API.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::items_after_statements,
    // Fixture indices are single- or double-digit (hour offsets), never near i64::MAX.
    clippy::cast_possible_wrap,
    // Test fixtures build a JSON literal at the call site and hand it over; copying it to satisfy
    // pedantic's by-reference rule would obscure the test.
    clippy::needless_pass_by_value,
    dead_code
)]

use std::collections::VecDeque;
use std::sync::Mutex;

use energy_battery::{BatteryError, BoxFuture, DeviceIo, Report, WriteAck, WriteRequest};
use serde_json::{Value, json};
use tokio::time::Instant;

pub mod gated;
pub mod latch_store;
pub mod learning;
pub mod learning_store;

/// Device serial used by the fixtures.
pub const SN: &str = "EOC1NLN9N4XXXXXX";
/// Pack serial inside the fixtures: `C` prefix, so an AB2000 at 1.92 kWh.
pub const PACK_SN: &str = "CO4A000000000001";

/// A report body with packs at the top level (the reference-library envelope variant).
pub fn report_json(sn: &str) -> Value {
    json!({
        "timestamp": 1_791_959_470,
        "messageId": 4211,
        "sn": sn,
        "version": 3,
        "product": "solarFlow800Plus",
        "properties": {
            "electricLevel": 45,
            "acMode": 2,
            "inputLimit": 0,
            "outputLimit": 0,
            "smartMode": 1,
            "minSoc": 100,
            "socSet": 1000,
            "socLimit": 0,
            "chargeMaxLimit": 1000,
            "inverseMaxPower": 800,
            "gridInputPower": 0,
            "outputHomePower": 0,
            "solarInputPower": 0,
            "outputPackPower": 0,
            "packInputPower": 0,
            "packNum": 1,
            "BatVolt": 4935,
            "hyperTmp": 2993,
            "solarPower1": 120,
            "solarPower2": 80,
            "gridReverse": 1,
            "oldMode": 2,
            "is_error": 0,
            "faultLevel": 1,
            "localApiEnable": true,
            "hemsState": 0,
            "remainOutTime": 59940,
            "writeRsp": 0,
            "ts": 1_791_959_470
        },
        "packData": [{
            "sn": PACK_SN,
            "packType": 1,
            "socLevel": 45,
            "state": 0,
            "power": 0,
            "totalVol": 4810,
            "maxVol": 321,
            "minVol": 321,
            "maxTemp": 3051,
            "batcur": 0,
            "softVersion": 4355,
            "heatState": 0
        }]
    })
}

/// Parse a JSON document the way the transport does.
pub fn parse(document: &Value) -> Report {
    energy_battery::parse_report(&serde_json::to_vec(document).unwrap()).unwrap()
}

/// The default report.
pub fn report(sn: &str) -> Report {
    parse(&report_json(sn))
}

/// The default report with individual properties overridden.
pub fn report_with(sn: &str, overrides: Value) -> Report {
    report_at(sn, 4211, 1_791_959_470, overrides)
}

/// A report with an explicit counter and device clock.
///
/// The controller only lets a report that is *newer* than the pre-write report confirm or fault a
/// command, so a test that scripts an echo has to move `messageId` (and the clock) forward.
pub fn report_at(sn: &str, message_id: u32, timestamp: i64, overrides: Value) -> Report {
    let mut document = report_json(sn);
    document["messageId"] = json!(message_id);
    document["timestamp"] = json!(timestamp);
    let properties = document["properties"].as_object_mut().unwrap();
    properties.insert("ts".to_owned(), json!(timestamp));
    for (key, value) in overrides.as_object().unwrap() {
        properties.insert(key.clone(), value.clone());
    }
    parse(&document)
}

/// A device that answers from a queue of reports and records every write.
///
/// Used for pacing and echo tests: no HTTP, so the timing assertions are about the pacer alone.
pub struct ScriptedDevice {
    sn: String,
    reports: Mutex<VecDeque<Report>>,
    writes: Mutex<Vec<(Instant, WriteRequest)>>,
}

impl ScriptedDevice {
    pub fn new(sn: &str, reports: Vec<Report>) -> Self {
        Self {
            sn: sn.to_owned(),
            reports: Mutex::new(reports.into()),
            writes: Mutex::new(Vec::new()),
        }
    }

    pub fn push_report(&self, report: Report) {
        self.reports.lock().unwrap().push_back(report);
    }

    pub fn write_times(&self) -> Vec<Instant> {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .map(|(at, _)| *at)
            .collect()
    }

    pub fn writes(&self) -> Vec<WriteRequest> {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .map(|(_, request)| request.clone())
            .collect()
    }

    pub fn write_count(&self) -> usize {
        self.writes.lock().unwrap().len()
    }
}

impl DeviceIo for ScriptedDevice {
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
        let sn = self.sn.clone();
        self.writes.lock().unwrap().push((Instant::now(), request));
        let ack = WriteAck {
            timestamp: None,
            message_id: None,
            code: 200,
            sn: Some(sn),
        };
        Box::pin(std::future::ready(Ok(ack)))
    }
}
