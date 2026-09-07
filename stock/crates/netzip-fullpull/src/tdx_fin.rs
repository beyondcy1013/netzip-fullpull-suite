use std::error::Error;
use std::fs;
use std::path::Path;

use serde::Serialize;

pub const FIN_FILE_MAGIC: u32 = 0x223f_d90e;
pub const FIN_KEY_SIZE: usize = 12;
pub const FIN_MIN_RECORD_SIZE: usize = 0xe0;
pub const FIN_EXPORTED_PAYLOAD_OFFSET: usize = 0x0c;
pub const FIN_EXPORTED_PAYLOAD_LEN: usize = 0xcc;
pub const SH_FIN_URL: &str = "http://filedown.gw.com.cn/download/FIN/full_sh.FIN";
pub const SZ_FIN_URL: &str = "http://filedown.gw.com.cn/download/FIN/full_sz.FIN";

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FinFieldSpec {
    pub name: &'static str,
    pub raw_offset: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FinGetterSpec {
    pub field_id: u8,
    pub name: &'static str,
    pub raw_offset: Option<usize>,
    pub note: &'static str,
}

pub const FIN_FIELD_SPECS: [FinFieldSpec; 48] = [
    FinFieldSpec {
        name: "mg_shou_yi",
        raw_offset: 0x18,
    },
    FinFieldSpec {
        name: "mg_jing_zhi",
        raw_offset: 0x1c,
    },
    FinFieldSpec {
        name: "jzcsyl",
        raw_offset: 0x20,
    },
    FinFieldSpec {
        name: "mg_xian_jin",
        raw_offset: 0x24,
    },
    FinFieldSpec {
        name: "mggjj",
        raw_offset: 0x28,
    },
    FinFieldSpec {
        name: "mgwfp",
        raw_offset: 0x2c,
    },
    FinFieldSpec {
        name: "gdqybl",
        raw_offset: 0x30,
    },
    FinFieldSpec {
        name: "jlrtb",
        raw_offset: 0x34,
    },
    FinFieldSpec {
        name: "zysytb",
        raw_offset: 0x38,
    },
    FinFieldSpec {
        name: "xsmll",
        raw_offset: 0x3c,
    },
    FinFieldSpec {
        name: "tz_jing_zhi",
        raw_offset: 0x40,
    },
    FinFieldSpec {
        name: "zong_zc",
        raw_offset: 0x44,
    },
    FinFieldSpec {
        name: "ldzc",
        raw_offset: 0x48,
    },
    FinFieldSpec {
        name: "gu_ding_zc",
        raw_offset: 0x4c,
    },
    FinFieldSpec {
        name: "wu_xing_zc",
        raw_offset: 0x50,
    },
    FinFieldSpec {
        name: "ld_fu_zhai",
        raw_offset: 0x54,
    },
    FinFieldSpec {
        name: "cq_fu_zhai",
        raw_offset: 0x58,
    },
    FinFieldSpec {
        name: "zong_fu_zhai",
        raw_offset: 0x5c,
    },
    FinFieldSpec {
        name: "quan_yi",
        raw_offset: 0x60,
    },
    FinFieldSpec {
        name: "zb_gong_ji",
        raw_offset: 0x64,
    },
    FinFieldSpec {
        name: "xian_jin",
        raw_offset: 0x68,
    },
    FinFieldSpec {
        name: "tzxjl",
        raw_offset: 0x6c,
    },
    FinFieldSpec {
        name: "czxjl",
        raw_offset: 0x70,
    },
    FinFieldSpec {
        name: "xjzje",
        raw_offset: 0x74,
    },
    FinFieldSpec {
        name: "shou_ru",
        raw_offset: 0x78,
    },
    FinFieldSpec {
        name: "zy_li_run",
        raw_offset: 0x7c,
    },
    FinFieldSpec {
        name: "yy_li_run",
        raw_offset: 0x80,
    },
    FinFieldSpec {
        name: "tz_shou_yi",
        raw_offset: 0x84,
    },
    FinFieldSpec {
        name: "yyw_shou_zhi",
        raw_offset: 0x88,
    },
    FinFieldSpec {
        name: "zong_li_run",
        raw_offset: 0x8c,
    },
    FinFieldSpec {
        name: "jing_li_run",
        raw_offset: 0x90,
    },
    FinFieldSpec {
        name: "wei_fen_pei",
        raw_offset: 0x94,
    },
    FinFieldSpec {
        name: "zong_gu",
        raw_offset: 0x98,
    },
    FinFieldSpec {
        name: "wxs_ag",
        raw_offset: 0x9c,
    },
    FinFieldSpec {
        name: "liu_tong_ag",
        raw_offset: 0xa0,
    },
    FinFieldSpec {
        name: "b_gu",
        raw_offset: 0xa4,
    },
    FinFieldSpec {
        name: "jingwai_gu",
        raw_offset: 0xa8,
    },
    FinFieldSpec {
        name: "qtltg",
        raw_offset: 0xac,
    },
    FinFieldSpec {
        name: "xsghj",
        raw_offset: 0xb0,
    },
    FinFieldSpec {
        name: "guojia_gu",
        raw_offset: 0xb4,
    },
    FinFieldSpec {
        name: "faren_gu",
        raw_offset: 0xb8,
    },
    FinFieldSpec {
        name: "jn_fa_ren_gu",
        raw_offset: 0xbc,
    },
    FinFieldSpec {
        name: "jn_zi_ran_gu",
        raw_offset: 0xc0,
    },
    FinFieldSpec {
        name: "qita_gu",
        raw_offset: 0xc4,
    },
    FinFieldSpec {
        name: "muji_gu",
        raw_offset: 0xc8,
    },
    FinFieldSpec {
        name: "jing_wai_gu",
        raw_offset: 0xcc,
    },
    FinFieldSpec {
        name: "jw_zi_ran_gu",
        raw_offset: 0xd0,
    },
    FinFieldSpec {
        name: "youxian_gu",
        raw_offset: 0xd4,
    },
];

pub const FIN_GETTER_SPECS: [FinGetterSpec; 18] = [
    FinGetterSpec {
        field_id: 0x2a,
        name: "quarter",
        raw_offset: None,
        note: "derived from bao_gao",
    },
    FinGetterSpec {
        field_id: 0x2b,
        name: "zong_gu",
        raw_offset: Some(0x98),
        note: "confirmed getter case",
    },
    FinGetterSpec {
        field_id: 0x2c,
        name: "liu_tong_ag",
        raw_offset: Some(0xa0),
        note: "confirmed getter case",
    },
    FinGetterSpec {
        field_id: 0x30,
        name: "reserved_zero",
        raw_offset: None,
        note: "hardcoded zero",
    },
    FinGetterSpec {
        field_id: 0x33,
        name: "b_gu",
        raw_offset: Some(0xa4),
        note: "confirmed getter case",
    },
    FinGetterSpec {
        field_id: 0x34,
        name: "reserved_zero",
        raw_offset: None,
        note: "hardcoded zero",
    },
    FinGetterSpec {
        field_id: 0x35,
        name: "mg_shou_yi",
        raw_offset: Some(0x18),
        note: "confirmed getter case",
    },
    FinGetterSpec {
        field_id: 0x36,
        name: "reserved_zero",
        raw_offset: None,
        note: "hardcoded zero",
    },
    FinGetterSpec {
        field_id: 0x3a,
        name: "zong_zc",
        raw_offset: Some(0x44),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x3b,
        name: "ldzc",
        raw_offset: Some(0x48),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x3c,
        name: "reserved_zero",
        raw_offset: None,
        note: "hardcoded zero",
    },
    FinGetterSpec {
        field_id: 0x3d,
        name: "gu_ding_zc",
        raw_offset: Some(0x4c),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x3e,
        name: "wu_xing_zc",
        raw_offset: Some(0x50),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x3f,
        name: "ld_fu_zhai",
        raw_offset: Some(0x54),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x40,
        name: "cq_fu_zhai",
        raw_offset: Some(0x58),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x41,
        name: "quan_yi",
        raw_offset: Some(0x60),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x42,
        name: "zb_gong_ji",
        raw_offset: Some(0x64),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x43,
        name: "reserved_zero",
        raw_offset: None,
        note: "hardcoded zero",
    },
];

pub const FIN_GETTER_TAIL_SPECS: [FinGetterSpec; 7] = [
    FinGetterSpec {
        field_id: 0x44,
        name: "mg_jing_zhi",
        raw_offset: Some(0x1c),
        note: "confirmed getter case",
    },
    FinGetterSpec {
        field_id: 0x46,
        name: "shou_ru",
        raw_offset: Some(0x78),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x47,
        name: "zy_li_run",
        raw_offset: Some(0x7c),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x48,
        name: "yyw_shou_zhi",
        raw_offset: Some(0x88),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x49,
        name: "zong_li_run",
        raw_offset: Some(0x8c),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x4a,
        name: "jing_li_run",
        raw_offset: Some(0x90),
        note: "confirmed getter chain",
    },
    FinGetterSpec {
        field_id: 0x4b,
        name: "wei_fen_pei",
        raw_offset: Some(0x94),
        note: "confirmed getter chain",
    },
];

pub const FIN_GETTER_GAP_SPECS: [FinGetterSpec; 9] = [
    FinGetterSpec {
        field_id: 0x2d,
        name: "unresolved_0x2d",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x2e,
        name: "unresolved_0x2e",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x2f,
        name: "unresolved_0x2f",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x31,
        name: "unresolved_0x31",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x32,
        name: "unresolved_0x32",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x37,
        name: "unresolved_0x37",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x38,
        name: "unresolved_0x38",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x39,
        name: "unresolved_0x39",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
    FinGetterSpec {
        field_id: 0x45,
        name: "unresolved_0x45",
        raw_offset: None,
        note: "observed in vendor getter switch; current samples still return None",
    },
];

pub const FIN_GETTER_UNRESOLVED_IDS: [u8; 9] =
    [0x2d, 0x2e, 0x2f, 0x31, 0x32, 0x37, 0x38, 0x39, 0x45];

#[derive(Clone, Debug)]
pub struct TdxFinFile {
    pub magic: u32,
    pub record_size: u32,
    pub records: Vec<TdxFinRecord>,
    pub trailing_bytes: Vec<u8>,
}

impl TdxFinFile {
    pub fn find_record(&self, symbol: &str) -> Option<&TdxFinRecord> {
        self.records.iter().find(|record| record.symbol == symbol)
    }
}

#[derive(Clone, Debug)]
pub struct TdxFinRecord {
    pub key_raw: [u8; FIN_KEY_SIZE],
    pub symbol: String,
    pub time: u32,
    pub bao_gao: u32,
    pub shang_shi: u32,
    pub mg_shou_yi: f32,
    pub mg_jing_zhi: f32,
    pub jzcsyl: f32,
    pub mg_xian_jin: f32,
    pub mggjj: f32,
    pub mgwfp: f32,
    pub gdqybl: f32,
    pub jlrtb: f32,
    pub zysytb: f32,
    pub xsmll: f32,
    pub tz_jing_zhi: f32,
    pub zong_zc: f32,
    pub ldzc: f32,
    pub gu_ding_zc: f32,
    pub wu_xing_zc: f32,
    pub ld_fu_zhai: f32,
    pub cq_fu_zhai: f32,
    pub zong_fu_zhai: f32,
    pub quan_yi: f32,
    pub zb_gong_ji: f32,
    pub xian_jin: f32,
    pub tzxjl: f32,
    pub czxjl: f32,
    pub xjzje: f32,
    pub shou_ru: f32,
    pub zy_li_run: f32,
    pub yy_li_run: f32,
    pub tz_shou_yi: f32,
    pub yyw_shou_zhi: f32,
    pub zong_li_run: f32,
    pub jing_li_run: f32,
    pub wei_fen_pei: f32,
    pub zong_gu: f32,
    pub wxs_ag: f32,
    pub liu_tong_ag: f32,
    pub b_gu: f32,
    pub jingwai_gu: f32,
    pub qtltg: f32,
    pub xsghj: f32,
    pub guojia_gu: f32,
    pub faren_gu: f32,
    pub jn_fa_ren_gu: f32,
    pub jn_zi_ran_gu: f32,
    pub qita_gu: f32,
    pub muji_gu: f32,
    pub jing_wai_gu: f32,
    pub jw_zi_ran_gu: f32,
    pub youxian_gu: f32,
    pub trailer: Vec<u8>,
}

impl TdxFinRecord {
    pub fn market(&self) -> Option<&str> {
        let prefix = self.symbol.get(0..2)?;
        if prefix.as_bytes().iter().all(u8::is_ascii_alphabetic) {
            Some(prefix)
        } else {
            None
        }
    }

    pub fn code(&self) -> Option<&str> {
        let code = self.symbol.get(2..8)?;
        if code.as_bytes().iter().all(u8::is_ascii_digit) {
            Some(code)
        } else {
            None
        }
    }

    pub fn quarter(&self) -> u8 {
        quarter_from_bao_gao(self.bao_gao)
    }

    pub fn metric_array(&self) -> [f32; 48] {
        [
            self.mg_shou_yi,
            self.mg_jing_zhi,
            self.jzcsyl,
            self.mg_xian_jin,
            self.mggjj,
            self.mgwfp,
            self.gdqybl,
            self.jlrtb,
            self.zysytb,
            self.xsmll,
            self.tz_jing_zhi,
            self.zong_zc,
            self.ldzc,
            self.gu_ding_zc,
            self.wu_xing_zc,
            self.ld_fu_zhai,
            self.cq_fu_zhai,
            self.zong_fu_zhai,
            self.quan_yi,
            self.zb_gong_ji,
            self.xian_jin,
            self.tzxjl,
            self.czxjl,
            self.xjzje,
            self.shou_ru,
            self.zy_li_run,
            self.yy_li_run,
            self.tz_shou_yi,
            self.yyw_shou_zhi,
            self.zong_li_run,
            self.jing_li_run,
            self.wei_fen_pei,
            self.zong_gu,
            self.wxs_ag,
            self.liu_tong_ag,
            self.b_gu,
            self.jingwai_gu,
            self.qtltg,
            self.xsghj,
            self.guojia_gu,
            self.faren_gu,
            self.jn_fa_ren_gu,
            self.jn_zi_ran_gu,
            self.qita_gu,
            self.muji_gu,
            self.jing_wai_gu,
            self.jw_zi_ran_gu,
            self.youxian_gu,
        ]
    }

    pub fn getter_value(&self, field_id: u8) -> Option<f32> {
        match field_id {
            0x2a => Some(self.quarter() as f32),
            0x2b => Some(self.zong_gu),
            0x2c => Some(self.liu_tong_ag),
            // These ids are explicit vendor zero cases in the current samples.
            0x30 | 0x34 | 0x36 | 0x3c | 0x43 => Some(0.0),
            // These ids still do not map to a stable field in current traces.
            0x2d | 0x2e | 0x2f | 0x31 | 0x32 | 0x37 | 0x38 | 0x39 | 0x45 => None,
            0x33 => Some(self.b_gu),
            0x35 => Some(self.mg_shou_yi),
            0x3a => Some(self.zong_zc),
            0x3b => Some(self.ldzc),
            0x3d => Some(self.gu_ding_zc),
            0x3e => Some(self.wu_xing_zc),
            0x3f => Some(self.ld_fu_zhai),
            0x40 => Some(self.cq_fu_zhai),
            0x41 => Some(self.quan_yi),
            0x42 => Some(self.zb_gong_ji),
            0x44 => Some(self.mg_jing_zhi),
            0x46 => Some(self.shou_ru),
            0x47 => Some(self.zy_li_run),
            0x48 => Some(self.yyw_shou_zhi),
            0x49 => Some(self.zong_li_run),
            0x4a => Some(self.jing_li_run),
            0x4b => Some(self.wei_fen_pei),
            _ => None,
        }
    }
}

pub fn quarter_from_bao_gao(bao_gao: u32) -> u8 {
    let month = ((bao_gao % 10_000) / 100) as u8;
    let quarter = month / 3;
    if quarter == 0 { 4 } else { quarter }
}

pub fn fin_field_specs() -> &'static [FinFieldSpec] {
    &FIN_FIELD_SPECS
}

pub fn fin_getter_specs() -> impl Iterator<Item = &'static FinGetterSpec> {
    FIN_GETTER_SPECS.iter().chain(FIN_GETTER_TAIL_SPECS.iter())
}

