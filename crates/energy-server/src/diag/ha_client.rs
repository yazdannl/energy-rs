//! One Home Assistant client for the diagnostics, without opening a WebSocket yet.

use std::sync::Arc;

use energy_ha::{Config, HaClient};

use crate::bootstrap::options::Options;

/// The client the configured transport resolves to; the token is never part of the error.
pub fn build(options: &Options) -> Result<Arc<HaClient>, String> {
    let config = match options.ha() {
        Ok((true, _, token)) => Config::supervisor(token),
        Ok((false, url, token)) => {
            Config::direct(url, token).map_err(|error| format!("unusable URL: {error}"))?
        }
        Err(error) => return Err(error.to_string()),
    };
    HaClient::new(config)
        .map(Arc::new)
        .map_err(|error| error.to_string())
}

/// Which token kind the process resolved, for the table.
pub fn kind(options: &Options) -> &'static str {
    match options.ha() {
        Ok((true, _, _)) => "supervisor",
        Ok((false, _, _)) => "direct",
        Err(_) => "absent",
    }
}
