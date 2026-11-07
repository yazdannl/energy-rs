//! What the HTTP layer may ask the engine to do.
//!
//! These are the only writes the API has: choose the mode, pin one pack, request the maintenance
//! full charge, and ask for a relearn pass. Everything else (plans, dispatch) is the engine's. A
//! command is a request, not an effect: the next [`EngineSnapshot`](crate::stream::EngineSnapshot)
//! is what proves it happened.

use crate::safety::Authority;
use crate::stream::OverrideCommand;

/// One validated command from the API to the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    /// Persist and obey a new control mode (the API also wrote the ledger setting).
    SetMode(Authority),
    /// Pin one pack to charge/hold/off until the TTL expires.
    Override(OverrideCommand),
    /// Run the weekly full-charge maintenance now, for one pack or the whole fleet.
    FullCharge {
        /// Pack serial, or `None` for every pack.
        pack: Option<String>,
    },
    /// Relearn capacity/efficiency, for one pack or the whole fleet.
    Relearn {
        /// Pack serial, or `None` for every pack.
        pack: Option<String>,
    },
}

impl EngineCommand {
    /// A stable slug for logs.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SetMode(_) => "set_mode",
            Self::Override(_) => "override",
            Self::FullCharge { .. } => "full_charge",
            Self::Relearn { .. } => "relearn",
        }
    }
}
