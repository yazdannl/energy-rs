//! Price storage: publications are immutable, one row set per publication, and loads pick the
//! newest publication per slot.  Money is rounded to 1e-6 before it reaches the database.

pub mod memory;
pub mod outcome;
pub mod publication;
pub mod repository;
pub mod schema;
pub mod sqlite;

pub use memory::MemoryPriceRepository;
pub use outcome::SaveOutcome;
pub use publication::Publication;
pub use repository::PriceRepository;
pub use sqlite::SqlitePriceRepository;
