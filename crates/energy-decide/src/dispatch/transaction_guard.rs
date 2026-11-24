//! Per-transaction release owned by the write unit that acquired it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::dispatch::PackIo;

/// Releases exactly one transaction when its owning write unit ends.
///
/// The guard lives inside the per-pack write worker. Completion, cancellation and a
/// panic all end that worker, so release happens on the pack's own unit. The arbiter
/// never calls `PackIo::release` and no queue, extra thread or shutdown path exists.
pub struct TransactionGuard {
    io: Arc<dyn PackIo>,
    pack_id: String,
    tx_id: u64,
    released: AtomicBool,
}

impl TransactionGuard {
    /// Transaction identity owned by this pack's write unit.
    pub fn tx_id(&self) -> u64 {
        self.tx_id
    }

    /// Guard the transaction acquired by `begin`.
    pub fn new(io: Arc<dyn PackIo>, pack_id: &str, tx_id: u64) -> Self {
        Self {
            io,
            pack_id: pack_id.to_owned(),
            tx_id,
            released: AtomicBool::new(false),
        }
    }
}

impl Drop for TransactionGuard {
    fn drop(&mut self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.io.release(&self.pack_id, self.tx_id);
        }
    }
}
