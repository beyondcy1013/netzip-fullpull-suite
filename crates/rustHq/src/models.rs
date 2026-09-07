use std::collections::BTreeMap;
use std::time::Duration;

pub type Metadata = BTreeMap<String, String>;
pub type GenericRow = BTreeMap<String, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum KlineType {
    Kline5Min = 0,
    Kline15Min = 1,
    Kline30Min = 2,
    Kline1Hour = 3,
    KlineDaily = 4,
    KlineWeekly = 5,
    KlineMonthly = 6,
    Kline1Min = 7,
    Kline1MinAlt = 8,
    KlineDailyAlt = 9,
    KlineQuarterly = 10,
    KlineYearly = 11,
}

impl KlineType {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Kline5Min,
            1 => Self::Kline15Min,
            2 => Self::Kline30Min,
            3 => Self::Kline1Hour,
            4 => Self::KlineDaily,
            5 => Self::KlineWeekly,
            6 => Self::KlineMonthly,
            7 => Self::Kline1Min,
            8 => Self::Kline1MinAlt,
            9 => Self::KlineDailyAlt,
            10 => Self::KlineQuarterly,
            11 => Self::KlineYearly,
            _ => Self::KlineDaily,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn is_minute(self) -> bool {
        matches!(
            self,
            Self::Kline1Min
                | Self::Kline1MinAlt
                | Self::Kline5Min
                | Self::Kline15Min
                | Self::Kline30Min
                | Self::Kline1Hour
        )
    }

