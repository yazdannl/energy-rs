//! Transport worker: completion is arbitrated at worker finish, not at collection time.
//!
//! Cancellation and generation fencing cannot un-send a request already at the device.
//! They only stop a stale acknowledgement from contaminating M4's state: the physical
//! effect may still have happened, which is why every timeout marks the pack Uncertain.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::dispatch::{LateWrite, PackCommand, PackIo, WriteAck};

const LATE_REASON: &str = "late_after_deadline_superseded";

use crate::dispatch::write_claim::Claim;

pub(crate) struct WorkerArgs {
    pub(crate) io: Arc<dyn PackIo>,
    pub(crate) command: PackCommand,
    pub(crate) tx_id: u64,
    pub(crate) budget_ms: u64,
    pub(crate) generation: u64,
    pub(crate) late: Arc<Mutex<Vec<LateWrite>>>,
    pub(crate) claim: Arc<Claim>,
    pub(crate) done: Arc<AtomicBool>,
    pub(crate) sender: Sender<WriteAck>,
    pub(crate) deadline: Instant,
}

pub(crate) fn run_worker(args: WorkerArgs) {
    let ack = args.io.write(
        &args.command.pack_id,
        &args.command,
        args.budget_ms,
        args.tx_id,
    );
    args.done.store(true, Ordering::SeqCst);
    if Instant::now() >= args.deadline || !args.claim.claim() {
        let mut log = args
            .late
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        log.push(LateWrite {
            pack_id: args.command.pack_id,
            generation: args.generation,
            reason: LATE_REASON,
        });
        return;
    }
    drop(args.sender.send(ack));
}
