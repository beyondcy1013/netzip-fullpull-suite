use dllhqarrow_rs::table::DayBar;
use dllhqarrow_rs::{
    ClientMode, KlineType, StockDataHub, TdxDataService, calculate_equal_weight_index,
    compute_daily_indicators, resample_minute_to_daily_with_features,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let service = Arc::new(TdxDataService::new());
    let client_mode = ClientMode::from_env();
    service.set_client_mode(client_mode);
    let mut manager_config = service.manager_config();
    if client_mode == ClientMode::Real && manager_config.health_history_path.is_none() {
        manager_config.health_history_path = Some(PathBuf::from("tdx_server_health.tsv"));
        service.set_manager_config(manager_config.clone());
    }
    let stock_codes = vec![
        "000001".to_string(),
        "600000".to_string(),
        "300059".to_string(),
    ];

    if !service.initialize(&stock_codes, 4) {
        eprintln!("failed to initialize service");
        return;
    }

    let mut tasks = BTreeMap::new();
    tasks.insert("000001".to_string(), 120);
    tasks.insert("600000".to_string(), 120);

    println!(
        "== batch demo == mode={client_mode:?} candidates={} history={}",
        manager_config.candidate_multiplier,
        manager_config
            .health_history_path
            .as_ref()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
    let handle = service.start_kline_batch(&tasks, KlineType::Kline1Min);
    let _ = handle.wait_produced(Some(Duration::from_secs(2)));
    while let Some(result) = handle.try_take() {
        if result.success {
            println!(
                "{} success=true rows={} server={}",
                result.stock_code,
                result.kline_count(),
                result.server_used
            );
        } else {
            println!(
                "{} success=false server={} error={}",
                result.stock_code, result.server_used, result.error_message
            );
        }
    }

    println!("\n== quote demo ==");
    for quote in service.get_quotes(&["000001".to_string(), "600000".to_string()]) {
        println!(
            "{} market={} price={:.3} open={:.3} high={:.3} low={:.3} preclose={:.3} volume={:.0} amount={:.0}",
            quote.code,
            quote.market,
            quote.price,
            quote.open,
            quote.high,
            quote.low,
            quote.last_close,
            quote.volume,
            quote.amount
        );
    }

    println!("\n== security list demo ==");
    let securities = service.get_security_list(0);
    println!("market=0 securities={}", securities.len());
    for row in securities.iter().take(3) {
        println!(
            "{} {} pre_close={} decimal_point={} volunit={}",
            row.get("code").map(String::as_str).unwrap_or(""),
            row.get("name").map(String::as_str).unwrap_or(""),
            row.get("pre_close").map(String::as_str).unwrap_or(""),
            row.get("decimal_point").map(String::as_str).unwrap_or(""),
            row.get("volunit").map(String::as_str).unwrap_or("")
        );
    }

    println!("\n== minute time demo ==");
    let minute_rows = service.get_minute_time_data(0, "000001");
    println!("000001 minute_rows={}", minute_rows.len());
    for row in minute_rows.iter().take(3) {
        println!(
            "{} price={} vol={}",
            row.get("time").map(String::as_str).unwrap_or(""),
            row.get("price").map(String::as_str).unwrap_or(""),
            row.get("vol").map(String::as_str).unwrap_or("")
        );
    }
    if let Some(last) = minute_rows.last() {
        println!(
            "last {} price={} vol={}",
            last.get("time").map(String::as_str).unwrap_or(""),
            last.get("price").map(String::as_str).unwrap_or(""),
            last.get("vol").map(String::as_str).unwrap_or("")
        );
    }

    println!("\n== f10 demo ==");
    let categories = service.get_company_info_category(0, "000001");
    println!("000001 categories={}", categories.len());
    if let Some(first) = categories.first() {
        let filename = first.get("filename").cloned().unwrap_or_default();
        let start = first
            .get("start")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let length = first
            .get("length")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let content = service.get_company_info_content(0, "000001", &filename, start, length);
        println!(
            "first category={} file={} bytes={} preview={}",
            first.get("name").map(String::as_str).unwrap_or(""),
            filename,
            length,
            content
                .chars()
                .take(60)
                .collect::<String>()
                .replace('\n', " ")
        );
    }

    println!("\n== finance demo ==");
    let finance = service.get_finance_info(0, "000001");
    println!(
        "000001 ipo={} zongguben={} jingzichan={} zhuyingshouru={}",
        finance
            .fields
            .get("ipo_date_str")
            .map(String::as_str)
            .unwrap_or(""),
        finance
            .fields
            .get("zongguben")
            .map(String::as_str)
            .unwrap_or(""),
        finance
            .fields
            .get("jingzichan")
            .map(String::as_str)
            .unwrap_or(""),
        finance
            .fields
            .get("zhuyingshouru")
            .map(String::as_str)
            .unwrap_or("")
    );

    println!("\n== block demo ==");
    let blocks = service.get_block_info("block_gn.dat");
    println!("block_gn.dat blocks={}", blocks.len());
    if let Some(first) = blocks.first() {
        let sample = first
            .get("code_list")
            .map(String::as_str)
            .unwrap_or("")
            .split(',')
            .take(8)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{} type={} stocks={} sample={}",
            first.get("blockname").map(String::as_str).unwrap_or(""),
            first.get("block_type").map(String::as_str).unwrap_or(""),
            first.get("stock_count").map(String::as_str).unwrap_or(""),
            sample
        );
    }

    println!("\n== hub demo ==");
    let hub = StockDataHub::new(service.clone());
    let _ = hub.initialize(&stock_codes, 8, 4);
    hub.request_update_batch(&tasks, KlineType::Kline1Min);
    let _ = hub.wait_until_idle(Duration::from_secs(2));

    if let Some(table) = hub.get_data("000001") {
        println!("hub cached {} rows for {}", table.len(), table.code);
        let mut daily = resample_minute_to_daily_with_features(&table.rows, true, 0.0001);
        compute_daily_indicators(&mut daily);
        if let Some(first) = daily.first() {
            println!(
                "first daily row {} close={:.3} huitoubo={:?}",
                first.date, first.close, first.huitoubo
            );
        }
    }

    println!("\n== index demo ==");
    let index = calculate_equal_weight_index(
        &["000001".to_string(), "600000".to_string()],
        &[
            DayBar {
                code: "000001".to_string(),
                date: "2026-03-09".to_string(),
                open: 10.0,
                high: 10.2,
                low: 9.9,
                close: 10.0,
                volume: 100.0,
            },
            DayBar {
                code: "600000".to_string(),
                date: "2026-03-09".to_string(),
                open: 20.0,
                high: 20.3,
                low: 19.8,
                close: 20.0,
                volume: 120.0,
            },
            DayBar {
                code: "000001".to_string(),
                date: "2026-03-10".to_string(),
                open: 10.3,
                high: 10.6,
                low: 10.2,
                close: 10.5,
                volume: 110.0,
            },
            DayBar {
                code: "600000".to_string(),
                date: "2026-03-10".to_string(),
                open: 20.2,
                high: 20.4,
                low: 19.9,
                close: 20.1,
                volume: 130.0,
            },
        ],
        "ZS999",
        1000.0,
    );

    for row in index.rows.iter().take(2) {
        println!(
            "{} open={:.2} high={:.2} low={:.2} close={:.2}",
            row.date, row.open, row.high, row.low, row.close
        );
    }

    println!("\n== server health ==");
    for snapshot in service.get_server_health().iter().take(6) {
        println!(
            "{} selected={} validated={} connected={} ok={} fail={} avg_ms={} cooldown_ms={} error={}",
            snapshot.server,
            snapshot.selected,
            snapshot.validated,
            snapshot.connected,
            snapshot.success_count,
            snapshot.failure_count,
            snapshot
                .avg_latency_ms
                .map(|value| format!("{value:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            snapshot.cooldown_remaining_ms,
            snapshot.last_error
        );
    }

    println!("\nmanager stats: {:?}", service.get_statistics());
    service.shutdown();
}
