//! Forward-only schema migrations.
//!
//! Schema changes are additive and versioned; a database with an unknown (future) version makes the
//! app exit with a clear error instead of running on a mismatched schema.
//!
//! No code yet: P0 scaffold only.
