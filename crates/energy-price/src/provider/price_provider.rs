//! The provider abstraction.  One async call fetches a validated book for a local date range; the
//! trait is object safe so the engine can hold `Box<dyn PriceProvider>`.

use std::future::Future;
use std::pin::Pin;

use crate::book::{PriceBook, PriceQuery};
use crate::error::PriceError;
use crate::settlement::Contract;

/// Boxed future used to keep [`PriceProvider`] object safe without an `async_trait` dependency.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A source of quarter-hourly (or legacy hourly) electricity prices.
pub trait PriceProvider: Send + Sync {
    /// Stable provider name for logs and the store.
    fn name(&self) -> &'static str;

    /// Fetch and validate the book for `query`, deriving `market` with `contract` constants.
    fn fetch<'a>(
        &'a self,
        query: PriceQuery,
        contract: &'a Contract,
    ) -> BoxFuture<'a, Result<PriceBook, PriceError>>;
}
