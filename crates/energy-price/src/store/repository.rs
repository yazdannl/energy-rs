//! The store contract used by the engine: save a fetched publication once, load a range for the
//! planner.  Implementations are synchronous; the engine wraps them in its own DB actor.

use crate::book::{PriceBook, Slot};
use crate::error::PriceError;
use crate::store::{Publication, SaveOutcome};

/// Persisted price publications.
pub trait PriceRepository: Send + Sync {
    /// Store a fetched book as a new publication, or report the identical publication as unchanged.
    fn save_publication(&self, book: &PriceBook) -> Result<SaveOutcome, PriceError>;

    /// Newest publication per slot start in `[from, to)`, ascending.
    fn load(&self, from: i64, to: i64) -> Result<Vec<Slot>, PriceError>;

    /// The publication with the furthest coverage (newest id breaks ties).
    fn latest_publication(&self) -> Result<Option<Publication>, PriceError>;
}
