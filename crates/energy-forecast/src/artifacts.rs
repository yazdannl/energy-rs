//! Model artifacts: loading, versioning and integrity checks.
//!
//! The GBT weights ship as JSON next to the binary (embedded or read from `/data`), and the golden
//! vectors ship with the tests. A missing or mismatched artifact disables the model and falls back
//! to physics + EWMA instead of guessing.
//!
//! No code yet: P0 scaffold only.
