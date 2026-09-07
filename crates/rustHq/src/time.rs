#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    pub date: Date,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Date {
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    pub fn parse_iso(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        Some(Self {
            year: parse_digits_i32(&bytes[0..4])?,
            month: parse_digits_u8(&bytes[5..7])?,
            day: parse_digits_u8(&bytes[8..10])?,
        })
    }

    pub fn format_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn add_days(self, delta: i32) -> Self {
        days_to_civil(civil_to_days(self.year, self.month, self.day) + i64::from(delta))
    }

    pub fn weekday_monday_one(self) -> u8 {
        let days = civil_to_days(self.year, self.month, self.day);
        ((days + 3).rem_euclid(7) + 1) as u8
    }

    pub fn quarter(self) -> u8 {
        ((self.month - 1) / 3) + 1
    }
}

impl DateTime {
    pub const fn new(date: Date, hour: u8, minute: u8, second: u8) -> Self {
        Self {
            date,
            hour,
            minute,
            second,
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if bytes.len() != 16 && bytes.len() != 19 {
            return None;
        }
        if bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes[10] != b' '
            || bytes[13] != b':'
            || (bytes.len() == 19 && bytes[16] != b':')
        {
            return None;
        }

        let date = Date {
            year: parse_digits_i32(&bytes[0..4])?,
            month: parse_digits_u8(&bytes[5..7])?,
            day: parse_digits_u8(&bytes[8..10])?,
        };
        let hour = parse_digits_u8(&bytes[11..13])?;
        let minute = parse_digits_u8(&bytes[14..16])?;
        let second = if bytes.len() == 19 {
            parse_digits_u8(&bytes[17..19])?
        } else {
            0
        };

        Some(Self {
            date,
            hour,
            minute,
            second,
        })
    }

    pub fn format(self, include_seconds: bool) -> String {
        if include_seconds {
            format!(
                "{} {:02}:{:02}:{:02}",
                self.date.format_iso(),
                self.hour,
                self.minute,
                self.second
            )
        } else {
            format!(
                "{} {:02}:{:02}",
                self.date.format_iso(),
                self.hour,
                self.minute
            )
        }
    }

    pub fn minute_of_day(self) -> u16 {
        u16::from(self.hour) * 60 + u16::from(self.minute)
    }
}

pub fn civil_to_days(year: i32, month: u8, day: u8) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i32::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i32::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + i64::from(doe) - 719_468
}

pub fn days_to_civil(days: i64) -> Date {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i32::from(month <= 2);
    Date {
        year,
        month: month as u8,
        day: day as u8,
    }
}

fn parse_digits_i32(slice: &[u8]) -> Option<i32> {
    let mut value = 0_i32;
    for byte in slice {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + i32::from(byte - b'0');
    }
    Some(value)
}

fn parse_digits_u8(slice: &[u8]) -> Option<u8> {
    let mut value = 0_u8;
    for byte in slice {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.saturating_mul(10).saturating_add(byte - b'0');
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{Date, DateTime, civil_to_days, days_to_civil};

    #[test]
    fn date_round_trip_is_stable() {
        let original = Date::new(2026, 3, 10);
        let days = civil_to_days(original.year, original.month, original.day);
        let restored = days_to_civil(days);
        assert_eq!(restored, original);
        assert_eq!(original.weekday_monday_one(), 2);
    }

    #[test]
    fn parse_datetime_handles_minutes_and_seconds() {
        let short = DateTime::parse("2026-03-10 09:31").unwrap();
        assert_eq!(short.second, 0);
        let long = DateTime::parse("2026-03-10 09:31:45").unwrap();
        assert_eq!(long.second, 45);
    }
}
