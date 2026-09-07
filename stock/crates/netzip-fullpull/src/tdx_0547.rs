#[derive(Clone, Debug, PartialEq)]
pub struct Tdx0547Body {
    pub xor93_count: Option<u16>,
    pub printable_ratio: f64,
    pub records: Vec<Tdx0547Record>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tdx0547QuoteHead {
    pub active1: u16,
    pub price: f64,
    pub last_close: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tdx0547Record {
    pub start: usize,
    pub len: usize,
    pub market: u8,
    pub code: String,
    pub active1_raw: Option<u16>,
    pub time_hhmmss_raw: Option<u32>,
    pub renewal_token_raw: Option<u32>,
    pub extra0_raw: Option<i32>,
    pub extra0_time_hhmmss: Option<String>,
    pub extra1_raw: Option<i32>,
    pub extra2_raw: Option<i32>,
    pub extra3_raw: Option<i32>,
    pub volume: Option<f64>,
    pub current_volume: Option<f64>,
    pub amount: Option<f64>,
    pub amount_raw: Option<u32>,
    pub quote_head: Option<Tdx0547QuoteHead>,
}

impl Tdx0547Body {
    pub fn query_records<'a>(&'a self, query: &str) -> Vec<&'a Tdx0547Record> {
        query_tdx_0547_records(self, query)
    }
}

pub fn xor93_decode(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|byte| byte ^ 0x93).collect()
}

pub fn parse_tdx_0547_body(bytes: &[u8]) -> Tdx0547Body {
    let decoded = xor93_decode(bytes);
    let xor93_count = decoded
        .get(..2)
        .map(|prefix| u16::from_le_bytes([prefix[0], prefix[1]]));
    let printable_ratio = if decoded.is_empty() {
        0.0
    } else {
        let printable = decoded
            .iter()
            .filter(|byte| matches!(byte, 0x20..=0x7e))
            .count();
        printable as f64 / decoded.len() as f64
    };

    let starts = find_record_starts(&decoded);
    let mut records = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().copied().enumerate() {
        let next_start = starts.get(index + 1).copied().unwrap_or(decoded.len());
        let record_bytes = &decoded[start..next_start];
        let market = decoded[start];
        let code = String::from_utf8_lossy(&decoded[start + 1..start + 7]).into_owned();
        let active1_raw = decoded
            .get(start + 7..start + 9)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u16::from_le_bytes);
        let time_hhmmss_raw_at_fixed_offset = decoded
            .get(start + 15..start + 19)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .filter(|raw| is_valid_hhmmss_raw(*raw));
        let fields = parse_quote_fields_from_record_bytes(record_bytes);
        let time_hhmmss_raw = fields
            .as_ref()
            .and_then(|fields| fields.time_hhmmss_raw)
            .or(time_hhmmss_raw_at_fixed_offset);
        let extra0_raw = fields.as_ref().map(|fields| fields.extras[0]);
        let renewal_token_raw = decoded
            .get(start + 0x21..start + 0x25)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes);
        records.push(Tdx0547Record {
            start,
            len: next_start.saturating_sub(start),
            market,
            code,
            active1_raw,
            time_hhmmss_raw,
            renewal_token_raw,
            extra0_time_hhmmss: extra0_raw.and_then(format_extra0_time_hint),
            extra0_raw,
            extra1_raw: fields.as_ref().map(|fields| fields.extras[1]),
            extra2_raw: fields.as_ref().map(|fields| fields.extras[2]),
            extra3_raw: fields.as_ref().map(|fields| fields.extras[3]),
            volume: fields.as_ref().and_then(|fields| fields.volume()),
            current_volume: fields.as_ref().and_then(|fields| fields.current_volume()),
            amount: fields.as_ref().and_then(|fields| fields.amount()),
            amount_raw: fields.as_ref().and_then(|fields| fields.amount_raw),
            quote_head: fields.and_then(|fields| fields.quote_head()),
        });
    }

    Tdx0547Body {
        xor93_count,
        printable_ratio,
        records,
    }
}

