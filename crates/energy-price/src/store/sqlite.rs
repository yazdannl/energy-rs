//! SQLite price store: WAL, in-code migrations, append-only publications and an idempotent save
//! keyed by content hash.  Loads pick the newest publication per slot start.

use std::path::Path;
use std::sync::Mutex;
use std::sync::MutexGuard;

use rusqlite::{Connection, OptionalExtension, params};

use crate::book::{PriceBook, Slot, SlotSource};
use crate::error::PriceError;
use crate::store::publication::{
    already_stored, book_bounds, canonical_book, content_hash, publication_from_book,
};
use crate::store::{PriceRepository, Publication, SaveOutcome, schema};

const PUBLICATION_COLUMNS: &str =
    "id, source, content_hash, first_t_start, last_t_end, slot_count, fetched_at";

/// SQLite-backed price repository.
pub struct SqlitePriceRepository {
    conn: Mutex<Connection>,
}

impl SqlitePriceRepository {
    /// Open (and migrate) a database file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PriceError> {
        let conn = Connection::open(path)?;
        Self::prepare(conn)
    }

    /// Open an in-memory database (tests).
    pub fn open_in_memory() -> Result<Self, PriceError> {
        let conn = Connection::open_in_memory()?;
        Self::prepare(conn)
    }

    fn prepare(conn: Connection) -> Result<Self, PriceError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>, PriceError> {
        self.conn
            .lock()
            .map_err(|_| PriceError::Store("sqlite price repository is poisoned".to_owned()))
    }
}

impl PriceRepository for SqlitePriceRepository {
    fn save_publication(&self, book: &PriceBook) -> Result<SaveOutcome, PriceError> {
        let book = canonical_book(book)?;
        let hash = content_hash(&book);
        let (first, end) = book_bounds(&book)?;
        let mut conn = self.lock()?;
        if let Some(publication) = read_publication_by_hash(&conn, &hash)? {
            return Ok(SaveOutcome::Unchanged { publication });
        }
        if let Some(latest) = read_latest_publication(&conn)? {
            let stored = load_slots(&conn, first, end)?;
            if already_stored(&book, &stored, Some(&latest)) {
                return Ok(SaveOutcome::Unchanged {
                    publication: latest,
                });
            }
        }
        let mut publication = publication_from_book(&book, 0, hash)?;
        let slot_count = i64::try_from(publication.slot_count)
            .map_err(|_| PriceError::Store("slot count does not fit SQLite".to_owned()))?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO price_publications
                (source, content_hash, first_t_start, last_t_end, slot_count, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                publication.source.as_str(),
                publication.content_hash,
                publication.first_t_start,
                publication.last_t_end,
                slot_count,
                publication.fetched_at,
            ],
        )?;
        publication.id = tx.last_insert_rowid();
        insert_slots(&tx, publication.id, &book)?;
        tx.commit()?;
        Ok(SaveOutcome::Stored { publication })
    }

    fn load(&self, from: i64, to: i64) -> Result<Vec<Slot>, PriceError> {
        let conn = self.lock()?;
        load_slots(&conn, from, to)
    }

    fn latest_publication(&self) -> Result<Option<Publication>, PriceError> {
        let conn = self.lock()?;
        read_latest_publication(&conn)
    }
}

/// Append the publication's slot rows (one row set per publication, never updated).
fn insert_slots(
    tx: &rusqlite::Transaction<'_>,
    id: i64,
    book: &PriceBook,
) -> Result<(), PriceError> {
    let mut insert = tx.prepare(
        "INSERT INTO price_slots (publication_id, t_start, buy_all_in, market, source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(publication_id, t_start) DO NOTHING",
    )?;
    for slot in book {
        insert.execute(params![
            id,
            slot.t_start,
            slot.buy_all_in,
            slot.market,
            slot.source.as_str(),
        ])?;
    }
    Ok(())
}

/// Newest publication per slot start in `[from, to)`, ascending.
fn load_slots(conn: &Connection, from: i64, to: i64) -> Result<Vec<Slot>, PriceError> {
    let mut statement = conn.prepare(
        "SELECT s.t_start, s.buy_all_in, s.market, s.source
                 FROM price_slots s
                 WHERE s.t_start >= ?1 AND s.t_start < ?2
                   AND s.publication_id = (
                       SELECT MAX(newer.publication_id) FROM price_slots newer
                       WHERE newer.t_start = s.t_start
                   )
                 ORDER BY s.t_start",
    )?;
    let mut rows = statement.query(params![from, to])?;
    let mut slots = Vec::new();
    while let Some(row) = rows.next()? {
        let source_text: String = row.get(3)?;
        let source = SlotSource::from_key(&source_text)?;
        slots.push(Slot::new(row.get(0)?, row.get(1)?, row.get(2)?, source));
    }
    Ok(slots)
}

fn read_latest_publication(conn: &Connection) -> Result<Option<Publication>, PriceError> {
    read_publication(
        conn,
        &format!(
            "SELECT {PUBLICATION_COLUMNS} FROM price_publications
             ORDER BY last_t_end DESC, id DESC LIMIT 1"
        ),
        [],
    )
}

fn read_publication_by_hash(
    conn: &Connection,
    hash: &str,
) -> Result<Option<Publication>, PriceError> {
    read_publication(
        conn,
        &format!("SELECT {PUBLICATION_COLUMNS} FROM price_publications WHERE content_hash = ?1"),
        params![hash],
    )
}

fn read_publication<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<Publication>, PriceError> {
    let raw: Option<(i64, String, String, i64, i64, i64, i64)> = conn
        .query_row(sql, params, |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .optional()?;
    let Some((id, source, content_hash, first_t_start, last_t_end, slot_count, fetched_at)) = raw
    else {
        return Ok(None);
    };
    Ok(Some(Publication {
        id,
        source: SlotSource::from_key(&source)?,
        content_hash,
        first_t_start,
        last_t_end,
        slot_count: usize::try_from(slot_count)
            .map_err(|_| PriceError::Store(format!("negative slot count {slot_count}")))?,
        fetched_at,
    }))
}
