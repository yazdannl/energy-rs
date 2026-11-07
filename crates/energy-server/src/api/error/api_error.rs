//! One typed error for every route, rendered through the envelope.

use axum::Json;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::service::CommandRefusal;

use super::envelope::ErrorEnvelope;
use super::error_code::ErrorCode;

/// A request failure with a stable code and a message that is safe to show.
///
/// Messages never carry a token, a file path or a device credential: internal causes are logged and
/// replaced by a generic sentence before they reach the client.
#[derive(Debug)]
pub(crate) struct ApiError {
    code: ErrorCode,
    message: String,
    details: Option<Value>,
}

impl ApiError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Attach structured, non-sensitive context (`{"key": ..., "expected": ...}`).
    pub(crate) fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// A malformed or out-of-range query parameter. 400.
    pub(crate) fn query(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidQuery, message)
    }

    /// A malformed JSON body. 400.
    pub(crate) fn body(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidBody, message)
    }

    /// A missing resource. 404.
    pub(crate) fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// A pack serial/name that the ledger does not know. 404.
    pub(crate) fn unknown_pack(pack: &str) -> Self {
        Self::new(ErrorCode::UnknownPack, format!("unknown pack {pack:?}")).with_details(json!({
            "pack": pack
        }))
    }

    /// A timestamped settings write that would rewrite history. 409.
    pub(crate) fn stale_write(key: &str, at: i64) -> Self {
        Self::new(
            ErrorCode::StaleWrite,
            "the write timestamp predates a stored change for this setting",
        )
        .with_details(json!({ "key": key, "at": at }))
    }

    /// A well-formed value the API refuses. 422.
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationFailed, message)
    }

    /// A known setting with an unacceptable value. 422.
    pub(crate) fn invalid_setting(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidSetting, message)
    }

    /// A setting key that is not writable here. 422.
    pub(crate) fn unsupported_setting(key: &str, hint: &str) -> Self {
        Self::new(
            ErrorCode::UnsupportedSetting,
            format!("setting {key:?} cannot be written through this endpoint"),
        )
        .with_details(json!({ "key": key, "hint": hint }))
    }

    /// No engine loop is draining the command queue. 503.
    pub(crate) fn engine(refusal: CommandRefusal) -> Self {
        Self::new(
            ErrorCode::EngineUnavailable,
            "the engine is not available to accept commands",
        )
        .with_details(json!({ "reason": refusal.reason() }))
    }

    /// A stored plan whose summary cannot be rendered. 500.
    pub(crate) fn corrupt_plan(cause: impl std::fmt::Display) -> Self {
        tracing::error!(error = %cause, "a stored plan summary could not be rendered");
        Self::new(
            ErrorCode::CorruptPlan,
            "the stored plan is not readable; see the add-on log",
        )
    }

    /// Anything unexpected. 500 with a fixed message; the cause is logged.
    pub(crate) fn internal(context: &'static str, cause: impl std::fmt::Display) -> Self {
        tracing::error!(error = %cause, context, "request failed");
        Self::new(ErrorCode::Internal, "the request could not be completed")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope::new(self.code.as_str(), self.message, self.details);
        (self.code.status(), Json(body)).into_response()
    }
}