pub fn fin_getter_gap_specs() -> impl Iterator<Item = &'static FinGetterSpec> {
    FIN_GETTER_GAP_SPECS.iter()
}

pub fn fin_getter_unresolved_ids() -> &'static [u8; 9] {
    &FIN_GETTER_UNRESOLVED_IDS
}

pub fn fin_getter_spec(field_id: u8) -> Option<&'static FinGetterSpec> {
    fin_getter_specs().find(|spec| spec.field_id == field_id)
}

pub fn parse_fin_file(path: impl AsRef<Path>) -> Result<TdxFinFile, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    parse_fin_bytes(&bytes)
}

pub fn parse_fin_bytes(bytes: &[u8]) -> Result<TdxFinFile, Box<dyn Error>> {
    if bytes.len() < 8 {
        return Err(format!("FIN file too short: {}", bytes.len()).into());
    }

    let magic = read_u32(bytes, 0);
    if magic != FIN_FILE_MAGIC {
        return Err(
            format!("unexpected FIN magic 0x{magic:08x}, expected 0x{FIN_FILE_MAGIC:08x}").into(),
        );
    }

    let record_size = read_u32(bytes, 4) as usize;
    if record_size < FIN_MIN_RECORD_SIZE {
        return Err(format!(
            "unexpected FIN record_size {record_size}, expected >= {FIN_MIN_RECORD_SIZE}"
        )
        .into());
    }

    let payload = &bytes[8..];
    let count = payload.len() / record_size;
    let used = count * record_size;
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let start = index * record_size;
        let end = start + record_size;
        records.push(parse_fin_record(&payload[start..end])?);
    }

    Ok(TdxFinFile {
        magic,
        record_size: record_size as u32,
        records,
        trailing_bytes: payload[used..].to_vec(),
    })
}

