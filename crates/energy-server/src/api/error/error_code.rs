//! Stable machine codes and their HTTP status.

use axum::http::StatusCode;

/// The stable `error.code` values of the API envelope.
///
/// These strings are the contract: the UI switches on them and a test asserts them. They only ever
/// grow; a code's meaning and status never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorCode {
    /// A query parameter is missing, malformed or out of range. 400.
    InvalidQuery,
    /// The JSON body is malformed or has the wrong shape. 400.
    InvalidBody,
    /// The resource does not exist. 404.
    NotFound,
    /// No such pack serial/name. 404.
    UnknownPack,
    /// A timestamped write would rewrite history. 409.
    StaleWrite,
    /// A value is well-formed JSON but not acceptable. 422.
    ValidationFailed,
    /// A known setting received an unacceptable value. 422.
    InvalidSetting,
    /// The setting key is not writable through this API. 422.
    UnsupportedSetting,
    /// The engine queue is not available. 503.
    EngineUnavailable,
    /// A stored plan could not be rendered. 500.
    CorruptPlan,
    /// Anything unexpected. 500; the details stay in the log.
    Internal,
}

impl ErrorCode {
    /// The `error.code` string.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidQuery => "invalid_query",
            Self::InvalidBody => "invalid_body",
            Self::NotFound => "not_found",
            Self::UnknownPack => "unknown_pack",
            Self::StaleWrite => "stale_write",
            Self::ValidationFailed => "validation_failed",
            Self::InvalidSetting => "invalid_setting",
            Self::UnsupportedSetting => "unsupported_setting",
            Self::EngineUnavailable => "engine_unavailable",
            Self::CorruptPlan => "corrupt_plan",
            Self::Internal => "internal",
        }
    }

    /// The HTTP status this code maps to.
    pub(crate) const fn status(self) -> StatusCode {
        match self {
            Self::InvalidQuery | Self::InvalidBody => StatusCode::BAD_REQUEST,
            Self::NotFound | Self::UnknownPack => StatusCode::NOT_FOUND,
            Self::StaleWrite => StatusCode::CONFLICT,
            Self::ValidationFailed | Self::InvalidSetting | Self::UnsupportedSetting => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::EngineUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::CorruptPlan | Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
