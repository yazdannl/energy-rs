//! The 5-minute rollup tier: means of the 1-minute rows, explicit empty buckets, and a guard that
//! stops a recomputation after 1-minute retention from erasing good 5-minute history.
//!
//! Every bucket in the requested window gets a row for every known pack: the mean of the 1-minute
//! rows inside it, or `NULL` with `samples = 0` when the bucket has no samples. A conflict only
//! overwrites a populated row when the new computation actually saw samples, so re-running the
//! rollup over a window whose raw rows were trimmed leaves the older 5-minute row intact.

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::error::LedgerError;

/// How many 5-minute buckets were written and how many rows that is (packs × buckets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollupReport {
    pub from: i64,
    pub to: i64,
    pub buckets: usize,
    pub rows: usize,
}

const BUCKET_S: i64 = 300;

const UPSERT: &str = "
INSERT INTO measurements_5m (pack_id, ts, power_w, soc_pct, samples)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(pack_id, ts) DO UPDATE SET
    power_w = excluded.power_w,
    soc_pct = excluded.soc_pct,
    samples = excluded.samples
WHERE excluded.samples > 0 OR measurements_5m.samples = 0
";

/// Means of the 1-minute rows per `(pack, bucket)`.
type Means = HashMap<i64, (f64, Option<f64>, i64)>;

pub(crate) fn run(conn: &Connection, from: i64, to: i64) -> Result<RollupReport, LedgerError> {
    let from = floor_bucket(from);
    let to = ceil_bucket(to);
    if from >= to {
        return Ok(RollupReport {
            from,
            to,
            buckets: 0,
            rows: 0,
        });
    }
    let pack_ids = pack_ids(conn)?;
    let bucket_count = usize::try_from((to - from) / BUCKET_S)
        .map_err(|_| LedgerError::Invalid("rollup window is too large".to_owned()))?;
    let tx = conn.unchecked_transaction()?;
    let mut rows = 0_usize;
    {
        let mut insert = tx.prepare(UPSERT)?;
        let mut aggregate = tx.prepare(
            "SELECT pack_id, AVG(power_w), AVG(soc_pct), COUNT(*) FROM measurements
             WHERE ts >= ?1 AND ts < ?2 GROUP BY pack_id",
        )?;
        let mut bucket = from;
        while bucket < to {
            let mut means = Means::new();
            let mut query = aggregate.query(params![bucket, bucket + BUCKET_S])?;
            while let Some(row) = query.next()? {
                let pack_id: i64 = row.get(0)?;
                let power: Option<f64> = row.get(1)?;
                let soc: Option<f64> = row.get(2)?;
                let count: i64 = row.get(3)?;
                if let Some(power) = power {
                    means.insert(pack_id, (power, soc, count));
                }
            }
            for pack_id in &pack_ids {
                let (power, soc, samples) = means
                    .get(pack_id)
                    .copied()
                    .map_or((None, None, 0), |(power, soc, count)| {
                        (Some(power), soc, count)
                    });
                insert.execute(params![pack_id, bucket, power, soc, samples])?;
                rows += 1;
            }
            bucket += BUCKET_S;
        }
    }
    tx.commit()?;
    Ok(RollupReport {
        from,
        to,
        buckets: bucket_count,
        rows,
    })
}

fn pack_ids(conn: &Connection) -> Result<Vec<i64>, LedgerError> {
    let mut statement = conn.prepare("SELECT id FROM packs ORDER BY id")?;
    let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
    let mut ids = Vec::new();
    for row in rows {
        ids.push(row?);
    }
    Ok(ids)
}

fn floor_bucket(ts: i64) -> i64 {
    ts.div_euclid(BUCKET_S) * BUCKET_S
}

fn ceil_bucket(ts: i64) -> i64 {
    let floor = floor_bucket(ts);
    if floor == ts { ts } else { floor + BUCKET_S }
}
