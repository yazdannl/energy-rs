//! The one-shot fail-safe hold after a real write fault.

use crate::dispatch::{CommandSink, Progress, SinkError, advance};

use super::{Dispatcher, Sent};

/// Hold once, then leave the pack alone for the backoff window.
///
/// The fault string is kept on the guard so the skipped rows name the fault that armed the window.
/// A read miss on the hold itself is recorded as an unverified hold, not treated as a second fault.
pub(super) async fn fail_safe(
    dispatcher: &Dispatcher,
    progress: Option<&dyn Progress>,
    now: i64,
    pack_id: &str,
    error: &SinkError,
    sink: &dyn CommandSink,
) -> Sent {
    let cause = error.to_string();
    dispatcher.faults.arm(pack_id, now, &cause);
    if !dispatcher.failsafe_on_error {
        return Sent {
            error: Some(cause),
            ..Sent::none()
        };
    }
    match sink.hold_with_progress(pack_id, progress, now).await {
        // The hold was sent but its own readback missed. It is already the one hold this fault
        // gets: do not loop into another hold or extend the backoff.
        Err(SinkError::ReadUnverified { reason, .. }) => Sent {
            written: true,
            verified: false,
            fail_safe: true,
            error: Some(format!("{cause}; fail-safe hold not echoed: {reason}")),
            ..Sent::none()
        },
        Ok(receipt) => {
            advance(progress, now);
            Sent {
                written: true,
                verified: receipt.verified,
                applied_w: receipt.applied_w,
                fail_safe: true,
                error: Some(format!("{cause}; fail-safe hold")),
            }
        }
        Err(hold_error) => Sent {
            error: Some(format!("{cause}; fail-safe hold failed: {hold_error}")),
            ..Sent::none()
        },
    }
}