pub fn write_fin_csv(
    path: impl AsRef<Path>,
    records: &[TdxFinRecord],
) -> Result<(), Box<dyn Error>> {
    let mut out = String::from(
        "symbol,market,code,time,bao_gao,quarter,shang_shi,\
mg_shou_yi,mg_jing_zhi,jzcsyl,mg_xian_jin,mggjj,mgwfp,gdqybl,jlrtb,zysytb,xsmll,\
tz_jing_zhi,zong_zc,ldzc,gu_ding_zc,wu_xing_zc,ld_fu_zhai,cq_fu_zhai,zong_fu_zhai,\
quan_yi,zb_gong_ji,xian_jin,tzxjl,czxjl,xjzje,shou_ru,zy_li_run,yy_li_run,tz_shou_yi,\
yyw_shou_zhi,zong_li_run,jing_li_run,wei_fen_pei,zong_gu,wxs_ag,liu_tong_ag,b_gu,\
jingwai_gu,qtltg,xsghj,guojia_gu,faren_gu,jn_fa_ren_gu,jn_zi_ran_gu,qita_gu,muji_gu,\
jing_wai_gu,jw_zi_ran_gu,youxian_gu,trailer_hex\n",
    );

    for record in records {
        let row = vec![
            csv_escape(&record.symbol),
            csv_escape(record.market().unwrap_or("")),
            csv_escape(record.code().unwrap_or("")),
            record.time.to_string(),
            record.bao_gao.to_string(),
            record.quarter().to_string(),
            record.shang_shi.to_string(),
            record.mg_shou_yi.to_string(),
            record.mg_jing_zhi.to_string(),
            record.jzcsyl.to_string(),
            record.mg_xian_jin.to_string(),
            record.mggjj.to_string(),
            record.mgwfp.to_string(),
            record.gdqybl.to_string(),
            record.jlrtb.to_string(),
            record.zysytb.to_string(),
            record.xsmll.to_string(),
            record.tz_jing_zhi.to_string(),
            record.zong_zc.to_string(),
            record.ldzc.to_string(),
            record.gu_ding_zc.to_string(),
            record.wu_xing_zc.to_string(),
            record.ld_fu_zhai.to_string(),
            record.cq_fu_zhai.to_string(),
            record.zong_fu_zhai.to_string(),
            record.quan_yi.to_string(),
            record.zb_gong_ji.to_string(),
            record.xian_jin.to_string(),
            record.tzxjl.to_string(),
            record.czxjl.to_string(),
            record.xjzje.to_string(),
            record.shou_ru.to_string(),
            record.zy_li_run.to_string(),
            record.yy_li_run.to_string(),
            record.tz_shou_yi.to_string(),
            record.yyw_shou_zhi.to_string(),
            record.zong_li_run.to_string(),
            record.jing_li_run.to_string(),
            record.wei_fen_pei.to_string(),
            record.zong_gu.to_string(),
            record.wxs_ag.to_string(),
            record.liu_tong_ag.to_string(),
            record.b_gu.to_string(),
            record.jingwai_gu.to_string(),
            record.qtltg.to_string(),
            record.xsghj.to_string(),
            record.guojia_gu.to_string(),
            record.faren_gu.to_string(),
            record.jn_fa_ren_gu.to_string(),
            record.jn_zi_ran_gu.to_string(),
            record.qita_gu.to_string(),
            record.muji_gu.to_string(),
            record.jing_wai_gu.to_string(),
            record.jw_zi_ran_gu.to_string(),
            record.youxian_gu.to_string(),
            csv_escape(&hex_line(&record.trailer)),
        ];
        out.push_str(&row.join(","));
        out.push('\n');
    }

    fs::write(path, out)?;
    Ok(())
}