pub fn market_name(market: u8) -> Option<&'static str> {
    match market {
        0 => Some("SZ"),
        1 => Some("SH"),
        2 => Some("BJ"),
        _ => None,
    }
}

pub fn record_symbol(record: &Tdx0547Record) -> Option<String> {
    market_name(record.market).map(|prefix| format!("{prefix}{}", record.code))
}

pub fn is_valid_hhmmss_raw(raw: u32) -> bool {
    if raw > 235_959 {
        return false;
    }
    let second = raw % 100;
    let minute = (raw / 100) % 100;
    let hour = raw / 10_000;
    hour < 24 && minute < 60 && second < 60
}

pub fn format_hhmmss_raw(raw: u32) -> Option<String> {
    if !is_valid_hhmmss_raw(raw) {
        return None;
    }
    let second = raw % 100;
    let minute = (raw / 100) % 100;
    let hour = raw / 10_000;
    Some(format!("{hour:02}:{minute:02}:{second:02}"))
}

pub fn format_public_hhmmss_raw(raw: u32) -> Option<String> {
    if !is_valid_hhmmss_raw(raw) {
        return None;
    }
    format_hhmmss_raw(raw.min(150_000))
}

pub fn public_time_hhmmss(record: &Tdx0547Record) -> Option<String> {
    record
        .time_hhmmss_raw
        .and_then(format_public_hhmmss_raw)
        .or_else(|| {
            record
                .extra0_raw
                .and_then(extra0_time_hint_seconds)
                .and_then(format_public_seconds)
        })
}

fn format_public_seconds(seconds: u32) -> Option<String> {
    if seconds >= 24 * 60 * 60 {
        return None;
    }
    let seconds = seconds.min(15 * 60 * 60);
    let hour = seconds / 3600;
    let minute = (seconds % 3600) / 60;
    let second = seconds % 60;
    Some(format!("{hour:02}:{minute:02}:{second:02}"))
}

pub fn hhmmss_raw_to_seconds(raw: u32) -> Option<u32> {
    if !is_valid_hhmmss_raw(raw) {
        return None;
    }
    let second = raw % 100;
    let minute = (raw / 100) % 100;
    let hour = raw / 10_000;
    Some(hour * 3600 + minute * 60 + second)
}

pub fn format_extra0_time_hint(raw: i32) -> Option<String> {
    if (-4_779..=-4_720).contains(&raw) {
        let second = -4_720 - raw;
        return Some(format!("15:00:{second:02}"));
    }

    if (5_480..=5_539).contains(&raw) {
        let second = raw - 5_480;
        return Some(format!("15:30:{second:02}"));
    }

    None
}

pub fn extra0_time_hint_seconds(raw: i32) -> Option<u32> {
    if (-4_779..=-4_720).contains(&raw) {
        let second = (-4_720 - raw) as u32;
        return Some(15 * 3600 + second);
    }

    if (5_480..=5_539).contains(&raw) {
        let second = (raw - 5_480) as u32;
        return Some(15 * 3600 + 30 * 60 + second);
    }

    None
}

pub fn normalize_quote_head_by_decimal_point(
    quote_head: &Tdx0547QuoteHead,
    decimal_point: u8,
) -> Option<Tdx0547QuoteHead> {
    if !(2..=6).contains(&decimal_point) {
        return None;
    }

    let scale = 10_f64.powi(i32::from(decimal_point) - 2);
    Some(Tdx0547QuoteHead {
        active1: quote_head.active1,
        price: quote_head.price / scale,
        last_close: quote_head.last_close / scale,
        open: quote_head.open / scale,
        high: quote_head.high / scale,
        low: quote_head.low / scale,
    })
}

pub fn matches_tdx_0547_query(record: &Tdx0547Record, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }

    let query_lower = query.to_ascii_lowercase();
    if record.code.contains(&query_lower) {
        return true;
    }

    record_symbol(record)
        .map(|symbol| symbol.to_ascii_lowercase().contains(&query_lower))
        .unwrap_or(false)
}

