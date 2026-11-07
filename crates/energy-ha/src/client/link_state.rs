//! WebSocket link state exposed by the supervisor.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Connected,
    Disconnected,
}
