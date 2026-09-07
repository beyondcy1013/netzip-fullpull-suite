use crate::models::{FinanceInfo, GenericRow, KlineBar, QuoteRecord};
use crate::time::{Date, DateTime};
use encoding_rs::GBK;
use flate2::read::ZlibDecoder;
use std::io::Read;

pub const RESPONSE_HEADER_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResponseHeader {
    pub field1: u32,
    pub field2: u32,
    pub field3: u32,
    pub zip_size: u16,
    pub unzip_size: u16,
}

impl ResponseHeader {
    pub fn is_valid(self) -> bool {
        self.zip_size > 0 && self.unzip_size > 0
    }
}

pub fn parse_response_header(data: &[u8]) -> Option<ResponseHeader> {
    if data.len() < RESPONSE_HEADER_LEN {
        return None;
    }
    Some(ResponseHeader {
        field1: u32::from_le_bytes(data[0..4].try_into().ok()?),
        field2: u32::from_le_bytes(data[4..8].try_into().ok()?),
        field3: u32::from_le_bytes(data[8..12].try_into().ok()?),
        zip_size: u16::from_le_bytes(data[12..14].try_into().ok()?),
        unzip_size: u16::from_le_bytes(data[14..16].try_into().ok()?),
    })
}

pub fn parse_security_count_body(body: &[u8]) -> Option<u16> {
    if body.len() < 2 {
        None
    } else {
        Some(u16::from_le_bytes(body[0..2].try_into().ok()?))
    }
}

pub fn parse_quote_body(body: &[u8]) -> Result<Vec<QuoteRecord>, String> {
    if body.len() < 4 {
        return Err("body too short for quote header".to_string());
    }

    let mut pos = 2_usize;
    let count = u16::from_le_bytes(body[pos..pos + 2].try_into().unwrap()) as usize;
    pos += 2;
    let mut quotes = Vec::with_capacity(count);

    for _ in 0..count {
        if pos + 9 > body.len() {
            break;
        }

        let market = body[pos];
        pos += 1;

        let mut code_bytes = body[pos..pos + 6].to_vec();
        if let Some(null_index) = code_bytes.iter().position(|byte| *byte == 0) {
            code_bytes.truncate(null_index);
        }
        let code = String::from_utf8_lossy(&code_bytes).into_owned();
        pos += 6;

        let active1 = u16::from_le_bytes(body[pos..pos + 2].try_into().unwrap());
        pos += 2;

        let price = parse_price(body, &mut pos)?;
        let last_close_diff = parse_price(body, &mut pos)?;
        let open_diff = parse_price(body, &mut pos)?;
        let high_diff = parse_price(body, &mut pos)?;
        let low_diff = parse_price(body, &mut pos)?;
        let reversed0 = parse_price(body, &mut pos)?;
        let _reversed1 = parse_price(body, &mut pos)?;
        let volume = parse_price(body, &mut pos)?;
        let current_volume = parse_price(body, &mut pos)?;

        if pos + 4 > body.len() {
            break;
        }
        let amount_raw = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap());
        pos += 4;

        for _ in 0..4 {
            let _ = parse_price(body, &mut pos)?;
        }
        for _ in 0..20 {
            let _ = parse_price(body, &mut pos)?;
        }

        if pos + 2 > body.len() {
            break;
        }
        pos += 2;

        for _ in 0..4 {
            let _ = parse_price(body, &mut pos)?;
        }

        if pos + 4 > body.len() {
            break;
        }
        pos += 4;

        let base_price = f64::from(price) / 100.0;
        quotes.push(QuoteRecord {
            market,
            code,
            active1,
            price: base_price,
            open: f64::from(price + open_diff) / 100.0,
            high: f64::from(price + high_diff) / 100.0,
            low: f64::from(price + low_diff) / 100.0,
            last_close: f64::from(price + last_close_diff) / 100.0,
            volume: f64::from(volume),
            current_volume: f64::from(current_volume),
            amount: parse_tdx_packed_number(amount_raw),
            timestamp: 0,
            servertime: format_quote_servertime(reversed0),
        });
    }

    Ok(quotes)
}

