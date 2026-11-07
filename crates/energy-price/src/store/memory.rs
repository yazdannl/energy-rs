//! In-memory repository for tests and `advisory` runs: same publication semantics as SQLite
//! (content-hash idempotency, immutable publications, newest publication per slot wins).

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::book::{PriceBook, Slot};
use crate::error::PriceError;
use crate::store::publication::{content_hash, publication_from_book};
use crate::store::{PriceRepository, Publication, SaveOutcome};

#[derive(Debug, Clone)]
struct StoredPublication {
    publication: Publication,
    slots: Vec<Slot>,
}

/// Process-local price repository.
#[derive(Debug, Default)]
pub struct MemoryPriceRepository {
    publications: Mutex<Vec<StoredPublication>>,
}

impl MemoryPriceRepository {
    /// Empty repository.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Vec<StoredPublication>>, PriceError> {
        self.publications
            .lock()
            .map_err(|_| PriceError::Store("memory price repository is poisoned".to_owned()))
    }
}

impl PriceRepository for MemoryPriceRepository {
    fn save_publication(&self, book: &PriceBook) -> Result<SaveOutcome, PriceError> {
        let hash = content_hash(book);
        let mut publications = self.lock()?;
        if let Some(existing) = publications
            .iter()
            .find(|stored| stored.publication.content_hash == hash)
        {
            return Ok(SaveOutcome::Unchanged {
                publication: existing.publication.clone(),
            });
        }
        let next_id = publications
            .iter()
            .map(|stored| stored.publication.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let publication = publication_from_book(book, next_id, hash)?;
        publications.push(StoredPublication {
            publication: publication.clone(),
            slots: book.slots().to_vec(),
        });
        Ok(SaveOutcome::Stored { publication })
    }

    fn load(&self, from: i64, to: i64) -> Result<Vec<Slot>, PriceError> {
        let publications = self.lock()?;
        let mut ordered: Vec<&StoredPublication> = publications.iter().collect();
        ordered.sort_by_key(|stored| stored.publication.id);
        let mut newest: BTreeMap<i64, Slot> = BTreeMap::new();
        for stored in ordered {
            for slot in stored
                .slots
                .iter()
                .filter(|slot| slot.t_start >= from && slot.t_start < to)
            {
                newest.insert(slot.t_start, *slot);
            }
        }
        Ok(newest.into_values().collect())
    }

    fn latest_publication(&self) -> Result<Option<Publication>, PriceError> {
        let publications = self.lock()?;
        Ok(publications
            .iter()
            .max_by_key(|stored| (stored.publication.last_t_end, stored.publication.id))
            .map(|stored| stored.publication.clone()))
    }
}
