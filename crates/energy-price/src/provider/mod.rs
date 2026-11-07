//! Price providers.  Both impls return the same validated [`PriceBook`](crate::book::PriceBook);
//! the direct API is preferred and the HA sensor forecast is the fallback.  Credentials are held in
//! redacting wrappers and are never logged.

pub mod ha;
pub mod ha_credentials;
pub mod oauth;
pub mod parse;
pub mod parse_ha;
pub mod parse_zonneplan;
pub mod parsed_series;
pub mod price_provider;
pub mod raw_slot;
pub mod zonneplan;
pub mod zonneplan_credentials;

pub use ha::HaSensorFallback;
pub use ha_credentials::HaCredentials;
pub use parse::{ProviderKind, ingest};
pub use parse_ha::parse_state;
pub use parse_zonneplan::parse_chart;
pub use parsed_series::ParsedSeries;
pub use price_provider::{BoxFuture, PriceProvider};
pub use raw_slot::RawSlot;
pub use zonneplan::ZonneplanDirect;
pub use zonneplan_credentials::ZonneplanCredentials;