fn format_quote_servertime(raw: i32) -> Option<String> {
    if raw <= 0 {
        return None;
    }

    let text = raw.to_string();
    if text.len() <= 6 {
        return None;
    }

    let (hour_part, tail) = text.split_at(text.len() - 6);
    let minute_prefix = tail.get(..2)?.parse::<u32>().ok()?;

    let mut out = String::with_capacity(12);
    out.push_str(hour_part);
    out.push(':');

    if minute_prefix < 60 {
        let second_fraction = tail.get(2..)?.parse::<f64>().ok()? * 60.0 / 10_000.0;
        out.push_str(&format!("{minute_prefix:02}:{second_fraction:06.3}"));
    } else {
        let total = tail.parse::<f64>().ok()?;
        let minute = (total * 60.0 / 1_000_000.0).floor() as u32;
        let second_fraction = ((total * 60.0) % 1_000_000.0) * 60.0 / 1_000_000.0;
        out.push_str(&format!("{minute:02}:{second_fraction:06.3}"));
    }

    Some(out)
}

pub fn parse_security_list_body(body: &[u8], market: u8) -> Result<Vec<GenericRow>, String> {
    if body.len() < 2 {
        return Err("body too short for security list count".to_string());
    }

    let count = u16::from_le_bytes(body[0..2].try_into().unwrap()) as usize;
    let mut pos = 2_usize;
    let mut rows = Vec::with_capacity(count);

    for _ in 0..count {
        if pos + 29 > body.len() {
            break;
        }

        let code = decode_ascii_trimmed(&body[pos..pos + 6]);
        pos += 6;

        let volunit = u16::from_le_bytes(body[pos..pos + 2].try_into().unwrap());
        pos += 2;

        let name = decode_gbk_trimmed(&body[pos..pos + 8]);
        pos += 8;

        pos += 4;

        let decimal_point = body[pos];
        pos += 1;

        let pre_close_raw = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap());
        pos += 4;

        pos += 4;

        let pre_close = f32::from_bits(pre_close_raw) as f64;

        let mut row = GenericRow::new();
        row.insert("market".to_string(), market.to_string());
        row.insert("code".to_string(), code);
        row.insert("volunit".to_string(), volunit.to_string());
        row.insert("name".to_string(), name);
        row.insert("decimal_point".to_string(), decimal_point.to_string());
        row.insert("pre_close".to_string(), format!("{pre_close:.3}"));
        rows.push(row);
    }

    Ok(rows)
}

pub fn parse_minute_time_body(body: &[u8]) -> Result<Vec<GenericRow>, String> {
    if body.len() < 16 {
        return Err("body too short for minute time data".to_string());
    }

    let count = u16::from_le_bytes(body[0..2].try_into().unwrap()) as usize;
    let mut pos = find_minute_time_start(body, count)
        .ok_or_else(|| "unable to locate minute time payload start".to_string())?;
    let mut rows = Vec::with_capacity(count);

    let base_price = i64::from(parse_price(body, &mut pos)?);
    let _base_reserved = parse_price(body, &mut pos)?;
    let base_volume = parse_price(body, &mut pos)?;
    let mut last_price = base_price;

    let mut first_row = GenericRow::new();
    first_row.insert("time".to_string(), format_minute_slot(0));
    first_row.insert(
        "price".to_string(),
        format!("{:.3}", last_price as f64 / 100.0),
    );
    first_row.insert("vol".to_string(), base_volume.to_string());
    rows.push(first_row);

    for index in 1..count {
        let price_delta = i64::from(parse_price(body, &mut pos)?);
        let _reserved = parse_price(body, &mut pos)?;
        let volume = parse_price(body, &mut pos)?;
        let mut row = GenericRow::new();
        row.insert("time".to_string(), format_minute_slot(index));
        row.insert(
            "price".to_string(),
            format!("{:.3}", (last_price + price_delta) as f64 / 100.0),
        );
        row.insert("vol".to_string(), volume.to_string());
        last_price += price_delta;
        rows.push(row);
    }

    Ok(rows)
}

