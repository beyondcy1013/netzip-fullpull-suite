#![forbid(unsafe_code)]

//! Rust replication boundary for Netzip's authenticated vendor full-push chain.
//!
//! `fullpull` corresponds to the official full-push behavior observed in `quoteNetzipWine`:
//! authenticated, non-7709 vendor data connections that continuously deliver market updates.
//! 7709 query-based supplementation belongs to the sibling `netzip-supplement` crate.
//!
//! The modules below still include 7709/0547 compatibility code inherited from the former
//! `netzip-native` crate. That is transitional physical layout, not the long-term ownership model
//! and not evidence that the authenticated full-push replication is complete.

pub mod auth_7100;
pub mod auth_manifest;
pub mod client;
pub mod official_5188;
pub mod slot_supervisor;
pub mod tdx7709;
pub mod tdx_0547;
pub mod tdx_0547_delivery;
pub mod tdx_0547_scheduler;
pub mod tdx_fin;
pub mod tdx_push_coalescer;
pub mod tdx_push_poll_policy;
pub mod tdx_wire_finance;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialQuoteEndpoint {
    pub host: IpAddr,
    pub port: u16,
}

impl OfficialQuoteEndpoint {
    pub fn new(host: IpAddr, port: u16) -> Result<Self, String> {
        if port == 7709 || port == 547 {
            return Err(format!(
                "supplement endpoint port is not valid for official 5188: {port}"
            ));
        }
        if port == 0 {
            return Err("official quote endpoint port cannot be zero".into());
        }
        Ok(Self { host, port })
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub use client::{
    DriverProbe, NativeDriverConfig, NativeInstrument, NativeMarket, NativeQuote, NativeSession,
};
pub use official_5188::{
    OFFICIAL_5188_BULK_BODY_LEN, OFFICIAL_5188_INTERNAL_RECORD_LEN,
    OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT,
    OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN, Official5188BaselineResolver,
    Official5188BitReader, Official5188BulkEnvelope, Official5188BulkStream,
    Official5188ClientSessionEnvelope, Official5188CodeTable, Official5188CodeTableHeader,
    Official5188CodeTableRecord, Official5188DecodedValueRecord, Official5188DeltaEnvelope,
    Official5188DeltaIndexState, Official5188DeltaStreams, Official5188Direction,
    Official5188EmbeddedClientFrame, Official5188EmbeddedZlibCandidate, Official5188Frame,
    Official5188Handshake, Official5188InitialCodeTables, Official5188InitializationStage,
    Official5188InternalRecord, Official5188Kind, Official5188MapBaselineResolver,
    Official5188OemState, Official5188PartialValueDecode, Official5188PayloadHint,
    Official5188PrimarySubscriptionPartitions, Official5188PublicQuote, Official5188Reassembler,
    Official5188ReceiveListPartitions, Official5188Session, Official5188SubscriptionEntry,
    Official5188SubscriptionEnvelope, Official5188SubscriptionRange, Official5188Token,
    Official5188TracedPartialValueDecode, Official5188ValueDecodeTrace,
    Official5188ValueRecordHeader, Official5188ZlibObjectEnvelope, assemble_bulk_envelopes,
    build_official_5188_client_session_triplet,
    build_official_5188_primary_subscription_partitions,
    build_official_5188_receive_list_partitions, decode_official_5188_delta_indexes,
    decode_official_5188_token, decode_official_5188_token_with_operation,
    decode_official_5188_values, decode_official_5188_values_native_wine_clamp_with_trace,
    decode_official_5188_values_partial_with_fresh_fallback,
    decode_official_5188_values_partial_with_trace,
    decode_official_5188_values_partial_with_wine_tail_trace,
    decode_official_5188_values_with_fresh_fallback, embedded_client_frames,
    embedded_zlib_candidates, official_5188_ack_source_prefix, official_5188_value_token_table,
    parse_socket_endpoint, payload_hint,
};
pub use tdx_0547::{
    Tdx0547Body, Tdx0547QuoteHead, Tdx0547Record,
    extra0_time_hint_seconds as tdx_0547_extra0_time_hint_seconds,
    format_extra0_time_hint as tdx_0547_format_extra0_time_hint,
    format_hhmmss_raw as tdx_0547_format_hhmmss_raw,
    format_public_hhmmss_raw as tdx_0547_format_public_hhmmss_raw,
    hhmmss_raw_to_seconds as tdx_0547_hhmmss_raw_to_seconds,
    is_valid_hhmmss_raw as tdx_0547_is_valid_hhmmss_raw, market_name as tdx_0547_market_name,
    matches_tdx_0547_query, normalize_quote_head_by_decimal_point as tdx_0547_normalize_quote_head,
    parse_tdx_0547_body, public_time_hhmmss as tdx_0547_public_time_hhmmss, query_tdx_0547_records,
    record_symbol as tdx_0547_record_symbol, xor93_decode as tdx_0547_xor93_decode,
};
pub use tdx_fin::{
    FIN_EXPORTED_PAYLOAD_LEN, FIN_EXPORTED_PAYLOAD_OFFSET, FIN_FILE_MAGIC, FIN_GETTER_GAP_SPECS,
    FIN_GETTER_SPECS, FIN_GETTER_TAIL_SPECS, FIN_GETTER_UNRESOLVED_IDS, FIN_KEY_SIZE,
    FIN_MIN_RECORD_SIZE, FinGetterSpec, SH_FIN_URL, SZ_FIN_URL, TdxFinFile, TdxFinRecord,
    fin_getter_gap_specs, fin_getter_specs, fin_getter_unresolved_ids, parse_fin_bytes,
    parse_fin_file, quarter_from_bao_gao, write_fin_csv,
};
pub use tdx_wire_finance::{
    TdxWireFinanceRecord, WIRE_FINANCE_FLOAT_FIELDS, WIRE_FINANCE_RECORD_SIZE,
    parse_wire_finance_batch_body, parse_wire_finance_record,
};
pub use tdx7709::{
    BOOTSTRAP0_BODY_HEX as TDX7709_BOOTSTRAP0_BODY_HEX,
    BOOTSTRAP2_BODY_HEX as TDX7709_BOOTSTRAP2_BODY_HEX, DEFAULT_HOST as TDX7709_DEFAULT_HOST,
    DEFAULT_PORT as TDX7709_DEFAULT_PORT, PROBE_HELLO_HEX as TDX7709_PROBE_HELLO_HEX,
    Tdx7709BootstrapPacket, Tdx7709CodeTableRecord, Tdx7709Config, Tdx7709F10CategoriesResult,
    Tdx7709F10Category, Tdx7709F10ContentResult, Tdx7709KlineBar, Tdx7709KlineResult,
    Tdx7709LiveQuoteResult, Tdx7709QuoteObservation, Tdx7709QuoteRequestItem, Tdx7709ServerFrame,
    Tdx7709Session, Tdx7709SyncResult, Tdx7709TimedQuoteDelivery,
    ascii_preview_from_zlib as tdx7709_ascii_preview_from_zlib, build_bootstrap_packets,
    build_probe_hello, fetch_f10_categories, fetch_f10_content, fetch_kline, fetch_live_quotes,
    open_tdx7709_session, sync_code_table, write_code_table_csv,
};
