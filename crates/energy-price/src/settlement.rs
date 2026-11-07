//! Settlement: the one place that turns kWh into money.
//!
//! `export_price(t)` is `buy_all_in(t)` while a saldering budget remains, otherwise the
//! post-saldering feed-in price; negative prices block export and make grid charging attractive.
//! Everything (optimizer objective, dispatch, cost basis, reporting) calls this module.
//!
//! No code yet: P0 scaffold only.
