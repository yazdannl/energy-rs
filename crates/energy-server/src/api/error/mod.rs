//! The one error shape every `/api/v1` route uses.

pub(crate) mod api_error;
pub(crate) mod envelope;
pub(crate) mod error_code;

pub(crate) use api_error::ApiError;
