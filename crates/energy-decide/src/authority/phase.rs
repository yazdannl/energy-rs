//! Whether a durable, audited authority token exists at all.

/// Authority phase, kept separate from the three operational states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityPhase {
    /// No token yet: only the sealed bootstrap transaction may mutate authority.
    Uninitialized,
    /// Valid token and matching initialization event: the M5 state machine applies.
    Initialized,
    /// Missing, malformed or unexplained token: no writes except the diagnostic record.
    Invalid,
}

impl AuthorityPhase {
    /// Stable name used in audit rows and API payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uninitialized => "AUTHORITY_UNINITIALIZED",
            Self::Initialized => "AUTHORITY_INITIALIZED",
            Self::Invalid => "AUTHORITY_INVALID",
        }
    }
}
