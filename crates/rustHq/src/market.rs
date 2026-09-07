pub const DEFAULT_TDX_SERVER_SOURCE: &str = "qt_dllhqarrow_2026-09-04_verified100";

pub const DEFAULT_TDX_SERVERS: &[&str] = &[
    "101.133.231.193",
    "101.230.66.1",
    "101.230.78.245",
    "103.221.142.65",
    "103.221.142.66",
    "103.221.142.67",
    "103.221.142.68",
    "103.221.142.69",
    "103.221.142.70",
    "103.221.142.71",
    "103.221.142.72",
    "103.221.142.82",
    "103.221.142.83",
    "103.251.85.90",
    "103.251.85.91",
    "103.251.85.92",
    "103.251.85.93",
    "103.251.85.94",
    "110.41.174.169",
    "111.15.15.43",
    "112.54.160.211",
    "114.118.82.142",
    "114.118.82.143",
    "114.118.82.144",
    "114.118.82.145",
    "114.118.82.207",
    "114.118.82.208",
    "114.118.82.209",
    "114.118.82.210",
    "114.118.82.211",
    "114.118.82.212",
    "114.118.82.213",
    "114.118.82.214",
    "114.118.82.215",
    "114.118.82.216",
    "115.238.56.198",
    "115.238.90.165",
    "117.149.2.68",
    "117.149.2.70",
    "117.185.6.39",
    "117.34.114.13",
    "117.34.114.14",
    "117.34.114.15",
    "117.34.114.16",
    "117.34.114.17",
    "117.34.114.18",
    "117.34.114.20",
    "117.34.114.27",
    "120.199.2.122",
    "120.199.2.123",
    "120.253.221.207",
    "121.33.228.164",
    "123.125.108.14",
    "123.125.108.213",
    "123.125.108.214",
    "123.125.108.90",
    "124.222.66.214",
    "124.95.141.11",
    "124.95.141.12",
    "124.95.141.13",
    "139.159.143.228",
    "139.159.183.76",
    "139.159.193.118",
    "139.159.195.177",
    "139.159.202.253",
    "139.9.43.104",
    "139.9.43.31",
    "139.9.50.246",
    "139.9.90.169",
    "148.70.110.41",
    "148.70.111.63",
    "148.70.31.16",
    "148.70.93.117",
    "175.6.5.153",
    "180.153.18.170",
    "182.118.47.151",
    "182.131.3.245",
    "183.131.224.21",
    "183.131.224.27",
    "202.100.166.27",
    "202.108.253.158",
    "210.21.65.136",
    "218.106.92.182",
    "218.106.92.183",
    "218.6.170.47",
    "218.75.126.9",
    "219.146.254.27",
    "220.178.55.71",
    "220.178.55.84",
    "220.178.55.86",
    "221.0.195.48",
    "222.180.170.163",
    "45.116.35.251",
    "58.211.17.22",
    "58.34.106.207",
    "58.63.254.191",
    "58.63.254.217",
    "59.36.5.11",
    "60.12.136.250",
    "60.191.117.167",
];

pub fn default_servers() -> Vec<String> {
    DEFAULT_TDX_SERVERS
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

pub fn market_for_code(code: &str) -> u8 {
    if code.starts_with("880") {
        return 1;
    }

    if matches!(&code.get(..2), Some("43" | "83" | "87" | "88" | "92")) {
        return 2;
    }

    if matches!(code.as_bytes().first().copied(), Some(b'5' | b'6' | b'9'))
        || code.starts_with("11")
        || code.starts_with("13")
        || code.starts_with("51")
        || code.starts_with("56")
        || code.starts_with("58")
        || code.starts_with("68")
    {
        return 1;
    }

    0
}

pub fn static_limit_ratio(code: &str, name: Option<&str>) -> f64 {
    if code.starts_with("30") || code.starts_with("68") {
        return 0.20;
    }
    if code.starts_with("43")
        || code.starts_with("83")
        || code.starts_with("87")
        || code.starts_with("92")
    {
        return 0.30;
    }
    if name.is_some_and(|value| value.contains("ST")) {
        return 0.05;
    }
    0.10
}

pub fn pick_servers(count: usize) -> Vec<String> {
    default_servers()
        .into_iter()
        .take(count.max(1))
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TDX_SERVERS, market_for_code, static_limit_ratio};

    #[test]
    fn market_detection_matches_common_prefixes() {
        assert_eq!(market_for_code("600000"), 1);
        assert_eq!(market_for_code("000001"), 0);
        assert_eq!(market_for_code("880001"), 1);
        assert_eq!(market_for_code("830001"), 2);
    }

    #[test]
    fn default_server_list_matches_qt_pool_size() {
        assert_eq!(DEFAULT_TDX_SERVERS.len(), 100);
    }

    #[test]
    fn limit_ratio_uses_board_and_st_rules() {
        assert_eq!(static_limit_ratio("300001", None), 0.20);
        assert_eq!(static_limit_ratio("830001", None), 0.30);
        assert_eq!(static_limit_ratio("000001", Some("*ST Demo")), 0.05);
    }
}
