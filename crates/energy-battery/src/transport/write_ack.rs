//! The `POST /properties/write` reply.

use serde_json::Value;

use crate::error::BatteryError;

/// HTTP 200 is not success: the body must say `success: true`, `code: 200` and echo our serial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteAck {
    pub timestamp: Option<i64>,
    pub message_id: Option<u32>,
    pub code: i32,
    pub sn: Option<String>,
}

impl WriteAck {
    /// Parse and validate a reply body against the serial we wrote.
    ///
    /// A code of 400, 401 or 413 becomes its own typed error, armed with the body's `error` text
    /// (the vendor's discriminator when a code is ambiguous). Any other non-200 code is
    /// [`BatteryError::Rejected`].
    pub fn parse(body: &[u8], expected_sn: &str) -> Result<Self, BatteryError> {
        let value: Value = serde_json::from_slice(body).map_err(|err| BatteryError::Payload {
            reason: format!("write reply is not JSON: {err}"),
        })?;
        let code = value
            .get("code")
            .and_then(Value::as_i64)
            .map_or(200, |raw| raw as i32);
        let success = value
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(code == 200);
        let got_sn = value.get("sn").and_then(Value::as_str).map(str::to_owned);
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        reject_non_200(code, &message, expected_sn, got_sn.as_deref())?;
        if !success {
            return Err(BatteryError::Rejected {
                code,
                message: detail(&message, "success=false"),
            });
        }
        if got_sn.as_deref() != Some(expected_sn) {
            return Err(BatteryError::SnMismatch {
                sent: expected_sn.to_owned(),
                got: got_sn.unwrap_or_else(|| "<missing>".to_owned()),
            });
        }
        Ok(Self {
            timestamp: value.get("timestamp").and_then(Value::as_i64),
            message_id: value
                .get("messageId")
                .and_then(Value::as_i64)
                .and_then(|raw| u32::try_from(raw).ok()),
            code,
            sn: got_sn,
        })
    }
}

fn reject_non_200(
    code: i32,
    message: &str,
    expected_sn: &str,
    got_sn: Option<&str>,
) -> Result<(), BatteryError> {
    match code {
        200 => Ok(()),
        // In-body 401 is a serial mismatch, never authentication: there is no token to be wrong.
        401 => Err(BatteryError::SnMismatch {
            sent: expected_sn.to_owned(),
            got: got_sn.unwrap_or("<missing>").to_owned(),
        }),
        400 => Err(BatteryError::Rejected {
            code,
            message: detail(message, "bad parameters"),
        }),
        413 => Err(BatteryError::BufferOverflow {
            detail: detail(message, "buffer overflow risk"),
        }),
        _ => Err(BatteryError::Rejected {
            code,
            message: detail(message, "write rejected"),
        }),
    }
}

fn detail(message: &str, fallback: &str) -> String {
    if message.is_empty() {
        fallback.to_owned()
    } else {
        message.to_owned()
    }
}