fn parse_fin_record(record: &[u8]) -> Result<TdxFinRecord, Box<dyn Error>> {
    if record.len() < FIN_MIN_RECORD_SIZE {
        return Err(format!(
            "FIN record too short: {} < {FIN_MIN_RECORD_SIZE}",
            record.len()
        )
        .into());
    }

    let mut key_raw = [0u8; FIN_KEY_SIZE];
    key_raw.copy_from_slice(&record[..FIN_KEY_SIZE]);

    Ok(TdxFinRecord {
        key_raw,
        symbol: decode_symbol_slot(&key_raw),
        time: read_u32(record, 0x0c),
        bao_gao: read_u32(record, 0x10),
        shang_shi: read_u32(record, 0x14),
        mg_shou_yi: read_f32(record, 0x18),
        mg_jing_zhi: read_f32(record, 0x1c),
        jzcsyl: read_f32(record, 0x20),
        mg_xian_jin: read_f32(record, 0x24),
        mggjj: read_f32(record, 0x28),
        mgwfp: read_f32(record, 0x2c),
        gdqybl: read_f32(record, 0x30),
        jlrtb: read_f32(record, 0x34),
        zysytb: read_f32(record, 0x38),
        xsmll: read_f32(record, 0x3c),
        tz_jing_zhi: read_f32(record, 0x40),
        zong_zc: read_f32(record, 0x44),
        ldzc: read_f32(record, 0x48),
        gu_ding_zc: read_f32(record, 0x4c),
        wu_xing_zc: read_f32(record, 0x50),
        ld_fu_zhai: read_f32(record, 0x54),
        cq_fu_zhai: read_f32(record, 0x58),
        zong_fu_zhai: read_f32(record, 0x5c),
        quan_yi: read_f32(record, 0x60),
        zb_gong_ji: read_f32(record, 0x64),
        xian_jin: read_f32(record, 0x68),
        tzxjl: read_f32(record, 0x6c),
        czxjl: read_f32(record, 0x70),
        xjzje: read_f32(record, 0x74),
        shou_ru: read_f32(record, 0x78),
        zy_li_run: read_f32(record, 0x7c),
        yy_li_run: read_f32(record, 0x80),
        tz_shou_yi: read_f32(record, 0x84),
        yyw_shou_zhi: read_f32(record, 0x88),
        zong_li_run: read_f32(record, 0x8c),
        jing_li_run: read_f32(record, 0x90),
        wei_fen_pei: read_f32(record, 0x94),
        zong_gu: read_f32(record, 0x98),
        wxs_ag: read_f32(record, 0x9c),
        liu_tong_ag: read_f32(record, 0xa0),
        b_gu: read_f32(record, 0xa4),
        jingwai_gu: read_f32(record, 0xa8),
        qtltg: read_f32(record, 0xac),
        xsghj: read_f32(record, 0xb0),
        guojia_gu: read_f32(record, 0xb4),
        faren_gu: read_f32(record, 0xb8),
        jn_fa_ren_gu: read_f32(record, 0xbc),
        jn_zi_ran_gu: read_f32(record, 0xc0),
        qita_gu: read_f32(record, 0xc4),
        muji_gu: read_f32(record, 0xc8),
        jing_wai_gu: read_f32(record, 0xcc),
        jw_zi_ran_gu: read_f32(record, 0xd0),
        youxian_gu: read_f32(record, 0xd4),
        trailer: record[0xd8..].to_vec(),
    })
}

