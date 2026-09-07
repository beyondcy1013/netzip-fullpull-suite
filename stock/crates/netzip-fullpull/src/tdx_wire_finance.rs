use std::collections::BTreeMap;

use serde::Serialize;

pub const WIRE_FINANCE_RECORD_SIZE: usize = 143;

pub const WIRE_FINANCE_FLOAT_FIELDS: [&str; 30] = [
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

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TdxWireFinanceRecord {
    pub market: u8,
    pub code: String,
    pub liutongguben: f32,
    pub province: u16,
    pub industry: u16,
    pub updated_date: u32,
    pub ipo_date: u32,
    pub float_fields: Vec<f32>,
}

impl TdxWireFinanceRecord {
    pub fn field_map(&self) -> BTreeMap<&'static str, f32> {
        let mut out = BTreeMap::new();
        for (name, value) in WIRE_FINANCE_FLOAT_FIELDS
            .iter()
            .zip(self.float_fields.iter().copied())
        {
            out.insert(*name, value);
        }
        out
    }

    pub fn float_field(&self, name: &str) -> Option<f32> {
        WIRE_FINANCE_FLOAT_FIELDS
            .iter()
            .position(|candidate| *candidate == name)
            .and_then(|index| self.float_fields.get(index).copied())
    }
}

pub fn parse_wire_finance_batch_body(body: &[u8]) -> Result<Vec<TdxWireFinanceRecord>, String> {
    if body.len() < 2 {
        return Err("wire finance body too short for count".to_string());
    }

    let count = u16::from_le_bytes([body[0], body[1]]) as usize;
    let needed = 2 + count * WIRE_FINANCE_RECORD_SIZE;
    if body.len() < needed {
        return Err(format!(
            "wire finance body truncated: count={count} need_at_least={needed} actual={}",
            body.len()
        ));
    }

    let mut out = Vec::with_capacity(count);
    let mut offset = 2usize;
    for _ in 0..count {
        let record = parse_wire_finance_record(&body[offset..offset + WIRE_FINANCE_RECORD_SIZE])?;
        out.push(record);
        offset += WIRE_FINANCE_RECORD_SIZE;
    }
    Ok(out)
}

pub fn parse_wire_finance_record(record: &[u8]) -> Result<TdxWireFinanceRecord, String> {
    if record.len() < WIRE_FINANCE_RECORD_SIZE {
        return Err(format!(
            "wire finance record too short: need={} actual={}",
            WIRE_FINANCE_RECORD_SIZE,
            record.len()
        ));
    }

    let market = record[0];
    let code = decode_ascii_trimmed(&record[1..7]);
    let liutongguben = read_f32(record, 7)?;
    let province = read_u16(record, 11)?;
    let industry = read_u16(record, 13)?;
    let updated_date = read_u32(record, 15)?;
    let ipo_date = read_u32(record, 19)?;

    let mut float_fields = Vec::with_capacity(WIRE_FINANCE_FLOAT_FIELDS.len());
    let mut offset = 23usize;
    for _ in WIRE_FINANCE_FLOAT_FIELDS {
        float_fields.push(read_f32(record, offset)?);
        offset += 4;
    }

    Ok(TdxWireFinanceRecord {
        market,
        code,
        liutongguben,
        province,
        industry,
        updated_date,
        ipo_date,
        float_fields,
    })
}

fn decode_ascii_trimmed(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("u16 out of bounds at offset {offset}"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("u32 out of bounds at offset {offset}"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_f32(bytes: &[u8], offset: usize) -> Result<f32, String> {
    Ok(f32::from_bits(read_u32(bytes, offset)?))
}

#[cfg(test)]
mod tests {
    use super::{
        WIRE_FINANCE_FLOAT_FIELDS, WIRE_FINANCE_RECORD_SIZE, parse_wire_finance_batch_body,
        parse_wire_finance_record,
    };

    fn build_record() -> Vec<u8> {
        let mut record = Vec::with_capacity(WIRE_FINANCE_RECORD_SIZE);
        record.push(1);
        record.extend_from_slice(b"600000");
        record.extend_from_slice(&1234.5_f32.to_le_bytes());
        record.extend_from_slice(&10_u16.to_le_bytes());
        record.extend_from_slice(&20_u16.to_le_bytes());
        record.extend_from_slice(&20260328_u32.to_le_bytes());
        record.extend_from_slice(&19991110_u32.to_le_bytes());
        for index in 0..WIRE_FINANCE_FLOAT_FIELDS.len() {
            record.extend_from_slice(&((index + 1) as f32).to_le_bytes());
        }
        assert_eq!(record.len(), WIRE_FINANCE_RECORD_SIZE);
        record
    }

    #[test]
    fn parses_single_wire_finance_record() {
        let record = build_record();
        let parsed = parse_wire_finance_record(&record).unwrap();
        assert_eq!(parsed.market, 1);
        assert_eq!(parsed.code, "600000");
        assert_eq!(parsed.liutongguben, 1234.5);
        assert_eq!(parsed.province, 10);
        assert_eq!(parsed.industry, 20);
        assert_eq!(parsed.updated_date, 20260328);
        assert_eq!(parsed.ipo_date, 19991110);
        assert_eq!(parsed.float_field("zongguben"), Some(1.0));
        assert_eq!(parsed.float_field("baoliu2"), Some(30.0));
    }

    #[test]
    fn parses_batch_with_count_prefix() {
        let record = build_record();
        let mut body = Vec::new();
        body.extend_from_slice(&2_u16.to_le_bytes());
        body.extend_from_slice(&record);
        body.extend_from_slice(&record);
        let parsed = parse_wire_finance_batch_body(&body).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].code, "600000");
        assert_eq!(parsed[1].float_field("jingzichan"), Some(16.0));
    }
}
