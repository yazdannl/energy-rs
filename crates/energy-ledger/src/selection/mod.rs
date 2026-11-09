//! The latched pack selection (`docs/PACK_SELECTION.md` §5): one pack per cycle, held on purpose.

mod repo;
mod row;

pub use repo::Selection;
pub use row::SelectionRow;