fn decode_symbol_slot(bytes: &[u8; FIN_KEY_SIZE]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&bytes[offset..offset + 4]);
    u32::from_le_bytes(raw)
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32(bytes, offset))
}

fn csv_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

fn hex_line(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        FIN_FILE_MAGIC, FIN_GETTER_GAP_SPECS, FIN_GETTER_UNRESOLVED_IDS, FIN_MIN_RECORD_SIZE,
        TdxFinRecord, fin_getter_gap_specs, fin_getter_unresolved_ids, parse_fin_bytes,
        parse_fin_record, quarter_from_bao_gao,
    };

    #[test]
    fn parses_synthetic_fin_file() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FIN_FILE_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&(FIN_MIN_RECORD_SIZE as u32).to_le_bytes());
        bytes.extend_from_slice(&build_record(
            "SH600000", 20260328, 20241231, 19991210, 1.23,
        ));
        bytes.extend_from_slice(&build_record(
            "SZ000001", 20260329, 20240930, 19910403, 2.34,
        ));
        bytes.extend_from_slice(&[0xaa, 0xbb]);

        let parsed = parse_fin_bytes(&bytes).expect("parse synthetic FIN");
        assert_eq!(parsed.record_size as usize, FIN_MIN_RECORD_SIZE);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.trailing_bytes, vec![0xaa, 0xbb]);

        let first = &parsed.records[0];
        assert_eq!(first.symbol, "SH600000");
        assert_eq!(first.market(), Some("SH"));
        assert_eq!(first.code(), Some("600000"));
        assert_eq!(first.time, 20260328);
        assert_eq!(first.bao_gao, 20241231);
        assert_eq!(first.quarter(), 4);
        assert!((first.mg_shou_yi - 1.23).abs() < 0.0001);
        assert!((first.mg_jing_zhi - 4.56).abs() < 0.0001);
        assert!((first.zong_gu - 1200.0).abs() < 0.0001);
        assert_eq!(first.trailer, b"TRAILER!".to_vec());

        let second = &parsed.records[1];
        assert_eq!(second.symbol, "SZ000001");
        assert_eq!(second.market(), Some("SZ"));
        assert_eq!(second.code(), Some("000001"));
        assert_eq!(second.quarter(), 3);
        assert_metric_array_shape(second);
    }

    #[test]
    fn rejects_short_record_size() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FIN_FILE_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&((FIN_MIN_RECORD_SIZE - 1) as u32).to_le_bytes());
        let err = parse_fin_bytes(&bytes).expect_err("record_size should be rejected");
        assert!(err.to_string().contains("record_size"));
    }

    #[test]
    fn quarter_helper_matches_vendor_behavior() {
        assert_eq!(quarter_from_bao_gao(20240331), 1);
        assert_eq!(quarter_from_bao_gao(20240630), 2);
        assert_eq!(quarter_from_bao_gao(20240930), 3);
        assert_eq!(quarter_from_bao_gao(20241231), 4);
        assert_eq!(quarter_from_bao_gao(20240000), 4);
    }

    #[test]
    fn getter_value_matches_confirmed_cases() {
        let record = parse_fin_record(&build_record(
            "SH600000", 20260328, 20241231, 19991210, 1.23,
        ))
        .expect("record");

        assert_eq!(record.getter_value(0x2a), Some(4.0));
        assert_eq!(record.getter_value(0x2b), Some(record.zong_gu));
        assert_eq!(record.getter_value(0x35), Some(record.mg_shou_yi));
        assert_eq!(record.getter_value(0x44), Some(record.mg_jing_zhi));
        assert_eq!(record.getter_value(0x30), Some(0.0));
        assert_eq!(record.getter_value(0x45), None);
    }

    #[test]
    fn getter_gaps_are_explicit_and_stable() {
        assert_eq!(fin_getter_unresolved_ids(), &FIN_GETTER_UNRESOLVED_IDS);
        assert_eq!(fin_getter_gap_specs().count(), FIN_GETTER_GAP_SPECS.len());

        let record = parse_fin_record(&build_record(
            "SH600000", 20260328, 20241231, 19991210, 1.23,
        ))
        .expect("record");

        for field_id in FIN_GETTER_UNRESOLVED_IDS {
            assert_eq!(
                record.getter_value(field_id),
                None,
                "field_id=0x{field_id:02x}"
            );
        }
    }

    fn assert_metric_array_shape(record: &TdxFinRecord) {
        let metrics = record.metric_array();
        assert_eq!(metrics.len(), 48);
        assert!((metrics[0] - record.mg_shou_yi).abs() < 0.0001);
        assert!((metrics[32] - record.zong_gu).abs() < 0.0001);
        assert!((metrics[47] - record.youxian_gu).abs() < 0.0001);
    }

    fn build_record(
        symbol: &str,
        time: u32,
        bao_gao: u32,
        shang_shi: u32,
        mg_shou_yi: f32,
    ) -> Vec<u8> {
        let mut record = vec![0u8; FIN_MIN_RECORD_SIZE];
        record[..symbol.len()].copy_from_slice(symbol.as_bytes());

        write_u32(&mut record, 0x0c, time);
        write_u32(&mut record, 0x10, bao_gao);
        write_u32(&mut record, 0x14, shang_shi);

        let mut value = mg_shou_yi;
        for offset in (0x18..=0xd4).step_by(4) {
            write_f32(&mut record, offset, value);
            value += 3.33;
        }

        write_f32(&mut record, 0x1c, 4.56);
        write_f32(&mut record, 0x98, 1200.0);
        record[0xd8..0xe0].copy_from_slice(b"TRAILER!");
        record
    }

    fn write_u32(record: &mut [u8], offset: usize, value: u32) {
        record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_f32(record: &mut [u8], offset: usize, value: f32) {
        record[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
}
