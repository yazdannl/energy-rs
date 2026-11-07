//! The one funnel every engine command passes through.

use std::sync::Arc;

use crate::dispatch::{BoxFuture, CommandSink, SinkError, WriteReceipt};
use crate::engine::write_gate::WriteGate;
use crate::engine::write_intent::WriteIntent;

/// A [`CommandSink`] that enforces authority, control generation and durable ownership.
///
/// The engine never calls the transport sink directly: the dispatcher gets one of these bound to
/// the token the tick captured, and the engine's own holds get one bound to the token taken
/// immediately before the hold. Both then pass the same three checks, so there is no path around
/// them:
///
/// 1. **Authority.** `advisory` and `dry_run` write nothing at all, holds included
///    ([`SinkError::Fenced`]).
/// 2. **Generation.** A token older than the live word is refused, so a command queued by a tick
///    that was overtaken by an emergency hold or a mode change never reaches the device.
/// 3. **Ownership.** A non-zero setpoint is journalled durably *before* the transport call
///    ([`WriteIntent`]); a journal that fails means the command is not sent
///    ([`SinkError::NotOwned`]). Holds claim nothing, because a hold only ever removes ownership,
///    and only a verified echo does that.
pub struct GuardedSink {
    inner: Arc<dyn CommandSink>,
    gate: Arc<WriteGate>,
    intent: Arc<dyn WriteIntent>,
    token: u64,
    now: i64,
}

impl GuardedSink {
    /// Bind `inner` to the authority word and the token of one control action.
    pub fn new(
        inner: Arc<dyn CommandSink>,
        gate: Arc<WriteGate>,
        intent: Arc<dyn WriteIntent>,
        token: u64,
        now: i64,
    ) -> Self {
        Self {
            inner,
            gate,
            intent,
            token,
            now,
        }
    }

    /// The refusal for a command that may not be written, if any.
    fn refused(&self, pack_id: &str) -> Option<SinkError> {
        (!self.gate.admit(self.token)).then(|| SinkError::Fenced {
            pack_id: pack_id.to_owned(),
        })
    }
}

impl CommandSink for GuardedSink {
    fn write(&self, pack_id: &str, watts: f64) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        if let Some(error) = self.refused(pack_id) {
            return Box::pin(async move { Err(error) });
        }
        if let Err(error) = self.intent.claim(pack_id, watts, self.now) {
            return Box::pin(async move { Err(error) });
        }
        self.inner.write(pack_id, watts)
    }

    fn hold(&self, pack_id: &str) -> BoxFuture<'_, Result<WriteReceipt, SinkError>> {
        if let Some(error) = self.refused(pack_id) {
            return Box::pin(async move { Err(error) });
        }
        self.inner.hold(pack_id)
    }
}
