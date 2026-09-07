use crate::models::{KlineBar, KlineType};
use crate::table::DailyFeatureRow;
use crate::time::{Date, DateTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BucketKey {
    date: Date,
    minute_floor: Option<u16>,
}

pub fn resample(rows: &[KlineBar], from: KlineType, to: KlineType) -> Vec<KlineBar> {
    if rows.is_empty() || from == to {
        return rows.to_vec();
    }

    let mut sorted = rows.to_vec();
    sorted.sort_by_key(|row| DateTime::parse(&row.datetime));

    match (from.is_minute(), to.is_minute()) {
        (true, true) => {
            let from_minutes = from.minutes().unwrap_or(1);
            let to_minutes = to.minutes().unwrap_or(1);
            if to_minutes < from_minutes || !to_minutes.is_multiple_of(from_minutes) {
                return rows.to_vec();
            }
            aggregate_rows(&sorted, |row| {
                let dt = DateTime::parse(&row.datetime)?;
                let minute_floor = (u32::from(dt.minute_of_day()) / to_minutes) * to_minutes;
                Some(BucketKey {
                    date: dt.date,
                    minute_floor: Some(minute_floor as u16),
                })
            })
        }
        (true, false) => {
            let daily = aggregate_rows(&sorted, |row| {
                let dt = DateTime::parse(&row.datetime)?;
                Some(BucketKey {
                    date: dt.date,
                    minute_floor: None,
                })
            });
            if matches!(to, KlineType::KlineDaily | KlineType::KlineDailyAlt) {
                daily
            } else {
                resample_daily_up(&daily, to)
            }
        }
        (false, false) if matches!(from, KlineType::KlineDaily | KlineType::KlineDailyAlt) => {
            resample_daily_up(&sorted, to)
        }
        _ => rows.to_vec(),
    }
}

pub fn resample_minute_to_daily_with_features(
    rows: &[KlineBar],
    scale_vol_amt: bool,
    scale_factor: f64,
) -> Vec<DailyFeatureRow> {
    if rows.is_empty() {
        return Vec::new();
    }

    let mut sorted = rows.to_vec();
    sorted.sort_by_key(|row| DateTime::parse(&row.datetime));

    let mut output = Vec::new();
    let mut bucket = Vec::new();
    let mut current_date = None;

    for row in sorted {
        let dt = match DateTime::parse(&row.datetime) {
            Some(value) => value,
            None => continue,
        };
        if current_date.is_some() && current_date != Some(dt.date) {
            if let Some(item) = finish_daily_bucket(&bucket, scale_vol_amt, scale_factor) {
                output.push(item);
            }
            bucket.clear();
        }
        current_date = Some(dt.date);
        bucket.push(row);
    }

    if let Some(item) = finish_daily_bucket(&bucket, scale_vol_amt, scale_factor) {
        output.push(item);
    }

    output
}

pub fn compute_daily_indicators(rows: &mut [DailyFeatureRow]) {
    rows.sort_by(|left, right| left.date.cmp(&right.date));
    let mut previous_close = None;

    for row in rows.iter_mut() {
        row.huitoubo = if row.close != 0.0 {
            Some(round2((row.high / row.close - 1.0) * 100.0))
        } else {
            None
        };

        row.chonggao = match (row.tenmax, previous_close) {
            (Some(tenmax), Some(prev_close)) if prev_close != 0.0 => {
                Some(round2((tenmax / prev_close - 1.0) * 100.0))
            }
            _ => None,
        };

        previous_close = Some(row.close);
    }
}

fn resample_daily_up(rows: &[KlineBar], to: KlineType) -> Vec<KlineBar> {
    aggregate_rows(rows, |row| {
        let dt = DateTime::parse(&row.datetime)?;
        let date = dt.date;
        let bucket_date = match to {
            KlineType::KlineWeekly => date.add_days(-i32::from(date.weekday_monday_one()) + 1),
            KlineType::KlineMonthly => Date::new(date.year, date.month, 1),
            KlineType::KlineQuarterly => Date::new(date.year, (date.quarter() - 1) * 3 + 1, 1),
            KlineType::KlineYearly => Date::new(date.year, 1, 1),
            _ => date,
        };
        Some(BucketKey {
            date: bucket_date,
            minute_floor: None,
        })
    })
}

fn aggregate_rows<F>(rows: &[KlineBar], mut key_fn: F) -> Vec<KlineBar>
where
    F: FnMut(&KlineBar) -> Option<BucketKey>,
{
    let mut output = Vec::new();
    let mut bucket_rows = Vec::new();
    let mut previous_key = None;

    for row in rows {
        let key = match key_fn(row) {
            Some(value) => value,
            None => continue,
        };

        if previous_key.is_some() && previous_key != Some(key) {
            if let Some(aggregated) = finish_kline_bucket(&bucket_rows) {
                output.push(aggregated);
            }
            bucket_rows.clear();
        }

        previous_key = Some(key);
        bucket_rows.push(row.clone());
    }

    if let Some(aggregated) = finish_kline_bucket(&bucket_rows) {
        output.push(aggregated);
    }

    output
}

fn finish_kline_bucket(rows: &[KlineBar]) -> Option<KlineBar> {
    let first = rows.first()?;
    let last = rows.last()?;
    let mut high = f64::NEG_INFINITY;
    let mut low = f64::INFINITY;
    let mut volume = 0.0;
    let mut amount = 0.0;

    for row in rows {
        high = high.max(row.high);
        low = low.min(row.low);
        volume += row.volume;
        amount += row.amount;
    }

    Some(KlineBar {
        code: first.code.clone(),
        datetime: last.datetime.clone(),
        timestamp: last.timestamp,
        open: first.open,
        high,
        low,
        close: last.close,
        volume,
        amount,
    })
}

fn finish_daily_bucket(
    rows: &[KlineBar],
    scale_vol_amt: bool,
    scale_factor: f64,
) -> Option<DailyFeatureRow> {
    let first = rows.first()?;
    let last = rows.last()?;
    let mut high = f64::NEG_INFINITY;
    let mut low = f64::INFINITY;
    let mut volume = 0.0;
    let mut amount = 0.0;
    let mut tenmax: Option<f64> = None;
    let mut notfull = 0_u8;

    for row in rows {
        high = high.max(row.high);
        low = low.min(row.low);
        volume += row.volume;
        amount += row.amount;
        if let Some(dt) = DateTime::parse(&row.datetime) {
            notfull = notfull.max(dt.hour);
            let is_am_window = dt.hour < 10 || (dt.hour == 10 && dt.minute == 0);
            if is_am_window {
                tenmax = Some(match tenmax {
                    Some(current) => current.max(row.high),
                    None => row.high,
                });
            }
        }
    }

    if scale_vol_amt {
        volume = (volume * scale_factor).round();
        amount = (amount * scale_factor).round();
    }

    let date = DateTime::parse(&last.datetime)?.date.format_iso();
    Some(DailyFeatureRow {
        date,
        open: first.open,
        high,
        low,
        close: last.close,
        volume,
        amount,
        tenmax,
        notfull,
        chonggao: None,
        huitoubo: None,
    })
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::{compute_daily_indicators, resample, resample_minute_to_daily_with_features};
    use crate::models::{KlineBar, KlineType};

    fn bar(datetime: &str, open: f64, high: f64, low: f64, close: f64) -> KlineBar {
        KlineBar::new("000001", datetime, open, high, low, close, 100.0, 1000.0)
    }

    #[test]
    fn minute_resample_builds_5m_bucket() {
        let rows = vec![
            bar("2026-03-10 09:30:00", 10.0, 10.1, 9.9, 10.0),
            bar("2026-03-10 09:31:00", 10.0, 10.2, 9.9, 10.1),
            bar("2026-03-10 09:32:00", 10.1, 10.3, 10.0, 10.2),
            bar("2026-03-10 09:33:00", 10.2, 10.4, 10.1, 10.3),
            bar("2026-03-10 09:34:00", 10.3, 10.5, 10.2, 10.4),
        ];
        let result = resample(&rows, KlineType::Kline1Min, KlineType::Kline5Min);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].open, 10.0);
        assert_eq!(result[0].close, 10.4);
        assert_eq!(result[0].high, 10.5);
    }

    #[test]
    fn daily_features_compute_chonggao_and_huitoubo() {
        let rows = vec![
            bar("2026-03-10 09:30:00", 10.0, 10.1, 9.9, 10.0),
            bar("2026-03-10 10:00:00", 10.0, 10.4, 9.9, 10.2),
            bar("2026-03-10 14:59:00", 10.2, 10.5, 10.1, 10.3),
            bar("2026-03-11 09:30:00", 10.3, 10.8, 10.2, 10.6),
            bar("2026-03-11 10:00:00", 10.6, 10.9, 10.5, 10.7),
            bar("2026-03-11 14:59:00", 10.7, 10.8, 10.3, 10.4),
        ];
        let mut daily = resample_minute_to_daily_with_features(&rows, false, 1.0);
        compute_daily_indicators(&mut daily);
        assert_eq!(daily.len(), 2);
        assert!(daily[0].huitoubo.is_some());
        assert!(daily[1].chonggao.is_some());
    }
}
