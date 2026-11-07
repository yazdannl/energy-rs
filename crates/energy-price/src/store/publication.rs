//! Publication identity.  A publication is detected by the content hash of its slots (plus the
//! source), which is also the idempotency key for saves; stored rows are never rewritten.

use crate::book::{PriceBook, Slot, SlotSource};
use crate::error::PriceError;

const FNV_OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// One immutable stored publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    /// Monotonic row id; higher means newer.
    pub id: i64,
    /// Resolution/provider of every slot in the publication.
    pub source: SlotSource,
    /// FNV-1a 128-bit content hash, hex encoded.
    pub content_hash: String,
    /// First slot start instant.
    pub first_t_start: i64,
    /// Exclusive end instant of the last slot.
    pub last_t_end: i64,
    /// Number of slots.
    pub slot_count: usize,
    /// Fetch time in UTC epoch seconds (publication age for the UI).
    pub fetched_at: i64,
}

impl Publication {
    /// Whether a slot start instant falls inside the stored coverage.
    pub const fn covers(&self, t_start: i64) -> bool {
        t_start >= self.first_t_start && t_start < self.last_t_end
    }
}

/// Hash of a book's source and slot money/time values; identical fetches map to the same hash.
pub fn content_hash(book: &PriceBook) -> String {
    let mut hash = FNV_OFFSET;
    if let Some(first) = book.first() {
        for byte in first.source.as_str().bytes() {
            hash = fold(hash, byte);
        }
    }
    for slot in book.iter() {
        for value in [
            slot.t_start as u64,
            slot.buy_all_in.to_bits(),
            slot.market.to_bits(),
        ] {
            for byte in value.to_be_bytes() {
                hash = fold(hash, byte);
            }
        }
    }
    format!("{hash:032x}")
}

/// Build publication metadata from a validated, non-empty book (id assigned by the store).
pub(crate) fn publication_from_book(
    book: &PriceBook,
    id: i64,
    content_hash: String,
) -> Result<Publication, PriceError> {
    let (first_t_start, last_t_end) = book_bounds(book)?;
    let source = book.first().ok_or_else(empty_book)?.source;
    Ok(Publication {
        id,
        source,
        content_hash,
        first_t_start,
        last_t_end,
        slot_count: book.len(),
        fetched_at: now_utc(),
    })
}

/// First slot start and exclusive coverage end of a book; an empty book cannot be stored.
pub(crate) fn book_bounds(book: &PriceBook) -> Result<(i64, i64), PriceError> {
    let first = book.first().ok_or_else(empty_book)?;
    let last = book.last().ok_or_else(empty_book)?;
    Ok((first.t_start, last.t_end()))
}

/// Whether the stored slots already are exactly the fetched publication.
///
/// A provider is polled every 15 minutes but publishes roughly once a day; the same publication
/// must not create a new row set on every poll.  The check is `max_end` (no new coverage) plus
/// slot-for-slot equality of the loaded range, so a genuinely revised or extended publication is
/// still stored as a new (immutable) publication.
pub(crate) fn already_stored(
    book: &PriceBook,
    stored: &[Slot],
    latest: Option<&Publication>,
) -> bool {
    let (Some(last), Some(latest)) = (book.last(), latest) else {
        return false;
    };
    latest.last_t_end >= last.t_end()
        && stored.len() == book.len()
        && stored
            .iter()
            .zip(book.iter())
            .all(|(stored, fetched)| stored == fetched)
}

fn empty_book() -> PriceError {
    PriceError::Payload("cannot store an empty price book".to_owned())
}

/// Current UTC time in epoch seconds.
pub(crate) fn now_utc() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Rebuild a book with money rounded to the store's 1e-6 resolution, so stored and loaded values
/// are identical and the content hash is stable.
pub(crate) fn canonical_book(book: &PriceBook) -> Result<PriceBook, PriceError> {
    PriceBook::new(book.iter().map(|slot| slot.money_rounded()).collect())
}

/// Map a SQLite failure to the typed store error.
pub(crate) fn store_error(err: rusqlite::Error) -> PriceError {
    PriceError::Store(err.to_string())
}

fn fold(mut state: u128, byte: u8) -> u128 {
    state ^= u128::from(byte);
    state.wrapping_mul(FNV_PRIME)
}
