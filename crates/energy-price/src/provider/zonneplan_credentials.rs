//! The Zonneplan refresh token.  Deliberately not `Serialize`, and its `Debug` never prints the
//! secret, so neither a log line nor a config dump can leak it.

use std::fmt;

use crate::error::PriceError;

/// OAuth2 refresh token for the Zonneplan app API.
#[derive(Clone, PartialEq, Eq)]
pub struct ZonneplanCredentials {
    refresh_token: String,
}

impl ZonneplanCredentials {
    /// Wrap a refresh token; an empty token is refused at construction.
    pub fn new(refresh_token: impl Into<String>) -> Result<Self, PriceError> {
        let refresh_token = refresh_token.into();
        if refresh_token.trim().is_empty() {
            return Err(PriceError::Auth("empty Zonneplan refresh token".to_owned()));
        }
        Ok(Self { refresh_token })
    }

    /// The secret, for the token endpoint only.
    pub(crate) fn refresh_token(&self) -> &str {
        &self.refresh_token
    }
}

impl fmt::Debug for ZonneplanCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ZonneplanCredentials { refresh_token: <redacted> }")
    }
}
