use crate::table::{DayBar, IndexBar, IndexTable, KlineTable};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub fn filter_day_bars_by_code(rows: &[DayBar], stock_code: &str) -> Vec<DayBar> {
    rows.iter()
        .filter(|row| row.code == stock_code)
        .cloned()
        .collect()
}

pub fn unique_dates(rows: &[DayBar]) -> Vec<String> {
    rows.iter()
        .map(|row| row.date.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn calculate_equal_weight_index(
    stock_codes: &[String],
    day_rows: &[DayBar],
    index_code: &str,
    base_value: f64,
) -> IndexTable {
    let dates = unique_dates(day_rows);
    let code_set = stock_codes.iter().cloned().collect::<BTreeSet<_>>();
    let mut by_code_date = HashMap::<(String, String), DayBar>::new();
    for row in day_rows {
        if code_set.contains(&row.code) {
            by_code_date.insert((row.code.clone(), row.date.clone()), row.clone());
        }
    }

    let mut rows = Vec::new();
    let mut previous_index_close = base_value;

    for (index, date) in dates.iter().enumerate() {
        let mut opens = Vec::new();
        let mut highs = Vec::new();
        let mut lows = Vec::new();
        let mut closes = Vec::new();
        let mut prev_closes = Vec::new();
        let mut volumes = Vec::new();

        for code in stock_codes {
            let Some(today) = by_code_date.get(&(code.clone(), date.clone())) else {
                continue;
            };
            closes.push(today.close);
            opens.push(today.open);
            highs.push(today.high);
            lows.push(today.low);
            volumes.push(today.volume);

            if index > 0 {
                let prev_date = &dates[index - 1];
                let prev_close = by_code_date
                    .get(&(code.clone(), prev_date.clone()))
                    .map(|row| row.close)
                    .unwrap_or(today.close);
                prev_closes.push(prev_close);
            }
        }

        if closes.is_empty() {
            continue;
        }

        let avg_change = if index == 0 {
            0.0
        } else {
            average_ratio(&closes, &prev_closes)
        };

        let close = if index == 0 {
            base_value
        } else {
            previous_index_close * (1.0 + avg_change)
        };

        let open = if index == 0 {
            close
        } else {
            previous_index_close * (1.0 + average_ratio(&opens, &prev_closes))
        };
        let high = if index == 0 {
            close
        } else {
            previous_index_close * (1.0 + average_ratio(&highs, &prev_closes))
        };
        let low = if index == 0 {
            close
        } else {
            previous_index_close * (1.0 + average_ratio(&lows, &prev_closes))
        };
        let volume = volumes.iter().sum();

        rows.push(IndexBar {
            code: index_code.to_string(),
            date: date.clone(),
            open,
            high: open.max(high).max(close),
            low: open.min(low).min(close),
            close,
            volume,
        });

        previous_index_close = close;
    }

    IndexTable {
        code: index_code.to_string(),
        rows,
    }
}

pub fn calculate_equal_weight_index_min1<F>(
    stock_codes: &[String],
    mut provider: F,
    index_code: &str,
    base_value: f64,
) -> KlineTable
where
    F: FnMut(&str) -> Option<KlineTable>,
{
    let mut bars_by_datetime = BTreeMap::<String, Vec<crate::models::KlineBar>>::new();
    for code in stock_codes {
        if let Some(table) = provider(code) {
            for row in table.rows {
                bars_by_datetime
                    .entry(row.datetime.clone())
                    .or_default()
                    .push(row);
            }
        }
    }

    let mut rows = Vec::new();
    let mut previous_close = base_value;

    for (index, (datetime, bucket)) in bars_by_datetime.into_iter().enumerate() {
        let avg_open = bucket.iter().map(|row| row.open).sum::<f64>() / bucket.len() as f64;
        let avg_high = bucket.iter().map(|row| row.high).sum::<f64>() / bucket.len() as f64;
        let avg_low = bucket.iter().map(|row| row.low).sum::<f64>() / bucket.len() as f64;
        let avg_close = bucket.iter().map(|row| row.close).sum::<f64>() / bucket.len() as f64;
        let volume = bucket.iter().map(|row| row.volume).sum::<f64>();

        let close = if index == 0 {
            base_value
        } else {
            previous_close * (avg_close / avg_open.max(0.0001))
        };
        let open = if index == 0 {
            close
        } else {
            previous_close * (avg_open / avg_open.max(0.0001))
        };
        let high = close.max(open) * (avg_high / avg_close.max(0.0001));
        let low = close.min(open) * (avg_low / avg_close.max(0.0001));
        let amount = close * volume;

        rows.push(crate::models::KlineBar::new(
            index_code.to_string(),
            datetime,
            open,
            high,
            low,
            close,
            volume,
            amount,
        ));

        previous_close = close;
    }

    KlineTable::new(index_code.to_string(), rows)
}

fn average_ratio(values: &[f64], bases: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut count = 0;
    for (value, base) in values.iter().zip(bases.iter()) {
        if *base > 0.0 {
            sum += (value - base) / base;
            count += 1;
        }
    }
    if count == 0 { 0.0 } else { sum / count as f64 }
}

#[cfg(test)]
mod tests {
    use super::calculate_equal_weight_index;
    use crate::table::DayBar;

    #[test]
    fn equal_weight_index_tracks_average_returns() {
        let codes = vec!["000001".to_string(), "600000".to_string()];
        let rows = vec![
            DayBar {
                code: "000001".to_string(),
                date: "2026-03-09".to_string(),
                open: 10.0,
                high: 10.2,
                low: 9.8,
                close: 10.0,
                volume: 10.0,
            },
            DayBar {
                code: "600000".to_string(),
                date: "2026-03-09".to_string(),
                open: 20.0,
                high: 20.1,
                low: 19.9,
                close: 20.0,
                volume: 20.0,
            },
            DayBar {
                code: "000001".to_string(),
                date: "2026-03-10".to_string(),
                open: 10.5,
                high: 10.7,
                low: 10.4,
                close: 10.6,
                volume: 11.0,
            },
            DayBar {
                code: "600000".to_string(),
                date: "2026-03-10".to_string(),
                open: 20.5,
                high: 20.6,
                low: 20.4,
                close: 20.4,
                volume: 21.0,
            },
        ];
        let index = calculate_equal_weight_index(&codes, &rows, "ZS999", 1000.0);
        assert_eq!(index.rows.len(), 2);
        assert!(index.rows[1].close > 1000.0);
    }
}
