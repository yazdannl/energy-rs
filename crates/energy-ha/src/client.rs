//! REST + WebSocket client for Home Assistant Core.
//!
//! Auth is streamed once per connection (`auth_required` -> `auth` -> `auth_ok`); every request
//! has a timeout and the token is never logged. Both transports are restartable supervisors so a
//! Core restart degrades to a stale-input hold instead of losing the process.
//!
//! No code yet: P0 scaffold only.