pub fn query_tdx_0547_records<'a>(body: &'a Tdx0547Body, query: &str) -> Vec<&'a Tdx0547Record> {
    body.records
        .iter()
        .filter(|record| matches_tdx_0547_query(record, query))
        .collect()
}

struct ParsedQuoteFields {
    active1: u16,
    price_raw: i32,
    last_close_diff: i32,
    open_diff: i32,
    high_diff: i32,
    low_diff: i32,
    time_hhmmss_raw: Option<u32>,
    extras: [i32; 4],
    volume_raw: Option<i32>,
    current_volume_raw: Option<i32>,
    amount_raw: Option<u32>,
}

impl ParsedQuoteFields {
    fn quote_head(&self) -> Option<Tdx0547QuoteHead> {
        let price_raw = i64::from(self.price_raw);
        let quote_head = Tdx0547QuoteHead {
            active1: self.active1,
            price: price_raw as f64 / 100.0,
            last_close: (price_raw + i64::from(self.last_close_diff)) as f64 / 100.0,
            open: (price_raw + i64::from(self.open_diff)) as f64 / 100.0,
            high: (price_raw + i64::from(self.high_diff)) as f64 / 100.0,
            low: (price_raw + i64::from(self.low_diff)) as f64 / 100.0,
        };
        is_consistent_quote_head(&quote_head).then_some(quote_head)
    }

    fn volume(&self) -> Option<f64> {
        self.volume_raw.filter(|value| *value >= 0).map(|value| {
            // Positive 0547 cumulative volume is one below OEM_REPORT's public total.
            f64::from(value) + if value > 0 { 1.0 } else { 0.0 }
        })
    }

    fn current_volume(&self) -> Option<f64> {
        self.current_volume_raw
            .filter(|value| *value >= 0)
            .map(f64::from)
    }

    fn amount(&self) -> Option<f64> {
        self.amount_raw.map(parse_tdx_packed_number)
    }
}