fn find_minute_time_start(body: &[u8], count: usize) -> Option<usize> {
    let target_values = count.checked_mul(3)?;
    let search_end = body.len().min(128);
    let mut best: Option<(usize, i32)> = None;

    for start in 8..search_end {
        let mut pos = start;
        let mut parsed = 0_usize;
        let mut valid = true;
        while pos < body.len() {
            if parse_price(body, &mut pos).is_err() {
                valid = false;
                break;
            }
            parsed += 1;
        }
        if !valid || parsed != target_values {
            continue;
        }

        let mut probe = start;
        let first_value = match parse_price(body, &mut probe) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if first_value <= 0 {
            continue;
        }

        match best {
            Some((_, best_value)) if first_value <= best_value => {}
            _ => best = Some((start, first_value)),
        }
    }

    best.map(|(start, _)| start)
}

fn format_minute_slot(index: usize) -> String {
    let total_minutes = if index < 120 {
        9 * 60 + 30 + index
    } else {
        13 * 60 + (index - 120)
    };
    let hour = total_minutes / 60;
    let minute = total_minutes % 60;

    format!("{hour:02}:{minute:02}")
}

pub fn parse_company_info_category_body(body: &[u8]) -> Result<Vec<GenericRow>, String> {
    if body.len() < 2 {
        return Err("body too short for company info category count".to_string());
    }

    let count = u16::from_le_bytes(body[0..2].try_into().unwrap()) as usize;
    let mut pos = 2_usize;
    let mut rows = Vec::with_capacity(count);

    for _ in 0..count {
        if pos + 152 > body.len() {
            break;
        }

        let name = decode_gbk_trimmed(&body[pos..pos + 64]);
        pos += 64;

        let filename = decode_gbk_trimmed(&body[pos..pos + 80]);
        pos += 80;

        let start = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap());
        pos += 4;

        let length = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap());
        pos += 4;

        let mut row = GenericRow::new();
        row.insert("name".to_string(), name);
        row.insert("filename".to_string(), filename);
        row.insert("start".to_string(), start.to_string());
        row.insert("length".to_string(), length.to_string());
        rows.push(row);
    }

    Ok(rows)
}

pub fn parse_company_info_content_body(body: &[u8]) -> Result<String, String> {
    if body.len() < 12 {
        return Err("body too short for company info content".to_string());
    }

    let content_len = u16::from_le_bytes(body[10..12].try_into().unwrap()) as usize;
    if content_len == 0 {
        return Ok(String::new());
    }
    if body.len() < 12 + content_len {
        return Err("body truncated while reading company info content".to_string());
    }

    let (decoded, _, _) = GBK.decode(&body[12..12 + content_len]);
    Ok(decoded.into_owned())
}

pub fn parse_finance_info_body(body: &[u8]) -> Result<FinanceInfo, String> {
    if body.len() < 145 {
        return Err(format!("finance info body too short: {}", body.len()));
    }

    let mut pos = 2_usize;
    let market = *body
        .get(pos)
        .ok_or_else(|| "finance info missing market".to_string())?;
    pos += 1;

    if pos + 6 > body.len() {
        return Err("finance info missing code".to_string());
    }
    let code = decode_ascii_trimmed(&body[pos..pos + 6]);
    pos += 6;

    let mut fields = GenericRow::new();

    let liutongguben = read_f32(body, &mut pos)?;
    fields.insert("liutongguben".to_string(), format_float(liutongguben));

    fields.insert(
        "province".to_string(),
        read_u16(body, &mut pos)?.to_string(),
    );
    fields.insert(
        "industry".to_string(),
        read_u16(body, &mut pos)?.to_string(),
    );

    let updated_date = read_u32(body, &mut pos)?;
    fields.insert("updated_date".to_string(), updated_date.to_string());

    let ipo_date = read_u32(body, &mut pos)?;
    fields.insert("ipo_date".to_string(), ipo_date.to_string());
    if let Some(ipo_date_str) = format_yyyymmdd(ipo_date) {
        fields.insert("ipo_date_str".to_string(), ipo_date_str);
    }

    const FLOAT_FIELDS: &[&str] = &[
        "zongguben",
        "guojiagu",
        "faqirenfarengu",
        "farengu",
        "bgu",
        "hgu",
        "zhigonggu",
        "zongzichan",
        "liudongzichan",
        "gudingzichan",
        "wuxingzichan",
        "gudongrenshu",
        "liudongfuzhai",
        "changqifuzhai",
        "zibengongjijin",
        "jingzichan",
        "zhuyingshouru",
        "zhuyinglirun",
        "yingshouzhangkuan",
        "yingyelirun",
        "touzishouyu",
        "jingyingxianjinliu",
        "zongxianjinliu",
        "cunhuo",
        "lirunzonghe",
        "shuihoulirun",
        "jinglirun",
        "weifenpeilirun",
        "meigujingzichan",
        "baoliu2",
    ];

    for key in FLOAT_FIELDS {
        fields.insert((*key).to_string(), format_float(read_f32(body, &mut pos)?));
    }

    Ok(FinanceInfo {
        market,
        code,
        fields,
    })
}

