//! Home Assistant transport: REST, WebSocket, auth, timeouts and reconnect supervision.

mod config;
mod connection;
mod core_config;
mod event_stream;
mod ha_client;
mod ha_error;
mod link_state;
mod reconnect_policy;
mod socket;
mod supervisor;

#[cfg(test)]
mod tests;

pub use config::Config;
pub use connection::Connection;
pub use core_config::CoreConfig;
pub use event_stream::EventStream;
pub use ha_client::HaClient;
pub use ha_error::HaError;
pub use link_state::LinkState;
pub use reconnect_policy::ReconnectPolicy;
pub use supervisor::Supervisor;
