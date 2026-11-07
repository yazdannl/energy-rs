//! Synchronous stores on the async runtime: one blocking-pool hop per request.
//!
//! `rusqlite` is synchronous, so a handler must not call it on a Tokio worker. Both helpers move the
//! closure (and an `Arc` clone of the handle) onto the blocking pool and map a join failure to a
//! generic 500.

use std::sync::Arc;

use energy_ledger::Ledger;
use energy_price::PriceRepository;

use crate::api::error::ApiError;
use crate::service::Service;

/// Run `work` against the ledger on the blocking pool.
pub(crate) async fn with_ledger<T>(
    service: &Service,
    work: impl FnOnce(&Ledger) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    let ledger = Arc::clone(service.ledger());
    tokio::task::spawn_blocking(move || work(&ledger))
        .await
        .map_err(|err| ApiError::internal("ledger task", err))?
}

/// Run `work` against the price store on the blocking pool.
pub(crate) async fn with_prices<T>(
    service: &Service,
    work: impl FnOnce(&dyn PriceRepository) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError>
where
    T: Send + 'static,
{
    let prices = Arc::clone(service.prices());
    tokio::task::spawn_blocking(move || work(prices.as_ref()))
        .await
        .map_err(|err| ApiError::internal("price task", err))?
}
