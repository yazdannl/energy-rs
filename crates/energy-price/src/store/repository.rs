//! The store contract used by the engine: save a fetched publication once, load a range for the
//! planner.  Implementations are synchronous; the engine wraps them in its own DB actor.

use crate::book::{PriceBook, Slot};
use crate::error::PriceError;
use crate::store::{Publication, SaveOutcome};

/// Persisted price publications.
pub trait PriceRepository: Send + Sync {
    /// Store a fetched book as a new publication, or report the identical publication as unchanged.
    ///
    /// Saves are idempotent (content hash) and never extend a stored publication: a repeated poll of
    /// the same publication reports [`SaveOutcome::Unchanged`], and a revised or extended one is
    /// written as a new publication beside the old rows.
    fn save_publication(&self, book: &PriceBook) -> Result<SaveOutcome, PriceError>;

    /// Newest publication per slot start in `[from, to)`, ascending.
    fn load(&self, from: i64, to: i64) -> Result<Vec<Slot>, PriceError>;

    /// The publication with the furthest coverage (newest id breaks ties).
    fn latest_publication(&self) -> Result<Option<Publication>, PriceError>;

    /// Planner-facing load: the `[from, to)` range as one validated, gap-free book.
    ///
    /// A hole inside the range is refused ([`PriceError::Gap`]) rather than skipped or filled; an
    /// empty range yields an empty book, so callers check coverage explicitly.
    fn load_book(&self, from: i64, to: i64) -> Result<PriceBook, PriceError> {
        PriceBook::new(self.load(from, to)?)
    }
}