    pub fn minutes(self) -> Option<u32> {
        match self {
            Self::Kline1Min | Self::Kline1MinAlt => Some(1),
            Self::Kline5Min => Some(5),
            Self::Kline15Min => Some(15),
            Self::Kline30Min => Some(30),
            Self::Kline1Hour => Some(60),
            _ => None,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Kline1Min | Self::Kline1MinAlt => "1m",
            Self::Kline5Min => "5m",
            Self::Kline15Min => "15m",
            Self::Kline30Min => "30m",
            Self::Kline1Hour => "60m",
            Self::KlineDaily | Self::KlineDailyAlt => "1d",
            Self::KlineWeekly => "1w",
            Self::KlineMonthly => "1mo",
            Self::KlineQuarterly => "1q",
            Self::KlineYearly => "1y",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TaskType {
    Kline,
    Quote,
    SecurityList,
    MinuteTime,
    BlockInfo,
    CompanyInfoCategory,
    CompanyInfoContent,
    FinanceInfo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KlineBar {
    pub code: String,
    pub datetime: String,
    pub timestamp: Option<i64>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub amount: f64,
}

impl KlineBar {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code: impl Into<String>,
        datetime: impl Into<String>,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        amount: f64,
    ) -> Self {
        Self {
            code: code.into(),
            datetime: datetime.into(),
            timestamp: None,
            open,
            high,
            low,
            close,
            volume,
            amount,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteRecord {
    pub market: u8,
    pub code: String,
    pub active1: u16,
    pub price: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub last_close: f64,
    pub volume: f64,
    pub current_volume: f64,
    pub amount: f64,
    pub timestamp: i64,
    pub servertime: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FinanceInfo {
    pub market: u8,
    pub code: String,
    pub fields: Metadata,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum TaskData {
    Klines(Vec<KlineBar>),
    Quotes(Vec<QuoteRecord>),
    Generic(Vec<GenericRow>),
    Text(String),
    Finance(FinanceInfo),
    #[default]
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub task_type: TaskType,
    pub stock_code: String,
    pub klines_count: usize,
    pub kline_type: KlineType,
    pub quote_stocks: Vec<(u8, String)>,
    pub task_id: String,
    pub metadata: Metadata,
    pub market: u8,
    pub block_file: String,
    pub filename: String,
    pub start: u32,
    pub length: u32,
    pub priority: i32,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            task_type: TaskType::Kline,
            stock_code: String::new(),
            klines_count: 0,
            kline_type: KlineType::KlineDaily,
            quote_stocks: Vec::new(),
            task_id: String::new(),
            metadata: Metadata::new(),
            market: 0,
            block_file: String::new(),
            filename: String::new(),
            start: 0,
            length: 0,
            priority: 30,
        }
    }
}

impl Task {
    pub fn kline(code: impl Into<String>, count: usize, kline_type: KlineType) -> Self {
        Self {
            stock_code: code.into(),
            klines_count: count,
            kline_type,
            ..Self::default()
        }
    }

    pub fn quote(stocks: Vec<(u8, String)>) -> Self {
        Self {
            task_type: TaskType::Quote,
            quote_stocks: stocks,
            priority: 35,
            ..Self::default()
        }
    }

    pub fn security_list(market: u8) -> Self {
        Self {
            task_type: TaskType::SecurityList,
            market,
            priority: 10,
            ..Self::default()
        }
    }

    pub fn minute_time(market: u8, code: impl Into<String>) -> Self {
        Self {
            task_type: TaskType::MinuteTime,
            market,
            stock_code: code.into(),
            priority: 10,
            ..Self::default()
        }
    }

    pub fn block_info(file: impl Into<String>) -> Self {
        Self {
            task_type: TaskType::BlockInfo,
            block_file: file.into(),
            priority: 10,
            ..Self::default()
        }
    }

    pub fn company_info_category(market: u8, code: impl Into<String>) -> Self {
        Self {
            task_type: TaskType::CompanyInfoCategory,
            market,
            stock_code: code.into(),
            priority: -5,
            ..Self::default()
        }
    }

    pub fn company_info_content(
        market: u8,
        code: impl Into<String>,
        filename: impl Into<String>,
        start: u32,
        length: u32,
    ) -> Self {
        Self {
            task_type: TaskType::CompanyInfoContent,
            market,
            stock_code: code.into(),
            filename: filename.into(),
            start,
            length,
            priority: -10,
            ..Self::default()
        }
    }

    pub fn finance_info(market: u8, code: impl Into<String>) -> Self {
        Self {
            task_type: TaskType::FinanceInfo,
            market,
            stock_code: code.into(),
            priority: -5,
            ..Self::default()
        }
    }

    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = task_id.into();
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn key(&self) -> String {
        if self.task_id.is_empty() {
            match self.task_type {
                TaskType::Quote => "quote".to_string(),
                TaskType::SecurityList => format!("security_list_{}", self.market),
                TaskType::MinuteTime => format!("minute_{}_{}", self.market, self.stock_code),
                TaskType::BlockInfo => format!("block_{}", self.block_file),
                TaskType::CompanyInfoCategory => {
                    format!("company_category_{}_{}", self.market, self.stock_code)
                }
                TaskType::CompanyInfoContent => format!(
                    "company_content_{}_{}_{}",
                    self.market, self.stock_code, self.filename
                ),
                TaskType::FinanceInfo => format!("finance_{}_{}", self.market, self.stock_code),
                TaskType::Kline => self.stock_code.clone(),
            }
        } else {
            self.task_id.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskResult {
    pub task_type: TaskType,
    pub stock_code: String,
    pub task_id: String,
    pub success: bool,
    pub data: TaskData,
    pub error_message: String,
    pub download_time: Duration,
    pub server_used: String,
    pub metadata: Metadata,
    pub quote_count: usize,
    pub market: u8,
}

impl TaskResult {
    pub fn success(
        task: &Task,
        data: TaskData,
        server_used: impl Into<String>,
        download_time: Duration,
    ) -> Self {
        let quote_count = match &data {
            TaskData::Quotes(values) => values.len(),
            _ => task.quote_stocks.len(),
        };
        Self {
            task_type: task.task_type,
            stock_code: task.stock_code.clone(),
            task_id: task.key(),
            success: true,
            data,
            error_message: String::new(),
            download_time,
            server_used: server_used.into(),
            metadata: task.metadata.clone(),
            quote_count,
            market: task.market,
        }
    }

    pub fn failure(
        task: &Task,
        error_message: impl Into<String>,
        server_used: impl Into<String>,
        download_time: Duration,
    ) -> Self {
        Self {
            task_type: task.task_type,
            stock_code: task.stock_code.clone(),
            task_id: task.key(),
            success: false,
            data: TaskData::None,
            error_message: error_message.into(),
            download_time,
            server_used: server_used.into(),
            metadata: task.metadata.clone(),
            quote_count: task.quote_stocks.len(),
            market: task.market,
        }
    }

    pub fn kline_count(&self) -> usize {
        match &self.data {
            TaskData::Klines(values) => values.len(),
            _ => 0,
        }
    }
}
