#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use crate::db::Ledger;

#[test]
fn a_precision_sensitive_real_is_preserved_bit_for_bit() {
    let ledger = Arc::new(Ledger::open_in_memory().expect("ledger"));
    ledger
        .lock()
        .expect("connection")
        .execute(
            "INSERT INTO learning_outbox (pack_id, at, summary)
             VALUES ('SN-REAL', 0.1 + 0.2, 'real payload')",
            [],
        )
        .expect("seed precision-sensitive real");
    let quarantined = {
        let outbox = ledger.learning_outbox().expect("outbox");
        let _ = outbox.pending().expect("pending");
        outbox.quarantined().expect("quarantine")
    };

    assert_eq!(quarantined.len(), 1);
    assert_eq!(quarantined[0].at.storage_type, "real");
    let expected = 0.1_f64 + 0.2_f64;
    assert_eq!(
        quarantined[0].at.as_real().expect("real").to_bits(),
        expected.to_bits()
    );
}