fn parse_quote_fields_from_record_bytes(bytes: &[u8]) -> Option<ParsedQuoteFields> {
    if bytes.len() < 9 {
        return None;
    }

    let active1 = u16::from_le_bytes(bytes[7..9].try_into().ok()?);
    let mut pos = 9usize;
    let price_raw = parse_varint_price(bytes, &mut pos)?;
    let last_close_diff = parse_varint_price(bytes, &mut pos)?;
    let open_diff = parse_varint_price(bytes, &mut pos)?;
    let high_diff = parse_varint_price(bytes, &mut pos)?;
    let low_diff = parse_varint_price(bytes, &mut pos)?;

    // This 0547 variant stores HHMMSS as a fixed u32. Treating those bytes as
    // varints makes the following volume cursor depend on the encoded time.
    let time_hhmmss_raw = bytes
        .get(pos..pos + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .filter(|raw| is_valid_hhmmss_raw(*raw));
    let mut extras_pos = pos;
    let extras = [
        parse_varint_price(bytes, &mut extras_pos)?,
        parse_varint_price(bytes, &mut extras_pos)?,
        parse_varint_price(bytes, &mut extras_pos)?,
        parse_varint_price(bytes, &mut extras_pos)?,
    ];
    pos += 4;
    let _reserved_before_volume = parse_varint_price(bytes, &mut pos)?;
    let first_volume_field = parse_varint_price(bytes, &mut pos);
    let second_volume_field = first_volume_field.and_then(|_| parse_varint_price(bytes, &mut pos));
    let (volume_raw, current_volume_raw) =
        if first_volume_field == Some(0) && second_volume_field.is_some_and(|value| value > 0) {
            (second_volume_field, parse_varint_price(bytes, &mut pos))
        } else {
            (first_volume_field, second_volume_field)
        };
    let amount_raw = current_volume_raw.and_then(|_| {
        Some(u32::from_le_bytes(
            bytes.get(pos..pos + 4)?.try_into().ok()?,
        ))
    });

    Some(ParsedQuoteFields {
        active1,
        price_raw,
        last_close_diff,
        open_diff,
        high_diff,
        low_diff,
        time_hhmmss_raw,
        extras,
        volume_raw,
        current_volume_raw,
        amount_raw,
    })
}

pub fn parse_tdx_packed_number(raw: u32) -> f64 {
    dllhqarrow_rs::parse_tdx_packed_number(raw)
}

fn parse_varint_price(bytes: &[u8], pos: &mut usize) -> Option<i32> {
    if *pos >= bytes.len() {
        return None;
    }

    let mut shift = 6;
    let mut byte = bytes[*pos];
    let mut value = i32::from(byte & 0x3f);
    let negative = byte & 0x40 != 0;

    if byte & 0x80 != 0 {
        loop {
            *pos += 1;
            if *pos >= bytes.len() {
                return None;
            }
            if shift >= 31 {
                return None;
            }
            byte = bytes[*pos];
            value += i32::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
    }

    *pos += 1;
    Some(if negative { -value } else { value })
}

fn is_consistent_quote_head(quote_head: &Tdx0547QuoteHead) -> bool {
    let all_positive_and_bounded = [
        quote_head.price,
        quote_head.last_close,
        quote_head.open,
        quote_head.high,
        quote_head.low,
    ]
    .into_iter()
    .all(|value| value.is_finite() && value > 0.0 && value < 100_000.0);

    all_positive_and_bounded
        && quote_head.low <= quote_head.price
        && quote_head.low <= quote_head.open
        && quote_head.high >= quote_head.price
        && quote_head.high >= quote_head.open
}

fn find_record_starts(bytes: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut index = 1usize;
    while index + 6 < bytes.len() {
        let market = bytes[index - 1];
        let code = &bytes[index..index + 6];
        if market <= 2 && code.iter().all(|byte| byte.is_ascii_digit()) {
            out.push(index - 1);
            index += 6;
        } else {
            index += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        Tdx0547QuoteHead, extra0_time_hint_seconds, format_extra0_time_hint, format_hhmmss_raw,
        format_public_hhmmss_raw, hhmmss_raw_to_seconds, market_name,
        normalize_quote_head_by_decimal_point, parse_tdx_0547_body, query_tdx_0547_records,
        record_symbol, xor93_decode,
    };

    #[test]
    fn xor93_roundtrip_simple_bytes() {
        let raw = [0x00_u8, 0x93, 0xff];
        let decoded = xor93_decode(&raw);
        assert_eq!(decoded, vec![0x93, 0x00, 0x6c]);
    }

    #[test]
    fn parse_tdx_0547_body_detects_count_and_records() {
        let decoded = [
            0x02, 0x00, 0x01, b'6', b'0', b'0', b'0', b'0', b'0', 0xaa, 0xbb, 0x00, b'0', b'0',
            b'0', b'0', b'0', b'1', 0xcc,
        ];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);
        assert_eq!(parsed.xor93_count, Some(2));
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].market, 1);
        assert_eq!(parsed.records[0].code, "600000");
        assert_eq!(parsed.records[0].start, 2);
        assert_eq!(parsed.records[0].len, 9);
        assert_eq!(parsed.records[0].active1_raw, Some(48_042));
        assert_eq!(parsed.records[0].time_hhmmss_raw, None);
        assert_eq!(parsed.records[0].extra0_raw, None);
        assert_eq!(parsed.records[0].extra0_time_hhmmss, None);
        assert_eq!(parsed.records[0].extra1_raw, None);
        assert_eq!(parsed.records[0].extra2_raw, None);
        assert_eq!(parsed.records[0].extra3_raw, None);
        assert_eq!(parsed.records[0].quote_head, None);
        assert_eq!(parsed.records[1].market, 0);
        assert_eq!(parsed.records[1].code, "000001");
        assert_eq!(parsed.records[1].start, 11);
        assert_eq!(parsed.records[1].len, 8);
        assert_eq!(parsed.records[1].active1_raw, None);
        assert_eq!(parsed.records[1].time_hhmmss_raw, None);
        assert_eq!(parsed.records[1].extra0_raw, None);
        assert_eq!(parsed.records[1].extra0_time_hhmmss, None);
        assert_eq!(parsed.records[1].extra1_raw, None);
        assert_eq!(parsed.records[1].extra2_raw, None);
        assert_eq!(parsed.records[1].extra3_raw, None);
        assert_eq!(parsed.records[1].quote_head, None);
    }

    #[test]
    fn market_name_maps_known_markets() {
        assert_eq!(market_name(0), Some("SZ"));
        assert_eq!(market_name(1), Some("SH"));
        assert_eq!(market_name(2), Some("BJ"));
        assert_eq!(market_name(9), None);
    }

    #[test]
    fn query_records_matches_code_and_symbol_case_insensitively() {
        let decoded = [
            0x02, 0x00, 0x01, b'6', b'0', b'0', b'0', b'0', b'0', 0xaa, 0xbb, 0x00, b'0', b'0',
            b'0', b'0', b'0', b'1', 0xcc,
        ];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);

        let by_code = query_tdx_0547_records(&parsed, "600000");
        assert_eq!(by_code.len(), 1);
        assert_eq!(by_code[0].code, "600000");

        let by_symbol = query_tdx_0547_records(&parsed, "sh600000");
        assert_eq!(by_symbol.len(), 1);
        assert_eq!(record_symbol(by_symbol[0]).as_deref(), Some("SH600000"));

        let by_suffix = query_tdx_0547_records(&parsed, "000001");
        assert_eq!(by_suffix.len(), 1);
        assert_eq!(by_suffix[0].code, "000001");
    }

    #[test]
    fn parser_extracts_valid_hhmmss_only() {
        let mut decoded = vec![0x01, b'6', b'0', b'0', b'0', b'0', b'0'];
        decoded.extend_from_slice(&[0xaa; 8]);
        decoded.extend_from_slice(&150_003_u32.to_le_bytes());
        decoded.extend_from_slice(&[0xbb; 6]);
        decoded.push(0x00);
        decoded.extend_from_slice(b"000001");
        decoded.extend_from_slice(&[0xcc; 8]);
        decoded.extend_from_slice(&40_266_572_u32.to_le_bytes());
        decoded.extend_from_slice(&[0xdd; 4]);

        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].active1_raw, Some(43_690));
        assert_eq!(parsed.records[0].time_hhmmss_raw, Some(150_003));
        assert_eq!(parsed.records[0].extra0_raw, None);
        assert_eq!(parsed.records[0].extra0_time_hhmmss, None);
        assert_eq!(parsed.records[0].extra1_raw, None);
        assert_eq!(parsed.records[0].extra2_raw, None);
        assert_eq!(parsed.records[0].extra3_raw, None);
        assert_eq!(
            format_hhmmss_raw(parsed.records[0].time_hhmmss_raw.unwrap()).as_deref(),
            Some("15:00:03")
        );
        assert_eq!(parsed.records[1].active1_raw, Some(52_428));
        assert_eq!(parsed.records[1].time_hhmmss_raw, None);
        assert_eq!(parsed.records[1].extra0_raw, None);
        assert_eq!(parsed.records[1].extra0_time_hhmmss, None);
        assert_eq!(parsed.records[1].extra1_raw, None);
        assert_eq!(parsed.records[1].extra2_raw, None);
        assert_eq!(parsed.records[1].extra3_raw, None);
    }

    #[test]
    fn parser_extracts_vendor_renewal_token_at_record_offset_0x21() {
        let mut decoded = vec![0x01, 0x00, 0x01, b'6', b'0', b'0', b'0', b'0', b'0'];
        decoded.resize(2 + 0x21, 0);
        decoded.extend_from_slice(&0x1234_5678_u32.to_le_bytes());

        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);

        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].renewal_token_raw, Some(0x1234_5678));
    }

    #[test]
    fn parser_extracts_consistent_quote_head_from_sample_like_record() {
        let decoded = [
            0x01, b'6', b'0', b'0', b'0', b'0', b'0', 0x25, 0x0f, 0xaa, 0x0f, 0x04, 0x41, 0x0a,
            0x44, 0xf3, 0x49, 0x02, 0x00, 0x00, 0xb7, 0xf3, 0x2f, 0x87, 0x78, 0x74, 0x9c, 0xbb,
            0x4d,
        ];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(
            parsed.records[0].quote_head,
            Some(Tdx0547QuoteHead {
                active1: 3877,
                price: 10.02,
                last_close: 10.06,
                open: 10.01,
                high: 10.12,
                low: 9.98,
            })
        );
        assert_eq!(parsed.records[0].extra0_raw, Some(-4_723));
        assert_eq!(
            parsed.records[0].extra0_time_hhmmss.as_deref(),
            Some("15:00:03")
        );
        assert_eq!(parsed.records[0].extra1_raw, Some(2));
        assert_eq!(parsed.records[0].extra2_raw, Some(0));
        assert_eq!(parsed.records[0].extra3_raw, Some(0));
        assert_eq!(parsed.records[0].volume, Some(392_440.0));
        assert_eq!(parsed.records[0].current_volume, Some(7_687.0));
        assert_eq!(parsed.records[0].amount, Some(393_449_088.0));
    }

    #[test]
    fn parser_skips_verified_zero_marker_before_volume_fields() {
        let decoded = [
            0x01, b'6', b'0', b'0', b'0', b'0', b'0', 0x25, 0x0f, 0xaa, 0x0f, 0x04, 0x41, 0x0a,
            0x44, 0xf3, 0x49, 0x02, 0x00, 0x00, 0x00, 0xb7, 0xf3, 0x2f, 0x87, 0x78, 0x74, 0x9c,
            0xbb, 0x4d,
        ];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);

        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].volume, Some(392_440.0));
        assert_eq!(parsed.records[0].current_volume, Some(7_687.0));
        assert_eq!(parsed.records[0].amount, Some(393_449_088.0));
    }

    #[test]
    fn parser_keeps_volume_aligned_when_fixed_time_bytes_look_like_varints() {
        let decoded = [
            0x01, b'6', b'0', b'0', b'0', b'0', b'0', 0x25, 0x0f, 0xaa, 0x0f, 0x04, 0x41, 0x0a,
            0x44, 0xa0, 0x86, 0x01, 0x00, 0x00, 0xb7, 0xf3, 0x2f, 0x87, 0x78, 0x74, 0x9c, 0xbb,
            0x4d,
        ];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);

        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].time_hhmmss_raw, Some(100_000));
        assert_eq!(parsed.records[0].volume, Some(392_440.0));
        assert_eq!(parsed.records[0].current_volume, Some(7_687.0));
        assert_eq!(parsed.records[0].amount, Some(393_449_088.0));
    }

    #[test]
    fn parser_rejects_inconsistent_quote_head() {
        let decoded = [
            0x01, b'6', b'0', b'0', b'0', b'0', b'0', 0x25, 0x0f, 0xaa, 0x0f, 0x04, 0x41, 0x00,
            0x00, 0xf3, 0x49, 0x02, 0x00, 0x00,
        ];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let parsed = parse_tdx_0547_body(&encoded);
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].quote_head, None);
    }

    #[test]
    fn normalize_quote_head_by_decimal_point_scales_by_decimal_point() {
        let quote_head = Tdx0547QuoteHead {
            active1: 3877,
            price: 10.02,
            last_close: 10.06,
            open: 10.01,
            high: 10.12,
            low: 9.98,
        };

        let normalized_dp2 = normalize_quote_head_by_decimal_point(&quote_head, 2).unwrap();
        assert_eq!(normalized_dp2.active1, quote_head.active1);
        assert!((normalized_dp2.price - 10.02).abs() < 1e-9);
        assert!((normalized_dp2.last_close - 10.06).abs() < 1e-9);
        assert!((normalized_dp2.open - 10.01).abs() < 1e-9);
        assert!((normalized_dp2.high - 10.12).abs() < 1e-9);
        assert!((normalized_dp2.low - 9.98).abs() < 1e-9);

        let normalized_dp4 = normalize_quote_head_by_decimal_point(&quote_head, 4).unwrap();
        assert_eq!(normalized_dp4.active1, quote_head.active1);
        assert!((normalized_dp4.price - 0.1002).abs() < 1e-9);
        assert!((normalized_dp4.last_close - 0.1006).abs() < 1e-9);
        assert!((normalized_dp4.open - 0.1001).abs() < 1e-9);
        assert!((normalized_dp4.high - 0.1012).abs() < 1e-9);
        assert!((normalized_dp4.low - 0.0998).abs() < 1e-9);
    }

    #[test]
    fn normalize_quote_head_by_decimal_point_rejects_invalid_ranges() {
        let quote_head = Tdx0547QuoteHead {
            active1: 1,
            price: 1.0,
            last_close: 1.0,
            open: 1.0,
            high: 1.0,
            low: 1.0,
        };

        assert_eq!(normalize_quote_head_by_decimal_point(&quote_head, 0), None);
        assert_eq!(normalize_quote_head_by_decimal_point(&quote_head, 1), None);
        assert_eq!(normalize_quote_head_by_decimal_point(&quote_head, 7), None);
    }

    #[test]
    fn format_extra0_time_hint_maps_verified_ranges_only() {
        assert_eq!(format_extra0_time_hint(-4_720).as_deref(), Some("15:00:00"));
        assert_eq!(format_extra0_time_hint(-4_723).as_deref(), Some("15:00:03"));
        assert_eq!(format_extra0_time_hint(5_480).as_deref(), Some("15:30:00"));
        assert_eq!(format_extra0_time_hint(5_492).as_deref(), Some("15:30:12"));
        assert_eq!(format_extra0_time_hint(0), None);
        assert_eq!(format_extra0_time_hint(-4_836), None);
    }

    #[test]
    fn hhmmss_raw_to_seconds_accepts_verified_values_only() {
        assert_eq!(hhmmss_raw_to_seconds(150_003), Some(54_003));
        assert_eq!(hhmmss_raw_to_seconds(153_000), Some(55_800));
        assert_eq!(hhmmss_raw_to_seconds(236_000), None);
    }

    #[test]
    fn public_time_matches_wangjifeng_oem_close_time_cap() {
        assert_eq!(
            format_public_hhmmss_raw(153_050).as_deref(),
            Some("15:00:00")
        );
        assert_eq!(
            format_public_hhmmss_raw(153_000).as_deref(),
            Some("15:00:00")
        );
        assert_eq!(
            format_public_hhmmss_raw(150_000).as_deref(),
            Some("15:00:00")
        );
        assert_eq!(
            format_public_hhmmss_raw(113_200).as_deref(),
            Some("11:32:00")
        );
        assert_eq!(format_public_hhmmss_raw(236_000), None);
    }

    #[test]
    fn extra0_time_hint_seconds_accepts_verified_ranges_only() {
        assert_eq!(extra0_time_hint_seconds(-4_720), Some(54_000));
        assert_eq!(extra0_time_hint_seconds(-4_723), Some(54_003));
        assert_eq!(extra0_time_hint_seconds(5_480), Some(55_800));
        assert_eq!(extra0_time_hint_seconds(5_492), Some(55_812));
        assert_eq!(extra0_time_hint_seconds(0), None);
        assert_eq!(extra0_time_hint_seconds(-4_836), None);
    }
}
