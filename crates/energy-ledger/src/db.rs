//! Connection actor and repositories.
//!
//! One `tokio-rusqlite` connection serializes all access, so the rest of the system never touches a
//! `rusqlite::Connection` directly. Tables cover price slots, plans, dispatch decisions,
//! measurements, charge/discharge sessions, learning estimates and savings rollups.
//!
//! No code yet: P0 scaffold only.
