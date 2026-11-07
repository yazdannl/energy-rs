//! Home Assistant base URL and long-lived token.  The token is redacted from `Debug` and never
//! appears in errors or URLs.

use std::fmt;

use crate::error::PriceError;

/// REST access to one Home Assistant instance.
#[derive(Clone, PartialEq, Eq)]
pub struct HaCredentials {
    base_url: String,
    token: String,
}

impl HaCredentials {
    /// Build credentials from the HA root URL (e.g. `http://supervisor/core`) and a token.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, PriceError> {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(PriceError::Auth(format!(
                "HA base URL {base_url} is not http(s)"
            )));
        }
        let token = token.into();
        if token.trim().is_empty() {
            return Err(PriceError::Auth("empty Home Assistant token".to_owned()));
        }
        Ok(Self { base_url, token })
    }

    /// HA root URL without a trailing slash.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The secret, for the `Authorization` header only.
    pub(crate) fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for HaCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HaCredentials")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}
