//! 集合竞价窗口(09:24:50 ~ 09:31:10)原始 TDX quote 采样探针。
//!
//! 目的：用 rustHq 最底层的 TdxHqApi::real 直连服务器，绕过 quote-gateway 的
//! batch/manager/minute-cache 门控层，证明 TDX 在 09:25 是否返回非零有效字段。
//! 输出统一的 key=value 行，便于和 pytdx 侧脚本 diff 比对。
//!
//! 详见 docs/history/2026-07-07_0925集合竞价无效参与数据追溯与双库验证方案.md。

use dllhqarrow_rs::{TaskData, TdxHqApi, default_servers, market_for_code};
use std::env;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_CODES: &[&str] = &["000001", "600000", "300750", "600519"];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let servers = arg_value(&args, "--servers")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| default_servers().into_iter().take(4).collect());
    let codes = arg_value(&args, "--codes")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| DEFAULT_CODES.iter().map(|s| s.to_string()).collect());
    let interval_ms = arg_value(&args, "--interval-ms")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1000);
    let timeout_ms = arg_value(&args, "--timeout-ms")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(6000);

    eprintln!(
        "auction_window_quote_probe impl=rust servers={} codes={} interval_ms={}",
        servers.len(),
        codes.len(),
        interval_ms
    );
    eprintln!("probe_codes {}", codes.join(","));

    // 可选：等待到指定时间再开始采样。
    if let Some(start_at) = arg_value(&args, "--start-at") {
        wait_until_local(&start_at);
    }
    let end_at = arg_value(&args, "--end-at");

    let quote_stocks: Vec<(u8, String)> = codes
        .iter()
        .map(|c| (market_for_code(c), c.clone()))
        .collect();

    loop {
        let now = chrono::Local::now();
        let now_text = now.format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        if let Some(ref end) = end_at
            && passed_end(&now_text, end)
        {
            eprintln!("probe_end reached end_at={end}");
            break;
        }
        for server in &servers {
            probe_one_server(server, &quote_stocks, &now_text, timeout_ms);
        }
        thread::sleep(Duration::from_millis(interval_ms));
    }
}

fn probe_one_server(server: &str, stocks: &[(u8, String)], now_text: &str, timeout_ms: u64) {
    let api = TdxHqApi::real(server);
    let connect_started = Instant::now();
    if let Err(err) = api.connect_to_server() {
        println!(
            "probe_error impl=rust server={server} connect_ms={} error=connect_failed err={err}",
            connect_started.elapsed().as_millis()
        );
        return;
    }
    let connect_ms = connect_started.elapsed().as_millis();

    let task = dllhqarrow_rs::Task::quote(stocks.to_vec());
    let started = Instant::now();
    let result = api.execute_task_with_read_timeout(&task, Some(Duration::from_millis(timeout_ms)));
    let quote_ms = started.elapsed().as_millis();
    api.disconnect();

    let server_used = result.server_used.clone();
    let quotes = match &result.data {
        TaskData::Quotes(values) => values.clone(),
        _ => Vec::new(),
    };
    if !result.success || quotes.is_empty() {
        println!(
            "probe_error impl=rust server={server} connect_ms={connect_ms} quote_ms={quote_ms} success={} err={} quote_count={}",
            result.success,
            result.error_message,
            quotes.len()
        );
        return;
    }
    for quote in &quotes {
        let retry_invalid = quote.price <= 0.0
            && quote.last_close <= 0.0
            && quote.volume <= 0.0
            && quote.amount <= 0.0;
        let st = quote.servertime.as_deref().unwrap_or("");
        let seeds_minute_cache = seeds_minute_bar_time(st);
        let is_auction_window = in_auction_window(st);
        let is_pre_auction = st < "09:25:00" && !st.is_empty();
        let is_post_open = st >= "09:30:00";
        println!(
            "probe_tick impl=rust server={server} server_used={server_used} \
             code={} market={} servertime={st} \
             price={} last_close={} open={} high={} low={} \
             vol={} cur_vol={} amount={} active1={} \
             retry_invalid={retry_invalid} seeds_minute_cache={seeds_minute_cache} \
             is_auction_window={is_auction_window} is_pre_auction={is_pre_auction} \
             is_post_open={is_post_open} connect_ms={connect_ms} quote_ms={quote_ms} \
             sampled_at_local={now_text}",
            quote.code,
            quote.market,
            quote.price,
            quote.last_close,
            quote.open,
            quote.high,
            quote.low,
            quote.volume,
            quote.current_volume,
            quote.amount,
            quote.active1,
        );
    }
}

// 与 quote-gateway is_valid_minute_bar_time 对齐的口径（不依赖 chrono 时区）。
// 有效分钟 bar 时段：09:31~11:30、13:01~15:00。
fn seeds_minute_bar_time(servertime: &str) -> bool {
    let t = servertime.trim();
    if t.len() < 5 {
        return false;
    }
    let hhmm = &t[..5];
    matches!(
        hhmm.cmp("09:31"),
        std::cmp::Ordering::Less | std::cmp::Ordering::Equal
    ) && ("09:31"..="11:30").contains(&hhmm)
        || ("13:01"..="15:00").contains(&hhmm)
}

// 正式集合竞价撮合窗口：09:25:00 ~ 09:25:59。
fn in_auction_window(servertime: &str) -> bool {
    let t = servertime.trim();
    ("09:25:00".."09:26:00").contains(&t)
}

fn wait_until_local(target_text: &str) {
    // 仅支持 HH:MM:SS 或 HH:MM:SS.fff，按今天日期拼。
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let full = if target_text.len() <= 12 {
        format!("{today} {target_text}")
    } else {
        target_text.to_string()
    };
    match chrono::NaiveDateTime::parse_from_str(&full, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&full, "%Y-%m-%d %H:%M:%S%.f"))
    {
        Ok(target_ndt) => {
            let now = chrono::Local::now();
            let target_local = chrono::TimeZone::from_local_datetime(&chrono::Local, &target_ndt)
                .single()
                .unwrap_or(now);
            if target_local > now {
                let dur = (target_local - now).to_std().unwrap_or(Duration::ZERO);
                eprintln!(
                    "probe_wait target={} sleep_ms={}",
                    target_ndt,
                    dur.as_millis()
                );
                thread::sleep(dur);
            }
        }
        Err(err) => {
            eprintln!("probe_wait ignored invalid --start-at={target_text} err={err}");
        }
    }
}

fn passed_end(now_text: &str, end: &str) -> bool {
    let now_short = &now_text[11..];
    let end_short = if end.len() <= 12 { end } else { &end[11..] };
    now_short >= end_short
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