pub fn parse_block_info_meta_body(body: &[u8]) -> Result<(u32, Vec<u8>), String> {
    if body.len() < 37 {
        return Err("block meta body too short".to_string());
    }

    let size = u32::from_le_bytes(body[0..4].try_into().unwrap());
    let hash = body[5..37].to_vec();
    Ok((size, hash))
}

pub fn parse_block_data(data: &[u8]) -> Result<Vec<GenericRow>, String> {
    if data.len() < 386 {
        return Err("block data too short".to_string());
    }

    let mut pos = 384_usize;
    let block_count = u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()) as usize;
    pos += 2;
    let mut rows = Vec::with_capacity(block_count);

    for _ in 0..block_count {
        if pos + 13 > data.len() {
            break;
        }

        let block_name = decode_gbk_trimmed(&data[pos..pos + 9]);
        pos += 9;

        let stock_count = u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap());
        pos += 2;
        let block_type = u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap());
        pos += 2;

        let code_area_start = pos;
        let mut codes = Vec::with_capacity(stock_count as usize);
        for _ in 0..stock_count {
            if pos + 7 > data.len() {
                break;
            }
            let code = decode_ascii_trimmed(&data[pos..pos + 7]);
            if !code.is_empty() {
                codes.push(code);
            }
            pos += 7;
        }

        pos = code_area_start.saturating_add(2800);
        if pos > data.len() {
            break;
        }

        let mut row = GenericRow::new();
        row.insert("blockname".to_string(), block_name);
        row.insert("block_type".to_string(), block_type.to_string());
        row.insert("stock_count".to_string(), stock_count.to_string());
        row.insert("code_list".to_string(), codes.join(","));
        rows.push(row);
    }

    Ok(rows)
}

pub fn parse_kline_body(code: &str, category: i32, body: &[u8]) -> Result<Vec<KlineBar>, String> {
    parse_single_kline_body(code, category, body, false)
}

