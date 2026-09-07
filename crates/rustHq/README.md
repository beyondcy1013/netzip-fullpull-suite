# dllhqarrow-rs

Rust reconstruction of the Qt/C++ `dllhqarrow` project located at:

`/home/beyondcy/BaiduSyncdisk/QT_Stock/_third_party/dllhqarrow`

This first pass keeps the original architecture shape while replacing Qt/Arrow runtime pieces with standard Rust data structures and a deterministic mock transport so the whole pipeline can compile, run, and be tested locally.

## What Is Recreated

- `TdxHqApi` abstraction with a pluggable `TdxClient` trait
- `TdxDataManager` worker pool, task queues, batch handles, and result draining
- `TdxDataService` orchestration layer and sync helper methods
- `StockDataHub` cache-first update hub
- `TdxPacket` request builders aligned with the original packet headers
- real `TcpStream` client with three-way handshake, response header parsing, zlib body decode, reconnect retry and worker heartbeat checks
- startup-time real-server validation/ranking and runtime server health tracking with cooldown, worker-side candidate replacement, configurable quality policy, and optional persisted server history
- K-line resampling and minute-to-daily feature generation
- Equal-weight sector index calculation
- C ABI entry points compatible with the original `tdx_c_api.h` shape

## Current Simplifications

- default mode still uses `MockClient`
- real mode currently covers all currently implemented task types: K-line, quote, security-list, minute-time, F10/company info, finance info and block info
- server quality thresholds can be tuned via `ManagerConfig` or environment variables such as `TDX_SLOW_LATENCY_THRESHOLD_MS`, `TDX_FAILURE_COOLDOWN_MS`, `TDX_SERVER_CANDIDATE_MULTIPLIER`, and `TDX_SERVER_HEALTH_HISTORY_PATH`
- Arrow and DuckDB integration are intentionally deferred
- Qt signal/slot behavior is modeled with handles, locks, and worker threads
- Generic datasets use `BTreeMap<String, String>` rows for now

## Source Mapping

- Original `tdx_hq_api.*` -> [`src/api.rs`](./src/api.rs)
- Original `tdx_data_manager.*` -> [`src/manager.rs`](./src/manager.rs)
- Original `tdx_data_service.*` -> [`src/service.rs`](./src/service.rs)
- Original `stock_data_hub.*` -> [`src/hub.rs`](./src/hub.rs)
- Original `tools_tdx_packet.*` -> [`src/packet.rs`](./src/packet.rs)
- Original `tools_kline_resampler.*` -> [`src/resampler.rs`](./src/resampler.rs)
- Original `sector_index_calculator.*` -> [`src/sector_index.rs`](./src/sector_index.rs)
- Original `tdx_c_api.*` -> [`src/c_api.rs`](./src/c_api.rs)

## Run

```bash
cargo test
cargo run --bin demo
TDX_CLIENT_MODE=real cargo run --bin demo
TDX_CLIENT_MODE=real TDX_SERVER_HEALTH_HISTORY_PATH=./tdx_server_health.tsv cargo run --bin demo
```

## Next Migration Steps

1. Tighten replacement heuristics further with richer latency windows and historical success-rate weighting.
2. Reintroduce Arrow tables behind feature flags or a dedicated storage layer.
3. Add DuckDB query utilities once the columnar layer is in place.
4. Tighten protocol edge cases and richer field normalization around real-time payloads.
