//! Query-string parsing shared by the list routes.

use crate::api::error::ApiError;

/// Parse a `limit` value: absent means `default`, otherwise `1..=max`.
pub(crate) fn limit(raw: Option<&str>, default: usize, max: usize) -> Result<usize, ApiError> {
    let Some(raw) = raw else {
        return Ok(default);
    };
    let value = raw
        .trim()
        .parse::<usize>()
        .map_err(|_| ApiError::query(format!("limit {raw:?} is not a positive integer")))?;
    if value == 0 || value > max {
        return Err(ApiError::query(format!(
            "limit must be between 1 and {max}"
        )));
    }
    Ok(value)
}
