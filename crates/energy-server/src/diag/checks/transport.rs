//! Home Assistant reachability and which token kind the process resolved.

use std::sync::Arc;

use energy_ha::HaClient;

use crate::bootstrap::options::Options;
use crate::diag::check::Check;
use crate::diag::ha_client;

/// `GET /api/config` over the configured transport; the token kind is named, the token is not.
pub async fn check(options: &Options) -> (Check, Option<Arc<HaClient>>) {
    let kind = ha_client::kind(options);
    let client = match ha_client::build(options) {
        Ok(client) => client,
        Err(error) => {
            return (
                Check::fail("ha transport", format!("{kind} token, {error}")),
                None,
            );
        }
    };
    match client.core_config().await {
        Ok(core) => (
            Check::pass(
                "ha transport",
                format!(
                    "{kind} token, HA {}, time zone {}, currency {}",
                    core.version, core.time_zone, core.currency
                ),
            ),
            Some(client),
        ),
        Err(error) => (
            Check::fail(
                "ha transport",
                format!("{kind} token, GET /api/config failed: {error}"),
            ),
            None,
        ),
    }
}
