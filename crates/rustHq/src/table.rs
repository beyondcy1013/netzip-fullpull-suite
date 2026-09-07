use crate::models::KlineBar;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KlineTable {
    pub code: String,
    pub name: Option<String>,
    pub rows: Vec<KlineBar>,
}

impl KlineTable {
    pub fn new(code: impl Into<String>, rows: Vec<KlineBar>) -> Self {
        Self {
            code: code.into(),
            name: None,
            rows,
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DailyFeatureRow {
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub amount: f64,
    pub tenmax: Option<f64>,
    pub notfull: u8,
    pub chonggao: Option<f64>,
    pub huitoubo: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DailyFeatureTable {
    pub code: String,
    pub name: Option<String>,
    pub rows: Vec<DailyFeatureRow>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DayBar {
    pub code: String,
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndexBar {
    pub code: String,
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndexTable {
    pub code: String,
    pub rows: Vec<IndexBar>,
}