pub fn parse_single_kline_body(
    code: &str,
    category: i32,
    body: &[u8],
    is_index: bool,
) -> Result<Vec<KlineBar>, String> {
    if body.len() < 2 {
        return Err("body too short for kline count".to_string());
    }

    let count = u16::from_le_bytes([body[0], body[1]]) as usize;
    let mut pos = 2_usize;
    let mut pre_diff_base = 0_i32;
    let mut rows = Vec::with_capacity(count);

    for _ in 0..count {
        let dt = parse_datetime(category, body, &mut pos)?;
        let price_open_diff = parse_price(body, &mut pos)?;
        let price_close_diff = parse_price(body, &mut pos)?;
        let price_high_diff = parse_price(body, &mut pos)?;
        let price_low_diff = parse_price(body, &mut pos)?;

        if pos + 8 > body.len() {
            return Err("body truncated while reading volume".to_string());
        }
        let volume_raw = u32::from_le_bytes(body[pos..pos + 4].try_into().unwrap());
        let amount_raw = u32::from_le_bytes(body[pos + 4..pos + 8].try_into().unwrap());
        pos += 8;

        let open = calculate_price_1000(price_open_diff, pre_diff_base);
        let price_open_updated = price_open_diff + pre_diff_base;
        let close = calculate_price_1000(price_open_updated, price_close_diff);
        let high = calculate_price_1000(price_open_updated, price_high_diff);
        let low = calculate_price_1000(price_open_updated, price_low_diff);
        pre_diff_base = price_open_updated + price_close_diff;

        if is_index {
            if pos + 4 > body.len() {
                return Err("body truncated while reading index up/down counts".to_string());
            }
            pos += 4;
        }

        let timestamp = Some(
            crate::time::civil_to_days(dt.date.year, dt.date.month, dt.date.day) * 86_400_000
                + i64::from(dt.hour) * 3_600_000
                + i64::from(dt.minute) * 60_000
                + i64::from(dt.second) * 1_000,
        );

        rows.push(KlineBar {
            code: code.to_string(),
            datetime: dt.format(true),
            timestamp,
            open,
            high,
            low,
            close,
            volume: parse_tdx_packed_number(volume_raw),
            amount: parse_tdx_packed_number(amount_raw),
        });
    }

    Ok(rows)
}

pub fn uncompress_zlib(data: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    if expected_len == 0 {
        return Err("expected uncompressed size is zero".to_string());
    }

    let mut decoder = ZlibDecoder::new(data);
    let mut output = Vec::with_capacity(expected_len);
    decoder
        .read_to_end(&mut output)
        .map_err(|error| format!("zlib uncompress failed: {error}"))?;
    if output.len() > expected_len {
        return Err(format!(
            "zlib uncompress exceeded expected size: {} > {expected_len}",
            output.len()
        ));
    }
    Ok(output)
}

fn parse_datetime(category: i32, buffer: &[u8], pos: &mut usize) -> Result<DateTime, String> {
    if *pos + 4 > buffer.len() {
        return Err("body truncated while reading datetime".to_string());
    }

    let date_time = if category < 4 || category == 7 || category == 8 {
        let zip_day = u16::from_le_bytes(buffer[*pos..*pos + 2].try_into().unwrap());
        let minutes = u16::from_le_bytes(buffer[*pos + 2..*pos + 4].try_into().unwrap());
        let month = ((zip_day % 2048) / 100) as u8;
        let year = i32::from(zip_day >> 11) + 2004;
        let day = ((zip_day % 2048) % 100) as u8;
        let hour = (minutes / 60) as u8;
        let minute = (minutes % 60) as u8;
        DateTime::new(Date::new(year, month, day), hour, minute, 0)
    } else {
        let zip_day = u32::from_le_bytes(buffer[*pos..*pos + 4].try_into().unwrap());
        let year = (zip_day / 10_000) as i32;
        let month = ((zip_day % 10_000) / 100) as u8;
        let day = (zip_day % 100) as u8;
        DateTime::new(Date::new(year, month, day), 15, 0, 0)
    };

    *pos += 4;
    Ok(date_time)
}

