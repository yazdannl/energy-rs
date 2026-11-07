//! One numbered, forward-only schema migration.

/// A migration is applied once, in version order, inside its own transaction.
pub(crate) struct Migration {
    pub(crate) version: i64,
    pub(crate) name: &'static str,
    pub(crate) sql: &'static str,
}
