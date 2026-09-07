//! Live probe for 7709 history transaction ticks.
//!
//! Requests one bounded tick page from the third-party server and prints the
//! decoded ticks next to the raw reply head, so the wire layout can be
//! verified against real bytes. Read-only diagnostics, no credentials.

use netzip_fullpull::tdx7709::{Tdx7709Config, Tdx7709Session};

fn main() {
    let endpoint = std::env::var("NETZIP_SUPPLEMENT_7709")
        .unwrap_or_else(|_| "120.195.71.160:7709".to_string());
    let (host, port) = endpoint.rsplit_once(':').expect("host:port");
    let config = Tdx7709Config {
        host: host.to_string(),
        port: port.parse().expect("port"),
        ..Tdx7709Config::default()
    };
    let mut session = Tdx7709Session::open(&config).expect("open session");

    for (market, code) in [(1u8, "600000"), (0u8, "000001")] {
        let result = match session.request_history_transactions(market, code, 20_260_903, 0, 10) {
            Ok(result) => result,
            Err(error) => {
                println!("market={market} {code} failed: {error}");
                continue;
            }
        };
        println!(
            "market={market} {code} ticks={} reply-head={}",
            result.ticks.len(),
            hex(&result.transactions_reply[..result.transactions_reply.len().min(48)])
        );
        for tick in &result.ticks {
            println!(
                "  {} {} price={:.2} vol={} bos={}",
                tick.date, tick.time, tick.price, tick.volume, tick.buyorsell,
            );
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
