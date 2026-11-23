//! Shared timing constants for one pack's command path.

use std::time::Duration;

/// One end-to-end I/O budget for a pack command: apply, echo and one bounded fallback.
/// Every caller -- dispatch, holds, boot and shutdown -- gets the same ceiling.
pub const PACK_IO_BUDGET: Duration = Duration::from_secs(20);