fn parse_price(data: &[u8], pos: &mut usize) -> Result<i32, String> {
    if *pos >= data.len() {
        return Err("body truncated while reading price".to_string());
    }

    let mut shift = 6;
    let mut byte = data[*pos];
    let mut value = i32::from(byte & 0x3f);
    let negative = byte & 0x40 != 0;

    if byte & 0x80 != 0 {
        loop {
            *pos += 1;
            if *pos >= data.len() {
                return Err("body truncated inside varint price".to_string());
            }
            byte = data[*pos];
            value += i32::from(byte & 0x7f) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
    }

    *pos += 1;
    if negative {
        value = -value;
    }
    Ok(value)
}

fn decode_ascii_trimmed(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn decode_gbk_trimmed(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let (decoded, _, _) = GBK.decode(&bytes[..end]);
    decoded.trim().to_string()
}

fn read_u16(data: &[u8], pos: &mut usize) -> Result<u16, String> {
    if *pos + 2 > data.len() {
        return Err("body truncated while reading u16".to_string());
    }
    let value = u16::from_le_bytes(data[*pos..*pos + 2].try_into().unwrap());
    *pos += 2;
    Ok(value)
}

fn read_u32(data: &[u8], pos: &mut usize) -> Result<u32, String> {
    if *pos + 4 > data.len() {
        return Err("body truncated while reading u32".to_string());
    }
    let value = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(value)
}

fn read_f32(data: &[u8], pos: &mut usize) -> Result<f32, String> {
    if *pos + 4 > data.len() {
        return Err("body truncated while reading f32".to_string());
    }
    let value = f32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(value)
}

fn format_yyyymmdd(value: u32) -> Option<String> {
    if value == 0 {
        None
    } else {
        let year = value / 10_000;
        let month = (value % 10_000) / 100;
        let day = value % 100;
        Some(format!("{year:04}-{month:02}-{day:02}"))
    }
}

fn format_float(value: f32) -> String {
    format!("{:.6}", value)
}

pub fn parse_tdx_packed_number(raw: u32) -> f64 {
    if raw == 0 {
        return 0.0;
    }

    let logpoint = (raw >> 24) as u8;
    let hleax = ((raw >> 16) & 0xff) as u8;
    let lheax = ((raw >> 8) & 0xff) as u8;
    let lleax = (raw & 0xff) as u8;

    let dw_ecx = i32::from(logpoint) * 2 - 0x7f;
    let dw_edx = i32::from(logpoint) * 2 - 0x86;
    let dw_esi = i32::from(logpoint) * 2 - 0x8e;
    let dw_eax = i32::from(logpoint) * 2 - 0x96;

    let magnitude = 2_f64.powi(dw_ecx.unsigned_abs() as i32);
    let base = if dw_ecx < 0 {
        1.0 / magnitude
    } else {
        magnitude
    };

    let mid = if hleax > 0x80 {
        let tmp = 2_f64.powi(dw_edx + 1);
        2_f64.powi(dw_edx) * 128.0 + f64::from(hleax & 0x7f) * tmp
    } else if dw_edx >= 0 {
        2_f64.powi(dw_edx) * f64::from(hleax)
    } else {
        (1.0 / 2_f64.powi(-dw_edx)) * f64::from(hleax)
    };

    let mut lower_mid = 2_f64.powi(dw_esi) * f64::from(lheax);
    let mut lower = 2_f64.powi(dw_eax) * f64::from(lleax);
    if hleax & 0x80 != 0 {
        lower_mid *= 2.0;
        lower *= 2.0;
    }

    base + mid + lower_mid + lower
}

fn calculate_price_1000(base: i32, diff: i32) -> f64 {
    f64::from(base + diff) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::{
        format_quote_servertime, parse_block_data, parse_block_info_meta_body,
        parse_company_info_category_body, parse_company_info_content_body, parse_finance_info_body,
        parse_kline_body, parse_minute_time_body, parse_quote_body, parse_response_header,
        parse_security_list_body,
    };

    #[test]
    fn response_header_reads_sizes() {
        let header =
            parse_response_header(&[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0x10, 0x00, 0x20, 0x00])
                .unwrap();
        assert_eq!(header.field1, 1);
        assert_eq!(header.zip_size, 16);
        assert_eq!(header.unzip_size, 32);
    }

    #[test]
    fn kline_parser_decodes_single_minute_bar() {
        let mut body = Vec::new();
        body.extend_from_slice(&1_u16.to_le_bytes());
        let zip_day = (((2026 - 2004) as u16) << 11) | (3 * 100 + 10);
        body.extend_from_slice(&zip_day.to_le_bytes());
        let minutes = (9 * 60 + 31) as u16;
        body.extend_from_slice(&minutes.to_le_bytes());
        body.extend_from_slice(&encode_price(10));
        body.extend_from_slice(&encode_price(1));
        body.extend_from_slice(&encode_price(2));
        body.extend_from_slice(&encode_price(-1));
        body.extend_from_slice(&0_u32.to_le_bytes());
        body.extend_from_slice(&0_u32.to_le_bytes());

        let rows = parse_kline_body("000001", 7, &body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].datetime, "2026-03-10 09:31:00");
        assert_eq!(rows[0].open, 0.010);
        assert_eq!(rows[0].close, 0.011);
        assert_eq!(rows[0].high, 0.012);
        assert_eq!(rows[0].low, 0.009);
    }

    #[test]
    fn quote_parser_decodes_single_quote() {
        let mut body = vec![0, 0];
        body.extend_from_slice(&1_u16.to_le_bytes());
        body.push(0);
        body.extend_from_slice(b"000001");
        body.extend_from_slice(&0_u16.to_le_bytes());
        body.extend_from_slice(&encode_price(1000));
        body.extend_from_slice(&encode_price(-10));
        body.extend_from_slice(&encode_price(5));
        body.extend_from_slice(&encode_price(15));
        body.extend_from_slice(&encode_price(-5));
        body.extend_from_slice(&encode_price(0));
        body.extend_from_slice(&encode_price(0));
        body.extend_from_slice(&encode_price(1234));
        body.extend_from_slice(&encode_price(12));
        body.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..4 {
            body.extend_from_slice(&encode_price(0));
        }
        for _ in 0..20 {
            body.extend_from_slice(&encode_price(0));
        }
        body.extend_from_slice(&0_u16.to_le_bytes());
        for _ in 0..4 {
            body.extend_from_slice(&encode_price(0));
        }
        body.extend_from_slice(&0_i16.to_le_bytes());
        body.extend_from_slice(&0_u16.to_le_bytes());

        let quotes = parse_quote_body(&body).unwrap();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].code, "000001");
        assert_eq!(quotes[0].active1, 0);
        assert_eq!(quotes[0].price, 10.0);
        assert_eq!(quotes[0].last_close, 9.9);
        assert_eq!(quotes[0].high, 10.15);
        assert_eq!(quotes[0].current_volume, 12.0);
    }

    #[test]
    fn security_list_parser_decodes_single_row() {
        let mut body = Vec::new();
        body.extend_from_slice(&1_u16.to_le_bytes());
        body.extend_from_slice(b"000001");
        body.extend_from_slice(&100_u16.to_le_bytes());
        body.extend_from_slice(b"PingAn  ");
        body.extend_from_slice(&0_u32.to_le_bytes());
        body.push(2);
        body.extend_from_slice(&12.34_f32.to_bits().to_le_bytes());
        body.extend_from_slice(&0_u32.to_le_bytes());

        let rows = parse_security_list_body(&body, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["market"], "0");
        assert_eq!(rows[0]["code"], "000001");
        assert_eq!(rows[0]["name"], "PingAn");
        assert_eq!(rows[0]["pre_close"], "12.340");
    }

    #[test]
    fn company_info_category_parser_decodes_single_row() {
        let mut body = Vec::new();
        body.extend_from_slice(&1_u16.to_le_bytes());
        let mut name = [0_u8; 64];
        name[..8].copy_from_slice(b"Profile ");
        body.extend_from_slice(&name);
        let mut filename = [0_u8; 80];
        filename[..10].copy_from_slice(b"000001.txt");
        body.extend_from_slice(&filename);
        body.extend_from_slice(&100_u32.to_le_bytes());
        body.extend_from_slice(&200_u32.to_le_bytes());

        let rows = parse_company_info_category_body(&body).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "Profile");
        assert_eq!(rows[0]["filename"], "000001.txt");
        assert_eq!(rows[0]["start"], "100");
        assert_eq!(rows[0]["length"], "200");
    }

    #[test]
    fn company_info_content_parser_decodes_single_chunk() {
        let mut body = vec![0_u8; 10];
        body.extend_from_slice(&5_u16.to_le_bytes());
        body.extend_from_slice(b"Hello");
        let text = parse_company_info_content_body(&body).unwrap();
        assert_eq!(text, "Hello");
    }

    #[test]
    fn finance_info_parser_decodes_core_fields() {
        let mut body = vec![0_u8; 2];
        body.push(0);
        body.extend_from_slice(b"000001");
        body.extend_from_slice(&1000.0_f32.to_le_bytes());
        body.extend_from_slice(&1_u16.to_le_bytes());
        body.extend_from_slice(&2_u16.to_le_bytes());
        body.extend_from_slice(&20260310_u32.to_le_bytes());
        body.extend_from_slice(&19910403_u32.to_le_bytes());
        for idx in 0..30 {
            body.extend_from_slice(&((idx + 1) as f32).to_le_bytes());
        }

        let info = parse_finance_info_body(&body).unwrap();
        assert_eq!(info.market, 0);
        assert_eq!(info.code, "000001");
        assert_eq!(info.fields["liutongguben"], "1000.000000");
        assert_eq!(info.fields["ipo_date"], "19910403");
        assert_eq!(info.fields["ipo_date_str"], "1991-04-03");
        assert_eq!(info.fields["zongguben"], "1.000000");
        assert_eq!(info.fields["baoliu2"], "30.000000");
    }

    #[test]
    fn block_meta_parser_decodes_size() {
        let mut body = Vec::new();
        body.extend_from_slice(&1234_u32.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&[1_u8; 32]);

        let (size, hash) = parse_block_info_meta_body(&body).unwrap();
        assert_eq!(size, 1234);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn block_data_parser_decodes_single_block() {
        let mut data = vec![0_u8; 384];
        data.extend_from_slice(&1_u16.to_le_bytes());
        let mut name = [0_u8; 9];
        name[..5].copy_from_slice(b"TEST1");
        data.extend_from_slice(&name);
        data.extend_from_slice(&2_u16.to_le_bytes());
        data.extend_from_slice(&7_u16.to_le_bytes());
        let mut code_area = vec![0_u8; 2800];
        code_area[..7].copy_from_slice(b"000001\0");
        code_area[7..14].copy_from_slice(b"600000\0");
        data.extend_from_slice(&code_area);

        let rows = parse_block_data(&data).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["blockname"], "TEST1");
        assert_eq!(rows[0]["block_type"], "7");
        assert_eq!(rows[0]["stock_count"], "2");
        assert_eq!(rows[0]["code_list"], "000001,600000");
    }

    #[test]
    fn minute_time_parser_decodes_cumulative_prices() {
        let mut body = vec![0_u8; 67];
        body[0..2].copy_from_slice(&2_u16.to_le_bytes());
        body.extend_from_slice(&encode_price(1000));
        body.extend_from_slice(&encode_price(7));
        body.extend_from_slice(&encode_price(12));
        body.extend_from_slice(&encode_price(5));
        body.extend_from_slice(&encode_price(0));
        body.extend_from_slice(&encode_price(34));

        let rows = parse_minute_time_body(&body).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["time"], "09:30");
        assert_eq!(rows[0]["price"], "10.000");
        assert_eq!(rows[0]["vol"], "12");
        assert_eq!(rows[1]["time"], "09:31");
        assert_eq!(rows[1]["price"], "10.050");
        assert_eq!(rows[1]["vol"], "34");
    }

    #[test]
    fn quote_servertime_formatter_matches_pytdx_output() {
        assert_eq!(
            format_quote_servertime(15_261_068).as_deref(),
            Some("15:26:06.408")
        );
    }

    fn encode_price(value: i32) -> Vec<u8> {
        let negative = value < 0;
        let mut remaining = value.unsigned_abs();
        let mut bytes = Vec::new();

        let mut first = (remaining as u8) & 0x3f;
        remaining >>= 6;
        if negative {
            first |= 0x40;
        }
        if remaining > 0 {
            first |= 0x80;
        }
        bytes.push(first);

        while remaining > 0 {
            let mut next = (remaining as u8) & 0x7f;
            remaining >>= 7;
            if remaining > 0 {
                next |= 0x80;
            }
            bytes.push(next);
        }

        bytes
    }
}
