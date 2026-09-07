pub mod api;
pub mod batch;
pub mod c_api;
pub mod hub;
pub mod manager;
pub mod market;
pub mod models;
pub mod packet;
pub mod parser;
pub mod resampler;
pub mod sector_index;
pub mod service;
pub mod table;
pub mod time;

pub use api::{ClientMode, MockClient, RealClient, TdxClient, TdxHqApi};
pub use batch::BatchHandle;
pub use hub::{HubStatistics, StockDataHub};
pub use manager::{
    ManagerConfig, ManagerStatistics, ServerHealthSnapshot, TdxDataManager, WorkerActivitySnapshot,
};
pub use market::{DEFAULT_TDX_SERVER_SOURCE, default_servers, market_for_code, static_limit_ratio};
pub use models::{
    FinanceInfo, GenericRow, KlineBar, KlineType, Metadata, QuoteRecord, Task, TaskData,
    TaskResult, TaskType,
};
pub use packet::TdxPacket;
pub use parser::{
    ResponseHeader, parse_block_data, parse_block_info_meta_body, parse_company_info_category_body,
    parse_company_info_content_body, parse_finance_info_body, parse_kline_body,
    parse_minute_time_body, parse_response_header, parse_security_count_body,
    parse_security_list_body, parse_tdx_packed_number, uncompress_zlib,
};
pub use resampler::{compute_daily_indicators, resample, resample_minute_to_daily_with_features};
pub use sector_index::{
    calculate_equal_weight_index, calculate_equal_weight_index_min1, filter_day_bars_by_code,
    unique_dates,
};
pub use service::TdxDataService;
pub use table::{DailyFeatureRow, DailyFeatureTable, DayBar, IndexBar, IndexTable, KlineTable};
