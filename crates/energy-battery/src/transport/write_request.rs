//! The `POST /properties/write` body.

use serde::Serialize;

use crate::error::BatteryError;
use crate::transport::write_properties::WriteProperties;

/// `source: 2` is what the vendor's Home Assistant integration sends ("this came from HA").
pub const SOURCE_HOME_ASSISTANT: u32 = 2;

/// The device truncates or refuses anything larger ("local API receive length is 512 bytes").
pub const MAX_BODY_BYTES: usize = 512;

/// One write command: device serial, request counter, source marker and the property set.
///
/// The JSON is compact and field-ordered by the struct definition, so a payload is predictable:
/// `{"sn":"...","id":7,"source":2,"properties":{"smartMode":1,...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteRequest {
    sn: String,
    id: u32,
    source: u32,
    properties: WriteProperties,
}

impl WriteRequest {
    pub fn new(sn: impl Into<String>, id: u32, properties: WriteProperties) -> Self {
        Self {
            sn: sn.into(),
            id,
            source: SOURCE_HOME_ASSISTANT,
            properties,
        }
    }

    pub fn sn(&self) -> &str {
        &self.sn
    }

    /// Monotonic request counter. The device does not echo it, so it is for logs only.
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn properties(&self) -> &WriteProperties {
        &self.properties
    }

    /// Serialize to bytes and enforce the device's 512-byte receive limit before sending.
    pub fn body(&self) -> Result<Vec<u8>, BatteryError> {
        let body = serde_json::to_vec(self).map_err(|err| BatteryError::Payload {
            reason: format!("write body cannot be serialized: {err}"),
        })?;
        if body.len() > MAX_BODY_BYTES {
            return Err(BatteryError::BodyTooLarge {
                len: body.len(),
                limit: MAX_BODY_BYTES,
            });
        }
        Ok(body)
    }
}
