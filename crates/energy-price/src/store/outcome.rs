//! What happened to a save: a new publication, or the same publication already stored.

use crate::store::Publication;

/// Result of [`PriceRepository::save_publication`](crate::store::PriceRepository::save_publication).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// A new publication was written.
    Stored { publication: Publication },
    /// The identical publication (same content hash) was already present; nothing was written.
    Unchanged { publication: Publication },
}

impl SaveOutcome {
    /// Whether a new publication was written.
    pub const fn is_new(&self) -> bool {
        matches!(self, SaveOutcome::Stored { .. })
    }

    /// The publication the save referred to.
    pub fn publication(&self) -> &Publication {
        match self {
            SaveOutcome::Stored { publication } | SaveOutcome::Unchanged { publication } => {
                publication
            }
        }
    }
}
