//! Single-run, single-transaction bootstrap capability. Deliberately not cloneable or serialisable.

/// The only key that can open `BootstrapAuthorityTx::begin`.
///
/// It has private fields, no `Clone`, no `Serialize` and no storage constructor. The matching
/// `BootstrapIssue` lives in the separate anchor, so replaying durable state cannot recreate it.
#[derive(Debug)]
pub struct IssuedBootstrapCapability {
    pub(crate) run_id: u64,
    pub(crate) tx_id: u64,
    pub(crate) nonce: u64,
}

impl IssuedBootstrapCapability {
    pub(crate) fn new(run_id: u64, tx_id: u64, nonce: u64) -> Self {
        Self {
            run_id,
            tx_id,
            nonce,
        }
    }

    /// Run identity bound into this capability.
    pub fn run_id(&self) -> u64 {
        self.run_id
    }
    /// Transaction identity bound into this capability.
    pub fn tx_id(&self) -> u64 {
        self.tx_id
    }
}
