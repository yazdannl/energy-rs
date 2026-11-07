//! Applying a write to the fake device, including every way it can refuse.

use serde_json::{Value, json};

use crate::fake::state::FakeState;
use crate::report::decode;

impl FakeState {
    /// Handle one `POST /properties/write`: record it, then answer what the real device would.
    ///
    /// A dropped write (too close to the previous one) still answers `success: true` — that is
    /// exactly why a write is only believed after the next report echoes it.
    pub(crate) fn apply_write(&mut self, body: &Value) -> (u16, Value) {
        self.writes.push(body.clone());
        if let Some(status) = self.behaviour.http_status {
            let reply = json!({
                "code": status,
                "success": false,
                "sn": self.sn,
                "error": "injected http status",
            });
            return self.refuse(Some(status), reply);
        }
        let body_sn = body.get("sn").and_then(Value::as_str).unwrap_or_default();
        if let Some(code) = self.behaviour.write_code {
            let reply = json!({
                "code": code,
                "success": false,
                "sn": self.sn,
                "error": error_text(code),
            });
            return self.refuse(None, reply);
        }
        if body_sn != self.sn {
            let reply = json!({
                "code": 401,
                "success": false,
                "sn": self.sn,
                "error": "sn does not match",
            });
            return self.refuse(None, reply);
        }
        let now = tokio::time::Instant::now();
        let too_close = self.last_write_at.is_some_and(|last| {
            now.saturating_duration_since(last) < self.behaviour.drop_writes_below
        });
        self.last_write_at = Some(now);
        let applies = !too_close && !self.behaviour.ignore_writes && !self.behaviour.wedged;
        if applies {
            self.apply_properties(body.get("properties"));
        } else {
            self.dropped += 1;
        }
        self.applied.push(applies);
        self.message_id = self.message_id.wrapping_add(1);
        self.timestamp = self.timestamp.saturating_add(1);
        let ack = json!({
            "timestamp": self.timestamp,
            "messageId": self.message_id,
            "success": true,
            "code": 200,
            "sn": self.sn,
        });
        (200, ack)
    }

    fn refuse(&mut self, status: Option<u16>, body: Value) -> (u16, Value) {
        self.applied.push(false);
        self.dropped += 1;
        (status.unwrap_or(200), body)
    }

    fn apply_properties(&mut self, properties: Option<&Value>) {
        let Some(properties) = properties.and_then(Value::as_object) else {
            return;
        };
        if let Some(mode) = properties.get("smartMode").and_then(Value::as_i64) {
            self.smart_mode = mode as i32;
        }
        if let Some(mode) = properties.get("acMode").and_then(Value::as_i64) {
            self.ac_mode = mode as i32;
        }
        if let Some(limit) = properties.get("inputLimit").and_then(Value::as_i64) {
            self.input_limit_w = decode::watts(limit);
        }
        if let Some(limit) = properties.get("outputLimit").and_then(Value::as_i64) {
            self.output_limit_w = decode::watts(limit);
        }
    }
}

fn error_text(code: i32) -> &'static str {
    match code {
        400 => "bad parameters",
        401 => "sn does not match",
        413 => "buffer overflow risk",
        _ => "write rejected",
    }
}
