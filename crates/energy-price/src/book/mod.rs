//! The price book: validated 15-minute (or hourly) slots keyed by UTC start instant.

pub mod book;
pub mod query;
pub mod slot;
pub mod source;

pub use book::PriceBook;
pub use query::PriceQuery;
pub use slot::Slot;
pub use source::SlotSource;
