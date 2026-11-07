//! Why a command did not reach the pack.

use energy_battery::control::ControlFault;
use thiserror::Error;

/// A write that failed before the device confirmed anything.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SinkError {
    /// The pack is not one this process may command.
    #[error("no device is mapped to pack {pack_id}")]
    UnknownPack {
        /// The pack that was asked for.
        pack_id: String,
    },
    /// The transport refused or the device is unreachable.
    #[error("pack {pack_id}: {reason}")]
    Device {
        /// The pack that refused.
        pack_id: String,
        /// Human-readable cause.
        reason: String,
    },
    /// The device reported a mode, echo or restart fault.
    #[error("pack {pack_id}: {fault}")]
    Fault {
        /// The pack that faulted.
        pack_id: String,
        /// The typed controller fault.
        fault: Box<ControlFault>,
    },
}
