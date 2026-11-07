//! The error body: `{"error": {"code", "message", "details"}}`.

use serde::Serialize;
use serde_json::Value;

/// The inner error object.
#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

/// The envelope every failure uses.
#[derive(Debug, Serialize)]
pub(crate) struct ErrorEnvelope {
    error: ErrorBody,
}

impl ErrorEnvelope {
    pub(crate) fn new(code: &'static str, message: String, details: Option<Value>) -> Self {
        Self {
            error: ErrorBody {
                code,
                message,
                details,
            },
        }
    }
}
