//! Statistics, history and slot series.

mod granularity;
mod history_query;
mod history_state;
mod mean_series;
mod period;
mod slot;
mod slot_series;
mod stat_row;
mod stat_type;
mod statistic_kind;
mod statistic_metadata;
mod statistics_query;

pub use granularity::Granularity;
pub use history_query::{HistoryQuery, history_during_period};
pub use history_state::{HistoryResponse, HistoryState};
pub use mean_series::mean_series_hourly;
pub use period::Period;
pub use slot::{Slot, parse_slots};
pub use slot_series::{slot_series, slot_series_5min, slot_series_15min, slot_series_hourly};
pub use stat_row::{StatRow, StatisticsResponse};
pub use stat_type::StatType;
pub use statistic_kind::StatisticKind;
pub use statistic_metadata::{
    StatisticMetadata, list_statistic_ids, statistic_metadata_for, statistics_metadata,
};
pub use statistics_query::{StatisticsQuery, statistics_during_period};
