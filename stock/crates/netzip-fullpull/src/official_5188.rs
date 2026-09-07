#![allow(clippy::obfuscated_if_else)]
#![allow(clippy::chunks_exact_to_as_chunks)]
#![allow(clippy::unnecessary_cast)]
#![allow(non_upper_case_globals)]

//!
//! The Wine capture shows a small client initialization sequence followed by a
//! long-lived server-driven stream. This module deliberately stops at the
//! evidence-backed frame boundary: payloads whose inner codec is not yet
//! reconstructed are retained verbatim instead of being interpreted as 7709.

use std::collections::VecDeque;
use std::fmt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

const HEADER_LEN: usize = 8;
const MAX_PAYLOAD_LEN: usize = 1_048_576;
pub const OFFICIAL_5188_BULK_BODY_LEN: usize = 5_120;
const OFFICIAL_5188_BULK_HEADER_LEN: usize = 12;
const OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN: usize = 10;
const OFFICIAL_5188_SUBSCRIPTION_ENTRY_LEN: usize = 6;
pub const OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT: usize = 5;
pub const OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN: usize = 1_024;
const OFFICIAL_5188_DELTA_PREFIX_LEN: usize = 6;
const OFFICIAL_5188_CODE_TABLE_HEADER_LEN: usize = 98;
const OFFICIAL_5188_CODE_TABLE_RECORD_LEN: usize = 68;
/// Size of the vendor's internal record passed through the 2704 value pass.
pub const OFFICIAL_5188_INTERNAL_RECORD_LEN: usize = 0x137;
const MAX_EMBEDDED_ZLIB_CANDIDATES: usize = 16;
const MAX_EMBEDDED_ZLIB_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_INITIAL_CODE_TABLE_FRAMES: usize = 256;

/// The five-bit header consumed by Wine's `0x449770` value decoder.
///
/// `level_count == 7` selects the decoder's special path.  The low bit is a
/// separate ladder-clear flag and must not be used as the branch condition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Official5188ValueRecordHeader {
    pub clear_ladder: bool,
    pub raw_level_count: u8,
    pub level_count: u8,
    pub has_book: bool,
    pub special_path: bool,
}

impl Official5188ValueRecordHeader {
    pub fn decode(header_bits: u8) -> Result<Self, String> {
        if header_bits > 0x1f {
            return Err(format!(
                "5188 value header must be five bits, got {header_bits:#x}"
            ));
        }
        let raw_level_count = (header_bits >> 2) & 0x07;
        Ok(Self {
            clear_ladder: header_bits & 1 != 0,
            raw_level_count,
            level_count: raw_level_count.min(4),
            has_book: header_bits & 2 != 0,
            special_path: raw_level_count == 7,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Official5188Kind(pub u16);

impl Official5188Kind {
    // Values are decoded little-endian integers. `wire_hex()` exposes the
    // capture spelling (for example, numeric 0x1036 is wire bytes 3610).
    pub const CLIENT_INIT: Self = Self(0x1036);
    pub const CLIENT_SESSION: Self = Self(0x102d);
    pub const CLIENT_SUBSCRIBE: Self = Self(0x102a);
    pub const CLIENT_HEARTBEAT: Self = Self(0x1007);
    /// Server-side initialization control. Captures show wire kind `3110`
    /// twice per complete Wine data connection; the inner payload is opaque.
    pub const SERVER_INIT_CONTROL: Self = Self(0x1031);
    /// Server-side initialization continuation. Captures show wire kind
    /// `3210` once per complete Wine data connection; semantics are unknown.
    pub const SERVER_INIT_CONTINUE: Self = Self(0x1032);
    /// Server zlib-compressed code-table object. Its decoded routing header
    /// supplies the current connection's `2d10` market/version words.
    pub const SERVER_CODE_TABLE: Self = Self(0x0401);
    pub const SERVER_DELTA: Self = Self(0x0427);
    pub const SERVER_BULK: Self = Self(0x040d);
    pub const SERVER_BULK_ALT: Self = Self(0x0454);
    pub const SERVER_STATE: Self = Self(0x0421);
    pub const SERVER_META: Self = Self(0x043e);
    /// Server compressed snapshot. Captures show that its 16-bit outer length
    /// wraps for payloads above 64 KiB; payload bytes 4..8 carry the full
    /// compressed-body length and bytes 8.. begin with a zlib stream.
    pub const SERVER_EXTENDED_ZLIB: Self = Self(0x0415);
    /// Additional server-side control/object kinds observed in the
    /// 2026-09-02 paired Wine capture. Their inner business schemas remain
    /// undecoded; naming these values only makes direction classification and
    /// telemetry lossless.
    pub const SERVER_OBJECT_3001: Self = Self(0x0130);
    pub const SERVER_CONTROL_0310: Self = Self(0x1003);
    /// Server-side periodic keep-alive observed after market close. Captures
    /// show a 2-byte all-zero payload emitted roughly every 0.8--1.2 seconds
    /// on every live primary 5188 connection (closing-parity-2020 run:
    /// 290 frames / ~180 s, every payload `00 00`). It carries no business
    /// data and is what keeps a Wine primary connection alive outside trading
    /// hours; a Rust session must tolerate it and never treat it as a decoder
    /// input or as proof of a business update.
    pub const SERVER_HEARTBEAT: Self = Self(0x0139);

    /// Returns the kind as it appears on the wire (little-endian bytes).
    ///
    /// This is intentionally distinct from the numeric little-endian value
    /// displayed by `Display` (for example, wire bytes `0d04` have value
    /// `0x040d`).
    #[must_use]
    pub fn wire_hex(self) -> String {
        let [lo, hi] = self.0.to_le_bytes();
        format!("{lo:02x}{hi:02x}")
    }
}

impl fmt::Display for Official5188Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04x}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188Frame {
    pub kind: Official5188Kind,
    pub metadata: [u8; 4],
    pub payload: Vec<u8>,
}

/// Returns the current-session text prefix consumed by Wine's `&ack=` packer.
///
/// Static tracing shows that Wine copies the complete second `3110` payload
/// into a zeroed buffer and then applies C `strlen` before code-page
/// conversion. Bytes after the first NUL remain opaque session data and are
/// deliberately excluded. The accepted length/NUL pairs are capture-backed;
/// unknown combinations remain rejected.
pub fn official_5188_ack_source_prefix(frame: &Official5188Frame) -> Result<&[u8], String> {
    if frame.kind != Official5188Kind::SERVER_INIT_CONTROL {
        return Err(format!(
            "5188 ACK source requires wire kind 3110, got {}",
            frame.kind.wire_hex()
        ));
    }
    // The 3110 payload is an encrypted/binary control object whose first NUL
    // offset is NOT a fixed protocol constant: it varies with the session
    // (observed 35/64/65/111/273/286/358/411/532/608 across live formal
    // sessions and historical captures). Wine treats the bytes before the
    // first NUL as the ACK `&ack=` source prefix, so the correct boundary is
    // "the first NUL", not a length-keyed lookup table. Only the wire kind
    // (3110) and the presence of a NUL terminator are invariant.
    let first_nul = frame
        .payload
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| "5188 ACK source payload is not NUL terminated".to_string())?;
    if first_nul == 0 {
        return Err("5188 ACK source payload starts with a NUL".to_string());
    }
    Ok(&frame.payload[..first_nul])
}

/// A complete Wine client frame found inside an authenticated control payload.
///
/// This is evidence extraction, not handshake acceptance. Callers must still
/// validate the ordered result with [`Official5188Handshake`] before sending it
/// on a data connection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188EmbeddedClientFrame {
    pub offset: usize,
    pub frame: Official5188Frame,
}

/// Inner layout of the 32-byte client `2d10` code-table acknowledgement.
///
/// The first four words echo the protocol, group, market and version fields
/// from the three server `0104` code-table headers received on this same 5188
/// connection. The final sixteen bytes are reserved and must remain zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188ClientSessionEnvelope {
    pub word0: u32,
    pub word1: u32,
    pub word2: u32,
    pub word3: u32,
}

impl Official5188ClientSessionEnvelope {
    pub fn decode(frame: &Official5188Frame) -> Result<Self, String> {
        if frame.kind != Official5188Kind::CLIENT_SESSION {
            return Err(format!(
                "5188 client session envelope requires wire kind 2d10, got {}",
                frame.kind.wire_hex()
            ));
        }
        if frame.payload.len() != 32 {
            return Err(format!(
                "5188 client session payload must be 32 bytes, got {}",
                frame.payload.len()
            ));
        }
        if frame.payload[16..] != [0; 16] {
            return Err("5188 client session payload has non-zero reserved tail".to_string());
        }
        Ok(Self {
            word0: u32::from_le_bytes(frame.payload[0..4].try_into().unwrap()),
            word1: u32::from_le_bytes(frame.payload[4..8].try_into().unwrap()),
            word2: u32::from_le_bytes(frame.payload[8..12].try_into().unwrap()),
            word3: u32::from_le_bytes(frame.payload[12..16].try_into().unwrap()),
        })
    }

    #[must_use]
    pub fn into_frame(self, metadata: [u8; 4]) -> Official5188Frame {
        let mut payload = Vec::with_capacity(32);
        payload.extend_from_slice(&self.word0.to_le_bytes());
        payload.extend_from_slice(&self.word1.to_le_bytes());
        payload.extend_from_slice(&self.word2.to_le_bytes());
        payload.extend_from_slice(&self.word3.to_le_bytes());
        payload.extend_from_slice(&[0; 16]);
        Official5188Frame {
            kind: Official5188Kind::CLIENT_SESSION,
            metadata,
            payload,
        }
    }

    /// Observed `word0` per frame index: `SH`/`SZ`/`B$` market bytes followed
    /// by protocol `0x010a` from the matching server `0104` header.
    #[must_use]
    pub fn observed_word0(frame_index: usize) -> Option<u32> {
        match frame_index {
            0 => Some(u32::from_le_bytes(*b"SH\x0a\x01")),
            1 => Some(u32::from_le_bytes(*b"SZ\x0a\x01")),
            2 => Some(u32::from_le_bytes([0x42, 0x24, 0x0a, 0x01])),
            _ => None,
        }
    }

    /// `word1`: trading day `YYYYMMDD mod 2^16` in the high half and the group
    /// tag from the matching server `0104` header in the low half.
    #[must_use]
    pub fn observed_word1(trading_day: u32, group: SessionGroupTag) -> u32 {
        (trading_day & 0xffff) << 16 | u16::from(group) as u32
    }

    /// `word3`: high half of the market code-table version Unix timestamp.
    #[must_use]
    pub fn observed_word3(unix_seconds: u32) -> u32 {
        (unix_seconds >> 16) & 0xffff
    }

    /// `word2` low half is the fixed acknowledgement marker `0x0135`; its high
    /// half is the low half of the market code-table version Unix timestamp.
    pub const OBSERVED_WORD2_LOW: u16 = 0x0135;

    /// Checks a triplet against the complete code-table derivation. Frame and
    /// version order is `SH`, `SZ`, `B$`; all frames must share one group tag.
    #[must_use]
    pub fn matches_observed_derivation(
        envelopes: &[Official5188ClientSessionEnvelope],
        trading_day: u32,
        version_seconds: [u32; 3],
    ) -> bool {
        let Some(group) = envelopes
            .first()
            .and_then(|envelope| SessionGroupTag::from_wire((envelope.word1 & 0xffff) as u16))
        else {
            return false;
        };
        envelopes.len() == 3
            && envelopes.iter().zip(version_seconds).enumerate().all(
                |(index, (envelope, version))| {
                    envelope.word0 == Self::observed_word0(index).unwrap_or(0)
                        && envelope.word1 == Self::observed_word1(trading_day, group)
                        && envelope.word2
                            == (version & 0xffff) << 16 | u32::from(Self::OBSERVED_WORD2_LOW)
                        && envelope.word3 == Self::observed_word3(version)
                },
            )
    }
}

/// Fixed low-half `word1` tags observed in every captured lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionGroupTag {
    Primary,
    Secondary,
}

impl From<SessionGroupTag> for u16 {
    fn from(tag: SessionGroupTag) -> Self {
        match tag {
            SessionGroupTag::Primary => 0x2746,
            SessionGroupTag::Secondary => 0xb246,
        }
    }
}

impl SessionGroupTag {
    #[must_use]
    pub fn from_wire(value: u16) -> Option<Self> {
        match value {
            0x2746 => Some(Self::Primary),
            0xb246 => Some(Self::Secondary),
            _ => None,
        }
    }
}

/// Header shared by a decoded server `0104` code table and the following
/// client `2d10` acknowledgement on the same connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Official5188CodeTableHeader {
    pub protocol: u16,
    pub group: SessionGroupTag,
    pub version_seconds: u32,
    pub market: [u8; 2],
    pub trading_day_low: u16,
    pub acknowledgement_marker: u16,
}

/// Initial server objects consumed while waiting for the current connection's
/// SH/SZ/B$ code-table headers.
///
/// `code_tables` contains all four fully decoded `0104` tables in arrival
/// order, including the fourth SF table that is not part of `2d10 x3`.
/// `observed_frames` retains every frame removed from the transport so control
/// and metadata objects that precede the three code tables are not silently
/// discarded by the initialization state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188InitialCodeTables {
    pub headers: [Official5188CodeTableHeader; 3],
    pub code_tables: Vec<Official5188CodeTable>,
    pub observed_frames: Vec<Official5188Frame>,
}

impl Official5188CodeTableHeader {
    pub fn decode(decoded: &[u8]) -> Result<Self, String> {
        if decoded.len() < 92 {
            return Err("5188 code-table object is shorter than its 92-byte session header".into());
        }
        let protocol = u16::from_le_bytes(decoded[0..2].try_into().unwrap());
        if protocol != 0x010a {
            return Err(format!(
                "5188 code-table protocol must be 010a, got {protocol:04x}"
            ));
        }
        let group_wire = u16::from_le_bytes(decoded[2..4].try_into().unwrap());
        let group = SessionGroupTag::from_wire(group_wire)
            .ok_or_else(|| format!("5188 code-table group tag is unknown: {group_wire:04x}"))?;
        let version_low = u16::from_le_bytes(decoded[4..6].try_into().unwrap());
        let version_high = u16::from_le_bytes(decoded[6..8].try_into().unwrap());
        let marker = u32::from_le_bytes(decoded[8..12].try_into().unwrap());
        // Live primary endpoints have emitted both routing markers: the
        // original offline capture uses 1, while the current production
        // route uses 3 with the same 010a table protocol. Keep 030a and
        // unknown markers rejected because they are different wire shapes.
        if !matches!(marker, 1 | 3) {
            return Err(format!(
                "5188 code-table routing marker must be 1 or 3, got {marker}"
            ));
        }
        let market = decoded[12..14].try_into().unwrap();
        if !matches!(&market, b"SH" | b"SZ" | b"B$") {
            return Err(format!(
                "5188 code-table routing market is unsupported: {:02x}{:02x}",
                market[0], market[1]
            ));
        }
        let trading_day_low = u16::from_le_bytes(decoded[88..90].try_into().unwrap());
        let acknowledgement_marker = u16::from_le_bytes(decoded[90..92].try_into().unwrap());
        if acknowledgement_marker != Official5188ClientSessionEnvelope::OBSERVED_WORD2_LOW {
            return Err(format!(
                "5188 code-table acknowledgement marker must be 0135, got {acknowledgement_marker:04x}"
            ));
        }
        Ok(Self {
            protocol,
            group,
            version_seconds: u32::from(version_low) | u32::from(version_high) << 16,
            market,
            trading_day_low,
            acknowledgement_marker,
        })
    }

    #[must_use]
    pub fn client_session_envelope(self) -> Official5188ClientSessionEnvelope {
        Official5188ClientSessionEnvelope {
            word0: u32::from_le_bytes([
                self.market[0],
                self.market[1],
                self.protocol as u8,
                (self.protocol >> 8) as u8,
            ]),
            word1: u32::from(self.trading_day_low) << 16 | u16::from(self.group) as u32,
            word2: (self.version_seconds & 0xffff) << 16 | u32::from(self.acknowledgement_marker),
            word3: Official5188ClientSessionEnvelope::observed_word3(self.version_seconds),
        }
    }
}

/// Builds the current connection's `2d10 x3` from the server-provided code
/// tables. Input order may vary; output is always `SH`, `SZ`, `B$`.
pub fn build_official_5188_client_session_triplet(
    headers: &[Official5188CodeTableHeader],
) -> Result<[Official5188Frame; 3], String> {
    if headers.len() != 3 {
        return Err(format!(
            "5188 client session requires three market headers, got {}",
            headers.len()
        ));
    }
    let group = headers[0].group;
    if headers.iter().any(|header| header.group != group) {
        return Err("5188 code-table headers disagree on connection group".into());
    }
    if headers.iter().any(|header| header.protocol != 0x010a) {
        return Err("5188 code-table headers use an unsupported protocol".into());
    }
    if headers.iter().any(|header| {
        header.trading_day_low != headers[0].trading_day_low
            || header.acknowledgement_marker
                != Official5188ClientSessionEnvelope::OBSERVED_WORD2_LOW
    }) {
        return Err("5188 code-table headers disagree on session day or marker".into());
    }
    let find = |market: [u8; 2]| {
        let matches = headers
            .iter()
            .filter(|header| header.market == market)
            .copied()
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [header] => Ok(*header),
            [] => Err(format!(
                "5188 client session is missing market {}{}",
                market[0] as char, market[1] as char
            )),
            _ => Err(format!(
                "5188 client session repeats market {}{}",
                market[0] as char, market[1] as char
            )),
        }
    };
    Ok([
        find(*b"SH")?.client_session_envelope().into_frame([0; 4]),
        find(*b"SZ")?.client_session_envelope().into_frame([0; 4]),
        find(*b"B$")?.client_session_envelope().into_frame([0; 4]),
    ])
}

/// Finds complete, known 5188 client frames embedded in a 6100/7100 control
/// payload. Truncated candidates and unknown kinds are retained by the caller's
/// raw control evidence, but are not returned as usable initialization frames.
#[must_use]
pub fn embedded_client_frames(control_payload: &[u8]) -> Vec<Official5188EmbeddedClientFrame> {
    let mut found = Vec::new();
    let mut offset = 0;
    while offset + HEADER_LEN <= control_payload.len() {
        let kind = Official5188Kind(u16::from_le_bytes([
            control_payload[offset],
            control_payload[offset + 1],
        ]));
        if !matches!(
            kind,
            Official5188Kind::CLIENT_INIT
                | Official5188Kind::CLIENT_SESSION
                | Official5188Kind::CLIENT_SUBSCRIBE
                | Official5188Kind::CLIENT_HEARTBEAT
        ) {
            offset += 1;
            continue;
        }

        let payload_len = usize::from(u16::from_le_bytes([
            control_payload[offset + 2],
            control_payload[offset + 3],
        ]));
        let Some(end) = offset
            .checked_add(HEADER_LEN)
            .and_then(|header_end| header_end.checked_add(payload_len))
        else {
            offset += 1;
            continue;
        };
        if payload_len > MAX_PAYLOAD_LEN || end > control_payload.len() {
            offset += 1;
            continue;
        }
        if let Ok(frame) = Official5188Frame::decode(&control_payload[offset..end]) {
            found.push(Official5188EmbeddedClientFrame { offset, frame });
            offset = end;
        } else {
            offset += 1;
        }
    }
    found
}

/// Evidence-backed envelope observed in recurring wire kind `3e04` frames.
///
/// The three header words are intentionally raw: their business meaning is
/// not established yet. The body is retained as fixed-size bytes until it is
/// matched to same-symbol Wine callbacks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188BulkEnvelope {
    pub block_offset: u32,
    pub header_word: u32,
    pub sequence_word: u32,
    pub body: Vec<u8>,
}

/// Reassembled body for one contiguous `3e04` block sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188BulkStream {
    pub start_offset: u32,
    pub header_word: u32,
    pub sequence_word: u32,
    pub body: Vec<u8>,
}

/// Envelope for recurring server `2704` frames.
///
/// Static tracing of Wine's decoder at `0x44aa30` establishes this prefix as
/// a 16-bit record count followed by a 32-bit offset. The offset is measured
/// from the start of the payload, so the value bitstream starts after this
/// six-byte prefix and ends at `value_end_offset`; the index bitstream follows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188DeltaEnvelope {
    pub record_count: u16,
    pub value_end_offset: u32,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Official5188DeltaStreams<'a> {
    pub record_count: usize,
    pub value_stream: &'a [u8],
    pub index_stream: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Official5188DeltaIndexState {
    pub market: [u8; 2],
    pub symbol_index: u16,
    pub timestamp: u32,
    pub uses_baseline: bool,
}

/// Supplies the last 311-byte record for a symbol when the value stream marks
/// the current record as baseline-relative.
pub trait Official5188BaselineResolver {
    fn resolve(&mut self, market: [u8; 2], symbol_index: u16)
    -> Option<Official5188InternalRecord>;

    /// Resolves a complete decoded state, excluding metadata-only seeds.
    fn resolve_baseline(
        &mut self,
        market: [u8; 2],
        symbol_index: u16,
    ) -> Option<Official5188InternalRecord> {
        self.resolve(market, symbol_index)
    }

    /// Commits one completely decoded record to the vendor-style global
    /// table. Decoding is record-transactional rather than frame-transactional:
    /// a malformed later record must not discard earlier valid updates.
    fn commit(
        &mut self,
        _market: [u8; 2],
        _symbol_index: u16,
        _record: Official5188InternalRecord,
    ) {
    }
}

/// In-memory baseline resolver for offline replay and fixture tests.
#[derive(Clone, Debug, Default)]
pub struct Official5188MapBaselineResolver {
    records: std::collections::HashMap<([u8; 2], u16), Official5188InternalRecord>,
    baselines: std::collections::HashMap<([u8; 2], u16), Official5188InternalRecord>,
}

impl Official5188MapBaselineResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        market: [u8; 2],
        symbol_index: u16,
        record: Official5188InternalRecord,
    ) {
        self.records.insert((market, symbol_index), record.clone());
        self.baselines.insert((market, symbol_index), record);
    }

    /// Seeds Wine-compatible per-symbol metadata state from 0104 tables.
    /// Existing decoded records are retained so a repeated table cannot
    /// roll a live resolver back to metadata-only state.
    pub fn seed_code_tables(&mut self, tables: &[Official5188CodeTable]) -> usize {
        let mut inserted = 0;
        for table in tables {
            for record in &table.records {
                let key = (table.market, record.symbol_index);
                if self.records.contains_key(&key) {
                    continue;
                }
                self.records.insert(
                    key,
                    Official5188InternalRecord::from_code_table_metadata(
                        table.market,
                        record.symbol_index,
                        record,
                    ),
                );
                inserted += 1;
            }
        }
        inserted
    }

    /// Applies decoded value-pass records using Wine's global-table lifetime.
    /// The outer `2704` loop copies every temporary record back, including
    /// `0x18` records whose inner decoder returns before tail/alignment work.
    pub fn update_from_decoded(&mut self, records: &[Official5188DecodedValueRecord]) {
        for decoded in records {
            self.insert(
                decoded.index.market,
                decoded.index.symbol_index,
                decoded.record.clone(),
            );
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl Official5188BaselineResolver for Official5188MapBaselineResolver {
    fn resolve(
        &mut self,
        market: [u8; 2],
        symbol_index: u16,
    ) -> Option<Official5188InternalRecord> {
        self.records.get(&(market, symbol_index)).cloned()
    }

    fn resolve_baseline(
        &mut self,
        market: [u8; 2],
        symbol_index: u16,
    ) -> Option<Official5188InternalRecord> {
        self.baselines.get(&(market, symbol_index)).cloned()
    }

    fn commit(&mut self, market: [u8; 2], symbol_index: u16, record: Official5188InternalRecord) {
        self.insert(market, symbol_index, record);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Official5188DecodedValueRecord {
    pub index: Official5188DeltaIndexState,
    pub mask: u8,
    pub header: Official5188ValueRecordHeader,
    pub bit_start: usize,
    pub bit_end: usize,
    pub record: Official5188InternalRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Official5188CodeTableRecord {
    pub symbol_index: u16,
    pub code: String,
    pub name: String,
    pub amount_mode: u8,
    pub opaque_tail: [u8; 23],
}

impl Official5188CodeTableRecord {
    /// Returns the price scale derived from the 0104 decimal-place metadata.
    ///
    /// The first opaque-tail byte stores the number of decimal places. The
    /// scale is therefore `10^decimal_places`; deriving it here keeps the
    /// runtime projection consistent with the callback-parity decoder.
    #[must_use]
    pub fn price_scale_hint(&self) -> Option<f64> {
        let decimal_places = self.opaque_tail[0];
        (decimal_places <= 6).then(|| 10_f64.powi(i32::from(decimal_places)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Official5188CodeTable {
    pub market: [u8; 2],
    pub records: Vec<Official5188CodeTableRecord>,
}

pub type Official5188SubscriptionEntry = ([u8; 2], u32);
pub type Official5188PrimarySubscriptionPartitions =
    [Vec<Official5188SubscriptionEntry>; OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT];

/// Evidence-backed view of one vendor 2704 internal record.
///
/// This is deliberately a layout parser, not a business decoder.  The value
/// bitstream still has to reconstruct the bytes before this type can be used.
/// Price and volume arrays retain the vendor order: bid levels in reverse
/// order followed by ask levels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188InternalRecord {
    bytes: [u8; OFFICIAL_5188_INTERNAL_RECORD_LEN],
}

/// Public quote-shaped projection of one decoded 5188 internal record.
///
/// The projection keeps the price scale explicit because the scale is supplied
/// by the security metadata, not encoded in the 311-byte value record.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Official5188PublicQuote {
    pub market: String,
    pub code: String,
    pub name: String,
    pub timestamp: u32,
    pub price: f64,
    pub last_close: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub amount: f64,
    pub ask_prices: [f64; 10],
    pub ask_volumes: [f64; 10],
    pub bid_prices: [f64; 10],
    pub bid_volumes: [f64; 10],
    pub source_protocol: &'static str,
}

/// Stateful Wine-compatible merge target for decoded 2704 records.
///
/// The caller owns one state per public symbol and supplies the previous-close
/// snapshot at the trading-day boundary. Zero-valued sparse updates retain the
/// prior public value; `mask & 0x38 == 0x18` updates only the timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188OemState {
    record: Official5188InternalRecord,
    previous_close: i32,
}

impl Official5188OemState {
    #[must_use]
    pub fn new(previous_close: i32) -> Self {
        Self {
            record: Official5188InternalRecord {
                bytes: [0; OFFICIAL_5188_INTERNAL_RECORD_LEN],
            },
            previous_close,
        }
    }

    pub fn merge(&mut self, decoded: &Official5188DecodedValueRecord) {
        let incoming = &decoded.record.bytes;
        self.record.bytes[0xdf..0xe3].copy_from_slice(&incoming[0xdf..0xe3]);
        if incoming[..4] != [0; 4] {
            self.record.bytes[..4].copy_from_slice(&incoming[..4]);
        }
        if decoded.mask & 0x38 == 0x18 {
            return;
        }
        for (offset, len) in [
            (0x04, 4),
            (0x08, 4),
            (0x0c, 4),
            (0x10, 4),
            (0x14, 8),
            (0x1c, 8),
        ] {
            if incoming[offset..offset + len].iter().any(|byte| *byte != 0) {
                self.record.bytes[offset..offset + len]
                    .copy_from_slice(&incoming[offset..offset + len]);
            }
        }
        for offset in (0x58..0x80).step_by(4).chain((0xa8..0xd0).step_by(4)) {
            if incoming[offset..offset + 4].iter().any(|byte| *byte != 0) {
                self.record.bytes[offset..offset + 4]
                    .copy_from_slice(&incoming[offset..offset + 4]);
            }
        }
    }

    /// Construct an OEM state from the complete same-session 0104 metadata row.
    pub fn from_code_table_metadata(
        market: [u8; 2],
        metadata: &Official5188CodeTableRecord,
    ) -> Result<Self, String> {
        let previous_close = metadata
            .opaque_tail
            .get(11..15)
            .and_then(|bytes| bytes.try_into().ok().map(i32::from_le_bytes))
            .ok_or_else(|| "0104 metadata row has no previous-close tail".to_string())?;
        Ok(Self {
            record: Official5188InternalRecord::from_code_table_metadata(
                market,
                metadata.symbol_index,
                metadata,
            ),
            previous_close,
        })
    }

    pub fn project(
        &self,
        code: impl Into<String>,
        name: impl Into<String>,
        price_scale: f64,
    ) -> Result<Official5188PublicQuote, String> {
        let mut record = self.record.clone();
        record.bytes[0x12b..0x12f].copy_from_slice(&self.previous_close.to_le_bytes());
        record.to_public_quote(code, name, price_scale)
    }

    /// Project using the complete same-session 0104 row, including its name
    /// and decimal-place scale metadata.
    pub fn project_from_code_table_metadata(
        &self,
        metadata: &Official5188CodeTableRecord,
    ) -> Result<Official5188PublicQuote, String> {
        let price_scale = metadata
            .price_scale_hint()
            .ok_or_else(|| "0104 metadata row has an invalid decimal-place hint".to_string())?;
        self.project(metadata.code.clone(), metadata.name.clone(), price_scale)
    }

    /// Merge one sparse 2704 update transactionally and project it with the
    /// complete same-session 0104 metadata row. The receiver is changed only
    /// when projection succeeds, so malformed values cannot poison a later
    /// update through a partially merged state.
    pub fn merge_and_project_from_code_table_metadata(
        &mut self,
        decoded: &Official5188DecodedValueRecord,
        metadata: &Official5188CodeTableRecord,
    ) -> Result<Official5188PublicQuote, String> {
        let mut candidate = self.clone();
        candidate.merge(decoded);
        let quote = candidate.project_from_code_table_metadata(metadata)?;
        *self = candidate;
        Ok(quote)
    }
}

impl serde::Serialize for Official5188InternalRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.bytes)
    }
}

impl Official5188InternalRecord {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let bytes: [u8; OFFICIAL_5188_INTERNAL_RECORD_LEN] = bytes.try_into().map_err(|_| {
            format!(
                "5188 internal record must be {} bytes, got {}",
                OFFICIAL_5188_INTERNAL_RECORD_LEN,
                bytes.len()
            )
        })?;
        Ok(Self { bytes })
    }

    /// Builds Wine's initial per-symbol state from one 0104 code-table row.
    ///
    /// The amount mode and 23-byte opaque tail are copied verbatim to the
    /// final 24 bytes of the 311-byte state (`0x11f..0x137`). This includes
    /// amount scaling and the reference price used by fresh value records.
    #[must_use]
    pub fn from_code_table_metadata(
        market: [u8; 2],
        symbol_index: u16,
        record: &Official5188CodeTableRecord,
    ) -> Self {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0xdf..0xe1].copy_from_slice(&symbol_index.to_le_bytes());
        bytes[0xe1..0xe3].copy_from_slice(&market);
        bytes[0xe3..0xe5].copy_from_slice(&market);
        let code = record.code.as_bytes();
        if code.len() == 6 && code.iter().all(u8::is_ascii_digit) {
            bytes[0xe5..0xeb].copy_from_slice(code);
        }
        bytes[0x11f] = record.amount_mode;
        bytes[0x120..0x137].copy_from_slice(&record.opaque_tail);
        Self { bytes }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; OFFICIAL_5188_INTERNAL_RECORD_LEN] {
        &self.bytes
    }

    #[must_use]
    pub fn timestamp(&self) -> u32 {
        u32::from_le_bytes(self.bytes[0..4].try_into().expect("fixed timestamp field"))
    }

    /// Clamp post-close timestamps to 15:00:00 in China Standard Time.
    ///
    /// OEM public callbacks normalize records emitted after the market close;
    /// the internal record keeps the original wire timestamp unchanged.
    #[must_use]
    pub fn public_timestamp(&self) -> u32 {
        const CHINA_UTC_OFFSET_SECONDS: i64 = 8 * 60 * 60;
        const CLOSE_SECONDS: i64 = 15 * 60 * 60;
        const DAY_SECONDS: i64 = 24 * 60 * 60;
        let timestamp = i64::from(self.timestamp());
        let local_seconds = timestamp + CHINA_UTC_OFFSET_SECONDS;
        let day = local_seconds.div_euclid(DAY_SECONDS);
        let close_timestamp = day * DAY_SECONDS + CLOSE_SECONDS - CHINA_UTC_OFFSET_SECONDS;
        if timestamp > close_timestamp && close_timestamp >= 0 {
            close_timestamp as u32
        } else {
            self.timestamp()
        }
    }

    #[must_use]
    pub fn open_price_integer(&self) -> i32 {
        self.i32_at(0x04)
    }

    #[must_use]
    pub fn high_price_integer(&self) -> i32 {
        self.i32_at(0x08)
    }

    #[must_use]
    pub fn low_price_integer(&self) -> i32 {
        self.i32_at(0x0c)
    }

    #[must_use]
    pub fn last_price_integer(&self) -> i32 {
        self.i32_at(0x10)
    }

    #[must_use]
    pub fn volume_integer(&self) -> i64 {
        self.i64_at(0x14)
    }

    #[must_use]
    pub fn amount_integer(&self) -> i64 {
        self.i64_at(0x1c)
    }

    /// Returns bid-reversed followed by ask price integers.
    #[must_use]
    pub fn ladder_price_integers(&self) -> [i32; 10] {
        std::array::from_fn(|index| self.i32_at(0x58 + index * 4))
    }

    /// Returns bid-reversed followed by ask volume integers.
    #[must_use]
    pub fn ladder_volume_integers(&self) -> [i32; 10] {
        std::array::from_fn(|index| self.i32_at(0xa8 + index * 4))
    }

    /// Projects a decoded internal record into the Wine callback field shape.
    ///
    /// `price_scale` must come from the matched 0104/security metadata. The
    /// method deliberately requires public code and name from that same table
    /// so an internal symbol index cannot be published as a guessed code.
    pub fn to_public_quote(
        &self,
        code: impl Into<String>,
        name: impl Into<String>,
        price_scale: f64,
    ) -> Result<Official5188PublicQuote, String> {
        if !price_scale.is_finite() || price_scale <= 0.0 {
            return Err(format!(
                "5188 price scale must be finite and positive: {price_scale}"
            ));
        }
        let market_bytes = self.market();
        let market = std::str::from_utf8(&market_bytes)
            .map_err(|_| "5188 internal record has a non-ASCII market")?;
        if !matches!(market, "SH" | "SZ" | "BJ") {
            return Err(format!(
                "5188 internal record has unknown market {market:?}"
            ));
        }
        let code = code.into();
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(format!(
                "5188 public code must contain six ASCII digits: {code:?}"
            ));
        }
        let name = name.into();
        let amount = self.amount_integer();
        if amount < 0 {
            return Err(format!(
                "5188 negative internal amount cannot be projected before Wine amount conversion: {amount}"
            ));
        }
        let prices = self.ladder_price_integers();
        let volumes = self.ladder_volume_integers();
        let price_scale = price_scale as f32;
        let scaled = |value: i32| f64::from(value as f32 / price_scale);
        let star_lots = market == "SH" && matches!(&code[..3], "688" | "689");
        let volume = |value: i64| {
            if star_lots {
                if value <= 0 {
                    0.0
                } else {
                    ((value + 50) / 100).max(1) as f64
                }
            } else {
                value as f64
            }
        };
        let book_volume = |price: i32, value: i32| {
            if star_lots {
                if value <= 0 {
                    0.0
                } else {
                    ((i64::from(value) + 50) / 100).max(1) as f64
                }
            } else if value == 0 && price != 0 {
                1.0
            } else {
                f64::from(value)
            }
        };
        // Wine exposes ten slots per side, while the 311-byte record carries
        // five decoded levels per side. Preserve the callback shape and leave
        // the remaining slots at their observed zero value.
        let bid_prices = std::array::from_fn(|index| {
            (index < 5)
                .then(|| scaled(prices[4 - index]))
                .unwrap_or(0.0)
        });
        let ask_prices = std::array::from_fn(|index| {
            (index < 5)
                .then(|| scaled(prices[5 + index]))
                .unwrap_or(0.0)
        });
        let bid_volumes = std::array::from_fn(|index| {
            (index < 5)
                .then(|| book_volume(prices[4 - index], volumes[4 - index]))
                .unwrap_or(0.0)
        });
        let ask_volumes = std::array::from_fn(|index| {
            (index < 5)
                .then(|| book_volume(prices[5 + index], volumes[5 + index]))
                .unwrap_or(0.0)
        });
        Ok(Official5188PublicQuote {
            market: market.to_owned(),
            code,
            name,
            timestamp: self.public_timestamp(),
            price: scaled(self.last_price_integer()),
            last_close: scaled(self.i32_at(0x12b)),
            open: scaled(self.open_price_integer()),
            high: scaled(self.high_price_integer()),
            low: scaled(self.low_price_integer()),
            volume: volume(self.volume_integer()),
            amount: f64::from(amount as f32),
            ask_prices,
            ask_volumes,
            bid_prices,
            bid_volumes,
            source_protocol: "wjf.oem_report.v5",
        })
    }

    /// Projects scalar values from this decoded record while taking the
    /// ladder from an explicitly supplied Wine public-state record. The
    /// caller owns the global-slot selection; this method never guesses a
    /// baseline or silently substitutes one.
    pub fn to_public_quote_with_public_state(
        &self,
        public_state: &Self,
        code: impl Into<String>,
        name: impl Into<String>,
        price_scale: f64,
    ) -> Result<Official5188PublicQuote, String> {
        let mut quote = self.to_public_quote(code, name, price_scale)?;
        let state_prices = public_state.ladder_price_integers();
        let state_volumes = public_state.ladder_volume_integers();
        quote.bid_prices = std::array::from_fn(|index| {
            (index < 5)
                .then(|| f64::from(state_prices[4 - index] as f32 / price_scale as f32))
                .unwrap_or(0.0)
        });
        quote.ask_prices = std::array::from_fn(|index| {
            (index < 5)
                .then(|| f64::from(state_prices[5 + index] as f32 / price_scale as f32))
                .unwrap_or(0.0)
        });
        quote.bid_volumes = std::array::from_fn(|index| {
            (index < 5)
                .then(|| {
                    let value = state_volumes[4 - index];
                    if quote.market == "SH" && matches!(&quote.code[..3], "688" | "689") {
                        if value <= 0 {
                            0.0
                        } else {
                            ((i64::from(value) + 50) / 100).max(1) as f64
                        }
                    } else if value == 0 && state_prices[4 - index] != 0 {
                        1.0
                    } else {
                        f64::from(value)
                    }
                })
                .unwrap_or(0.0)
        });
        quote.ask_volumes = std::array::from_fn(|index| {
            (index < 5)
                .then(|| {
                    let value = state_volumes[5 + index];
                    if quote.market == "SH" && matches!(&quote.code[..3], "688" | "689") {
                        if value <= 0 {
                            0.0
                        } else {
                            ((i64::from(value) + 50) / 100).max(1) as f64
                        }
                    } else if value == 0 && state_prices[5 + index] != 0 {
                        1.0
                    } else {
                        f64::from(value)
                    }
                })
                .unwrap_or(0.0)
        });
        Ok(quote)
    }

    #[must_use]
    pub fn symbol_index(&self) -> u16 {
        u16::from_le_bytes(
            self.bytes[0xdf..0xe1]
                .try_into()
                .expect("fixed symbol field"),
        )
    }

    #[must_use]
    pub fn market(&self) -> [u8; 2] {
        self.bytes[0xe1..0xe3]
            .try_into()
            .expect("fixed market field")
    }

    fn i32_at(&self, offset: usize) -> i32 {
        i32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("validated internal record offset"),
        )
    }

    fn i64_at(&self, offset: usize) -> i64 {
        i64::from_le_bytes(
            self.bytes[offset..offset + 8]
                .try_into()
                .expect("validated internal record offset"),
        )
    }

    fn zeroed() -> Self {
        Self {
            bytes: [0; OFFICIAL_5188_INTERNAL_RECORD_LEN],
        }
    }

    fn set_i32(&mut self, offset: usize, value: i32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn set_i64(&mut self, offset: usize, value: i64) {
        self.bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
}

impl Official5188CodeTable {
    pub fn decode(decoded: &[u8]) -> Result<Self, String> {
        if decoded.len() < OFFICIAL_5188_CODE_TABLE_HEADER_LEN {
            return Err("5188 code table is shorter than its 98-byte header".to_string());
        }
        let count_a = u32::from_le_bytes(decoded[92..96].try_into().unwrap()) as usize;
        let count_b = usize::from(u16::from_le_bytes(decoded[96..98].try_into().unwrap()));
        if count_a != count_b {
            return Err(format!(
                "5188 code-table record counts disagree: {count_a} != {count_b}"
            ));
        }
        if count_a > usize::from(u16::MAX) + 1 {
            return Err(format!(
                "5188 code-table record count exceeds u16 index: {count_a}"
            ));
        }
        let expected_len = count_a
            .checked_mul(OFFICIAL_5188_CODE_TABLE_RECORD_LEN)
            .and_then(|length| length.checked_add(OFFICIAL_5188_CODE_TABLE_HEADER_LEN))
            .ok_or_else(|| "5188 code-table length overflow".to_string())?;
        if decoded.len() != expected_len {
            return Err(format!(
                "5188 code-table length mismatch: expected {expected_len}, got {}",
                decoded.len()
            ));
        }

        let mut records = Vec::with_capacity(count_a);
        for (ordinal, bytes) in decoded[OFFICIAL_5188_CODE_TABLE_HEADER_LEN..]
            .chunks_exact(OFFICIAL_5188_CODE_TABLE_RECORD_LEN)
            .enumerate()
        {
            let symbol_index = u16::from_le_bytes(bytes[..2].try_into().unwrap());
            if usize::from(symbol_index) != ordinal {
                return Err(format!(
                    "5188 code-table index mismatch at ordinal {ordinal}: {symbol_index}"
                ));
            }
            let code_bytes = nul_terminated(&bytes[2..12]);
            let code = String::from_utf8_lossy(code_bytes).into_owned();
            let name_bytes = nul_terminated(&bytes[12..45]);
            let (name, _, _) = encoding_rs::GBK.decode(name_bytes);
            records.push(Official5188CodeTableRecord {
                symbol_index,
                code,
                name: name.into_owned(),
                amount_mode: bytes[44],
                opaque_tail: bytes[45..68].try_into().unwrap(),
            });
        }
        Ok(Self {
            market: decoded[12..14].try_into().unwrap(),
            records,
        })
    }

    #[must_use]
    pub fn record(&self, symbol_index: u16) -> Option<&Official5188CodeTableRecord> {
        self.records.get(usize::from(symbol_index))
    }
}

/// Builds the five stable 1024-entry market partitions observed in independent
/// official-client lifecycles.
///
/// Entries retain `0104` table order: eligible SH securities precede eligible
/// SZ securities, and each wire index is the current table's symbol ordinal in
/// the `0x10000` namespace. The two remaining dynamic subscription sets are
/// deliberately excluded because their current-session assignment source has
/// not yet been reconstructed.
pub fn build_official_5188_primary_subscription_partitions(
    sh: &Official5188CodeTable,
    sz: &Official5188CodeTable,
) -> Result<Official5188PrimarySubscriptionPartitions, String> {
    if sh.market != *b"SH" {
        return Err(format!(
            "5188 primary SH code table has market {:02x}{:02x}",
            sh.market[0], sh.market[1]
        ));
    }
    if sz.market != *b"SZ" {
        return Err(format!(
            "5188 primary SZ code table has market {:02x}{:02x}",
            sz.market[0], sz.market[1]
        ));
    }

    let required = OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT
        * OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN;
    let mut entries = sh
        .records
        .iter()
        .filter(|record| is_primary_sh_code(&record.code))
        .map(|record| (*b"SH", 0x1_0000 | u32::from(record.symbol_index)))
        .chain(
            sz.records
                .iter()
                .filter(|record| is_primary_sz_code(&record.code))
                .map(|record| (*b"SZ", 0x1_0000 | u32::from(record.symbol_index))),
        )
        .take(required)
        .collect::<Vec<_>>();
    if entries.len() != required {
        return Err(format!(
            "5188 primary subscription requires {required} eligible SH/SZ entries, got {}",
            entries.len()
        ));
    }

    Ok(std::array::from_fn(|_| {
        entries
            .drain(..OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN)
            .collect()
    }))
}

/// The full receive-list subscription universe split into 7 partitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188ReceiveListPartitions {
    pub partitions: Vec<Vec<Official5188SubscriptionEntry>>,
}

/// Builds the complete 7-partition subscription plan from the current SH/SZ
/// code tables plus the enabled receive-list codes.
///
/// Evidence (2026-09-02, two lifecycles): every decoded `2a10` entry equals
/// 接收清单 ∩ current 0104 (6203/6203).  Partitions 1-5 are the first 5120
/// eligible records (SH 600-699 then SZ 000/001/002/003/300/301) in table
/// order and are wire-verified.  Partitions 6/7 carry the remaining enabled
/// codes; their *set membership* is confirmed but their *wire order* is
/// `needs-verification` (Wine groups them in a 26-segment vendor category
/// order that is not 0104 order).  This helper returns those entries in
/// 0104 (SH-then-SZ) order; do not treat it as verified wire order until an
/// online 2704 coverage check passes.
pub fn build_official_5188_receive_list_partitions(
    sh: &Official5188CodeTable,
    sz: &Official5188CodeTable,
    receive_codes: &std::collections::BTreeSet<String>,
) -> Result<Official5188ReceiveListPartitions, String> {
    if sh.market != *b"SH" {
        return Err(format!(
            "receive-list SH code table has market {:02x}{:02x}",
            sh.market[0], sh.market[1]
        ));
    }
    if sz.market != *b"SZ" {
        return Err(format!(
            "receive-list SZ code table has market {:02x}{:02x}",
            sz.market[0], sz.market[1]
        ));
    }
    let entry = |table: &Official5188CodeTable, record: &Official5188CodeTableRecord| {
        (table.market, 0x1_0000u32 | u32::from(record.symbol_index))
    };
    let market_of = |market: &[u8; 2]| String::from_utf8_lossy(market).into_owned();
    let qualified = |table: &Official5188CodeTable, record: &Official5188CodeTableRecord| {
        format!("{}{}", market_of(&table.market), record.code)
    };
    let enabled_sh = sh
        .records
        .iter()
        .filter(|record| receive_codes.contains(&qualified(sh, record)))
        .map(|record| entry(sh, record))
        .collect::<Vec<_>>();
    let enabled_sz = sz
        .records
        .iter()
        .filter(|record| receive_codes.contains(&qualified(sz, record)))
        .map(|record| entry(sz, record))
        .collect::<Vec<_>>();

    let mut eligible = sh
        .records
        .iter()
        .filter(|record| {
            receive_codes.contains(&qualified(sh, record)) && is_primary_sh_code(&record.code)
        })
        .map(|record| entry(sh, record))
        .chain(
            sz.records
                .iter()
                .filter(|record| {
                    receive_codes.contains(&qualified(sz, record))
                        && is_primary_sz_code(&record.code)
                })
                .map(|record| entry(sz, record)),
        )
        .take(
            OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT
                * OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN,
        )
        .collect::<Vec<_>>();
    if eligible.len()
        < OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT
            * OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN
    {
        return Err(format!(
            "receive list cannot fill five primary partitions: {}/{} eligible",
            eligible.len(),
            OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT
                * OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN
        ));
    }
    let mut partitions = Vec::with_capacity(7);
    for _ in 0..OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_COUNT {
        partitions.push(
            eligible
                .drain(..OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN)
                .collect::<Vec<_>>(),
        );
    }
    let primary_used = partitions
        .iter()
        .flatten()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut remainder = enabled_sh
        .into_iter()
        .chain(enabled_sz)
        .filter(|entry| !primary_used.contains(entry))
        .collect::<Vec<_>>();
    let part6_len = remainder
        .len()
        .min(OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN);
    let part7 = remainder.split_off(part6_len);
    partitions.push(remainder);
    partitions.push(part7);
    Ok(Official5188ReceiveListPartitions { partitions })
}

impl Official5188InitialCodeTables {
    /// Builds the five stable subscription partitions directly from the full
    /// SH/SZ tables collected on this authenticated data connection.
    pub fn primary_subscription_partitions(
        &self,
    ) -> Result<Official5188PrimarySubscriptionPartitions, String> {
        let table = |market: [u8; 2]| {
            let mut matching = self
                .code_tables
                .iter()
                .filter(|table| table.market == market);
            let selected = matching.next().ok_or_else(|| {
                format!(
                    "5188 initial code tables have no {}{} table",
                    market[0] as char, market[1] as char
                )
            })?;
            if matching.next().is_some() {
                return Err(format!(
                    "5188 initial code tables repeat {}{} table",
                    market[0] as char, market[1] as char
                ));
            }
            Ok(selected)
        };
        build_official_5188_primary_subscription_partitions(table(*b"SH")?, table(*b"SZ")?)
    }
}

fn is_six_digit_code(code: &str) -> bool {
    code.len() == 6 && code.as_bytes().iter().all(u8::is_ascii_digit)
}

fn is_primary_sh_code(code: &str) -> bool {
    is_six_digit_code(code) && code.as_bytes() >= b"600000" && code.as_bytes() <= b"699999"
}

fn is_primary_sz_code(code: &str) -> bool {
    if !is_six_digit_code(code) {
        return false;
    }
    [b"000", b"001", b"002", b"003", b"300", b"301"]
        .iter()
        .any(|prefix| code.as_bytes().starts_with(*prefix))
}

fn nul_terminated(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len())]
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
    wine_tail_clamp: bool,
    eof_clamped: bool,
}

impl<'a> Official5188BitReader<'a> {
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
            wine_tail_clamp: false,
            eof_clamped: false,
        }
    }

    fn with_wine_tail_clamp(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
            wine_tail_clamp: true,
            eof_clamped: false,
        }
    }

    #[must_use]
    pub fn bit_offset(&self) -> usize {
        self.bit_offset
    }

    #[must_use]
    pub fn remaining_bits(&self) -> usize {
        self.bytes
            .len()
            .saturating_mul(8)
            .saturating_sub(self.bit_offset)
    }

    #[must_use]
    pub fn eof_clamped(&self) -> bool {
        self.eof_clamped
    }

    pub fn clear_eof_clamp(&mut self) {
        self.eof_clamped = false;
    }

    pub fn read_bits(&mut self, width: u8) -> Result<u32, String> {
        let remaining = self.remaining_bits();
        if self.wine_tail_clamp && usize::from(width) > remaining {
            self.eof_clamped = true;
        }
        let value = self.peek_bits(width)?;
        let consumed = if self.wine_tail_clamp {
            usize::from(width).min(remaining)
        } else {
            usize::from(width)
        };
        self.bit_offset += consumed;
        Ok(value)
    }

    /// Advances to the next byte boundary, matching the alignment performed
    /// by Wine after each successfully decoded 2704 value record.
    pub fn align_byte(&mut self) {
        self.bit_offset = self.bit_offset.saturating_add(7) & !7;
    }

    fn peek_bits(&self, width: u8) -> Result<u32, String> {
        if width == 0 || width > 32 {
            return Err(format!("5188 bit width must be in 1..=32, got {width}"));
        }
        if !self.wine_tail_clamp && usize::from(width) > self.remaining_bits() {
            return Err(format!(
                "5188 bitstream exhausted at bit {}: need {width}, have {}",
                self.bit_offset,
                self.remaining_bits()
            ));
        }
        let mut value = 0u32;
        let available_width = usize::from(width).min(self.remaining_bits());
        for offset in 0..available_width {
            let absolute = self.bit_offset + offset;
            let byte = self.bytes[absolute / 8];
            let bit = (byte >> (7 - absolute % 8)) & 1;
            value = (value << 1) | u32::from(bit);
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Official5188Token {
    pub prefix: u16,
    pub prefix_bits: u8,
    pub value_bits: u8,
    pub operation: u8,
    pub parameter: u32,
    pub base: u32,
}

const DELTA_MARKET_TOKENS: [Official5188Token; 6] = [
    token(0, 2, 0, b'E', 0x4853, 0),
    token(1, 2, 0, b'E', 0x5a53, 0),
    token(4, 3, 0, b'E', 0x4b48, 0),
    token(5, 3, 0, b'E', 0x4653, 0),
    token(6, 3, 0, b'E', 0x2442, 0),
    token(7, 3, 16, b'D', 0, 0),
];

const DELTA_SYMBOL_INDEX_TOKENS: [Official5188Token; 2] =
    [token(0, 1, 0, b'E', 1, 0), token(1, 1, 16, b'D', 0, 0)];

const DELTA_TIMESTAMP_TOKENS: [Official5188Token; 5] = [
    token(0, 2, 0, b'E', 0, 0),
    token(1, 2, 5, b'b', 5, 1),
    token(2, 2, 10, b'b', 10, 17),
    token(6, 3, 16, b'b', 16, 529),
    token(7, 3, 32, b'D', 0, 0),
];

const fn token(
    prefix: u16,
    prefix_bits: u8,
    value_bits: u8,
    operation: u8,
    parameter: u32,
    base: u32,
) -> Official5188Token {
    Official5188Token {
        prefix,
        prefix_bits,
        value_bits,
        operation,
        parameter,
        base,
    }
}

const VALUE_TOKENS_5b2368: [Official5188Token; 7] = [
    token(14, 4, 0, b'E', 0, 0),
    token(0, 1, 0, b'E', 2, 0),
    token(12, 4, 0, b'E', 3, 0),
    token(2, 2, 4, b'B', 4, 4),
    token(13, 4, 8, b'B', 8, 20),
    token(30, 5, 8, b'm', 8, 0),
    token(31, 5, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b23c8: [Official5188Token; 5] = [
    token(0, 2, 0, b'E', 999999, 0),
    token(1, 2, 0, b'E', 0, 0),
    token(2, 2, 4, b'b', 4, 0),
    token(6, 3, 8, b'b', 8, 8),
    token(7, 3, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b2410: [Official5188Token; 5] = [
    token(0, 1, 0, b'E', 0, 0),
    token(14, 4, 0, b'E', 16, 0),
    token(6, 3, 4, b'P', 9, 0),
    token(15, 4, 4, b'P', 9, 16),
    token(2, 2, 9, b'D', 0, 0),
];
const VALUE_TOKENS_5b2458: [Official5188Token; 5] = [
    token(7, 3, 0, b'E', 0, 0),
    token(4, 3, 0, b'E', 16, 0),
    token(5, 3, 4, b'P', 9, 0),
    token(6, 3, 4, b'P', 9, 16),
    token(0, 1, 9, b'D', 0, 0),
];
const VALUE_TOKENS_5b249c: [Official5188Token; 2] =
    [token(0, 1, 0, b'E', 1024, 0), token(1, 1, 10, b'D', 0, 0)];
const VALUE_TOKENS_5b24b8: [Official5188Token; 7] = [
    token(0, 1, 0, b'E', 45, 0),
    token(4, 3, 0, b'E', 40, 0),
    token(14, 4, 0, b'E', 5, 0),
    token(5, 3, 2, b'B', 2, 41),
    token(6, 3, 2, b'S', 12290, 13),
    token(30, 5, 6, b'B', 6, 0),
    token(31, 5, 6, b'B', 6, 64),
];
const VALUE_TOKENS_5b2518: [Official5188Token; 5] = [
    token(14, 4, 0, b'E', 0, 0),
    token(0, 1, 0, b'E', 1, 0),
    token(2, 2, 4, b'B', 4, 1),
    token(6, 3, 16, b'B', 16, 17),
    token(15, 4, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b2560: [Official5188Token; 11] = [
    token(0, 1, 0, b'E', 4294967295, 0),
    token(4, 3, 0, b'E', 144, 0),
    token(5, 3, 0, b'E', 9, 0),
    token(24, 5, 0, b'E', 149, 0),
    token(25, 5, 0, b'E', 4, 0),
    token(26, 5, 0, b'E', 89, 0),
    token(27, 5, 0, b'E', 64, 0),
    token(14, 4, 8, b'B', 8, 0),
    token(30, 5, 0, b'E', 4294967294, 0),
    token(62, 6, 0, b'E', 4294967293, 0),
    token(63, 6, 0, b'E', 4294967292, 0),
];
const VALUE_TOKENS_5b25f0: [Official5188Token; 4] = [
    token(6, 3, 12, b'b', 12, 0),
    token(2, 2, 16, b'b', 16, 2048),
    token(0, 1, 24, b'b', 24, 34816),
    token(7, 3, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b2628: [Official5188Token; 6] = [
    token(0, 1, 0, b'E', 0, 0),
    token(4, 3, 4, b'b', 4, 0),
    token(5, 3, 8, b'b', 8, 8),
    token(12, 4, 16, b'b', 16, 136),
    token(13, 4, 24, b'b', 24, 32904),
    token(7, 3, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b2678: [Official5188Token; 5] = [
    token(0, 2, 8, b'b', 8, 0),
    token(1, 2, 12, b'b', 12, 128),
    token(2, 2, 16, b'b', 16, 2176),
    token(6, 3, 24, b'b', 24, 34944),
    token(7, 3, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b26bc: [Official5188Token; 4] = [
    token(6, 3, 12, b'B', 12, 0),
    token(0, 1, 16, b'B', 16, 4096),
    token(2, 2, 24, b'B', 24, 69632),
    token(7, 3, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b26f0: [Official5188Token; 6] = [
    token(6, 3, 4, b'Z', 2, 1),
    token(14, 4, 6, b'Z', 4, 5),
    token(0, 1, 6, b'B', 6, 0),
    token(2, 2, 12, b'B', 12, 64),
    token(30, 5, 16, b'B', 16, 4160),
    token(31, 5, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b2740: [Official5188Token; 6] = [
    token(4, 3, 6, b'B', 6, 0),
    token(5, 3, 8, b'B', 8, 64),
    token(0, 2, 12, b'B', 12, 320),
    token(1, 2, 16, b'B', 16, 4416),
    token(6, 3, 24, b'B', 24, 69952),
    token(7, 3, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b2790: [Official5188Token; 6] = [
    token(0, 1, 2, b'B', 2, 0),
    token(2, 2, 4, b'B', 4, 4),
    token(6, 3, 8, b'B', 8, 20),
    token(14, 4, 12, b'B', 12, 276),
    token(30, 5, 16, b'B', 16, 4372),
    token(31, 5, 32, b'M', 0, 0),
];
const VALUE_TOKENS_5b27e0: [Official5188Token; 7] = [
    token(14, 4, 0, b'E', 0, 0),
    token(30, 5, 0, b'E', 1, 0),
    token(6, 3, 0, b'E', 2, 0),
    token(0, 2, 0, b'E', 3, 0),
    token(1, 2, 0, b'E', 4, 0),
    token(2, 2, 2, b'B', 2, 5),
    token(31, 5, 4, b'D', 0, 9),
];
const VALUE_TOKENS_5b283c: [Official5188Token; 4] = [
    token(0, 2, 4, b'b', 4, 0),
    token(1, 2, 8, b'b', 8, 8),
    token(2, 2, 16, b'b', 16, 136),
    token(3, 2, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b2870: [Official5188Token; 6] = [
    token(1, 2, 6, b'B', 6, 0),
    token(0, 2, 8, b'B', 8, 64),
    token(6, 3, 12, b'B', 12, 320),
    token(2, 2, 16, b'B', 16, 4416),
    token(14, 4, 24, b'B', 24, 69952),
    token(15, 4, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b28c0: [Official5188Token; 7] = [
    token(0, 1, 6, b'b', 6, 0),
    token(5, 3, 4, b'Z', 2, 1),
    token(14, 4, 6, b'Z', 4, 5),
    token(4, 3, 8, b'b', 8, 32),
    token(6, 3, 12, b'b', 12, 160),
    token(30, 5, 16, b'b', 16, 2208),
    token(31, 5, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b291c: [Official5188Token; 3] = [
    token(2, 2, 0, b'E', 0, 0),
    token(3, 2, 4, b'P', 10, 0),
    token(0, 1, 10, b'D', 0, 0),
];
const VALUE_TOKENS_5b2948: [Official5188Token; 6] = [
    token(14, 4, 0, b'E', 0, 0),
    token(2, 2, 4, b'b', 4, 0),
    token(0, 1, 6, b'b', 6, 8),
    token(6, 3, 8, b'b', 8, 40),
    token(30, 5, 12, b'b', 12, 168),
    token(31, 5, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b2998: [Official5188Token; 6] = [
    token(0, 1, 0, b'E', 0, 0),
    token(2, 2, 4, b'b', 4, 0),
    token(6, 3, 6, b'b', 6, 8),
    token(14, 4, 8, b'b', 8, 40),
    token(30, 5, 12, b'b', 12, 168),
    token(31, 5, 32, b'D', 0, 0),
];
const VALUE_TOKENS_5b29e8: [Official5188Token; 6] = [
    token(4, 3, 0, b'E', 0, 0),
    token(5, 3, 0, b'E', 1, 0),
    token(6, 3, 0, b'E', 2, 0),
    token(0, 1, 4, b'B', 4, 3),
    token(14, 4, 16, b'B', 16, 19),
    token(15, 4, 24, b'D', 0, 0),
];
const VALUE_TOKENS_5b2a38: [Official5188Token; 4] = [
    token(0, 1, 0, b'E', 0, 0),
    token(2, 2, 4, b'B', 4, 1),
    token(6, 3, 8, b'B', 8, 17),
    token(7, 3, 32, b'D', 0, 0),
];

/// Returns one of the vendor's 13-byte value token tables used by the 2704
/// decoder. The table addresses are stable within the verified Wine binary.
#[must_use]
pub fn official_5188_value_token_table(address: u32) -> Option<&'static [Official5188Token]> {
    match address {
        0x5b2368 => Some(&VALUE_TOKENS_5b2368),
        0x5b23c8 => Some(&VALUE_TOKENS_5b23c8),
        0x5b2410 => Some(&VALUE_TOKENS_5b2410),
        0x5b2458 => Some(&VALUE_TOKENS_5b2458),
        0x5b249c => Some(&VALUE_TOKENS_5b249c),
        0x5b24b8 => Some(&VALUE_TOKENS_5b24b8),
        0x5b2518 => Some(&VALUE_TOKENS_5b2518),
        0x5b2560 => Some(&VALUE_TOKENS_5b2560),
        0x5b25f0 => Some(&VALUE_TOKENS_5b25f0),
        0x5b2628 => Some(&VALUE_TOKENS_5b2628),
        0x5b2678 => Some(&VALUE_TOKENS_5b2678),
        0x5b26bc => Some(&VALUE_TOKENS_5b26bc),
        0x5b26f0 => Some(&VALUE_TOKENS_5b26f0),
        0x5b2740 => Some(&VALUE_TOKENS_5b2740),
        0x5b2790 => Some(&VALUE_TOKENS_5b2790),
        0x5b27e0 => Some(&VALUE_TOKENS_5b27e0),
        0x5b283c => Some(&VALUE_TOKENS_5b283c),
        0x5b2870 => Some(&VALUE_TOKENS_5b2870),
        0x5b28c0 => Some(&VALUE_TOKENS_5b28c0),
        0x5b291c => Some(&VALUE_TOKENS_5b291c),
        0x5b2948 => Some(&VALUE_TOKENS_5b2948),
        0x5b2998 => Some(&VALUE_TOKENS_5b2998),
        0x5b29e8 => Some(&VALUE_TOKENS_5b29e8),
        0x5b2a38 => Some(&VALUE_TOKENS_5b2a38),
        _ => None,
    }
}

/// Restores the per-record market, symbol index and timestamp state from the
/// `2704` index bitstream. The algorithm and token tables correspond to
/// Wine `0x44aa30` and its helpers at `0x448a10..0x448f8b`.
pub fn decode_official_5188_delta_indexes(
    streams: Official5188DeltaStreams<'_>,
) -> Result<Vec<Official5188DeltaIndexState>, String> {
    let mut reader = Official5188BitReader::new(streams.index_stream);
    let mut market = 0u16;
    let mut symbol_index = 0u16;
    let mut timestamp = 0u32;
    let mut decoded = Vec::with_capacity(streams.record_count);
    for record in 0..streams.record_count {
        if reader.read_bits(1)? != 0 {
            market = decode_delta_token(&mut reader, &DELTA_MARKET_TOKENS)? as u16;
        }
        let uses_baseline = reader.read_bits(1)? != 0;
        symbol_index = decode_relative_token(
            &mut reader,
            &DELTA_SYMBOL_INDEX_TOKENS,
            u32::from(symbol_index),
        )? as u16;
        timestamp = decode_relative_token(&mut reader, &DELTA_TIMESTAMP_TOKENS, timestamp)?;
        decoded.push(Official5188DeltaIndexState {
            market: market.to_le_bytes(),
            symbol_index,
            timestamp,
            uses_baseline,
        });
        if reader.bit_offset() > streams.index_stream.len() * 8 {
            return Err(format!("5188 index record {record} exceeded its bitstream"));
        }
    }
    Ok(decoded)
}

fn decode_relative_token(
    reader: &mut Official5188BitReader<'_>,
    tokens: &[Official5188Token],
    previous: u32,
) -> Result<u32, String> {
    let (value, operation) = decode_official_5188_token_with_operation(reader, tokens)?;
    if matches!(operation, b'D' | b'M') {
        Ok(value as u32)
    } else {
        Ok(previous.wrapping_add(value as u32))
    }
}

fn decode_delta_token(
    reader: &mut Official5188BitReader<'_>,
    tokens: &[Official5188Token],
) -> Result<i32, String> {
    decode_official_5188_token_with_operation(reader, tokens).map(|(value, _)| value)
}

/// Decodes one prefix/value token from a Wine-compatible MSB-first reader.
///
/// The token table is supplied by the caller so the crate does not need to
/// read or execute the vendor binary at runtime. This is also useful for
/// replay tools that load the verified table set from their fixture metadata.
pub fn decode_official_5188_token_with_operation(
    reader: &mut Official5188BitReader<'_>,
    tokens: &[Official5188Token],
) -> Result<(i32, u8), String> {
    let token = tokens
        .iter()
        .find(|token| {
            reader.remaining_bits() >= usize::from(token.prefix_bits)
                && reader
                    .peek_bits(token.prefix_bits)
                    .is_ok_and(|prefix| prefix == u32::from(token.prefix))
        })
        .copied();
    let Some(token) = token else {
        if reader.wine_tail_clamp && reader.remaining_bits() < 16 {
            reader.eof_clamped = true;
            return Ok((0, b'D'));
        }
        return Err(format!(
            "5188 token prefix did not match at bit {}",
            reader.bit_offset()
        ));
    };
    reader.read_bits(token.prefix_bits)?;
    let raw = if token.value_bits == 0 {
        0
    } else {
        reader.read_bits(token.value_bits)?
    };
    let value = match token.operation {
        b'E' => token.parameter as i32,
        b'P' => 1u32.checked_shl(raw).unwrap_or(0).wrapping_add(token.base) as i32,
        b'S' => raw
            .wrapping_shl((token.parameter >> 16 & 0xff) as u32)
            .wrapping_add(token.base) as i32,
        b'Z' => {
            let exponent = raw & 3;
            let scaled = (raw >> 2)
                .wrapping_add(token.base)
                .wrapping_mul(10u32.saturating_pow(exponent + 1));
            scaled as i32
        }
        b'b' => {
            let shift = 32 - u32::from(token.value_bits);
            let signed = ((raw << shift) as i32) >> shift;
            if signed < 0 {
                signed.wrapping_sub(token.base as i32)
            } else {
                signed.wrapping_add(token.base as i32)
            }
        }
        b'm' => {
            let high_bits = if token.value_bits >= 32 {
                0
            } else {
                u32::MAX << token.value_bits
            };
            (raw | high_bits).wrapping_add(token.base) as i32
        }
        b'B' => raw.wrapping_add(token.base) as i32,
        _ => raw as i32,
    };
    Ok((value, token.operation))
}

/// Convenience wrapper returning only the decoded token value.
pub fn decode_official_5188_token(
    reader: &mut Official5188BitReader<'_>,
    tokens: &[Official5188Token],
) -> Result<i32, String> {
    decode_official_5188_token_with_operation(reader, tokens).map(|(value, _)| value)
}

fn value_delta32(
    reader: &mut Official5188BitReader<'_>,
    address: u32,
    previous: i32,
    subtract: bool,
) -> Result<i32, String> {
    let table = official_5188_value_token_table(address)
        .ok_or_else(|| format!("unknown 5188 value token table {address:#x}"))?;
    let (value, operation) = decode_official_5188_token_with_operation(reader, table)?;
    if matches!(operation, b'D' | b'M') {
        Ok(value)
    } else if subtract {
        Ok(previous.wrapping_sub(value))
    } else {
        Ok(previous.wrapping_add(value))
    }
}

fn value_delta64(
    reader: &mut Official5188BitReader<'_>,
    address: u32,
    previous: i64,
) -> Result<i64, String> {
    let table = official_5188_value_token_table(address)
        .ok_or_else(|| format!("unknown 5188 value token table {address:#x}"))?;
    let (value, operation) = decode_official_5188_token_with_operation(reader, table)?;
    if operation == b'D' {
        Ok(i64::from(value))
    } else if operation == b'M' {
        Ok(decode_magnitude_m(value as u32))
    } else {
        Ok(previous.wrapping_add(i64::from(value)))
    }
}

fn decode_magnitude_m(raw: u32) -> i64 {
    let exponent = raw >> 30;
    let mantissa = ((raw << 2) as i32 >> 2) as i64;
    mantissa.wrapping_shl(exponent * 4)
}

fn decode_value_aux(
    reader: &mut Official5188BitReader<'_>,
    current: &mut Official5188InternalRecord,
    baseline: Option<&Official5188InternalRecord>,
) -> Result<(), String> {
    let d0 = baseline.map_or(0, |row| row.i32_at(0xd0));
    current.set_i32(0xd0, value_delta32(reader, 0x5b283c, d0, false)?);
    let (table, previous) = match baseline {
        Some(row) => (0x5b2998, row.i32_at(0xd4)),
        None => (0x5b2948, current.i32_at(0x12b)),
    };
    current.set_i32(0xd4, value_delta32(reader, table, previous, false)?);
    Ok(())
}

/// Reconstructs the Wine amount-token prediction used as the token's
/// previous value.  The multiplier/exponent fields are copied from the
/// 0104-backed metadata tail of the internal record; zeroed synthetic
/// records deliberately return `None` so fixture decoding keeps its legacy
/// token semantics until metadata is available.
fn amount_prediction(current: &Official5188InternalRecord, volume_delta: i64) -> Option<i64> {
    let mode = current.bytes[0x11f];
    let exponent = u32::from(current.bytes[0x120]);
    let multiplier = i128::from(u16::from_le_bytes(
        current.bytes[0x121..0x123]
            .try_into()
            .expect("fixed amount multiplier"),
    ));
    if multiplier == 0 || exponent > 18 {
        return None;
    }
    let scale = 10_i128.checked_pow(exponent)?;
    let last_price = current.last_price_integer();
    let prediction_price = if last_price != 0 {
        last_price
    } else {
        current.i32_at(0x12b)
    };
    // Wine jumps over the volume/price projection when mode is zero.
    let prediction = if mode == 0 {
        0
    } else {
        let raw = i128::from(volume_delta)
            .checked_mul(i128::from(prediction_price))?
            .checked_mul(multiplier)?
            .checked_div(scale)?;
        i64::try_from(raw).ok()?
    };
    let adjustment = u32::from_le_bytes(
        current.bytes[0x123..0x127]
            .try_into()
            .expect("fixed amount adjustment"),
    );
    if matches!(mode, 0 | 8) && adjustment != 0 {
        prediction
            .checked_add(1)?
            .checked_mul(i64::from(adjustment) + 1)
    } else {
        Some(prediction)
    }
}

fn decode_value_accumulators(
    reader: &mut Official5188BitReader<'_>,
    current: &mut Official5188InternalRecord,
    baseline: Option<&Official5188InternalRecord>,
    mask_class: u8,
    mask: u8,
) -> Result<i64, String> {
    let previous_volume = baseline.map_or(0, |row| row.volume_integer());
    let volume_table = if baseline.is_some() {
        0x5b26f0
    } else {
        0x5b26bc
    };
    let volume = value_delta64(reader, volume_table, previous_volume)?;
    current.set_i64(0x14, volume);
    let delta = volume.wrapping_sub(previous_volume);
    let amount = if let Some(row) = baseline {
        let mut amount = row.i64_at(0x24).abs();
        if reader.read_bits(1)? != 0 {
            amount = amount.wrapping_add(delta).wrapping_neg();
        }
        amount
    } else {
        value_delta64(reader, 0x5b2678, 0)?
    };
    current.set_i64(0x24, amount);
    if mask & 0x80 != 0 {
        let table = if baseline.is_some() {
            0x5b2628
        } else {
            0x5b25f0
        };
        let token_previous = amount_prediction(current, delta).unwrap_or(0);
        let delta_amount = value_delta64(reader, table, token_previous)?;
        let baseline_amount = baseline.map_or(0, |row| row.amount_integer());
        let amount = delta_amount.wrapping_add(baseline_amount);
        // Wine stores the wrapping 64-bit sum without rejecting its signed
        // representation. Public projection remains closed until the matching
        // Wine amount conversion has been reconstructed.
        current.set_i64(0x1c, amount);
    }
    let _ = mask_class;
    Ok(delta)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Official5188LadderMove {
    code: i32,
    merge_flag: u8,
    continue_ladder: bool,
}

fn decode_value_ladder_move(
    reader: &mut Official5188BitReader<'_>,
    current: &mut Official5188InternalRecord,
    baseline: Option<&Official5188InternalRecord>,
    mask_class: u8,
) -> Result<Official5188LadderMove, String> {
    if mask_class == 0x18 {
        return Ok(Official5188LadderMove {
            code: 0,
            merge_flag: 0,
            continue_ladder: false,
        });
    }
    let code = value_delta32(reader, 0x5b2560, 0, false)?;
    match code {
        -3 => {
            return Ok(Official5188LadderMove {
                code,
                merge_flag: 2,
                continue_ladder: true,
            });
        }
        -4 => {
            if let Some(row) = baseline {
                current.bytes[0x58..0x80].copy_from_slice(&row.bytes[0x58..0x80]);
            }
            return Ok(Official5188LadderMove {
                code,
                merge_flag: 2,
                continue_ladder: false,
            });
        }
        -2 => {
            return Ok(Official5188LadderMove {
                code,
                merge_flag: 0,
                continue_ladder: true,
            });
        }
        -1 => {
            if let Some(row) = baseline {
                current.bytes[0x58..0x80].copy_from_slice(&row.bytes[0x58..0x80]);
            }
            return Ok(Official5188LadderMove {
                code,
                merge_flag: 0,
                continue_ladder: true,
            });
        }
        _ => {}
    }
    if code < 0 {
        if let Some(row) = baseline {
            current.bytes[0x58..0x80].copy_from_slice(&row.bytes[0x58..0x80]);
        }
        return Ok(Official5188LadderMove {
            code,
            merge_flag: 0,
            continue_ladder: false,
        });
    }
    if let Some(row) = baseline {
        // Wine restores the baseline's internal 16-slot workspace before a
        // normal move; the public ladder may have been cleared by the header.
        current.bytes[0x58..0x98].copy_from_slice(&row.bytes[0x58..0x98]);
    }
    let hi = ((code >> 4) & 0xff) as usize;
    let lo = (code & 0xf) as usize;
    if hi >= 16 || lo >= 16 {
        return Err(format!(
            "5188 ladder move index out of range: hi={hi}, lo={lo}"
        ));
    }
    let mut prices = [0i32; 16];
    for (slot, value) in prices.iter_mut().enumerate() {
        *value = current.i32_at(0x58 + slot * 4);
    }
    if hi >= lo {
        for slot in lo..hi {
            prices[slot] = prices[slot + 1];
        }
    } else {
        for slot in (hi + 1..=lo).rev() {
            prices[slot] = prices[slot - 1];
        }
    }
    let reference = if hi == 0 {
        prices[0].wrapping_sub(2)
    } else {
        prices[hi - 1]
    };
    prices[hi] = value_delta32(reader, 0x5b2518, reference, false)?;
    for (slot, value) in prices.into_iter().enumerate() {
        current.set_i32(0x58 + slot * 4, value);
    }
    Ok(Official5188LadderMove {
        code,
        merge_flag: 0,
        continue_ladder: true,
    })
}

fn diagnostic_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn format_optional_i32(value: Option<i32>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "none".to_string(),
    }
}

fn stored_source_label(from_cache: bool, present: bool) -> &'static str {
    if from_cache {
        "frame_cache"
    } else if present {
        "resolver_record"
    } else {
        "none"
    }
}

fn ladder_anchor_source(operation: Option<u8>, value: Option<i32>) -> Option<&'static str> {
    match (operation, value) {
        (Some(b'E'), Some(999_999)) => Some("sentinel_current_last"),
        (Some(b'D' | b'M'), Some(_)) => Some("token_literal"),
        (Some(_), Some(_)) => Some("current_last_plus_delta"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Official5188LadderDecode {
    volume_mask: i32,
    flags: u8,
    layout: i32,
    price_mask: i32,
    anchor_value: i32,
    anchor_operation: u8,
}

fn merge_value_volumes(
    current: &mut Official5188InternalRecord,
    baseline: Option<&Official5188InternalRecord>,
    delta: i64,
    merge_flag: u8,
) {
    let Some(row) = baseline else { return };
    if merge_flag == 2 {
        for slot in 0..10 {
            let offset = 0xa8 + slot * 4;
            current.set_i32(
                offset,
                current.i32_at(offset).wrapping_add(row.i32_at(offset)),
            );
        }
        return;
    }
    for slot in 0..10 {
        let price = current.i32_at(0x58 + slot * 4);
        if price == 0 {
            continue;
        }
        for old_slot in 0..10 {
            if price == row.i32_at(0x58 + old_slot * 4) {
                let value = current
                    .i32_at(0xa8 + slot * 4)
                    .wrapping_add(row.i32_at(0xa8 + old_slot * 4));
                current.set_i32(0xa8 + slot * 4, value);
                break;
            }
        }
    }
    if current.i32_at(0x68) == current.last_price_integer() {
        current.set_i32(0xb8, current.i32_at(0xb8).wrapping_sub(delta as i32));
    } else if current.i32_at(0x6c) == current.last_price_integer() {
        current.set_i32(0xbc, current.i32_at(0xbc).wrapping_sub(delta as i32));
    }
}

fn decode_value_ladder(
    reader: &mut Official5188BitReader<'_>,
    current: &mut Official5188InternalRecord,
    baseline: Option<&Official5188InternalRecord>,
    mask_class: u8,
    mut volume_mask: i32,
    mut flags: u8,
) -> Result<Official5188LadderDecode, String> {
    let raw_layout = value_delta32(reader, 0x5b24b8, 0, false)?;
    if raw_layout & 0x40 != 0 {
        flags |= 1;
    }
    let layout = raw_layout & 0x3f;
    let upper = ((layout >> 3) & 7) as usize;
    let lower = (layout & 7) as usize;
    if baseline.is_none() {
        if flags & 1 != 0 {
            volume_mask = value_delta32(reader, 0x5b249c, 0, false)?;
            if volume_mask == 0x400 {
                let left = (5usize.wrapping_sub(upper)) & 31;
                let right = (5usize.wrapping_sub(lower)) & 31;
                volume_mask =
                    0x3ff_i32.wrapping_shl(left as u32) & 0x3ff_i32.wrapping_shr(right as u32);
            }
        } else {
            let left = (5usize.wrapping_sub(upper)) & 31;
            let right = (5usize.wrapping_sub(lower)) & 31;
            volume_mask =
                (0x3ff_i32.wrapping_shl(left as u32)) & (0x3ff_i32.wrapping_shr(right as u32));
        }
    }
    if mask_class == 0x18 {
        return Ok(Official5188LadderDecode {
            volume_mask,
            flags,
            layout: raw_layout,
            price_mask: 0,
            anchor_value: 0,
            anchor_operation: 0,
        });
    }
    let price_mask_table = if baseline.is_some() {
        0x5b2458
    } else {
        0x5b2410
    };
    let price_mask = value_delta32(reader, price_mask_table, 0, false)?;
    // Anchor token (Wine table 0x5b23c8). The two-bit prefix `00` carries the
    // literal 999999 which is not a price delta: Wine anchors the ladder on the
    // ask side at the last price instead (verified byte-for-byte against the
    // vendor client's memory after a cold start, 5171/5171 records).
    let anchor_table = official_5188_value_token_table(0x5b23c8).expect("ladder anchor table");
    let (anchor_value, anchor_operation) =
        decode_official_5188_token_with_operation(reader, anchor_table)?;
    let anchor_is_sentinel = anchor_operation == b'E' && anchor_value == 999_999;
    let index = if anchor_is_sentinel || upper == 0 {
        5
    } else {
        4
    };
    let anchor = if anchor_is_sentinel {
        current.last_price_integer()
    } else if matches!(anchor_operation, b'D' | b'M') {
        anchor_value
    } else {
        current.last_price_integer().wrapping_add(anchor_value)
    };
    current.set_i32(0x58 + index * 4, anchor);
    for slot in index..9 {
        let value = if (price_mask & (1 << slot)) != 0 {
            value_delta32(reader, 0x5b2368, current.i32_at(0x58 + slot * 4), false)?
        } else if slot < lower + 4 {
            current.i32_at(0x58 + slot * 4).wrapping_add(1)
        } else {
            continue;
        };
        current.set_i32(0x58 + (slot + 1) * 4, value);
    }
    for slot in (0..index).rev() {
        let value = if (price_mask & (1 << slot)) != 0 {
            value_delta32(
                reader,
                0x5b2368,
                current.i32_at(0x58 + (slot + 1) * 4),
                true,
            )?
        } else if slot >= 5usize.saturating_sub(upper) {
            current.i32_at(0x58 + (slot + 1) * 4).wrapping_sub(1)
        } else {
            continue;
        };
        current.set_i32(0x58 + slot * 4, value);
    }
    Ok(Official5188LadderDecode {
        volume_mask,
        flags,
        layout: raw_layout,
        price_mask,
        anchor_value,
        anchor_operation,
    })
}

/// Decodes the value bitstream into the vendor's 311-byte internal records.
///
/// This API intentionally returns the internal record and consumption range,
/// not a public quote. Callers must perform callback parity before publishing
/// these records as official full-push market data.
pub fn decode_official_5188_values<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
) -> Result<Vec<Official5188DecodedValueRecord>, String> {
    let outcome = decode_official_5188_values_partial_impl(
        streams,
        indexes,
        resolver,
        ValueDecodeOptions::new(MissingBaseline::Error, false, false, false, false),
    )
    .outcome;
    match outcome.error {
        Some(error) => Err(error),
        None => Ok(outcome.records),
    }
}

/// Same as [`decode_official_5188_values`], but a `mask & 1` record whose
/// symbol has no decoded baseline yet takes the fresh (metadata-relative)
/// path instead of failing the frame.
///
/// This mirrors the vendor client's global table right after a cold start,
/// which only holds 0104 seeds: the server's initial `2704` dump is decodable
/// without any prior business record. Verified on 2026-09-04 01:20 against a
/// memory snapshot of the vendor client taken after its own cold start:
/// 5171/5171 records byte-identical over `0x00..0xd8`.
///
/// Use this for sessions that start at connect (they always receive the
/// initial dump). Keep the strict variant for replaying a capture that starts
/// mid-stream, where a missing baseline means genuinely missing history.
pub fn decode_official_5188_values_with_fresh_fallback<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
) -> Result<Vec<Official5188DecodedValueRecord>, String> {
    let outcome =
        decode_official_5188_values_partial_with_fresh_fallback(streams, indexes, resolver);
    match outcome.error {
        Some(error) => Err(error),
        None => Ok(outcome.records),
    }
}

/// Completed records from a value stream plus the first later decode error.
///
/// A record is appended only after all of its fields and trailing token have
/// decoded successfully. This lets diagnostics retain valid prefix records
/// without treating a partially decoded record or failed frame as successful.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188PartialValueDecode {
    pub records: Vec<Official5188DecodedValueRecord>,
    pub error: Option<String>,
    pub omitted_tail_records: usize,
}

/// Bit boundaries consumed by each completed value record.
///
/// This is an opt-in forensic surface. Production decoding uses the regular
/// partial API and does not allocate these traces.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Official5188ValueDecodeTrace {
    pub record_index: usize,
    pub market: [u8; 2],
    pub symbol_index: u16,
    pub mask: u8,
    pub header: u8,
    pub baseline_mode: &'static str,
    pub stored_present: bool,
    pub baseline_present: bool,
    pub mask_has_bit0: bool,
    pub stored_source: &'static str,
    pub committed_state_present: bool,
    pub current_last_before: i32,
    pub stored_last_before: Option<i32>,
    pub committed_last_before: Option<i32>,
    pub baseline_last_before: Option<i32>,
    pub anchor_source: Option<&'static str>,
    pub mask_class: u8,
    pub clear_ladder: bool,
    pub raw_level_count: u8,
    pub level_count: u8,
    pub record_start: usize,
    pub header_end: usize,
    pub timestamp_end: usize,
    pub prefix_values_end: usize,
    pub ladder_move_bits: Option<(usize, usize)>,
    pub ladder_move_code: Option<i32>,
    pub ladder_merge_flag: Option<u8>,
    pub ladder_continue: Option<bool>,
    pub ladder_values_bits: Option<(usize, usize)>,
    pub ladder_layout: Option<i32>,
    pub ladder_flags: Option<u8>,
    pub ladder_price_mask: Option<i32>,
    pub ladder_anchor_value: Option<i32>,
    pub ladder_anchor_operation: Option<u8>,
    pub ladder_volume_mask: Option<i32>,
    pub ladder_volumes_end: Option<usize>,
    pub special_book_end: Option<usize>,
    pub trailing_da_end: Option<usize>,
    pub aligned_end: usize,
    pub is_eof: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188TracedPartialValueDecode {
    pub outcome: Official5188PartialValueDecode,
    pub traces: Vec<Official5188ValueDecodeTrace>,
}

/// Decodes with the production fresh-baseline policy while retaining complete
/// records that precede a later record failure.
pub fn decode_official_5188_values_partial_with_fresh_fallback<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
) -> Official5188PartialValueDecode {
    decode_official_5188_values_partial_impl(
        streams,
        indexes,
        resolver,
        ValueDecodeOptions::new(MissingBaseline::Fresh, false, false, true, false),
    )
    .outcome
}

/// Diagnostic variant that records stage boundaries for every completed
/// record. The failing record remains represented by `outcome.error`.
pub fn decode_official_5188_values_partial_with_trace<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
) -> Official5188TracedPartialValueDecode {
    decode_official_5188_values_partial_impl(
        streams,
        indexes,
        resolver,
        ValueDecodeOptions::new(MissingBaseline::Fresh, true, false, true, false),
    )
}

/// Forensic replay of Wine's bounded value-reader tail behavior.
///
/// Wine clamps a read wider than the remaining declared value stream and
/// returns zero after the final bit. Keep this opt-in until representative
/// replay and callback parity prove it suitable for the production path.
pub fn decode_official_5188_values_partial_with_wine_tail_trace<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
) -> Official5188TracedPartialValueDecode {
    decode_official_5188_values_partial_impl(
        streams,
        indexes,
        resolver,
        ValueDecodeOptions::new(MissingBaseline::Fresh, true, true, false, false),
    )
}

/// Explicit opt-in decoder implementing native Wine clamp EOF semantics.
///
/// BitReader clamps to available bits on EOF and token decoder returns 0
/// without advancing bit offset. Completed records are marked `is_eof` when
/// the record starts at or past the value-stream end, or when a clamped short
/// read / unmatched tail token supplied a synthetic zero during that record.
/// Default strict decoder is completely untouched.
pub fn decode_official_5188_values_native_wine_clamp_with_trace<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
) -> Official5188TracedPartialValueDecode {
    decode_official_5188_values_partial_impl(
        streams,
        indexes,
        resolver,
        ValueDecodeOptions::new(MissingBaseline::Fresh, true, true, false, true),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MissingBaseline {
    Error,
    Fresh,
}

#[derive(Clone, Copy)]
struct ValueDecodeOptions {
    missing_baseline: MissingBaseline,
    capture_trace: bool,
    wine_tail_clamp: bool,
    allow_omitted_baseline_tail: bool,
    allow_native_wine_eof: bool,
}

impl ValueDecodeOptions {
    const fn new(
        missing_baseline: MissingBaseline,
        capture_trace: bool,
        wine_tail_clamp: bool,
        allow_omitted_baseline_tail: bool,
        allow_native_wine_eof: bool,
    ) -> Self {
        Self {
            missing_baseline,
            capture_trace,
            wine_tail_clamp,
            allow_omitted_baseline_tail,
            allow_native_wine_eof,
        }
    }
}

fn decode_official_5188_values_partial_impl<R: Official5188BaselineResolver>(
    streams: Official5188DeltaStreams<'_>,
    indexes: &[Official5188DeltaIndexState],
    resolver: &mut R,
    options: ValueDecodeOptions,
) -> Official5188TracedPartialValueDecode {
    let ValueDecodeOptions {
        missing_baseline,
        capture_trace,
        wine_tail_clamp,
        allow_omitted_baseline_tail,
        allow_native_wine_eof,
    } = options;
    let mut decoded = Vec::with_capacity(indexes.len());
    let mut traces = capture_trace.then(|| Vec::with_capacity(indexes.len()));
    if indexes.len() != streams.record_count {
        return Official5188TracedPartialValueDecode {
            outcome: Official5188PartialValueDecode {
                records: decoded,
                error: Some(format!(
                    "5188 value/index record count mismatch: {} != {}",
                    indexes.len(),
                    streams.record_count
                )),
                omitted_tail_records: 0,
            },
            traces: traces.unwrap_or_default(),
        };
    }
    let total_stream_bits = streams.value_stream.len() * 8;
    let mut reader = if wine_tail_clamp {
        Official5188BitReader::with_wine_tail_clamp(streams.value_stream)
    } else {
        Official5188BitReader::new(streams.value_stream)
    };
    let mut cache: std::collections::HashMap<([u8; 2], u16), Official5188InternalRecord> =
        std::collections::HashMap::new();
    for (record_index, index) in indexes.iter().enumerate() {
        if allow_omitted_baseline_tail
            && reader.remaining_bits() < 13
            && indexes[record_index..]
                .iter()
                .all(|remaining| remaining.uses_baseline)
        {
            return Official5188TracedPartialValueDecode {
                outcome: Official5188PartialValueDecode {
                    records: decoded,
                    error: None,
                    omitted_tail_records: indexes.len() - record_index,
                },
                traces: traces.unwrap_or_default(),
            };
        }
        let bit_start = reader.bit_offset();
        reader.clear_eof_clamp();
        let mask = match reader.read_bits(8) {
            Ok(mask) => u8::try_from(mask).expect("8-bit mask"),
            Err(error) => {
                return Official5188TracedPartialValueDecode {
                    outcome: Official5188PartialValueDecode {
                        records: decoded,
                        error: Some(format!(
                            "5188 value record {record_index} baseline_mode=unknown stage=record_mask bit={}: {error}",
                            reader.bit_offset()
                        )),
                        omitted_tail_records: 0,
                    },
                    traces: traces.unwrap_or_default(),
                };
            }
        };
        let header_bits = match reader.read_bits(5) {
            Ok(header) => u8::try_from(header).expect("5-bit header"),
            Err(error) => {
                return Official5188TracedPartialValueDecode {
                    outcome: Official5188PartialValueDecode {
                        records: decoded,
                        error: Some(format!(
                            "5188 value record {record_index} mask={mask:#04x} baseline_mode=unknown stage=record_header bit={}: {error}",
                            reader.bit_offset()
                        )),
                        omitted_tail_records: 0,
                    },
                    traces: traces.unwrap_or_default(),
                };
            }
        };
        let header = Official5188ValueRecordHeader::decode(header_bits)
            .expect("a five-bit value always decodes as a header");
        let mask_class = mask & 0x38;
        let key = (index.market, index.symbol_index);
        let stored_from_cache = cache.get(&key).cloned();
        let stored = stored_from_cache
            .clone()
            .or_else(|| resolver.resolve(index.market, index.symbol_index));
        let committed = stored_from_cache
            .clone()
            .or_else(|| resolver.resolve_baseline(index.market, index.symbol_index));
        let stored_source = stored_source_label(stored_from_cache.is_some(), stored.is_some());
        let committed_state_present = committed.is_some();
        let stored_last_before = stored
            .as_ref()
            .map(Official5188InternalRecord::last_price_integer);
        let committed_last_before = committed
            .as_ref()
            .map(Official5188InternalRecord::last_price_integer);
        let baseline = if mask_class != 0x18 && mask & 1 != 0 {
            match committed.clone() {
                Some(record) => Some(record),
                None if missing_baseline == MissingBaseline::Fresh => None,
                None => {
                    return Official5188TracedPartialValueDecode {
                        outcome: Official5188PartialValueDecode {
                            records: decoded,
                            error: Some(format!(
                                "5188 value record {record_index} requires missing baseline for market={:?} symbol_index={}",
                                index.market, index.symbol_index
                            )),
                            omitted_tail_records: 0,
                        },
                        traces: traces.unwrap_or_default(),
                    };
                }
            }
        } else {
            None
        };
        let baseline_last_before = baseline
            .as_ref()
            .map(Official5188InternalRecord::last_price_integer);
        let baseline_mode = if baseline.is_some() {
            "relative"
        } else if mask_class != 0x18 && mask & 1 != 0 {
            "missing_fresh"
        } else {
            "absolute"
        };
        let header_end = reader.bit_offset();
        let mut current = baseline
            .clone()
            .unwrap_or_else(Official5188InternalRecord::zeroed);
        if let Some(metadata) = stored.as_ref() {
            current.bytes[0xdf..].copy_from_slice(&metadata.bytes[0xdf..]);
        }
        current.set_i32(0x00, index.timestamp as i32);
        current.bytes[0xde] = u8::from(index.uses_baseline);
        current.bytes[0xdf..0xe1].copy_from_slice(&index.symbol_index.to_le_bytes());
        current.bytes[0xe1..0xe3].copy_from_slice(&index.market);
        if header.clear_ladder {
            current.bytes[0x58..0x80].fill(0);
            current.bytes[0xa8..0xd0].fill(0);
        }
        let mut current_last_before = current.last_price_integer();
        macro_rules! value_step {
            ($stage:literal, $expression:expr) => {
                match $expression {
                    Ok(value) => value,
                    Err(error) => {
                        return Official5188TracedPartialValueDecode {
                            outcome: Official5188PartialValueDecode {
                                records: decoded,
                                error: Some(format!(
                                    "5188 value record {record_index} mask={mask:#04x} mask_class={mask_class:#04x} header={header_bits:#04x} clear_ladder={} raw_level_count={} level_count={} baseline_mode={baseline_mode} stored_present={} baseline_present={} mask_has_bit0={} stored_source={stored_source} committed_state_present={committed_state_present} current_last_before={current_last_before} stored_last_before={} committed_last_before={} baseline_last_before={} record_start={bit_start} stage={} bit={}: {error}",
                                    header.clear_ladder,
                                    header.raw_level_count,
                                    header.level_count,
                                    stored.is_some(),
                                    baseline.is_some(),
                                    mask & 1 != 0,
                                    format_optional_i32(stored_last_before),
                                    format_optional_i32(committed_last_before),
                                    format_optional_i32(baseline_last_before),
                                    $stage,
                                    reader.bit_offset()
                                )),
                                omitted_tail_records: 0,
                            },
                            traces: traces.unwrap_or_default(),
                        };
                    }
                }
            };
        }
        // Wine returns for mask class 0x18 before reading the optional
        // timestamp delta token or any value fields.
        if mask_class == 0x18 {
            let bit_end = reader.bit_offset();
            let is_eof = bit_start >= total_stream_bits || reader.eof_clamped();
            if let Some(traces) = traces.as_mut() {
                traces.push(Official5188ValueDecodeTrace {
                    record_index,
                    market: index.market,
                    symbol_index: index.symbol_index,
                    mask,
                    header: header_bits,
                    baseline_mode,
                    stored_present: stored.is_some(),
                    baseline_present: baseline.is_some(),
                    mask_has_bit0: mask & 1 != 0,
                    stored_source,
                    committed_state_present,
                    current_last_before,
                    stored_last_before,
                    committed_last_before,
                    baseline_last_before,
                    anchor_source: None,
                    mask_class,
                    clear_ladder: header.clear_ladder,
                    raw_level_count: header.raw_level_count,
                    level_count: header.level_count,
                    record_start: bit_start,
                    header_end,
                    timestamp_end: header_end,
                    prefix_values_end: header_end,
                    ladder_move_bits: None,
                    ladder_move_code: None,
                    ladder_merge_flag: None,
                    ladder_continue: None,
                    ladder_values_bits: None,
                    ladder_layout: None,
                    ladder_flags: None,
                    ladder_price_mask: None,
                    ladder_anchor_value: None,
                    ladder_anchor_operation: None,
                    ladder_volume_mask: None,
                    ladder_volumes_end: None,
                    special_book_end: None,
                    trailing_da_end: None,
                    aligned_end: header_end,
                    is_eof,
                });
            }
            cache.insert(key, current.clone());
            resolver.commit(index.market, index.symbol_index, current.clone());
            decoded.push(Official5188DecodedValueRecord {
                index: *index,
                mask,
                header,
                bit_start,
                bit_end,
                record: current,
            });
            continue;
        }
        if mask & 2 != 0 {
            current.set_i32(
                0,
                value_step!(
                    "timestamp_delta",
                    value_delta32(&mut reader, 0x5b2a38, index.timestamp as i32, false)
                ),
            );
        }
        let timestamp_end = reader.bit_offset();
        let delta = if !header.special_path {
            if matches!(mask_class, 8 | 0x20) {
                value_step!(
                    "aux",
                    decode_value_aux(&mut reader, &mut current, baseline.as_ref())
                );
            } else if mask & 0x40 != 0 {
                let previous = baseline.as_ref().map_or(0, |row| row.i32_at(0x2c));
                current.set_i32(
                    0x2c,
                    value_step!(
                        "offset_2c",
                        value_delta32(&mut reader, 0x5b29e8, previous, false)
                    ),
                );
            }
            for level in 0..usize::from(header.level_count) {
                let offset = 0x10 - level * 4;
                let previous = baseline
                    .as_ref()
                    .map_or(current.i32_at(0x12b), |row| row.i32_at(offset));
                let table = if baseline.is_some() {
                    0x5b2998
                } else {
                    0x5b2948
                };
                current.set_i32(
                    offset,
                    value_step!("ohlc", value_delta32(&mut reader, table, previous, false)),
                );
            }
            if header.has_book {
                value_step!(
                    "accumulators",
                    decode_value_accumulators(
                        &mut reader,
                        &mut current,
                        baseline.as_ref(),
                        mask_class,
                        mask,
                    )
                )
            } else {
                0
            }
        } else {
            0
        };
        let prefix_values_end = reader.bit_offset();
        current_last_before = current.last_price_integer();
        let mut ladder_move_bits = None;
        let mut ladder_move_code = None;
        let mut ladder_merge_flag = None;
        let mut ladder_continue = None;
        let mut ladder_values_bits = None;
        let mut ladder_layout = None;
        let mut ladder_flags = None;
        let mut ladder_price_mask = None;
        let mut ladder_anchor_value = None;
        let mut ladder_anchor_operation = None;
        let mut ladder_volume_mask = None;
        let mut ladder_volumes_end = None;
        let mut special_book_end = None;
        if header.clear_ladder && !matches!(mask_class, 8 | 0x10) {
            let ladder_move_start = reader.bit_offset();
            let ladder_move = if baseline.is_some() {
                value_step!(
                    "ladder_move",
                    decode_value_ladder_move(
                        &mut reader,
                        &mut current,
                        baseline.as_ref(),
                        mask_class,
                    )
                )
            } else {
                // Wine skips ladder_move when no baseline exists and enters
                // the fresh-ladder path with merge flag zero.
                Official5188LadderMove {
                    code: 0,
                    merge_flag: 0,
                    continue_ladder: true,
                }
            };
            let ladder_move_end = reader.bit_offset();
            ladder_move_bits = Some((ladder_move_start, ladder_move_end));
            ladder_move_code = Some(ladder_move.code);
            ladder_merge_flag = Some(ladder_move.merge_flag);
            ladder_continue = Some(ladder_move.continue_ladder);
            if ladder_move.continue_ladder {
                let ladder_values_start = reader.bit_offset();
                let ladder = value_step!(
                    "ladder_values",
                    decode_value_ladder(
                        &mut reader,
                        &mut current,
                        baseline.as_ref(),
                        mask_class,
                        0,
                        ladder_move.merge_flag,
                    )
                );
                let ladder_values_end = reader.bit_offset();
                ladder_values_bits = Some((ladder_values_start, ladder_values_end));
                ladder_layout = Some(ladder.layout);
                ladder_flags = Some(ladder.flags);
                ladder_price_mask = Some(ladder.price_mask);
                ladder_anchor_value = Some(ladder.anchor_value);
                ladder_anchor_operation = Some(ladder.anchor_operation);
                ladder_volume_mask = Some(ladder.volume_mask);
                if ladder.volume_mask != 0 {
                    let table = if baseline.is_some() {
                        0x5b28c0
                    } else {
                        0x5b2870
                    };
                    for slot in 0..10 {
                        if (ladder.volume_mask & (1 << slot)) != 0 {
                            let value = value_step!(
                                "ladder_volumes",
                                value_delta32(&mut reader, table, 0, false).map_err(|error| {
                                    let baseline_ladder = baseline.as_ref().map_or_else(
                                        || "none".to_string(),
                                        |row| diagnostic_hex(&row.bytes[0x58..0xd0]),
                                    );
                                    format!(
                                        "market={:?} symbol_index={} move_code={} move_bits={ladder_move_start}..{ladder_move_end} ladder_bits={ladder_values_start}..{ladder_values_end} layout={:#04x} flags={:#04x} price_mask={:#05x} anchor={}:{} volume_slot={slot} volume_mask={:#05x} baseline_ladder={baseline_ladder}: {error}",
                                        index.market,
                                        index.symbol_index,
                                        ladder_move.code,
                                        ladder.layout,
                                        ladder.flags,
                                        ladder.price_mask,
                                        char::from(ladder.anchor_operation),
                                        ladder.anchor_value,
                                        ladder.volume_mask,
                                    )
                                })
                            );
                            current.set_i32(0xa8 + slot * 4, value);
                        }
                    }
                }
                ladder_volumes_end = Some(reader.bit_offset());
            }
            merge_value_volumes(
                &mut current,
                baseline.as_ref(),
                delta,
                ladder_move.merge_flag,
            );
        }
        if header.clear_ladder && matches!(mask_class, 8 | 0x10) {
            let table = if baseline.is_some() {
                0x5b2998
            } else {
                0x5b2948
            };
            let bid_previous = baseline
                .as_ref()
                .map_or(current.i32_at(0x12b), |row| row.i32_at(0x58));
            let ask_previous = baseline
                .as_ref()
                .map_or(current.i32_at(0x12b), |row| row.i32_at(0x6c));
            current.set_i32(
                0x58,
                value_step!(
                    "special_book_prices",
                    value_delta32(&mut reader, table, bid_previous, false)
                ),
            );
            current.set_i32(
                0x6c,
                value_step!(
                    "special_book_prices",
                    value_delta32(&mut reader, table, ask_previous, false)
                ),
            );
            if mask_class == 8 {
                let volume_table = if baseline.is_some() {
                    0x5b28c0
                } else {
                    0x5b2870
                };
                let bid_volume = baseline.as_ref().map_or(0, |row| row.i32_at(0xa8));
                let ask_volume = baseline.as_ref().map_or(0, |row| row.i32_at(0xbc));
                current.set_i32(
                    0xa8,
                    value_step!(
                        "special_book_volumes",
                        value_delta32(&mut reader, volume_table, bid_volume, false)
                    ),
                );
                current.set_i32(
                    0xbc,
                    value_step!(
                        "special_book_volumes",
                        value_delta32(&mut reader, volume_table, ask_volume, false)
                    ),
                );
            }
            special_book_end = Some(reader.bit_offset());
        }
        // Every non-0x18 record consumes the trailing +0xda token before the
        // caller aligns to a byte boundary, independent of the clear flag.
        current.set_i32(
            0xda,
            value_step!(
                "trailing_da",
                value_delta32(&mut reader, 0x5b2998, current.last_price_integer(), false)
            ),
        );
        let bit_end = reader.bit_offset();
        reader.align_byte();
        let aligned_end = reader.bit_offset();
        let is_eof = bit_start >= total_stream_bits || reader.eof_clamped();
        if let Some(traces) = traces.as_mut() {
            traces.push(Official5188ValueDecodeTrace {
                record_index,
                market: index.market,
                symbol_index: index.symbol_index,
                mask,
                header: header_bits,
                baseline_mode,
                stored_present: stored.is_some(),
                baseline_present: baseline.is_some(),
                mask_has_bit0: mask & 1 != 0,
                stored_source,
                committed_state_present,
                current_last_before,
                stored_last_before,
                committed_last_before,
                baseline_last_before,
                anchor_source: ladder_anchor_source(ladder_anchor_operation, ladder_anchor_value),
                mask_class,
                clear_ladder: header.clear_ladder,
                raw_level_count: header.raw_level_count,
                level_count: header.level_count,
                record_start: bit_start,
                header_end,
                timestamp_end,
                prefix_values_end,
                ladder_move_bits,
                ladder_move_code,
                ladder_merge_flag,
                ladder_continue,
                ladder_values_bits,
                ladder_layout,
                ladder_flags,
                ladder_price_mask,
                ladder_anchor_value,
                ladder_anchor_operation,
                ladder_volume_mask,
                ladder_volumes_end,
                special_book_end,
                trailing_da_end: Some(bit_end),
                aligned_end,
                is_eof,
            });
        }
        cache.insert(key, current.clone());
        resolver.commit(index.market, index.symbol_index, current.clone());
        decoded.push(Official5188DecodedValueRecord {
            index: *index,
            mask,
            header,
            bit_start,
            bit_end,
            record: current,
        });
        if !allow_native_wine_eof && bit_end > streams.value_stream.len() * 8 {
            return Official5188TracedPartialValueDecode {
                outcome: Official5188PartialValueDecode {
                    records: decoded,
                    error: Some(format!(
                        "5188 value record {record_index} exceeded its bitstream"
                    )),
                    omitted_tail_records: 0,
                },
                traces: traces.unwrap_or_default(),
            };
        }
    }
    Official5188TracedPartialValueDecode {
        outcome: Official5188PartialValueDecode {
            records: decoded,
            error: None,
            omitted_tail_records: 0,
        },
        traces: traces.unwrap_or_default(),
    }
}

impl Official5188DeltaEnvelope {
    pub fn decode(frame: &Official5188Frame) -> Result<Self, String> {
        if frame.kind != Official5188Kind::SERVER_DELTA {
            return Err(format!(
                "5188 delta envelope requires wire kind 2704, got {}",
                frame.kind.wire_hex()
            ));
        }
        if frame.payload.len() < OFFICIAL_5188_DELTA_PREFIX_LEN {
            return Err("5188 delta payload is shorter than its prefix".to_string());
        }
        Ok(Self {
            record_count: u16::from_le_bytes(frame.payload[0..2].try_into().unwrap()),
            value_end_offset: u32::from_le_bytes(frame.payload[2..6].try_into().unwrap()),
            body: frame.payload[OFFICIAL_5188_DELTA_PREFIX_LEN..].to_vec(),
        })
    }

    pub fn split_streams(&self) -> Result<Official5188DeltaStreams<'_>, String> {
        let value_end = usize::try_from(self.value_end_offset)
            .map_err(|_| "5188 delta value offset does not fit usize")?;
        let value_len = value_end
            .checked_sub(OFFICIAL_5188_DELTA_PREFIX_LEN)
            .ok_or_else(|| {
                format!(
                    "5188 delta value offset precedes its prefix: {value_end} < {OFFICIAL_5188_DELTA_PREFIX_LEN}"
                )
            })?;
        if value_len > self.body.len() {
            return Err(format!(
                "5188 delta value stream exceeds body: {value_len} > {}",
                self.body.len()
            ));
        }
        Ok(Official5188DeltaStreams {
            record_count: usize::from(self.record_count),
            value_stream: &self.body[..value_len],
            index_stream: &self.body[value_len..],
        })
    }
}

/// Assemble fixed-size `3e04` blocks while rejecting gaps, overlaps and
/// inconsistent stream metadata. The resulting bytes remain opaque.
pub fn assemble_bulk_envelopes(
    envelopes: &[Official5188BulkEnvelope],
) -> Result<Official5188BulkStream, String> {
    if envelopes.is_empty() {
        return Err("5188 bulk sequence is empty".to_string());
    }
    let mut ordered = envelopes.to_vec();
    ordered.sort_by_key(|envelope| envelope.block_offset);
    let first = &ordered[0];
    let mut expected = ordered[0].block_offset;
    let header_word = ordered[0].header_word;
    let sequence_word = ordered[0].sequence_word;
    let mut body = Vec::with_capacity(ordered.len() * OFFICIAL_5188_BULK_BODY_LEN);
    for envelope in &ordered {
        if envelope.block_offset != expected {
            return Err(format!(
                "5188 bulk block offset gap or overlap: expected {expected}, got {}",
                envelope.block_offset
            ));
        }
        if envelope.header_word != header_word || envelope.sequence_word != sequence_word {
            return Err("5188 bulk stream metadata changed within block sequence".to_string());
        }
        let body_len =
            u32::try_from(envelope.body.len()).map_err(|_| "5188 bulk body too large")?;
        let block_end = envelope
            .block_offset
            .checked_add(body_len)
            .ok_or_else(|| "5188 bulk block offset overflow".to_string())?;
        if envelope.body.len() != OFFICIAL_5188_BULK_BODY_LEN && block_end != envelope.sequence_word
        {
            return Err(format!(
                "5188 bulk short block does not close stream: {block_end} != {}",
                envelope.sequence_word
            ));
        }
        body.extend_from_slice(&envelope.body);
        expected = block_end;
    }
    if expected != first.sequence_word {
        return Err(format!(
            "5188 bulk stream does not close declared length: {expected} != {}",
            first.sequence_word
        ));
    }
    Ok(Official5188BulkStream {
        start_offset: first.block_offset,
        header_word,
        sequence_word,
        body,
    })
}

/// Raw entry envelope observed in Wine client `2a10` subscription frames.
///
/// The ten-byte prefix and six-byte entries are stable in captures. Entry
/// bytes are intentionally opaque until matched with the requested worklist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188SubscriptionEnvelope {
    pub prefix: [u8; OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN],
    pub entries: Vec<[u8; OFFICIAL_5188_SUBSCRIPTION_ENTRY_LEN]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Official5188SubscriptionRange {
    pub market: [u8; 2],
    pub start_value: u32,
    pub count: usize,
}

impl Official5188SubscriptionEnvelope {
    /// Builds the observed structural `2a10` prefix for a current assignment.
    ///
    /// Four independent official-client lifecycles use `u32 marker=1`, the
    /// little-endian entry count, and a zero `u16` tail for all seven slots.
    pub fn structural_prefix(
        entry_count: usize,
    ) -> Result<[u8; OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN], String> {
        let entry_count = u32::try_from(entry_count)
            .map_err(|_| "5188 subscription entry count exceeds u32".to_string())?;
        let mut prefix = [0u8; OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN];
        prefix[..4].copy_from_slice(&1u32.to_le_bytes());
        prefix[4..8].copy_from_slice(&entry_count.to_le_bytes());
        Ok(prefix)
    }

    /// Builds a current-session `2a10` frame from its market/index assignment
    /// and the cross-lifecycle structural prefix.
    pub fn from_current_entries(
        entries: &[Official5188SubscriptionEntry],
    ) -> Result<Official5188Frame, String> {
        Self::from_entries(Self::structural_prefix(entries.len())?, entries)
    }

    /// Builds a `2a10` frame from a caller-owned prefix and market/index
    /// assignment. Prefer [`Self::from_current_entries`] for the observed
    /// official-client structure; this form remains available for fixtures.
    pub fn from_entries(
        prefix: [u8; OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN],
        entries: &[Official5188SubscriptionEntry],
    ) -> Result<Official5188Frame, String> {
        let declared = u32::from_le_bytes(prefix[4..8].try_into().unwrap());
        if declared != entries.len() as u32 {
            return Err(format!(
                "5188 subscription prefix declares {declared} entries, got {}",
                entries.len()
            ));
        }
        let mut payload = Vec::with_capacity(
            OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN
                + entries.len() * OFFICIAL_5188_SUBSCRIPTION_ENTRY_LEN,
        );
        payload.extend_from_slice(&prefix);
        for (market, symbol_index) in entries {
            if !matches!(market, b"SH" | b"SZ" | b"B$") {
                return Err(format!(
                    "5188 subscription market is unsupported: {:02x}{:02x}",
                    market[0], market[1]
                ));
            }
            payload.extend_from_slice(market);
            payload.extend_from_slice(&symbol_index.to_le_bytes());
        }
        Ok(Official5188Frame {
            kind: Official5188Kind::CLIENT_SUBSCRIBE,
            metadata: [0; 4],
            payload,
        })
    }

    pub fn decode(frame: &Official5188Frame) -> Result<Self, String> {
        if frame.kind != Official5188Kind::CLIENT_SUBSCRIBE {
            return Err(format!(
                "5188 subscription envelope requires wire kind 2a10, got {}",
                frame.kind.wire_hex()
            ));
        }
        if frame.payload.len() < OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN {
            return Err("5188 subscription payload is shorter than its prefix".to_string());
        }
        let rest = frame.payload.len() - OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN;
        if !rest.is_multiple_of(OFFICIAL_5188_SUBSCRIPTION_ENTRY_LEN) {
            return Err(format!(
                "5188 subscription payload has {} trailing bytes after prefix",
                rest % OFFICIAL_5188_SUBSCRIPTION_ENTRY_LEN
            ));
        }
        let prefix = frame.payload[..OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN]
            .try_into()
            .expect("validated prefix length");
        let entries = frame.payload[OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN..]
            .chunks_exact(OFFICIAL_5188_SUBSCRIPTION_ENTRY_LEN)
            .map(|chunk| chunk.try_into().expect("validated entry length"))
            .collect();
        Ok(Self { prefix, entries })
    }

    /// Returns the entry count declared by the observed prefix.
    ///
    /// Formal-primary captures use the little-endian word at bytes 4..8:
    /// `1024` for the 6154-byte form and `58` for the 358-byte form. This
    /// is structural evidence only; it does not define the entry encoding.
    #[must_use]
    pub fn declared_entry_count(&self) -> u32 {
        u32::from_le_bytes(self.prefix[4..8].try_into().expect("fixed prefix word"))
    }

    /// Returns the market bytes and raw little-endian value of one entry.
    /// The integer's mapping to a public security code is not established.
    #[must_use]
    pub fn opaque_code_value(&self, index: usize) -> Option<([u8; 2], u32)> {
        let entry = *self.entries.get(index)?;
        Some((
            entry[..2].try_into().expect("fixed market prefix"),
            u32::from_le_bytes(entry[2..].try_into().expect("fixed code value")),
        ))
    }

    #[must_use]
    pub fn consecutive_ranges(&self) -> Vec<Official5188SubscriptionRange> {
        let mut ranges: Vec<Official5188SubscriptionRange> = Vec::new();
        for (market, value) in
            (0..self.entries.len()).filter_map(|index| self.opaque_code_value(index))
        {
            match ranges.last_mut() {
                Some(range)
                    if range.market == market
                        && value == range.start_value + range.count as u32 =>
                {
                    range.count += 1;
                }
                _ => ranges.push(Official5188SubscriptionRange {
                    market,
                    start_value: value,
                    count: 1,
                }),
            }
        }
        ranges
    }
}

impl Official5188BulkEnvelope {
    pub fn decode(frame: &Official5188Frame) -> Result<Self, String> {
        if frame.kind != Official5188Kind::SERVER_META {
            return Err(format!(
                "5188 bulk envelope requires wire kind 3e04, got {}",
                frame.kind.wire_hex()
            ));
        }
        let body_len = frame.payload.len() - OFFICIAL_5188_BULK_HEADER_LEN;
        if body_len == 0 || body_len > OFFICIAL_5188_BULK_BODY_LEN {
            return Err(format!(
                "5188 bulk envelope body length out of range: {body_len}"
            ));
        }
        let block_offset = u32::from_le_bytes(frame.payload[0..4].try_into().unwrap());
        let sequence_word = u32::from_le_bytes(frame.payload[8..12].try_into().unwrap());
        let block_end = block_offset
            .checked_add(u32::try_from(body_len).map_err(|_| "5188 bulk body too large")?)
            .ok_or_else(|| "5188 bulk block offset overflow".to_string())?;
        if body_len != OFFICIAL_5188_BULK_BODY_LEN && block_end != sequence_word {
            return Err(format!(
                "5188 bulk short block does not close stream: {block_end} != {sequence_word}"
            ));
        }
        Ok(Self {
            block_offset,
            header_word: u32::from_le_bytes(frame.payload[4..8].try_into().unwrap()),
            sequence_word,
            body: frame.payload[12..].to_vec(),
        })
    }
}

/// Evidence-only hint for the first bytes of an inner payload.
///
/// This deliberately does not claim that a payload is decodable or that the
/// indicated codec is the complete 5188 object format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Official5188PayloadHint {
    ZstdMagic,
    ZlibMagic,
    Opaque,
}

/// A complete standard-zlib stream found at an offset inside an otherwise
/// opaque 5188 payload. This is evidence only; the surrounding object format
/// and the meaning of the decoded bytes remain unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Official5188EmbeddedZlibCandidate {
    pub offset: usize,
    pub decoded_len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188ZlibObjectEnvelope {
    pub uncompressed_len: usize,
    pub compressed_len: usize,
    pub decoded: Vec<u8>,
}

impl Official5188ZlibObjectEnvelope {
    /// Decodes the recurring server object shape
    /// `(uncompressed_len, compressed_len, zlib_bytes)`.
    pub fn decode(frame: &Official5188Frame) -> Result<Self, String> {
        if frame.payload.len() < 10 {
            return Err("5188 zlib object is shorter than its prefix".to_string());
        }
        let uncompressed_len = u32::from_le_bytes(frame.payload[0..4].try_into().unwrap()) as usize;
        let compressed_len = u32::from_le_bytes(frame.payload[4..8].try_into().unwrap()) as usize;
        if compressed_len.checked_add(8) != Some(frame.payload.len()) {
            return Err("5188 zlib object compressed length does not close payload".to_string());
        }
        if uncompressed_len > MAX_EMBEDDED_ZLIB_OUTPUT {
            return Err("5188 zlib object exceeds bounded output length".to_string());
        }
        if frame.payload[8] != 0x78 || !matches!(frame.payload[9], 0x01 | 0x5e | 0x9c | 0xda) {
            return Err("5188 zlib object has no zlib marker".to_string());
        }
        let mut decoder = flate2::read::ZlibDecoder::new(&frame.payload[8..]);
        let mut decoded = Vec::new();
        std::io::Read::by_ref(&mut decoder)
            .take((MAX_EMBEDDED_ZLIB_OUTPUT + 1) as u64)
            .read_to_end(&mut decoded)
            .map_err(|error| format!("5188 zlib object decode failed: {error}"))?;
        if decoded.len() != uncompressed_len {
            return Err(format!(
                "5188 zlib object output length mismatch: declared {uncompressed_len}, decoded {}",
                decoded.len()
            ));
        }
        Ok(Self {
            uncompressed_len,
            compressed_len,
            decoded,
        })
    }
}

/// Scan an opaque payload for bounded, complete zlib streams.
///
/// A `78 9c` prefix alone is not sufficient: truncated streams and random
/// bytes are rejected by the decoder. At most sixteen candidates are kept.
#[must_use]
pub fn embedded_zlib_candidates(payload: &[u8]) -> Vec<Official5188EmbeddedZlibCandidate> {
    let mut candidates = Vec::new();
    for offset in 0..payload.len().saturating_sub(1) {
        if payload[offset..].starts_with(&[0x78, 0x01])
            || payload[offset..].starts_with(&[0x78, 0x5e])
            || payload[offset..].starts_with(&[0x78, 0x9c])
            || payload[offset..].starts_with(&[0x78, 0xda])
        {
            let mut decoder = flate2::read::ZlibDecoder::new(&payload[offset..]);
            let mut decoded = Vec::new();
            let limited = std::io::Read::by_ref(&mut decoder)
                .take((MAX_EMBEDDED_ZLIB_OUTPUT + 1) as u64)
                .read_to_end(&mut decoded);
            if limited.is_ok() && decoded.len() <= MAX_EMBEDDED_ZLIB_OUTPUT {
                candidates.push(Official5188EmbeddedZlibCandidate {
                    offset,
                    decoded_len: decoded.len(),
                });
                if candidates.len() == MAX_EMBEDDED_ZLIB_CANDIDATES {
                    break;
                }
            }
        }
    }
    candidates
}

#[must_use]
pub fn payload_hint(payload: &[u8]) -> Official5188PayloadHint {
    if payload.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Official5188PayloadHint::ZstdMagic
    } else if payload.len() >= 2
        && payload[0] == 0x78
        && matches!(payload[1], 0x01 | 0x5e | 0x9c | 0xda)
    {
        Official5188PayloadHint::ZlibMagic
    } else {
        Official5188PayloadHint::Opaque
    }
}

impl Official5188Frame {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < HEADER_LEN {
            return Err(format!("5188 frame shorter than {HEADER_LEN} bytes"));
        }
        let payload_len = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
        let expected = HEADER_LEN + payload_len;
        if bytes.len() != expected {
            return Err(format!(
                "5188 frame length mismatch: header={expected}, bytes={}",
                bytes.len()
            ));
        }
        Ok(Self {
            kind: Official5188Kind(u16::from_le_bytes([bytes[0], bytes[1]])),
            metadata: bytes[4..8].try_into().expect("validated header length"),
            payload: bytes[HEADER_LEN..].to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let payload_len = u16::try_from(self.payload.len()).map_err(|_| {
            format!(
                "5188 payload exceeds u16 length field: {}",
                self.payload.len()
            )
        })?;
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(&self.kind.0.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&self.metadata);
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    #[must_use]
    pub fn direction(&self) -> Official5188Direction {
        if matches!(
            self.kind,
            Official5188Kind::CLIENT_INIT
                | Official5188Kind::CLIENT_SESSION
                | Official5188Kind::CLIENT_SUBSCRIBE
                | Official5188Kind::CLIENT_HEARTBEAT
        ) {
            Official5188Direction::Client
        } else if matches!(
            self.kind,
            Official5188Kind::SERVER_DELTA
                | Official5188Kind::SERVER_BULK
                | Official5188Kind::SERVER_BULK_ALT
                | Official5188Kind::SERVER_STATE
                | Official5188Kind::SERVER_META
                | Official5188Kind::SERVER_EXTENDED_ZLIB
                | Official5188Kind::SERVER_INIT_CONTROL
                | Official5188Kind::SERVER_INIT_CONTINUE
                | Official5188Kind::SERVER_CODE_TABLE
                | Official5188Kind::SERVER_OBJECT_3001
                | Official5188Kind::SERVER_CONTROL_0310
                | Official5188Kind::SERVER_HEARTBEAT
        ) || (0x0400..=0x04ff).contains(&self.kind.0)
        {
            Official5188Direction::Server
        } else {
            Official5188Direction::Unknown
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Official5188Direction {
    Client,
    Server,
    Unknown,
}

/// Evidence-backed stages of one Wine 5188 connection initialization.
///
/// The server payload remains opaque. Only frame kind and length are used to
/// prevent the 7100 control exchanges from being incorrectly batched ahead of
/// the data-connection responses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Official5188InitializationStage {
    Login,
    Abk,
    Ack,
}

impl Official5188InitializationStage {
    #[must_use]
    pub const fn client_payload_len(self) -> usize {
        match self {
            Self::Login => 95,
            Self::Abk => 94,
            Self::Ack => 67,
        }
    }

    #[must_use]
    pub const fn accepts_client_payload_len(self, payload_len: usize) -> bool {
        match self {
            // Observed ACK manifests produce 44B, 63B and 67B 3610
            // payloads; the length scales with manifest contents.
            Self::Ack => (payload_len >= 40) && (payload_len <= 80),
            Self::Abk => matches!(payload_len, 94 | 106),
            Self::Login => payload_len == 95,
        }
    }

    #[must_use]
    pub const fn server_kind(self) -> Official5188Kind {
        match self {
            Self::Login | Self::Abk => Official5188Kind::SERVER_INIT_CONTROL,
            Self::Ack => Official5188Kind::SERVER_INIT_CONTINUE,
        }
    }

    #[must_use]
    pub const fn accepts_server_payload_len(self, payload_len: usize) -> bool {
        match self {
            Self::Login => payload_len == 48,
            Self::Abk => payload_len >= 600 && payload_len <= 680,
            Self::Ack => payload_len >= 450 && payload_len <= 480,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Official5188Handshake {
    client_frames: Vec<Official5188Kind>,
}

impl Official5188Handshake {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_client(&mut self, frame: &Official5188Frame) -> Result<(), String> {
        if frame.direction() != Official5188Direction::Client {
            return Err(format!("expected 5188 client frame, got {}", frame.kind));
        }
        self.client_frames.push(frame.kind);
        Ok(())
    }

    #[must_use]
    pub fn client_frame_count(&self) -> usize {
        self.client_frames.len()
    }

    #[must_use]
    pub fn matches_wine_shape(&self) -> bool {
        let kinds = &self.client_frames;
        kinds.len() >= 6
            && kinds[..3]
                .iter()
                .all(|kind| *kind == Official5188Kind::CLIENT_INIT)
            && kinds[3..6]
                .iter()
                .all(|kind| *kind == Official5188Kind::CLIENT_SESSION)
            && kinds[6..].iter().all(|kind| {
                *kind == Official5188Kind::CLIENT_HEARTBEAT
                    || *kind == Official5188Kind::CLIENT_SUBSCRIBE
            })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Official5188Reassembler {
    buffer: Vec<u8>,
}

/// A bounded blocking transport for the authenticated 5188 data connection.
///
/// Authentication and server-list discovery remain owned by the product
/// integration. The transport does not invent credentials or silently fall
/// back to 7709.
pub struct Official5188Session {
    endpoint: String,
    stream: TcpStream,
    decoder: Official5188Reassembler,
    pending_frames: VecDeque<Official5188Frame>,
}

impl Official5188Session {
    pub fn connect_authenticated(
        endpoint: impl Into<String>,
        timeout: Duration,
        authenticated: bool,
    ) -> Result<Self, String> {
        let endpoint = endpoint.into();
        if !authenticated {
            return Err("5188 session requires completed authentication".to_string());
        }
        let socket = parse_socket_endpoint(&endpoint)
            .ok_or_else(|| format!("invalid 5188 endpoint: {endpoint}"))?;
        if matches!(socket.port(), 7709 | 547) {
            return Err(format!(
                "{} endpoint is supplementation, not official full-push",
                socket.port()
            ));
        }
        Self::connect(endpoint, timeout)
    }

    pub fn connect(endpoint: impl Into<String>, timeout: Duration) -> Result<Self, String> {
        let endpoint = endpoint.into();
        let address = endpoint
            .to_socket_addrs()
            .map_err(|error| format!("resolve 5188 endpoint {endpoint}: {error}"))?
            .next()
            .ok_or_else(|| format!("5188 endpoint has no resolved address: {endpoint}"))?;
        let stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|error| format!("connect 5188 endpoint {endpoint}: {error}"))?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| format!("set 5188 read timeout: {error}"))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|error| format!("set 5188 write timeout: {error}"))?;
        Ok(Self {
            endpoint,
            stream,
            decoder: Official5188Reassembler::new(),
            pending_frames: VecDeque::new(),
        })
    }

    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn try_clone_stream(&self) -> std::io::Result<TcpStream> {
        self.stream.try_clone()
    }

    pub fn send(&mut self, frame: &Official5188Frame) -> Result<(), String> {
        self.stream
            .write_all(&frame.encode()?)
            .map_err(|error| format!("write 5188 frame to {}: {error}", self.endpoint))
    }

    /// Re-establishes this session on the same authenticated endpoint.
    ///
    /// The old decoder buffer is discarded so an incomplete frame from a
    /// closed TCP connection cannot be joined with bytes from the new session.
    /// Callers must re-send the captured Wine initialization sequence after a
    /// successful reconnect.
    pub fn reconnect_authenticated(
        &mut self,
        timeout: Duration,
        authenticated: bool,
    ) -> Result<(), String> {
        let replacement =
            Self::connect_authenticated(self.endpoint.clone(), timeout, authenticated)?;
        self.stream = replacement.stream;
        self.decoder = replacement.decoder;
        self.pending_frames.clear();
        Ok(())
    }

    /// Sends one current-session `3610` and waits for its immediate Wine
    /// server initialization response.
    ///
    /// Real captures interleave all three stages. In particular, the second
    /// `3110` arrives before the ACK-stage 7100 request can be constructed, so
    /// collecting `3610 x3` before reading 5188 is not a valid runtime model.
    pub fn exchange_initialization_stage(
        &mut self,
        stage: Official5188InitializationStage,
        client_frame: &Official5188Frame,
    ) -> Result<Official5188Frame, String> {
        if client_frame.kind != Official5188Kind::CLIENT_INIT
            || !stage.accepts_client_payload_len(client_frame.payload.len())
        {
            return Err(format!(
                "5188 {stage:?} stage rejected 3610 payload length, got {} with {} bytes",
                client_frame.kind.wire_hex(),
                client_frame.payload.len()
            ));
        }
        self.send(client_frame)?;
        let server_frame = self.read_next_frame()?;
        if server_frame.kind != stage.server_kind()
            || !stage.accepts_server_payload_len(server_frame.payload.len())
        {
            return Err(format!(
                "5188 {stage:?} stage expected {} control payload, got {} with {} bytes",
                stage.server_kind().wire_hex(),
                server_frame.kind.wire_hex(),
                server_frame.payload.len()
            ));
        }
        Ok(server_frame)
    }

    /// Sends a current-session Wine-shaped initialization sequence.
    ///
    /// Payload bytes must be generated from the same authenticated session.
    /// Captured sequences are valid test fixtures but are not reusable runtime
    /// defaults: real captures show per-connection variation in `3610`, `2d10`
    /// and `2a10`. This method deliberately does not invent those values or
    /// perform 7709 fallback.
    pub fn send_wine_initialization(&mut self, frames: &[Official5188Frame]) -> Result<(), String> {
        let mut handshake = Official5188Handshake::new();
        for frame in frames {
            handshake.observe_client(frame)?;
        }
        if !handshake.matches_wine_shape() {
            return Err("5188 initialization does not match Wine frame shape".to_string());
        }
        for (index, frame) in frames[3..6].iter().enumerate() {
            Official5188ClientSessionEnvelope::decode(frame).map_err(|error| {
                format!("invalid 5188 client session frame {}: {error}", index + 1)
            })?;
        }
        for frame in frames {
            self.send(frame)?;
        }
        Ok(())
    }

    /// Sends the Wine initialization tail after the three interleaved `3610`
    /// exchanges have completed.
    ///
    /// The first three frames must be the current-session `2d10` envelopes.
    /// Optional heartbeat/subscription frames may follow. This separate entry
    /// point prevents a live orchestrator from re-sending the three `3610`
    /// frames merely to reuse [`Self::send_wine_initialization`].
    pub fn send_wine_post_initialization(
        &mut self,
        frames: &[Official5188Frame],
    ) -> Result<(), String> {
        if frames.len() < 3
            || frames[..3]
                .iter()
                .any(|frame| frame.kind != Official5188Kind::CLIENT_SESSION)
            || frames[3..].iter().any(|frame| {
                !matches!(
                    frame.kind,
                    Official5188Kind::CLIENT_HEARTBEAT | Official5188Kind::CLIENT_SUBSCRIBE
                )
            })
        {
            return Err(
                "5188 post-initialization requires 2d10 x3 followed only by 0710/2a10".to_string(),
            );
        }
        for (index, frame) in frames[..3].iter().enumerate() {
            Official5188ClientSessionEnvelope::decode(frame).map_err(|error| {
                format!("invalid 5188 client session frame {}: {error}", index + 1)
            })?;
        }
        for frame in frames {
            self.send(frame)?;
        }
        Ok(())
    }

    pub fn read_frames(&mut self) -> Result<Vec<Official5188Frame>, String> {
        let mut bytes = [0u8; 64 * 1024];
        let count = self
            .stream
            .read(&mut bytes)
            .map_err(|error| format!("read 5188 frames from {}: {error}", self.endpoint))?;
        if count == 0 {
            return Err(format!(
                "5188 endpoint {} closed the connection",
                self.endpoint
            ));
        }
        self.decoder.push(&bytes[..count])
    }

    pub fn receive_once(&mut self) -> Result<Vec<Official5188Frame>, String> {
        self.read_frames()
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.stream
            .shutdown(std::net::Shutdown::Both)
            .map_err(|error| format!("stop 5188 endpoint {}: {error}", self.endpoint))
    }

    fn read_next_frame(&mut self) -> Result<Official5188Frame, String> {
        loop {
            if let Some(frame) = self.pending_frames.pop_front() {
                return Ok(frame);
            }
            let frames = self.read_frames()?;
            self.pending_frames.extend(frames);
        }
    }

    /// Receives the server-provided SH/SZ/B$ `0104` objects required to build
    /// this connection's current-session `2d10 x3`.
    ///
    /// Captures can place other server objects between `3210` and the three
    /// code tables. Those frames are returned in arrival order. Collection is
    /// bounded so a changed server sequence cannot grow memory indefinitely.
    pub fn receive_initial_code_tables(&mut self) -> Result<Official5188InitialCodeTables, String> {
        let mut headers = Vec::with_capacity(3);
        let mut code_tables = Vec::with_capacity(4);
        let mut observed_frames = Vec::new();
        let mut code_table_count = 0;
        while code_table_count < 4 {
            if observed_frames.len() >= MAX_INITIAL_CODE_TABLE_FRAMES {
                return Err(format!(
                    "5188 initial code-table collection exceeded {MAX_INITIAL_CODE_TABLE_FRAMES} frames"
                ));
            }
            let frame = self.read_next_frame()?;
            if frame.kind == Official5188Kind::SERVER_CODE_TABLE {
                code_table_count += 1;
                let object = Official5188ZlibObjectEnvelope::decode(&frame)?;
                if object.decoded.len() < 14 {
                    return Err("5188 initial code-table object has no routing header".to_string());
                }
                let code_table = Official5188CodeTable::decode(&object.decoded)?;
                let market = code_table.market;
                if code_tables
                    .iter()
                    .any(|current: &Official5188CodeTable| current.market == market)
                {
                    return Err(format!(
                        "5188 initial code-table repeats market {}{}",
                        market[0] as char, market[1] as char
                    ));
                }
                if matches!(market, [b'S', b'H'] | [b'S', b'Z'] | [b'B', b'$']) {
                    let header = Official5188CodeTableHeader::decode(&object.decoded)?;
                    headers.push(header);
                }
                code_tables.push(code_table);
            }
            observed_frames.push(frame);
        }
        if headers.len() != 3 {
            return Err(format!(
                "5188 initial code-table set requires SH/SZ/B$ headers, got {}",
                headers.len()
            ));
        }
        let headers: [Official5188CodeTableHeader; 3] = headers
            .try_into()
            .map_err(|_| "5188 initial code-table collection is incomplete".to_string())?;
        build_official_5188_client_session_triplet(&headers)
            .map_err(|error| format!("5188 initial code-table set is inconsistent: {error}"))?;
        Ok(Official5188InitialCodeTables {
            headers,
            code_tables,
            observed_frames,
        })
    }

    /// Drives the authenticated stream until the peer closes it or the callback fails.
    ///
    /// The callback receives complete application frames only; TCP read and
    /// application-frame boundaries remain internal to this session. A peer
    /// close is returned as an error so callers can apply an explicit reconnect
    /// policy instead of silently switching to a supplementation endpoint.
    pub fn receive_until_disconnect<F>(&mut self, mut on_frame: F) -> Result<(), String>
    where
        F: FnMut(Official5188Frame) -> Result<(), String>,
    {
        loop {
            for frame in self.read_frames()? {
                on_frame(frame)?;
            }
        }
    }
}

#[must_use]
pub fn parse_socket_endpoint(endpoint: &str) -> Option<SocketAddr> {
    endpoint.to_socket_addrs().ok()?.next()
}

impl Official5188Reassembler {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one or more TCP payloads and returns every complete application frame.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Official5188Frame>, String> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        loop {
            if self.buffer.len() < HEADER_LEN {
                break;
            }
            let kind = u16::from_le_bytes([self.buffer[0], self.buffer[1]]);
            let outer_payload_len =
                usize::from(u16::from_le_bytes([self.buffer[2], self.buffer[3]]));
            let mut payload_len = outer_payload_len;
            let declared_frame_len = HEADER_LEN + outer_payload_len;
            // Several server object kinds (observed `1504` and `0104`) use
            // the same extended envelope once their compressed body exceeds
            // u16. Detect the envelope by both its zlib prefix and modulo
            // length invariant instead of guessing a growing kind allowlist.
            {
                // The full-length field and zlib marker are inside the payload.
                // Wait for that prefix before deciding whether this is the
                // evidence-backed extended shape. A complete short frame is
                // never an extended envelope, so parse it without delaying
                // for the optional 10-byte extended prefix.
                if self.buffer.len() < HEADER_LEN + 10 {
                    if self.buffer.len() < declared_frame_len {
                        break;
                    }
                } else {
                    let inner_len = u32::from_le_bytes(
                        self.buffer[HEADER_LEN + 4..HEADER_LEN + 8]
                            .try_into()
                            .expect("validated extended prefix"),
                    ) as usize;
                    let extended_len = 8usize
                        .checked_add(inner_len)
                        .ok_or_else(|| "5188 extended zlib length overflow".to_string())?;
                    let has_zlib_prefix = self.buffer[HEADER_LEN + 8] == 0x78
                        && matches!(self.buffer[HEADER_LEN + 9], 0x01 | 0x5e | 0x9c | 0xda);
                    if has_zlib_prefix
                        && extended_len > u16::MAX as usize
                        && (extended_len & 0xffff) == outer_payload_len
                    {
                        payload_len = extended_len;
                    }
                }
            }
            if payload_len > MAX_PAYLOAD_LEN {
                return Err(format!("5188 frame payload too large: {payload_len}"));
            }
            let frame_len = HEADER_LEN + payload_len;
            if self.buffer.len() < frame_len {
                break;
            }
            let metadata = self.buffer[4..8]
                .try_into()
                .expect("validated header length");
            let payload = self.buffer[HEADER_LEN..frame_len].to_vec();
            self.buffer.drain(..frame_len);
            frames.push(Official5188Frame {
                kind: Official5188Kind(kind),
                metadata,
                payload,
            });
        }
        Ok(frames)
    }

    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::net::TcpListener;
    use std::thread;

    fn code_table_frame(market: [u8; 2], version: u32) -> Official5188Frame {
        let mut decoded = vec![0u8; OFFICIAL_5188_CODE_TABLE_HEADER_LEN];
        decoded[0..2].copy_from_slice(&0x010a_u16.to_le_bytes());
        decoded[2..4].copy_from_slice(&0xb246_u16.to_le_bytes());
        decoded[4..8].copy_from_slice(&version.to_le_bytes());
        decoded[8..12].copy_from_slice(&1_u32.to_le_bytes());
        decoded[12..14].copy_from_slice(&market);
        decoded[88..90].copy_from_slice(&0x2826_u16.to_le_bytes());
        decoded[90..92].copy_from_slice(&0x0135_u16.to_le_bytes());
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&decoded).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut payload = Vec::with_capacity(8 + compressed.len());
        payload.extend_from_slice(&(decoded.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        payload.extend_from_slice(&compressed);
        Official5188Frame {
            kind: Official5188Kind::SERVER_CODE_TABLE,
            metadata: [0; 4],
            payload,
        }
    }

    fn subscription_code_table(
        market: [u8; 2],
        codes: impl IntoIterator<Item = String>,
    ) -> Official5188CodeTable {
        Official5188CodeTable {
            market,
            records: codes
                .into_iter()
                .enumerate()
                .map(|(index, code)| Official5188CodeTableRecord {
                    symbol_index: u16::try_from(index).unwrap(),
                    code,
                    name: String::new(),
                    amount_mode: 0,
                    opaque_tail: [0; 23],
                })
                .collect(),
        }
    }

    #[test]
    fn parses_frames_split_across_tcp_segments() {
        let first = Official5188Frame {
            kind: Official5188Kind::CLIENT_INIT,
            metadata: [0, 0, 3, 0],
            payload: vec![1, 2, 3],
        }
        .encode()
        .unwrap();
        let second = Official5188Frame {
            kind: Official5188Kind::SERVER_DELTA,
            metadata: [0; 4],
            payload: vec![9; 12],
        }
        .encode()
        .unwrap();
        let mut stream = Official5188Reassembler::new();
        assert!(stream.push(&first[..5]).unwrap().is_empty());
        let mut frames = stream.push(&first[5..]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].direction(), Official5188Direction::Client);
        frames.extend(stream.push(&second).unwrap());
        assert_eq!(frames[1].kind, Official5188Kind::SERVER_DELTA);
        assert_eq!(frames[1].direction(), Official5188Direction::Server);
        assert_eq!(stream.buffered_len(), 0);
    }

    #[test]
    fn reassembler_preserves_metadata_until_complete_frame() {
        let frame = Official5188Frame {
            kind: Official5188Kind::SERVER_BULK,
            metadata: [0x12, 0x34, 0x56, 0x78],
            payload: vec![0xaa; 32],
        };
        let encoded = frame.encode().unwrap();
        let mut stream = Official5188Reassembler::new();
        assert!(stream.push(&encoded[..7]).unwrap().is_empty());
        assert_eq!(stream.buffered_len(), 7);
        let parsed = stream.push(&encoded[7..]).unwrap();
        assert_eq!(parsed, vec![frame]);
        assert_eq!(parsed[0].metadata, [0x12, 0x34, 0x56, 0x78]);
        assert_eq!(stream.buffered_len(), 0);
    }

    #[test]
    fn reassembles_extended_1504_payload_after_u16_length_wrap() {
        let inner_len = 65_600usize;
        let payload_len = 8 + inner_len;
        let mut encoded = Vec::with_capacity(HEADER_LEN + payload_len);
        encoded.extend_from_slice(&Official5188Kind::SERVER_EXTENDED_ZLIB.0.to_le_bytes());
        encoded.extend_from_slice(&((payload_len & 0xffff) as u16).to_le_bytes());
        encoded.extend_from_slice(&[1, 0, 0x10, 0]);
        encoded.extend_from_slice(&[0x45, 0x14, 0x07, 0]);
        encoded.extend_from_slice(&(inner_len as u32).to_le_bytes());
        encoded.extend_from_slice(&[0x78, 0x9c]);
        encoded.resize(HEADER_LEN + payload_len, 0xa5);

        let split = HEADER_LEN + (payload_len & 0xffff);
        let mut stream = Official5188Reassembler::new();
        assert!(stream.push(&encoded[..split]).unwrap().is_empty());
        assert_eq!(stream.buffered_len(), split);
        let frames = stream.push(&encoded[split..]).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].kind, Official5188Kind::SERVER_EXTENDED_ZLIB);
        assert_eq!(frames[0].payload.len(), payload_len);
        assert_eq!(frames[0].direction(), Official5188Direction::Server);
        assert_eq!(stream.buffered_len(), 0);
    }

    #[test]
    fn decodes_length_closed_zlib_object_envelope() {
        let plain = b"verified 5188 object boundary".repeat(64);
        let compressed = {
            use std::io::Write as _;
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&plain).unwrap();
            encoder.finish().unwrap()
        };
        let mut payload = Vec::new();
        payload.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        payload.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        payload.extend_from_slice(&compressed);
        let object = Official5188ZlibObjectEnvelope::decode(&Official5188Frame {
            kind: Official5188Kind::SERVER_EXTENDED_ZLIB,
            metadata: [0, 0, 0x10, 0],
            payload,
        })
        .unwrap();
        assert_eq!(object.uncompressed_len, plain.len());
        assert_eq!(object.decoded, plain);
    }

    #[test]
    fn extracts_complete_client_frames_from_control_payload() {
        let init = Official5188Frame {
            kind: Official5188Kind::CLIENT_INIT,
            metadata: [0x11, 0x22, 0x33, 0x44],
            payload: vec![0xa5; 95],
        };
        let session = Official5188Frame {
            kind: Official5188Kind::CLIENT_SESSION,
            metadata: [0x55, 0x66, 0x77, 0x88],
            payload: vec![0x5a; 32],
        };
        let truncated = Official5188Frame {
            kind: Official5188Kind::CLIENT_SUBSCRIBE,
            metadata: [9, 8, 7, 6],
            payload: vec![3; 24],
        }
        .encode()
        .unwrap();

        let mut control_payload = vec![0xcc; 17];
        let init_offset = control_payload.len();
        control_payload.extend(init.encode().unwrap());
        control_payload.extend([0xde, 0xad, 0xbe]);
        let session_offset = control_payload.len();
        control_payload.extend(session.encode().unwrap());
        control_payload.extend_from_slice(&truncated[..truncated.len() - 5]);

        assert_eq!(
            embedded_client_frames(&control_payload),
            vec![
                Official5188EmbeddedClientFrame {
                    offset: init_offset,
                    frame: init,
                },
                Official5188EmbeddedClientFrame {
                    offset: session_offset,
                    frame: session,
                },
            ]
        );
    }

    #[test]
    fn preserves_unknown_server_payload() {
        let frame = Official5188Frame {
            kind: Official5188Kind(0x9999),
            metadata: [1, 2, 3, 4],
            payload: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let mut stream = Official5188Reassembler::new();
        let parsed = stream.push(&frame.encode().unwrap()).unwrap();
        assert_eq!(parsed, vec![frame]);
        assert_eq!(parsed[0].direction(), Official5188Direction::Unknown);
    }

    #[test]
    fn payload_hint_is_only_a_magic_classification() {
        assert_eq!(
            payload_hint(&[0x28, 0xb5, 0x2f, 0xfd]),
            Official5188PayloadHint::ZstdMagic
        );
        assert_eq!(
            payload_hint(&[0x78, 0x9c]),
            Official5188PayloadHint::ZlibMagic
        );
        assert_eq!(
            payload_hint(&[0x28, 0xb5, 0x2f]),
            Official5188PayloadHint::Opaque
        );
    }

    #[test]
    fn wire_hex_preserves_capture_byte_order() {
        let kind = Official5188Kind(0x040d);
        assert_eq!(kind.to_string(), "0x040d");
        assert_eq!(kind.wire_hex(), "0d04");
        let frame = Official5188Frame::decode(&[0x0d, 0x04, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(frame.kind.wire_hex(), "0d04");
    }

    #[test]
    fn embedded_zlib_candidates_require_complete_streams() {
        let source = b"5188 candidate".repeat(32);
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut encoder, &source).expect("write source");
        let compressed = encoder.finish().expect("finish zlib");
        let mut payload = vec![0xa5; 11];
        payload.extend_from_slice(&compressed);
        let candidates = embedded_zlib_candidates(&payload);
        assert_eq!(
            candidates,
            vec![Official5188EmbeddedZlibCandidate {
                offset: 11,
                decoded_len: source.len(),
            }]
        );
        payload.truncate(payload.len() - 2);
        assert!(embedded_zlib_candidates(&payload).is_empty());
    }

    #[test]
    fn assembles_contiguous_bulk_blocks_and_rejects_gaps() {
        let block = |offset, byte| Official5188BulkEnvelope {
            block_offset: offset,
            header_word: 7,
            sequence_word: 10_240,
            body: vec![byte; OFFICIAL_5188_BULK_BODY_LEN],
        };
        let stream = assemble_bulk_envelopes(&[block(5_120, 2), block(0, 1)])
            .expect("contiguous bulk blocks");
        assert_eq!(stream.start_offset, 0);
        assert_eq!(stream.body.len(), OFFICIAL_5188_BULK_BODY_LEN * 2);
        assert_eq!(stream.body[0], 1);
        assert!(assemble_bulk_envelopes(&[block(0, 1), block(10_240, 2)]).is_err());

        let tail = Official5188BulkEnvelope {
            block_offset: 5_120,
            header_word: 7,
            sequence_word: 6_000,
            body: vec![3; 880],
        };
        let mut first = block(0, 1);
        first.sequence_word = 6_000;
        let stream = assemble_bulk_envelopes(&[tail.clone(), first]).expect("short closing block");
        assert_eq!(stream.body.len(), 6_000);

        let mut invalid_tail = tail;
        invalid_tail.sequence_word = 6_001;
        assert!(
            Official5188BulkEnvelope::decode(&Official5188Frame {
                kind: Official5188Kind::SERVER_META,
                metadata: [0; 4],
                payload: {
                    let mut payload = Vec::new();
                    payload.extend_from_slice(&invalid_tail.block_offset.to_le_bytes());
                    payload.extend_from_slice(&invalid_tail.header_word.to_le_bytes());
                    payload.extend_from_slice(&invalid_tail.sequence_word.to_le_bytes());
                    payload.extend_from_slice(&invalid_tail.body);
                    payload
                },
            })
            .is_err()
        );
    }

    #[test]
    fn decodes_observed_2704_record_count_and_value_end_offset() {
        let frame = Official5188Frame {
            kind: Official5188Kind::SERVER_DELTA,
            metadata: [0; 4],
            payload: vec![7, 0, 8, 0, 0, 0, 0x81, 0x1e, 0x84, 0x21],
        };
        let envelope = Official5188DeltaEnvelope::decode(&frame).expect("2704 envelope");
        assert_eq!(envelope.record_count, 7);
        assert_eq!(envelope.value_end_offset, 8);
        assert_eq!(envelope.body, vec![0x81, 0x1e, 0x84, 0x21]);
    }

    #[test]
    fn splits_2704_streams_at_payload_relative_offset() {
        let envelope = Official5188DeltaEnvelope {
            record_count: 3,
            value_end_offset: 10,
            body: vec![1, 2, 3, 4, 0x84, 0x21],
        };
        let streams = envelope.split_streams().expect("valid delta streams");
        assert_eq!(streams.record_count, 3);
        assert_eq!(streams.value_stream, [1, 2, 3, 4]);
        assert_eq!(streams.index_stream, [0x84, 0x21]);

        let mut invalid = envelope.clone();
        invalid.value_end_offset = 5;
        assert!(invalid.split_streams().is_err());
        invalid.value_end_offset = 13;
        assert!(invalid.split_streams().is_err());
    }

    #[test]
    fn bit_reader_matches_wine_msb_first_order() {
        let mut reader = Official5188BitReader::new(&[0b1011_0010, 0b0110_0000]);
        assert_eq!(reader.read_bits(3).unwrap(), 0b101);
        assert_eq!(reader.read_bits(6).unwrap(), 0b100100);
        assert_eq!(reader.read_bits(4).unwrap(), 0b1100);
        assert_eq!(reader.bit_offset(), 13);
        assert_eq!(reader.remaining_bits(), 3);
        assert!(reader.read_bits(4).is_err());
    }

    #[test]
    fn wine_tail_eof_clamp_marks_short_read_and_unmatched_token() {
        let mut reader = Official5188BitReader::with_wine_tail_clamp(&[0b1011_0010]);
        assert!(!reader.eof_clamped());
        assert_eq!(reader.read_bits(5).unwrap(), 0b10110);
        assert!(!reader.eof_clamped());
        assert_eq!(reader.read_bits(6).unwrap(), 0b010);
        assert!(reader.eof_clamped());
        reader.clear_eof_clamp();
        assert!(!reader.eof_clamped());
        assert_eq!(reader.read_bits(32).unwrap(), 0);
        assert!(reader.eof_clamped());
        reader.clear_eof_clamp();
        let tokens = [Official5188Token {
            prefix: 0,
            prefix_bits: 1,
            value_bits: 0,
            operation: b'E',
            parameter: 99,
            base: 0,
        }];
        assert_eq!(
            decode_official_5188_token_with_operation(&mut reader, &tokens).unwrap(),
            (0, b'D')
        );
        assert!(reader.eof_clamped());
    }

    #[test]
    fn wine_tail_bit_reader_clamps_partial_raw_value() {
        let mut reader = Official5188BitReader::with_wine_tail_clamp(&[0b1011_0010]);
        assert_eq!(reader.read_bits(5).unwrap(), 0b10110);
        assert_eq!(reader.read_bits(6).unwrap(), 0b010);
        assert_eq!(reader.bit_offset(), 8);
        assert_eq!(reader.remaining_bits(), 0);
    }

    #[test]
    fn wine_tail_bit_reader_returns_zero_at_end_without_advancing() {
        let mut reader = Official5188BitReader::with_wine_tail_clamp(&[0]);
        assert_eq!(reader.read_bits(8).unwrap(), 0);
        assert_eq!(reader.bit_offset(), 8);
        assert_eq!(reader.read_bits(32).unwrap(), 0);
        assert_eq!(reader.bit_offset(), 8);
    }

    #[test]
    fn wine_tail_token_no_match_returns_zero_without_advancing() {
        let tokens = [Official5188Token {
            prefix: 0,
            prefix_bits: 1,
            value_bits: 0,
            operation: b'E',
            parameter: 99,
            base: 0,
        }];
        let mut reader = Official5188BitReader::with_wine_tail_clamp(&[]);
        assert_eq!(
            decode_official_5188_token_with_operation(&mut reader, &tokens).unwrap(),
            (0, b'D')
        );
        assert_eq!(reader.bit_offset(), 0);
    }

    #[test]
    fn public_value_token_decoder_preserves_bit_and_alignment_state() {
        let tokens = [Official5188Token {
            prefix: 0b10,
            prefix_bits: 2,
            value_bits: 4,
            operation: b'b',
            parameter: 4,
            base: 0,
        }];
        let mut reader = Official5188BitReader::new(&[0b1000_1100]);
        assert_eq!(decode_official_5188_token(&mut reader, &tokens).unwrap(), 3);
        assert_eq!(reader.bit_offset(), 6);
        reader.align_byte();
        assert_eq!(reader.bit_offset(), 8);
    }

    #[test]
    fn uppercase_s_token_applies_shift_and_base() {
        // 0x5b24b8 prefix 110 plus raw 11 selects S(raw << 0) + 13.
        let mut reader = Official5188BitReader::new(&[0b1101_1000]);
        let tokens = official_5188_value_token_table(0x5b24b8).unwrap();
        assert_eq!(
            decode_official_5188_token_with_operation(&mut reader, tokens).unwrap(),
            (16, b'S')
        );
        assert_eq!(reader.bit_offset(), 5);
    }

    #[test]
    fn uppercase_p_token_applies_power_and_base() {
        // 0x5b2458 prefix 110 plus raw 1001 selects (1 << 9) + 16.
        let mut reader = Official5188BitReader::new(&[0b1101_0010]);
        let tokens = official_5188_value_token_table(0x5b2458).unwrap();
        assert_eq!(
            decode_official_5188_token_with_operation(&mut reader, tokens).unwrap(),
            (528, b'P')
        );
        assert_eq!(reader.bit_offset(), 7);
    }

    #[test]
    fn lowercase_m_token_unconditionally_fills_high_bits() {
        // 0x5b2368 prefix 11110 plus raw 0x30 must become 0xffff_ff30.
        let mut reader = Official5188BitReader::new(&[0xf1, 0x80]);
        let tokens = official_5188_value_token_table(0x5b2368).unwrap();
        assert_eq!(
            decode_official_5188_token_with_operation(&mut reader, tokens).unwrap(),
            (-208, b'm')
        );
        assert_eq!(reader.bit_offset(), 13);
    }

    #[test]
    fn map_baseline_resolver_round_trips_internal_record() {
        let bytes = [7u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        let record = Official5188InternalRecord::decode(&bytes).unwrap();
        let mut resolver = Official5188MapBaselineResolver::new();
        resolver.insert(*b"SH", 24661, record.clone());
        assert_eq!(resolver.len(), 1);
        assert_eq!(resolver.resolve(*b"SH", 24661), Some(record));
        assert!(resolver.resolve(*b"SZ", 24661).is_none());
    }

    #[test]
    fn metadata_seed_is_not_usable_as_relative_baseline() {
        let table = Official5188CodeTable {
            market: *b"SH",
            records: vec![Official5188CodeTableRecord {
                symbol_index: 24661,
                code: "603059".into(),
                name: "fixture".into(),
                amount_mode: 1,
                opaque_tail: [0; 23],
            }],
        };
        let mut resolver = Official5188MapBaselineResolver::new();
        assert_eq!(resolver.seed_code_tables(std::slice::from_ref(&table)), 1);
        assert!(resolver.resolve(*b"SH", 24661).is_some());
        assert!(resolver.resolve_baseline(*b"SH", 24661).is_none());
    }

    #[test]
    fn map_baseline_resolver_persists_mask_18_outer_copyback() {
        let record = Official5188InternalRecord::zeroed();
        let index = Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        };
        let decoded = [Official5188DecodedValueRecord {
            index,
            mask: 0x18,
            header: Official5188ValueRecordHeader::decode(0).unwrap(),
            bit_start: 0,
            bit_end: 13,
            record: record.clone(),
        }];
        let mut resolver = Official5188MapBaselineResolver::new();
        resolver.update_from_decoded(&decoded);
        assert_eq!(resolver.resolve(*b"SH", 24661), Some(record));
    }

    #[test]
    fn value_decoder_rejects_missing_required_baseline() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            // Value mask bit 0, not the index-pass +0xde marker, selects the
            // Wine baseline pointer.
            value_stream: &[1, 0],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();
        let error = decode_official_5188_values(streams, &indexes, &mut resolver).unwrap_err();
        assert!(error.contains("requires missing baseline"));
        let decoded =
            decode_official_5188_values_with_fresh_fallback(streams, &indexes, &mut resolver)
                .expect("cold-start dump uses a fresh slot instead of failing");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].mask, 1);
        assert_eq!(decoded[0].record.timestamp(), 1_788_246_001);
        assert_eq!(decoded[0].record.last_price_integer(), 0);
    }

    #[test]
    fn value_decoder_consumes_special_record_tail_and_aligns() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            // mask=0, header level=7 (special path), tail token=1110 + 8 raw bits.
            value_stream: &[0x00, 0xe7, 0x00, 0x00],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();
        let decoded = decode_official_5188_values(streams, &indexes, &mut resolver).unwrap();
        assert_eq!(decoded.len(), 1);
        assert!(decoded[0].header.special_path);
        assert_eq!(decoded[0].bit_start, 0);
        assert_eq!(decoded[0].bit_end, 25);
        assert_eq!(decoded[0].record.timestamp(), 1_788_246_001);
    }

    #[test]
    fn traced_value_decoder_reports_completed_stage_boundaries() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            value_stream: &[0x00, 0xe7, 0x00, 0x00],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut plain_resolver = Official5188MapBaselineResolver::new();
        let plain = decode_official_5188_values_partial_with_fresh_fallback(
            streams,
            &indexes,
            &mut plain_resolver,
        );
        let mut resolver = Official5188MapBaselineResolver::new();
        let traced =
            decode_official_5188_values_partial_with_trace(streams, &indexes, &mut resolver);
        assert_eq!(traced.outcome, plain);
        assert!(traced.outcome.error.is_none());
        assert_eq!(traced.outcome.records.len(), 1);
        assert_eq!(
            traced.traces,
            vec![Official5188ValueDecodeTrace {
                record_index: 0,
                market: *b"SH",
                symbol_index: 24661,
                mask: 0,
                header: 0x1c,
                baseline_mode: "absolute",
                stored_present: false,
                baseline_present: false,
                mask_has_bit0: false,
                stored_source: "none",
                committed_state_present: false,
                current_last_before: 0,
                stored_last_before: None,
                committed_last_before: None,
                baseline_last_before: None,
                anchor_source: None,
                mask_class: 0,
                clear_ladder: false,
                raw_level_count: 7,
                level_count: 4,
                record_start: 0,
                header_end: 13,
                timestamp_end: 13,
                prefix_values_end: 13,
                ladder_move_bits: None,
                ladder_move_code: None,
                ladder_merge_flag: None,
                ladder_continue: None,
                ladder_values_bits: None,
                ladder_layout: None,
                ladder_flags: None,
                ladder_price_mask: None,
                ladder_anchor_value: None,
                ladder_anchor_operation: None,
                ladder_volume_mask: None,
                ladder_volumes_end: None,
                special_book_end: None,
                trailing_da_end: Some(25),
                aligned_end: 32,
                is_eof: false,
            }]
        );
    }

    #[test]
    fn traced_absolute_record_observes_unselected_committed_state() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            value_stream: &[0x00, 0xe7, 0x00, 0x00],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut prior = Official5188InternalRecord::zeroed();
        prior.set_i32(0x10, 1_552);
        let mut plain_resolver = Official5188MapBaselineResolver::new();
        plain_resolver.insert(*b"SH", 24661, prior.clone());
        let plain = decode_official_5188_values_partial_with_fresh_fallback(
            streams,
            &indexes,
            &mut plain_resolver,
        );
        let mut resolver = Official5188MapBaselineResolver::new();
        resolver.insert(*b"SH", 24661, prior);
        let traced =
            decode_official_5188_values_partial_with_trace(streams, &indexes, &mut resolver);
        assert_eq!(traced.outcome, plain);
        assert_eq!(traced.traces.len(), 1);
        let trace = &traced.traces[0];
        assert!(!trace.mask_has_bit0);
        assert_eq!(trace.baseline_mode, "absolute");
        assert!(trace.stored_present);
        assert!(!trace.baseline_present);
        assert!(trace.committed_state_present);
        assert_eq!(trace.stored_source, "resolver_record");
        assert_eq!(trace.stored_last_before, Some(1_552));
        assert_eq!(trace.committed_last_before, Some(1_552));
        assert_eq!(trace.baseline_last_before, None);
        assert_eq!(trace.current_last_before, 0);
        assert!(traced.outcome.error.is_none());
    }

    #[test]
    fn partial_decoder_omits_only_unencoded_baseline_tail() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            value_stream: &[],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: true,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();

        let outcome = decode_official_5188_values_partial_with_fresh_fallback(
            streams,
            &indexes,
            &mut resolver,
        );

        assert!(outcome.error.is_none());
        assert!(outcome.records.is_empty());
        assert_eq!(outcome.omitted_tail_records, 1);
    }

    #[test]
    fn partial_decoder_rejects_unencoded_nonbaseline_tail() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            value_stream: &[],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();

        let outcome = decode_official_5188_values_partial_with_fresh_fallback(
            streams,
            &indexes,
            &mut resolver,
        );

        assert!(
            outcome
                .error
                .as_deref()
                .is_some_and(|error| error.contains("stage=record_mask"))
        );
        assert!(outcome.records.is_empty());
        assert_eq!(outcome.omitted_tail_records, 0);
    }

    #[test]
    fn fresh_fallback_error_reports_missing_baseline_mode() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            // mask=1 requests a baseline, header=0x1d enters the clear-ladder
            // path, and the truncated body fails after fresh fallback.
            value_stream: &[0x01, 0xe8],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();
        let error =
            decode_official_5188_values_with_fresh_fallback(streams, &indexes, &mut resolver)
                .unwrap_err();
        assert!(error.contains("baseline_mode=missing_fresh"), "{error}");
    }

    #[test]
    fn value_decoder_commits_complete_records_before_a_later_failure() {
        let streams = Official5188DeltaStreams {
            record_count: 2,
            // The first special record is complete and byte-aligned. The
            // second contains only its mask, so reading its header fails.
            value_stream: &[0x00, 0xe7, 0x00, 0x00, 0x00],
            index_stream: &[],
        };
        let indexes = [
            Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 24661,
                timestamp: 1_788_246_001,
                uses_baseline: false,
            },
            Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 24662,
                timestamp: 1_788_246_002,
                uses_baseline: false,
            },
        ];
        let mut resolver = Official5188MapBaselineResolver::new();
        let error = decode_official_5188_values(streams, &indexes, &mut resolver).unwrap_err();
        assert!(error.contains("bitstream exhausted"));
        assert!(error.contains("stage=record_header"), "{error}");
        assert_eq!(
            resolver
                .resolve_baseline(*b"SH", 24661)
                .expect("first complete record must survive the later failure")
                .timestamp(),
            1_788_246_001
        );
        assert!(resolver.resolve_baseline(*b"SH", 24662).is_none());
    }

    #[test]
    fn partial_value_decoder_returns_only_complete_records_before_failure() {
        let streams = Official5188DeltaStreams {
            record_count: 2,
            value_stream: &[0x00, 0xe7, 0x00, 0x00, 0x00],
            index_stream: &[],
        };
        let indexes = [
            Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 24661,
                timestamp: 1_788_246_001,
                uses_baseline: false,
            },
            Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 24662,
                timestamp: 1_788_246_002,
                uses_baseline: false,
            },
        ];
        let mut resolver = Official5188MapBaselineResolver::new();
        let outcome = decode_official_5188_values_partial_with_fresh_fallback(
            streams,
            &indexes,
            &mut resolver,
        );

        assert_eq!(outcome.records.len(), 1);
        assert_eq!(outcome.records[0].index, indexes[0]);
        assert!(
            outcome
                .error
                .as_deref()
                .is_some_and(|error| error.contains("stage=record_header"))
        );
    }

    #[test]
    fn special_record_with_clear_flag_continues_into_ladder_decoder() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            // mask=0, header=0x1d (special + clear), layout=45 (`0`),
            // price mask 0 (`0`), sentinel anchor (`00` -> ask side at last
            // price), ten zero volume tokens (`01`+6 bits), then a zero tail.
            // No standalone volume-mask token precedes the fresh ladder.
            value_stream: &[
                0x00, 0xe8, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x00,
            ],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();
        let decoded = decode_official_5188_values(streams, &indexes, &mut resolver).unwrap();
        assert!(decoded[0].header.special_path);
        assert!(decoded[0].header.clear_ladder);
        assert_eq!(decoded[0].bit_end, 98);
        // Sentinel anchor: ask1 (slot 5) sits at the last price, the other
        // slots fill in one tick apart because the price mask is zero.
        assert_eq!(
            decoded[0].record.ladder_price_integers(),
            [-5, -4, -3, -2, -1, 0, 1, 2, 3, 4]
        );
        assert_eq!(decoded[0].record.ladder_volume_integers(), [0; 10]);
    }

    #[test]
    fn value_decoder_returns_mask_18_before_timestamp_delta() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            // mask=0x1a (class 0x18 plus timestamp bit), header=0. Wine
            // returns after the header and must leave the timestamp token
            // untouched.
            value_stream: &[0x1a, 0x00],
            index_stream: &[],
        };
        let indexes = [Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: true,
        }];
        let mut resolver = Official5188MapBaselineResolver::new();
        let decoded = decode_official_5188_values(streams, &indexes, &mut resolver).unwrap();
        assert_eq!(decoded[0].bit_end, 13);
        assert_eq!(decoded[0].record.timestamp(), 1_788_246_001);
    }

    #[test]
    fn value_decoder_copies_metadata_without_baseline_mask() {
        let streams = Official5188DeltaStreams {
            record_count: 1,
            value_stream: &[0x00, 0xe7, 0x00, 0x00],
            index_stream: &[],
        };
        let index = Official5188DeltaIndexState {
            market: *b"SH",
            symbol_index: 24661,
            timestamp: 1_788_246_001,
            uses_baseline: false,
        };
        let mut metadata = Official5188InternalRecord::zeroed();
        metadata.set_i32(0x12b, 2520);
        let mut resolver = Official5188MapBaselineResolver::new();
        resolver.insert(index.market, index.symbol_index, metadata);
        let decoded = decode_official_5188_values(streams, &[index], &mut resolver).unwrap();
        assert_eq!(decoded[0].record.i32_at(0x12b), 2520);
    }

    #[test]
    fn amount_prediction_matches_wine_sh603059_fixture() {
        let mut record = Official5188InternalRecord::zeroed();
        record.set_i32(0x10, 2_517);
        record.bytes[0x11f] = 1;
        record.bytes[0x120] = 2;
        record.bytes[0x121..0x123].copy_from_slice(&100u16.to_le_bytes());
        assert_eq!(amount_prediction(&record, 10_261), Some(25_826_937));
        assert_eq!(
            amount_prediction(&record, 10_261).unwrap() + 107_380,
            25_934_317
        );
    }

    #[test]
    fn amount_prediction_uses_reference_price_when_delta_omits_last_price() {
        let mut record = Official5188InternalRecord::zeroed();
        record.bytes[0x11f] = 1;
        record.bytes[0x120] = 2;
        record.bytes[0x121..0x123].copy_from_slice(&100u16.to_le_bytes());
        record.set_i32(0x12b, 765);

        assert_eq!(amount_prediction(&record, 6_452), Some(4_935_780));
    }

    #[test]
    fn amount_prediction_applies_wine_mode_eight_adjustment() {
        let mut record = Official5188InternalRecord::zeroed();
        record.set_i32(0x10, 765);
        record.bytes[0x11f] = 8;
        record.bytes[0x120] = 2;
        record.bytes[0x121..0x123].copy_from_slice(&100u16.to_le_bytes());
        record.bytes[0x123..0x127].copy_from_slice(&2u32.to_le_bytes());

        assert_eq!(amount_prediction(&record, 6_452), Some(14_807_343));
    }

    #[test]
    fn amount_prediction_mode_zero_skips_price_projection() {
        let mut record = Official5188InternalRecord::zeroed();
        record.set_i32(0x10, 765);
        record.bytes[0x11f] = 0;
        record.bytes[0x120] = 2;
        record.bytes[0x121..0x123].copy_from_slice(&100u16.to_le_bytes());
        record.bytes[0x123..0x127].copy_from_slice(&2u32.to_le_bytes());

        assert_eq!(amount_prediction(&record, 6_452), Some(3));
    }

    #[test]
    fn code_table_metadata_seeds_complete_internal_tail() {
        let metadata = Official5188CodeTableRecord {
            symbol_index: 24_661,
            code: "603059".into(),
            name: "fixture".into(),
            amount_mode: 8,
            opaque_tail: std::array::from_fn(|index| index as u8 + 1),
        };

        let record = Official5188InternalRecord::from_code_table_metadata(
            *b"SH",
            metadata.symbol_index,
            &metadata,
        );
        assert_eq!(&record.as_bytes()[0xdf..0xe1], &24_661u16.to_le_bytes());
        assert_eq!(&record.as_bytes()[0xe1..0xeb], b"SHSH603059");
        assert_eq!(&record.as_bytes()[0x120..0x137], &metadata.opaque_tail);
        assert_eq!(record.as_bytes()[0x11f], metadata.amount_mode);
    }

    #[test]
    fn resolver_seeds_code_tables_without_replacing_live_state() {
        let metadata = Official5188CodeTableRecord {
            symbol_index: 24_661,
            code: "603059".into(),
            name: "fixture".into(),
            amount_mode: 0,
            opaque_tail: [7; 23],
        };
        let table = Official5188CodeTable {
            market: *b"SH",
            records: vec![metadata],
        };
        let mut resolver = Official5188MapBaselineResolver::new();
        assert_eq!(resolver.seed_code_tables(std::slice::from_ref(&table)), 1);
        let seeded = resolver.resolve(*b"SH", 24_661).unwrap();
        assert_eq!(&seeded.as_bytes()[0x120..0x137], &[7; 23]);

        let mut live = seeded;
        live.set_i32(0x10, 2_517);
        resolver.insert(*b"SH", 24_661, live.clone());
        assert_eq!(resolver.seed_code_tables(&[table]), 0);
        assert_eq!(resolver.resolve(*b"SH", 24_661), Some(live));
    }

    #[test]
    fn mode_zero_state_preserves_tagged_amount_encoding() {
        let mut record = Official5188InternalRecord::zeroed();
        record.bytes[0x11f] = 0;
        record.set_i64(0x1c, -1);

        assert_eq!(record.amount_integer(), -1);
    }

    #[test]
    fn public_quote_rejects_unconverted_negative_internal_amount() {
        let mut record = Official5188InternalRecord::zeroed();
        record.bytes[0xe1..0xe3].copy_from_slice(b"SH");
        record.bytes[0x11f] = 0;
        record.set_i64(0x1c, -1);

        let error = record
            .to_public_quote("603059", "fixture", 100.0)
            .unwrap_err();
        assert!(error.contains("before Wine amount conversion"));
    }

    #[test]
    fn nonzero_mode_preserves_wine_negative_internal_amount() {
        let mut record = Official5188InternalRecord::zeroed();
        record.bytes[0x11f] = 1;
        record.set_i64(0x1c, -23_186);

        assert_eq!(record.amount_integer(), -23_186);
    }

    #[test]
    fn value_delta64_m_token_scales_by_powers_of_sixteen() {
        for (raw, expected) in [
            (0x0000_0003u32, 3i64),
            (0x4000_0003, 48),
            (0x8000_0003, 768),
            (0x3fff_fffd, -3),
            (0x7fff_fffd, -48),
        ] {
            assert_eq!(decode_magnitude_m(raw), expected);
        }
    }

    #[test]
    fn ladder_move_restores_baseline_workspace_before_shifting() {
        let mut baseline_bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        for slot in 0..16 {
            baseline_bytes[0x58 + slot * 4..0x5c + slot * 4]
                .copy_from_slice(&((slot as i32) + 10).to_le_bytes());
        }
        let baseline = Official5188InternalRecord::decode(&baseline_bytes).unwrap();
        let mut current = Official5188InternalRecord::zeroed();
        // 0x5b2560 token E(144): prefix 100; 0x5b2518 token E(1): prefix 0.
        let mut reader = Official5188BitReader::new(&[0b1000_0000]);
        let outcome =
            decode_value_ladder_move(&mut reader, &mut current, Some(&baseline), 0).unwrap();
        assert_eq!(
            outcome,
            Official5188LadderMove {
                code: 144,
                merge_flag: 0,
                continue_ladder: true,
            }
        );
        assert_eq!(current.i32_at(0x58), 11);
        assert_eq!(current.i32_at(0x58 + 8 * 4), 19);
        assert_eq!(current.i32_at(0x58 + 9 * 4), 20);
    }

    #[test]
    fn ladder_move_code_minus_one_restores_and_continues() {
        let mut baseline = Official5188InternalRecord::zeroed();
        for slot in 0..10 {
            baseline.set_i32(0x58 + slot * 4, 100 + slot as i32);
        }
        let mut current = Official5188InternalRecord::zeroed();
        // 0x5b2560 prefix 0 decodes the fixed code -1.
        let mut reader = Official5188BitReader::new(&[0]);
        let result =
            decode_value_ladder_move(&mut reader, &mut current, Some(&baseline), 0).unwrap();
        assert_eq!(
            result,
            Official5188LadderMove {
                code: -1,
                merge_flag: 0,
                continue_ladder: true,
            }
        );
        assert_eq!(current.i32_at(0x58), 100);
        assert_eq!(current.i32_at(0x58 + 9 * 4), 109);
    }

    #[test]
    fn restores_delta_index_state_with_vendor_token_tables() {
        // Record 1: set SH, no baseline, symbol +1, timestamp +0.
        // Record 2: retain SH, use baseline, symbol +1, timestamp +1.
        let streams = Official5188DeltaStreams {
            record_count: 2,
            value_stream: &[],
            index_stream: &[0x80, 0x90, 0x00],
        };
        let decoded = decode_official_5188_delta_indexes(streams).unwrap();
        assert_eq!(
            decoded,
            vec![
                Official5188DeltaIndexState {
                    market: *b"SH",
                    symbol_index: 1,
                    timestamp: 0,
                    uses_baseline: false,
                },
                Official5188DeltaIndexState {
                    market: *b"SH",
                    symbol_index: 2,
                    timestamp: 1,
                    uses_baseline: true,
                },
            ]
        );
    }

    #[test]
    fn parses_0104_code_table_and_maps_symbol_index() {
        let mut decoded = vec![0u8; OFFICIAL_5188_CODE_TABLE_HEADER_LEN];
        decoded[12..14].copy_from_slice(b"SH");
        decoded[92..96].copy_from_slice(&2u32.to_le_bytes());
        decoded[96..98].copy_from_slice(&2u16.to_le_bytes());
        let mut first = [0u8; OFFICIAL_5188_CODE_TABLE_RECORD_LEN];
        first[2..8].copy_from_slice(b"000001");
        first[12..20].copy_from_slice(&[0xc9, 0xcf, 0xd6, 0xa4, 0xd6, 0xb8, 0xca, 0xfd]);
        first[45] = 2;
        let mut second = [0u8; OFFICIAL_5188_CODE_TABLE_RECORD_LEN];
        second[..2].copy_from_slice(&1u16.to_le_bytes());
        second[2..8].copy_from_slice(b"000002");
        second[12..20].copy_from_slice(&[0xa3, 0xc1, 0xb9, 0xc9, 0xd6, 0xb8, 0xca, 0xfd]);
        second[45] = 7;
        decoded.extend_from_slice(&first);
        decoded.extend_from_slice(&second);

        let table = Official5188CodeTable::decode(&decoded).unwrap();
        assert_eq!(table.market, *b"SH");
        assert_eq!(table.records.len(), 2);
        assert_eq!(table.record(0).unwrap().code, "000001");
        assert_eq!(table.record(0).unwrap().name, "上证指数");
        assert_eq!(table.record(0).unwrap().price_scale_hint(), Some(100.0));
        assert_eq!(table.record(1).unwrap().price_scale_hint(), None);
        assert_eq!(table.record(1).unwrap().code, "000002");
        assert!(table.record(2).is_none());

        decoded[96..98].copy_from_slice(&3u16.to_le_bytes());
        assert!(Official5188CodeTable::decode(&decoded).is_err());
    }

    #[test]
    fn builds_five_primary_subscription_partitions_in_code_table_order() {
        let sh =
            subscription_code_table(*b"SH", (600_000..601_500).map(|code| format!("{code:06}")));
        let sz = subscription_code_table(*b"SZ", (0..4_000).map(|code| format!("{code:06}")));

        let partitions = build_official_5188_primary_subscription_partitions(&sh, &sz).unwrap();
        assert!(partitions.iter().all(|partition| {
            partition.len() == OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN
        }));
        assert_eq!(partitions[0][0], (*b"SH", 0x1_0000));
        assert_eq!(partitions[1][475], (*b"SH", 0x1_0000 | 1_499));
        assert_eq!(partitions[1][476], (*b"SZ", 0x1_0000));
        assert_eq!(partitions[4][1_023], (*b"SZ", 0x1_0000 | 3_619));
    }

    #[test]
    fn initial_code_tables_expose_current_connection_primary_partitions() {
        let sh =
            subscription_code_table(*b"SH", (600_000..601_500).map(|code| format!("{code:06}")));
        let sz = subscription_code_table(*b"SZ", (0..4_000).map(|code| format!("{code:06}")));
        let headers = [*b"SH", *b"SZ", *b"B$"].map(|market| Official5188CodeTableHeader {
            protocol: 0x010a,
            group: SessionGroupTag::Primary,
            version_seconds: 1,
            market,
            trading_day_low: 1,
            acknowledgement_marker: 0x0135,
        });
        let mut initial = Official5188InitialCodeTables {
            headers,
            code_tables: vec![sh, sz],
            observed_frames: Vec::new(),
        };

        let partitions = initial.primary_subscription_partitions().unwrap();
        assert_eq!(partitions[1][475], (*b"SH", 0x1_0000 | 1_499));
        assert_eq!(partitions[1][476], (*b"SZ", 0x1_0000));

        initial.code_tables.retain(|table| table.market != *b"SZ");
        assert!(
            initial
                .primary_subscription_partitions()
                .expect_err("missing current SZ table must fail")
                .contains("no SZ table")
        );
    }

    #[test]
    fn primary_subscription_filters_malformed_and_ineligible_codes() {
        let mut records = vec![
            Official5188CodeTableRecord {
                symbol_index: 0,
                code: "60000".to_string(),
                name: String::new(),
                amount_mode: 0,
                opaque_tail: [0; 23],
            },
            Official5188CodeTableRecord {
                symbol_index: 1,
                code: "60000A".to_string(),
                name: String::new(),
                amount_mode: 0,
                opaque_tail: [0; 23],
            },
            Official5188CodeTableRecord {
                symbol_index: 2,
                code: "700000".to_string(),
                name: String::new(),
                amount_mode: 0,
                opaque_tail: [0; 23],
            },
        ];
        records.extend((0..5_120).map(|offset| Official5188CodeTableRecord {
            symbol_index: u16::try_from(offset + 3).unwrap(),
            code: format!("{:06}", 600_000 + offset),
            name: String::new(),
            amount_mode: 0,
            opaque_tail: [0; 23],
        }));
        let sh = Official5188CodeTable {
            market: *b"SH",
            records,
        };
        let sz = subscription_code_table(*b"SZ", std::iter::empty());

        let partitions = build_official_5188_primary_subscription_partitions(&sh, &sz).unwrap();
        assert_eq!(partitions[0][0], (*b"SH", 0x1_0000 | 3));
        assert_eq!(partitions[4][1_023], (*b"SH", 0x1_0000 | 5_122));
    }

    #[test]
    fn primary_subscription_rejects_wrong_code_table_markets() {
        let wrong_sh = subscription_code_table(*b"SZ", std::iter::empty());
        let sz = subscription_code_table(*b"SZ", std::iter::empty());
        assert!(
            build_official_5188_primary_subscription_partitions(&wrong_sh, &sz)
                .expect_err("wrong SH market must fail")
                .contains("SH code table")
        );

        let sh = subscription_code_table(*b"SH", std::iter::empty());
        let wrong_sz = subscription_code_table(*b"SH", std::iter::empty());
        assert!(
            build_official_5188_primary_subscription_partitions(&sh, &wrong_sz)
                .expect_err("wrong SZ market must fail")
                .contains("SZ code table")
        );
    }

    #[test]
    fn primary_subscription_rejects_an_incomplete_eligible_universe() {
        let sh =
            subscription_code_table(*b"SH", (600_000..605_119).map(|code| format!("{code:06}")));
        let sz = subscription_code_table(*b"SZ", std::iter::empty());
        let error = build_official_5188_primary_subscription_partitions(&sh, &sz)
            .expect_err("5119 eligible entries must fail");
        assert!(error.contains("requires 5120"));
        assert!(error.contains("got 5119"));
    }

    #[test]
    fn receive_list_partitions_cover_enabled_intersection_only() {
        // SH eligible 600000..606500 (6500 >= 5120) plus ineligible ETF codes;
        // empty SZ table so the primary prefix stays inside SH.
        let mut sh_codes = (600_000..606_500)
            .map(|code| format!("{code:06}"))
            .collect::<Vec<_>>();
        sh_codes.extend(
            ["510010", "510020", "510050", "510060", "110075", "900901"].map(str::to_owned),
        );
        let sh = subscription_code_table(*b"SH", sh_codes.iter().cloned());
        let sz = subscription_code_table(*b"SZ", std::iter::empty::<String>());

        // Receive list: everything in the SH table (so intersection == table).
        let receive = sh_codes
            .iter()
            .map(|code| format!("SH{code}"))
            .collect::<std::collections::BTreeSet<_>>();
        let plan = build_official_5188_receive_list_partitions(&sh, &sz, &receive).unwrap();
        assert_eq!(plan.partitions.len(), 7);
        for partition in &plan.partitions[..5] {
            assert_eq!(
                partition.len(),
                OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN
            );
        }
        assert_eq!(
            plan.partitions[5].len(),
            OFFICIAL_5188_PRIMARY_SUBSCRIPTION_PARTITION_LEN
        );
        // 6500 eligible + 6 ineligible ETF = 6506 enabled records.  5120 go to
        // P1..P5, 1024 to P6, leaving 6506-5120-1024 = 362 in P7.
        assert_eq!(plan.partitions[6].len(), 362);

        // No entry appears twice across all seven partitions.
        let mut seen = std::collections::BTreeSet::new();
        let mut duplicates = 0;
        for partition in &plan.partitions {
            for entry in partition {
                if !seen.insert(*entry) {
                    duplicates += 1;
                }
            }
        }
        assert_eq!(duplicates, 0);

        // Primary entries are all SH 600-699.
        assert!(plan.partitions[0][0].0 == *b"SH");
        assert!(plan.partitions[0][0].1 == 0x1_0000);
        // Supplement includes the ineligible SH ETF codes (ordinals 6500+).
        let all_after_primary = plan.partitions[5..]
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        let ineligible = ["510010", "510020", "510050", "510060", "110075", "900901"];
        let sh_codes_after = sh_codes;
        for code in ineligible {
            let ordinal = sh_codes_after.iter().position(|c| c == code).unwrap();
            assert!(
                all_after_primary
                    .iter()
                    .any(|(_, index)| (*index & 0xffff) == ordinal as u32),
                "{code} should appear after the primary partitions"
            );
        }
    }

    #[test]
    fn receive_list_partitions_respect_disabled_codes() {
        let sh =
            subscription_code_table(*b"SH", (600_000..606_200).map(|code| format!("{code:06}")));
        let sz = subscription_code_table(*b"SZ", std::iter::empty::<String>());
        // Exclude a handful of codes from the receive list; they must not
        // appear in any partition.
        let excluded = ["600000", "600100", "600200"];
        let receive = (600_000..606_200)
            .map(|code| format!("{code:06}"))
            .filter(|code| !excluded.contains(&code.as_str()))
            .map(|code| format!("SH{code}"))
            .collect::<std::collections::BTreeSet<_>>();
        let plan = build_official_5188_receive_list_partitions(&sh, &sz, &receive).unwrap();
        assert_eq!(plan.partitions.len(), 7);
        // 6200 - 3 = 6197 total entries across all partitions.
        let total = plan.partitions.iter().map(Vec::len).sum::<usize>();
        assert_eq!(total, 6197);
    }

    #[test]
    fn receive_list_primary_partitions_match_the_verified_primary_builder() {
        // A receive list that covers every eligible security must yield the
        // exact P1..P5 of the byte-verified primary builder.
        let sh =
            subscription_code_table(*b"SH", (600_000..606_500).map(|code| format!("{code:06}")));
        let sz = subscription_code_table(
            *b"SZ",
            (0..4_000)
                .map(|code| format!("{code:06}"))
                .chain((300_000..303_500).map(|code| format!("{code:06}"))),
        );
        let receive = sh
            .records
            .iter()
            .map(|record| format!("SH{}", record.code))
            .chain(sz.records.iter().map(|record| format!("SZ{}", record.code)))
            .collect::<std::collections::BTreeSet<_>>();

        let primary = build_official_5188_primary_subscription_partitions(&sh, &sz).unwrap();
        let plan = build_official_5188_receive_list_partitions(&sh, &sz, &receive).unwrap();
        for (index, expected) in primary.iter().enumerate() {
            assert_eq!(
                &plan.partitions[index],
                expected,
                "receive-list P{} must equal the verified primary builder",
                index + 1
            );
        }
        let primary_set = primary
            .iter()
            .flatten()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        for entry in plan.partitions[5..].iter().flatten() {
            assert!(!primary_set.contains(entry));
        }
    }

    #[test]
    fn receive_list_tolerates_a_short_or_empty_remainder() {
        // Exactly 5120 eligible and no ineligible codes: P6/P7 are both empty.
        let sh =
            subscription_code_table(*b"SH", (600_000..605_120).map(|code| format!("{code:06}")));
        let sz = subscription_code_table(*b"SZ", std::iter::empty::<String>());
        let receive = sh
            .records
            .iter()
            .map(|record| format!("SH{}", record.code))
            .collect::<std::collections::BTreeSet<_>>();
        let plan = build_official_5188_receive_list_partitions(&sh, &sz, &receive).unwrap();
        assert_eq!(plan.partitions.len(), 7);
        assert!(plan.partitions[5].is_empty());
        assert!(plan.partitions[6].is_empty());

        // 5125 eligible + 3 ineligible ETF: P1..P5 take 5120, so P6 holds the
        // 5 eligible tail + 3 ETF = 8 entries and P7 is empty.
        let mut sh_codes = (600_000..605_125)
            .map(|code| format!("{code:06}"))
            .collect::<Vec<_>>();
        sh_codes.extend(["510010", "510020", "510050"].map(str::to_owned));
        let sh2 = subscription_code_table(*b"SH", sh_codes.iter().cloned());
        let receive2 = sh_codes
            .iter()
            .map(|code| format!("SH{code}"))
            .collect::<std::collections::BTreeSet<_>>();
        let plan2 = build_official_5188_receive_list_partitions(&sh2, &sz, &receive2).unwrap();
        assert_eq!(plan2.partitions[5].len(), 8);
        assert!(plan2.partitions[6].is_empty());
    }

    #[test]
    fn parses_confirmed_2704_internal_record_layout() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0..4].copy_from_slice(&1_788_246_001u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&2_509i32.to_le_bytes());
        bytes[8..12].copy_from_slice(&2_556i32.to_le_bytes());
        bytes[12..16].copy_from_slice(&2_502i32.to_le_bytes());
        bytes[16..20].copy_from_slice(&2_517i32.to_le_bytes());
        bytes[20..28].copy_from_slice(&10_261i64.to_le_bytes());
        bytes[28..36].copy_from_slice(&25_934_317i64.to_le_bytes());
        bytes[0xdf..0xe1].copy_from_slice(&24_661u16.to_le_bytes());
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let prices: [i32; 10] = [2505, 2508, 2509, 2510, 2511, 2517, 2519, 2520, 2525, 2530];
        let volumes: [i32; 10] = [19, 14, 8, 22, 2, 16, 12, 2, 44, 25];
        for (index, value) in prices.into_iter().enumerate() {
            bytes[0x58 + index * 4..0x5c + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        for (index, value) in volumes.into_iter().enumerate() {
            bytes[0xa8 + index * 4..0xac + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        let record = Official5188InternalRecord::decode(&bytes).unwrap();
        assert_eq!(record.timestamp(), 1_788_246_001);
        assert_eq!(record.open_price_integer(), 2509);
        assert_eq!(record.high_price_integer(), 2556);
        assert_eq!(record.low_price_integer(), 2502);
        assert_eq!(record.last_price_integer(), 2517);
        assert_eq!(record.volume_integer(), 10_261);
        assert_eq!(record.amount_integer(), 25_934_317);
        assert_eq!(record.symbol_index(), 24_661);
        assert_eq!(record.market(), *b"SH");
        assert_eq!(record.ladder_price_integers(), prices);
        assert_eq!(record.ladder_volume_integers(), volumes);
        assert!(Official5188InternalRecord::decode(&bytes[..310]).is_err());
    }

    #[test]
    fn projects_internal_record_to_wine_public_quote_shape() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0..4].copy_from_slice(&1_788_246_001u32.to_le_bytes());
        for (offset, value) in [
            (0x04, 2509i32),
            (0x08, 2556),
            (0x0c, 2502),
            (0x10, 2517),
            (0x12b, 2520),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[0x14..0x1c].copy_from_slice(&10_261i64.to_le_bytes());
        bytes[0x1c..0x24].copy_from_slice(&25_934_317i64.to_le_bytes());
        let prices: [i32; 10] = [2505, 2508, 2509, 2510, 2511, 2517, 2519, 2520, 2525, 2530];
        let volumes: [i32; 10] = [19, 14, 8, 22, 2, 16, 12, 2, 44, 25];
        for (slot, value) in prices.into_iter().enumerate() {
            bytes[0x58 + slot * 4..0x5c + slot * 4].copy_from_slice(&value.to_le_bytes());
        }
        for (slot, value) in volumes.into_iter().enumerate() {
            bytes[0xa8 + slot * 4..0xac + slot * 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let quote = Official5188InternalRecord::decode(&bytes)
            .unwrap()
            .to_public_quote("603059", "倍加洁", 100.0)
            .unwrap();
        assert_eq!(quote.market, "SH");
        assert_eq!(quote.code, "603059");
        assert_eq!(quote.price, f64::from(25.17f32));
        assert_eq!(quote.last_close, f64::from(25.20f32));
        assert_eq!(quote.volume, 10_261.0);
        assert_eq!(quote.amount, 25_934_316.0);
        assert_eq!(
            quote.bid_prices[..5],
            [25.11f32, 25.10, 25.09, 25.08, 25.05].map(f64::from)
        );
        assert_eq!(
            quote.ask_prices[..5],
            [25.17f32, 25.19, 25.20, 25.25, 25.30].map(f64::from)
        );
        assert_eq!(quote.bid_volumes[..5], [2.0, 22.0, 8.0, 14.0, 19.0]);
        assert_eq!(quote.ask_volumes[..5], [16.0, 12.0, 2.0, 44.0, 25.0]);
        assert_eq!(quote.source_protocol, "wjf.oem_report.v5");
    }

    #[test]
    fn public_quote_projection_rejects_unknown_scale_and_code() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let record = Official5188InternalRecord::decode(&bytes).unwrap();
        assert!(record.to_public_quote("603059", "倍加洁", 0.0).is_err());
        assert!(record.to_public_quote("60305X", "倍加洁", 100.0).is_err());
    }

    #[test]
    fn public_quote_projection_clamps_post_close_timestamp() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[..4].copy_from_slice(&1_788_505_201u32.to_le_bytes());
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let quote = Official5188InternalRecord::decode(&bytes)
            .unwrap()
            .to_public_quote("600001", "fixture", 100.0)
            .unwrap();
        assert_eq!(quote.timestamp, 1_788_505_200);
    }

    #[test]
    fn public_quote_projection_preserves_pre_close_timestamp() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[..4].copy_from_slice(&1_788_505_199u32.to_le_bytes());
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let quote = Official5188InternalRecord::decode(&bytes)
            .unwrap()
            .to_public_quote("600001", "fixture", 100.0)
            .unwrap();
        assert_eq!(quote.timestamp, 1_788_505_199);
    }

    #[test]
    fn public_quote_projection_accepts_explicit_public_state_ladder() {
        let mut decoded = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        decoded[0x10..0x14].copy_from_slice(&2517i32.to_le_bytes());
        decoded[0xe1..0xe3].copy_from_slice(b"SH");
        let mut public = decoded;
        let prices: [i32; 10] = [2505, 2508, 2509, 2510, 2511, 2517, 2519, 2520, 2525, 2530];
        let volumes: [i32; 10] = [19, 14, 8, 22, 2, 16, 12, 2, 44, 25];
        for (slot, value) in prices.into_iter().enumerate() {
            public[0x58 + slot * 4..0x5c + slot * 4].copy_from_slice(&value.to_le_bytes());
        }
        for (slot, value) in volumes.into_iter().enumerate() {
            public[0xa8 + slot * 4..0xac + slot * 4].copy_from_slice(&value.to_le_bytes());
        }
        let quote = Official5188InternalRecord::decode(&decoded)
            .unwrap()
            .to_public_quote_with_public_state(
                &Official5188InternalRecord::decode(&public).unwrap(),
                "603059",
                "倍加洁",
                100.0,
            )
            .unwrap();
        assert_eq!(
            quote.bid_prices[..5],
            [25.11f32, 25.10, 25.09, 25.08, 25.05].map(f64::from)
        );
        assert_eq!(quote.ask_volumes[..5], [16.0, 12.0, 2.0, 44.0, 25.0]);
    }

    #[test]
    fn public_quote_projection_uses_oem_f32_and_star_lots() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0x10..0x14].copy_from_slice(&12_345i32.to_le_bytes());
        bytes[0x14..0x1c].copy_from_slice(&12_349i64.to_le_bytes());
        bytes[0x1c..0x24].copy_from_slice(&16_777_217i64.to_le_bytes());
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        bytes[0xa8 + 4 * 4..0xac + 4 * 4].copy_from_slice(&149i32.to_le_bytes());
        bytes[0xa8 + 5 * 4..0xac + 5 * 4].copy_from_slice(&151i32.to_le_bytes());

        let quote = Official5188InternalRecord::decode(&bytes)
            .unwrap()
            .to_public_quote("688001", "fixture", 100.0)
            .unwrap();

        assert_eq!(quote.price, f64::from(12_345f32 / 100.0f32));
        assert_eq!(quote.volume, 123.0);
        assert_eq!(quote.amount, 16_777_216.0);
        assert_eq!(quote.bid_volumes[0], 1.0);
        assert_eq!(quote.ask_volumes[0], 2.0);
    }

    #[test]
    fn public_quote_projection_fills_non_star_book_lot_with_nonzero_price() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        bytes[0x58 + 4 * 4..0x5c + 4 * 4].copy_from_slice(&1234i32.to_le_bytes());
        bytes[0xa8 + 4 * 4..0xac + 4 * 4].copy_from_slice(&0i32.to_le_bytes());
        bytes[0x6c..0x70].copy_from_slice(&2345i32.to_le_bytes());
        bytes[0xbc..0xc0].copy_from_slice(&0i32.to_le_bytes());

        let quote = Official5188InternalRecord::decode(&bytes)
            .unwrap()
            .to_public_quote("600001", "fixture", 100.0)
            .unwrap();

        assert_eq!(quote.bid_volumes[0], 1.0);
        assert_eq!(quote.ask_volumes[0], 1.0);
    }

    #[test]
    fn public_quote_projection_uses_half_up_star_lots() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        bytes[0xa8 + 4 * 4..0xac + 4 * 4].copy_from_slice(&50i32.to_le_bytes());
        bytes[0xa8 + 5 * 4..0xac + 5 * 4].copy_from_slice(&1i32.to_le_bytes());

        let quote = Official5188InternalRecord::decode(&bytes)
            .unwrap()
            .to_public_quote("688001", "fixture", 100.0)
            .unwrap();

        assert_eq!(quote.bid_volumes[0], 1.0);
        assert_eq!(quote.ask_volumes[0], 1.0);
    }

    #[test]
    fn oem_state_carries_sparse_values_and_uses_explicit_previous_close() {
        let mut first = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        first[..4].copy_from_slice(&1_788_246_000u32.to_le_bytes());
        first[0x10..0x14].copy_from_slice(&1_234i32.to_le_bytes());
        first[0x14..0x1c].copy_from_slice(&500i64.to_le_bytes());
        first[0x1c..0x24].copy_from_slice(&60_000i64.to_le_bytes());
        first[0x58 + 4 * 4..0x5c + 4 * 4].copy_from_slice(&1_233i32.to_le_bytes());
        first[0xe1..0xe3].copy_from_slice(b"SH");
        let first = Official5188DecodedValueRecord {
            index: Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 1,
                timestamp: 1_788_246_000,
                uses_baseline: true,
            },
            mask: 0,
            header: Official5188ValueRecordHeader::decode(0).unwrap(),
            bit_start: 0,
            bit_end: 0,
            record: Official5188InternalRecord::decode(&first).unwrap(),
        };
        let mut sparse = first.clone();
        sparse.record.bytes[..4].copy_from_slice(&1_788_246_001u32.to_le_bytes());
        sparse.record.bytes[0x10..0x24].fill(0);
        sparse.record.bytes[0x58 + 4 * 4..0x5c + 4 * 4].fill(0);

        let mut state = Official5188OemState::new(1_200);
        state.merge(&first);
        state.merge(&sparse);
        let quote = state.project("600001", "fixture", 100.0).unwrap();

        assert_eq!(quote.timestamp, 1_788_246_000);
        assert_eq!(quote.price, f64::from(12.34f32));
        assert_eq!(quote.last_close, f64::from(12.0f32));
        assert_eq!(quote.volume, 500.0);
        assert_eq!(quote.amount, 60_000.0);
        assert_eq!(quote.bid_prices[0], f64::from(12.33f32));
    }

    #[test]
    fn oem_state_projects_from_complete_0104_metadata_row() {
        let mut tail = [0_u8; 23];
        tail[0] = 3;
        tail[11..15].copy_from_slice(&10_280_i32.to_le_bytes());
        let metadata = Official5188CodeTableRecord {
            symbol_index: 1,
            code: "512000".to_string(),
            name: "fixture".to_string(),
            amount_mode: 0,
            opaque_tail: tail,
        };
        let state = Official5188OemState::from_code_table_metadata(*b"SH", &metadata).unwrap();
        let quote = state.project_from_code_table_metadata(&metadata).unwrap();
        assert_eq!(quote.code, "512000");
        assert_eq!(quote.name, "fixture");
        assert_eq!(quote.last_close, f64::from(10.28f32));
    }

    #[test]
    fn transactional_oem_merge_does_not_commit_when_projection_fails() {
        let mut valid_tail = [0_u8; 23];
        valid_tail[0] = 2;
        valid_tail[11..15].copy_from_slice(&1_028_i32.to_le_bytes());
        let valid_metadata = Official5188CodeTableRecord {
            symbol_index: 1,
            code: "600001".to_string(),
            name: "fixture".to_string(),
            amount_mode: 0,
            opaque_tail: valid_tail,
        };
        let mut state =
            Official5188OemState::from_code_table_metadata(*b"SH", &valid_metadata).unwrap();
        let mut decoded_bytes = [0_u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        decoded_bytes[..4].copy_from_slice(&123_u32.to_le_bytes());
        decoded_bytes[0x10..0x14].copy_from_slice(&456_i32.to_le_bytes());
        decoded_bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let decoded = Official5188DecodedValueRecord {
            index: Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 1,
                timestamp: 123,
                uses_baseline: true,
            },
            mask: 0,
            header: Official5188ValueRecordHeader::decode(0).unwrap(),
            bit_start: 0,
            bit_end: 0,
            record: Official5188InternalRecord::decode(&decoded_bytes).unwrap(),
        };
        let before = state.clone();
        let mut invalid_metadata = valid_metadata.clone();
        invalid_metadata.opaque_tail[0] = 7;

        assert!(
            state
                .merge_and_project_from_code_table_metadata(&decoded, &invalid_metadata)
                .is_err()
        );
        assert_eq!(state, before);
    }

    #[test]
    fn oem_state_mask_18_updates_only_timestamp() {
        let mut bytes = [0u8; OFFICIAL_5188_INTERNAL_RECORD_LEN];
        bytes[..4].copy_from_slice(&1_788_246_002u32.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(&9_999i32.to_le_bytes());
        bytes[0xe1..0xe3].copy_from_slice(b"SH");
        let decoded = Official5188DecodedValueRecord {
            index: Official5188DeltaIndexState {
                market: *b"SH",
                symbol_index: 1,
                timestamp: 1_788_246_002,
                uses_baseline: true,
            },
            mask: 0x18,
            header: Official5188ValueRecordHeader::decode(0).unwrap(),
            bit_start: 0,
            bit_end: 0,
            record: Official5188InternalRecord::decode(&bytes).unwrap(),
        };
        let mut state = Official5188OemState::new(1_200);
        state.merge(&decoded);
        let quote = state.project("600001", "fixture", 100.0).unwrap();
        assert_eq!(quote.timestamp, 1_788_246_000);
        assert_eq!(quote.price, 0.0);
        assert_eq!(quote.last_close, f64::from(12.0f32));
    }

    #[test]
    fn value_header_keeps_special_path_separate_from_clear_flag() {
        let normal = Official5188ValueRecordHeader::decode(0x13).unwrap();
        assert!(normal.clear_ladder);
        assert!(normal.has_book);
        assert_eq!(normal.raw_level_count, 4);
        assert_eq!(normal.level_count, 4);
        assert!(!normal.special_path);

        let special = Official5188ValueRecordHeader::decode(0x1d).unwrap();
        assert!(special.clear_ladder);
        assert_eq!(special.raw_level_count, 7);
        assert_eq!(special.level_count, 4);
        assert!(special.special_path);
    }

    #[test]
    fn value_header_rejects_bits_above_five() {
        assert!(Official5188ValueRecordHeader::decode(0x20).is_err());
    }

    #[test]
    fn captured_client_and_server_kinds_have_expected_direction() {
        let client = Official5188Frame::decode(&[0x36, 0x10, 0, 0, 0, 0, 0, 0]).unwrap();
        let server = Official5188Frame::decode(&[0x3e, 0x04, 0, 0, 0, 0, 0, 0]).unwrap();
        let init_control = Official5188Frame::decode(&[0x31, 0x10, 0, 0, 0, 0, 0, 0]).unwrap();
        let init_continue = Official5188Frame::decode(&[0x32, 0x10, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(client.kind, Official5188Kind::CLIENT_INIT);
        assert_eq!(client.direction(), Official5188Direction::Client);
        assert_eq!(server.kind, Official5188Kind::SERVER_META);
        assert_eq!(server.direction(), Official5188Direction::Server);
        assert_eq!(init_control.kind, Official5188Kind::SERVER_INIT_CONTROL);
        assert_eq!(init_continue.kind, Official5188Kind::SERVER_INIT_CONTINUE);
        assert_eq!(init_control.kind.wire_hex(), "3110");
        assert_eq!(init_continue.kind.wire_hex(), "3210");
        assert_eq!(init_control.direction(), Official5188Direction::Server);
        assert_eq!(init_continue.direction(), Official5188Direction::Server);
    }

    #[test]
    fn parses_observed_3e04_bulk_envelope_without_assigning_field_semantics() {
        let mut payload = Vec::with_capacity(5132);
        payload.extend_from_slice(&0x23000u32.to_le_bytes());
        payload.extend_from_slice(&0x0e048e84u32.to_le_bytes());
        payload.extend_from_slice(&0x0007f469u32.to_le_bytes());
        payload.extend(std::iter::repeat_n(0xa5, OFFICIAL_5188_BULK_BODY_LEN));
        let frame = Official5188Frame {
            kind: Official5188Kind::SERVER_META,
            metadata: [0; 4],
            payload,
        };
        let envelope = Official5188BulkEnvelope::decode(&frame).unwrap();
        assert_eq!(envelope.block_offset, 0x23000);
        assert_eq!(envelope.header_word, 0x0e048e84);
        assert_eq!(envelope.sequence_word, 0x0007f469);
        assert_eq!(envelope.body.len(), OFFICIAL_5188_BULK_BODY_LEN);
    }

    #[test]
    fn parses_observed_2a10_subscription_entry_shape() {
        let mut payload = vec![1, 0, 0, 0, 0, 4, 0, 0, 0, 0];
        payload.extend_from_slice(b"SHB\x5c\x01\0");
        payload.extend_from_slice(b"SHC\x5c\x01\0");
        let frame = Official5188Frame {
            kind: Official5188Kind::CLIENT_SUBSCRIBE,
            metadata: [0; 4],
            payload,
        };
        let subscription = Official5188SubscriptionEnvelope::decode(&frame).unwrap();
        assert_eq!(subscription.entries.len(), 2);
        assert_eq!(subscription.entries[0], *b"SHB\x5c\x01\0");
        assert_eq!(subscription.opaque_code_value(0), Some((*b"SH", 0x15c42)));
        assert_eq!(subscription.consecutive_ranges()[0].count, 2);
    }

    #[test]
    fn exposes_formal_primary_subscription_entry_count() {
        for (payload_len, declared_count) in [(6154usize, 1024u32), (358, 58)] {
            let mut payload = vec![1, 0, 0, 0];
            payload.extend_from_slice(&declared_count.to_le_bytes());
            payload.extend_from_slice(&[0; 2]);
            payload.extend(std::iter::repeat_n(0, payload_len - 10));
            let frame = Official5188Frame {
                kind: Official5188Kind::CLIENT_SUBSCRIBE,
                metadata: [0; 4],
                payload,
            };
            let subscription = Official5188SubscriptionEnvelope::decode(&frame).unwrap();
            assert_eq!(subscription.entries.len(), declared_count as usize);
            assert_eq!(subscription.declared_entry_count(), declared_count);
        }
    }

    #[test]
    fn builds_dynamic_subscription_from_session_prefix_and_assignment() {
        let mut prefix = [0u8; OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN];
        prefix[4..8].copy_from_slice(&2_u32.to_le_bytes());
        let frame = Official5188SubscriptionEnvelope::from_entries(
            prefix,
            &[(*b"SH", 0x15c55), (*b"SZ", 0x10000)],
        )
        .unwrap();
        let decoded = Official5188SubscriptionEnvelope::decode(&frame).unwrap();
        assert_eq!(decoded.declared_entry_count(), 2);
        assert_eq!(decoded.opaque_code_value(0), Some((*b"SH", 0x15c55)));
        assert_eq!(decoded.opaque_code_value(1), Some((*b"SZ", 0x10000)));
    }

    #[test]
    fn builds_cross_lifecycle_structural_subscription_prefix() {
        for count in [58usize, 59, 122, 1_024] {
            let prefix = Official5188SubscriptionEnvelope::structural_prefix(count).unwrap();
            assert_eq!(u32::from_le_bytes(prefix[..4].try_into().unwrap()), 1);
            assert_eq!(
                u32::from_le_bytes(prefix[4..8].try_into().unwrap()),
                count as u32
            );
            assert_eq!(&prefix[8..], &[0, 0]);
        }
    }

    #[test]
    fn builds_current_subscription_from_dynamic_entries_only() {
        let entries = [(*b"SH", 0x15c55), (*b"SZ", 0x10000)];
        let frame = Official5188SubscriptionEnvelope::from_current_entries(&entries).unwrap();
        let decoded = Official5188SubscriptionEnvelope::decode(&frame).unwrap();
        assert_eq!(decoded.prefix[..4], 1u32.to_le_bytes());
        assert_eq!(decoded.declared_entry_count(), 2);
        assert_eq!(decoded.opaque_code_value(0), Some(entries[0]));
        assert_eq!(decoded.opaque_code_value(1), Some(entries[1]));
    }

    #[test]
    fn rejects_subscription_assignment_count_or_market_mismatch() {
        let mut prefix = [0u8; OFFICIAL_5188_SUBSCRIPTION_PREFIX_LEN];
        prefix[4..8].copy_from_slice(&1_u32.to_le_bytes());
        assert!(Official5188SubscriptionEnvelope::from_entries(prefix, &[]).is_err());
        assert!(Official5188SubscriptionEnvelope::from_entries(prefix, &[(*b"XX", 1)]).is_err());
    }

    #[test]
    fn classifies_new_wine_control_kinds_as_server_frames() {
        for kind in [
            Official5188Kind::SERVER_OBJECT_3001,
            Official5188Kind::SERVER_CONTROL_0310,
            Official5188Kind::SERVER_HEARTBEAT,
        ] {
            let frame = Official5188Frame {
                kind,
                metadata: [0; 4],
                payload: Vec::new(),
            };
            assert_eq!(frame.direction(), Official5188Direction::Server);
        }
    }

    #[test]
    fn server_heartbeat_wire_kind_and_empty_payload_are_lossless() {
        let kind = Official5188Kind::SERVER_HEARTBEAT;
        assert_eq!(kind.0, 0x0139);
        assert_eq!(kind.wire_hex(), "3901");
        let frame = Official5188Frame::decode(&[0x39, 0x01, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(frame.kind, kind);
        assert_eq!(frame.direction(), Official5188Direction::Server);
    }

    #[test]
    fn authenticated_connect_rejects_unverified_or_supplement_endpoint() {
        assert!(
            Official5188Session::connect_authenticated(
                "127.0.0.1:5188",
                Duration::from_millis(1),
                false
            )
            .is_err()
        );
        assert!(
            Official5188Session::connect_authenticated(
                "127.0.0.1:7709",
                Duration::from_millis(1),
                true
            )
            .is_err()
        );
        assert!(
            Official5188Session::connect_authenticated(
                "127.0.0.1:547",
                Duration::from_millis(1),
                true
            )
            .is_err()
        );
    }

    #[test]
    fn client_session_envelope_round_trips_four_words_and_rejects_tail_noise() {
        let envelope = Official5188ClientSessionEnvelope {
            word0: 1,
            word1: 2,
            word2: 3,
            word3: 4,
        };
        let frame = envelope.clone().into_frame([5, 6, 7, 8]);
        assert_eq!(
            Official5188ClientSessionEnvelope::decode(&frame),
            Ok(envelope)
        );

        let mut malformed = frame;
        malformed.payload[31] = 1;
        assert!(
            Official5188ClientSessionEnvelope::decode(&malformed)
                .expect_err("reserved tail must stay zero")
                .contains("reserved tail")
        );
    }

    #[test]
    fn observed_2d10_derivation_explains_both_captured_lifecycles() {
        let versions = [1_788_223_235, 1_788_223_231, 1_788_224_391];
        let envelope = |index: usize, word1: u32, word2: u32| Official5188ClientSessionEnvelope {
            word0: Official5188ClientSessionEnvelope::observed_word0(index).unwrap(),
            word1,
            word2,
            word3: Official5188ClientSessionEnvelope::observed_word3(versions[index]),
        };
        // formal-primary 2026-09-02 00:06 (trading day 2026-09-01).
        let captured = [
            envelope(0, 0x2825_2746, 0x1f03_0135),
            envelope(1, 0x2825_2746, 0x1eff_0135),
            envelope(2, 0x2825_2746, 0x2387_0135),
        ];
        assert!(
            Official5188ClientSessionEnvelope::matches_observed_derivation(
                &captured, 20_260_901, versions
            )
        );

        // Mismatched trading day must fail.
        assert!(
            !Official5188ClientSessionEnvelope::matches_observed_derivation(
                &captured, 20_260_902, versions
            )
        );
        // Unknown group tag must fail.
        let mut foreign = captured.clone();
        foreign[0].word1 = 0x2825_1234;
        assert!(
            !Official5188ClientSessionEnvelope::matches_observed_derivation(
                &foreign, 20_260_901, versions
            )
        );
    }

    #[test]
    fn observed_word1_and_word3_derivation_matches_vendor_pm_fixture() {
        // vendor_pm 2026-08-31: word1 high half is 20260831 & 0xffff, word3
        // bucket is 0x6a94, and both group tags appear across connections.
        assert_eq!(20_260_831 & 0xffff, 0x27df);
        assert_eq!(20_260_901 & 0xffff, 0x2825);
        assert_eq!(
            Official5188ClientSessionEnvelope::observed_word1(20_260_831, SessionGroupTag::Primary),
            0x27df_2746
        );
        assert_eq!(
            Official5188ClientSessionEnvelope::observed_word1(
                20_260_831,
                SessionGroupTag::Secondary
            ),
            0x27df_b246
        );
        assert_eq!(
            Official5188ClientSessionEnvelope::observed_word3(0x6a94_u32 << 16 | 0x1234),
            0x6a94
        );
        assert_eq!(
            Official5188ClientSessionEnvelope::OBSERVED_WORD2_LOW,
            0x0135
        );
    }

    #[test]
    fn code_table_headers_build_the_captured_current_session_triplet() {
        let decode = |routing: [u8; 14]| {
            let mut bytes = vec![0u8; 92];
            bytes[..14].copy_from_slice(&routing);
            bytes[88..90].copy_from_slice(&0x2826_u16.to_le_bytes());
            bytes[90..92].copy_from_slice(&0x0135_u16.to_le_bytes());
            Official5188CodeTableHeader::decode(&bytes).unwrap()
        };
        let headers = [
            decode([
                0x0a, 0x01, 0x46, 0xb2, 0x82, 0x70, 0x97, 0x6a, 1, 0, 0, 0, b'S', b'H',
            ]),
            decode([
                0x0a, 0x01, 0x46, 0xb2, 0x7f, 0x70, 0x97, 0x6a, 1, 0, 0, 0, b'S', b'Z',
            ]),
            decode([
                0x0a, 0x01, 0x46, 0xb2, 0x0d, 0x74, 0x97, 0x6a, 1, 0, 0, 0, b'B', b'$',
            ]),
        ];
        let frames = build_official_5188_client_session_triplet(&headers).unwrap();
        let envelopes = frames
            .iter()
            .map(Official5188ClientSessionEnvelope::decode)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            envelopes,
            vec![
                Official5188ClientSessionEnvelope {
                    word0: 0x010a_4853,
                    word1: 0x2826_b246,
                    word2: 0x7082_0135,
                    word3: 0x0000_6a97
                },
                Official5188ClientSessionEnvelope {
                    word0: 0x010a_5a53,
                    word1: 0x2826_b246,
                    word2: 0x707f_0135,
                    word3: 0x0000_6a97
                },
                Official5188ClientSessionEnvelope {
                    word0: 0x010a_2442,
                    word1: 0x2826_b246,
                    word2: 0x740d_0135,
                    word3: 0x0000_6a97
                },
            ]
        );
        assert!(
            Official5188ClientSessionEnvelope::matches_observed_derivation(
                &envelopes,
                20_260_902,
                [1_788_309_634, 1_788_309_631, 1_788_310_541]
            )
        );
    }

    #[test]
    fn session_collects_four_code_tables_and_preserves_preceding_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap();
        let writer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let frames = std::iter::once(Official5188Frame {
                kind: Official5188Kind::SERVER_META,
                metadata: [9, 8, 7, 6],
                payload: vec![0; OFFICIAL_5188_BULK_HEADER_LEN + OFFICIAL_5188_BULK_BODY_LEN],
            })
            .chain([
                code_table_frame(*b"SH", 0x6a97_7082),
                code_table_frame(*b"SZ", 0x6a97_707f),
                code_table_frame(*b"B$", 0x6a97_740d),
                code_table_frame(*b"SF", 0x6a97_6e62),
            ]);
            for frame in frames {
                stream.write_all(&frame.encode().unwrap()).unwrap();
            }
        });
        let mut session =
            Official5188Session::connect(endpoint.to_string(), Duration::from_secs(1)).unwrap();
        let tables = session.receive_initial_code_tables().unwrap();
        writer.join().unwrap();

        assert_eq!(tables.observed_frames.len(), 5);
        assert_eq!(
            tables.observed_frames[0].kind,
            Official5188Kind::SERVER_META
        );
        assert_eq!(
            tables.headers.map(|header| header.market),
            [*b"SH", *b"SZ", *b"B$"]
        );
        let triplet = build_official_5188_client_session_triplet(&tables.headers).unwrap();
        let envelopes = triplet
            .iter()
            .map(Official5188ClientSessionEnvelope::decode)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(envelopes[0].word1, 0x2826_b246);
        assert_eq!(envelopes[0].word2, 0x7082_0135);
        assert_eq!(envelopes[2].word2, 0x740d_0135);
    }

    #[test]
    fn client_session_triplet_rejects_unvalidated_protocol() {
        let headers = [*b"SH", *b"SZ", *b"B$"].map(|market| Official5188CodeTableHeader {
            protocol: 0x020a,
            group: SessionGroupTag::Secondary,
            version_seconds: 1,
            market,
            trading_day_low: 0x2826,
            acknowledgement_marker: 0x0135,
        });
        assert!(
            build_official_5188_client_session_triplet(&headers)
                .expect_err("unvalidated protocol must be rejected")
                .contains("unsupported protocol")
        );
    }

    #[test]
    fn client_session_triplet_rejects_mixed_server_groups() {
        let mut headers = [
            Official5188CodeTableHeader {
                protocol: 0x010a,
                group: SessionGroupTag::Primary,
                version_seconds: 1,
                market: *b"SH",
                trading_day_low: 0x2826,
                acknowledgement_marker: 0x0135,
            },
            Official5188CodeTableHeader {
                protocol: 0x010a,
                group: SessionGroupTag::Primary,
                version_seconds: 2,
                market: *b"SZ",
                trading_day_low: 0x2826,
                acknowledgement_marker: 0x0135,
            },
            Official5188CodeTableHeader {
                protocol: 0x010a,
                group: SessionGroupTag::Primary,
                version_seconds: 3,
                market: *b"B$",
                trading_day_low: 0x2826,
                acknowledgement_marker: 0x0135,
            },
        ];
        headers[1].group = SessionGroupTag::Secondary;
        assert!(
            build_official_5188_client_session_triplet(&headers)
                .expect_err("mixed group assignment must be rejected")
                .contains("disagree")
        );
    }

    #[test]
    fn send_wine_initialization_rejects_malformed_client_session_before_writing() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let endpoint = listener.local_addr().expect("fixture address");
        let mut session =
            Official5188Session::connect(endpoint.to_string(), Duration::from_secs(1))
                .expect("connect fixture session");
        let init = || Official5188Frame {
            kind: Official5188Kind::CLIENT_INIT,
            metadata: [0; 4],
            payload: Vec::new(),
        };
        let client_session = || {
            Official5188ClientSessionEnvelope {
                word0: 1,
                word1: 2,
                word2: 3,
                word3: 4,
            }
            .into_frame([0; 4])
        };
        let mut frames = vec![
            init(),
            init(),
            init(),
            client_session(),
            client_session(),
            client_session(),
        ];
        frames[4].payload[31] = 1;

        let error = session
            .send_wine_initialization(&frames)
            .expect_err("reserved tail must be rejected before sending");
        assert!(error.contains("client session frame 2"));
        assert!(error.contains("reserved tail"));
        drop(session);

        let (mut peer, _) = listener.accept().expect("accept fixture connection");
        let mut received = Vec::new();
        peer.read_to_end(&mut received)
            .expect("read fixture stream");
        assert!(
            received.is_empty(),
            "validation failure must write no frames"
        );
    }

    #[test]
    fn exchanges_the_three_interleaved_wine_initialization_stages() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let endpoint = listener.local_addr().expect("fixture address");
        let stages = [
            (Official5188InitializationStage::Login, 48),
            (Official5188InitializationStage::Abk, 631),
            (Official5188InitializationStage::Ack, 466),
        ];
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept fixture client");
            for (stage, response_len) in stages {
                let mut header = [0u8; HEADER_LEN];
                stream.read_exact(&mut header).expect("read client header");
                let payload_len = usize::from(u16::from_le_bytes([header[2], header[3]]));
                let mut payload = vec![0u8; payload_len];
                stream
                    .read_exact(&mut payload)
                    .expect("read client payload");
                assert_eq!(
                    u16::from_le_bytes([header[0], header[1]]),
                    Official5188Kind::CLIENT_INIT.0
                );
                assert_eq!(payload_len, stage.client_payload_len());

                let response = Official5188Frame {
                    kind: stage.server_kind(),
                    metadata: [1, 2, 3, 4],
                    payload: vec![response_len as u8; response_len],
                }
                .encode()
                .expect("encode server response");
                stream.write_all(&response).expect("write server response");
            }
        });

        let mut session =
            Official5188Session::connect(endpoint.to_string(), Duration::from_secs(1))
                .expect("connect fixture session");
        for (stage, response_len) in stages {
            let client = Official5188Frame {
                kind: Official5188Kind::CLIENT_INIT,
                metadata: [0; 4],
                payload: vec![0xa5; stage.client_payload_len()],
            };
            let response = session
                .exchange_initialization_stage(stage, &client)
                .expect("exchange initialization stage");
            assert_eq!(response.kind, stage.server_kind());
            assert_eq!(response.payload.len(), response_len);
        }
        server.join().expect("join fixture server");
    }

    #[test]
    fn accepts_observed_variable_ack_client_lengths() {
        for length in [40, 44, 63, 67, 80] {
            assert!(Official5188InitializationStage::Ack.accepts_client_payload_len(length));
        }
        for length in [39, 81] {
            assert!(!Official5188InitializationStage::Ack.accepts_client_payload_len(length));
        }
    }

    #[test]
    fn accepts_formal_primary_468_byte_ack_response() {
        assert!(Official5188InitializationStage::Ack.accepts_server_payload_len(466));
        assert!(Official5188InitializationStage::Ack.accepts_server_payload_len(467));
        assert!(Official5188InitializationStage::Ack.accepts_server_payload_len(468));
    }

    #[test]
    fn accepts_formal_primary_106_byte_abk_response() {
        assert!(Official5188InitializationStage::Abk.accepts_client_payload_len(94));
        assert!(Official5188InitializationStage::Abk.accepts_client_payload_len(106));
        assert!(!Official5188InitializationStage::Abk.accepts_client_payload_len(93));
    }

    #[test]
    fn accepts_observed_634_byte_abk_response_fixture() {
        let mut payload = vec![0x41; 634];
        payload[608] = 0;
        let frame = Official5188Frame {
            kind: Official5188Kind::SERVER_INIT_CONTROL,
            metadata: [0; 4],
            payload,
        };
        assert!(
            Official5188InitializationStage::Abk.accepts_server_payload_len(frame.payload.len())
        );
        assert_eq!(
            official_5188_ack_source_prefix(&frame)
                .expect("634-byte ABK fixture")
                .len(),
            608
        );
    }

    #[test]
    fn rejects_an_unobserved_initialization_response_shape() {
        assert!(
            !Official5188InitializationStage::Login.accepts_server_payload_len(49)
                && !Official5188InitializationStage::Abk.accepts_server_payload_len(500)
                && !Official5188InitializationStage::Abk.accepts_server_payload_len(900)
                && !Official5188InitializationStage::Ack.accepts_server_payload_len(500)
        );
    }

    #[test]
    fn extracts_only_the_observed_pre_nul_ack_source_prefix() {
        for (payload_len, prefix_len) in [
            (631, 64),
            (632, 35),
            (632, 65),
            (632, 358),
            (633, 411),
            (634, 608),
            (636, 111),
            (636, 286),
        ] {
            let mut payload = vec![0x41; payload_len];
            payload[prefix_len] = 0;
            let frame = Official5188Frame {
                kind: Official5188Kind::SERVER_INIT_CONTROL,
                metadata: [0; 4],
                payload,
            };
            assert_eq!(
                official_5188_ack_source_prefix(&frame)
                    .expect("observed ACK source shape")
                    .len(),
                prefix_len
            );
        }
    }

    #[test]
    fn rejects_ack_source_payload_that_starts_with_nul() {
        let payload = vec![0x00; 631];
        let frame = Official5188Frame {
            kind: Official5188Kind::SERVER_INIT_CONTROL,
            metadata: [0; 4],
            payload,
        };
        assert!(official_5188_ack_source_prefix(&frame).is_err());
    }

    #[test]
    fn ack_source_accepts_any_first_nul_offset_across_observed_lengths() {
        // 2026-09-03 live sessions showed the first-NUL offset is NOT a fixed
        // length-keyed constant (observed 628/631/632-byte 3110 payloads with
        // first NUL anywhere). The invariant is only "kind 3110 + a NUL
        // exists"; the prefix is everything before that NUL.
        for (payload_len, prefix_len) in [
            (628, 3),
            (628, 77),
            (631, 64),
            (631, 532),
            (632, 65),
            (633, 411),
            (636, 273),
        ] {
            let mut payload = vec![0x41; payload_len];
            payload[prefix_len] = 0;
            let frame = Official5188Frame {
                kind: Official5188Kind::SERVER_INIT_CONTROL,
                metadata: [0; 4],
                payload,
            };
            assert_eq!(
                official_5188_ack_source_prefix(&frame)
                    .expect("any first-NUL prefix is the ACK source")
                    .len(),
                prefix_len
            );
        }
    }

    #[test]
    fn receive_until_disconnect_delivers_complete_frames_and_reports_close() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let endpoint = listener.local_addr().expect("fixture address");
        let first = Official5188Frame {
            kind: Official5188Kind::SERVER_DELTA,
            metadata: [1, 2, 3, 4],
            payload: vec![0x81; 32],
        }
        .encode()
        .unwrap();
        let second = Official5188Frame {
            kind: Official5188Kind::SERVER_META,
            metadata: [5, 6, 7, 8],
            payload: vec![0x42; 12],
        }
        .encode()
        .unwrap();
        let writer = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept fixture client");
            stream.write_all(&first[..5]).expect("write split prefix");
            stream
                .write_all(&first[5..])
                .expect("write split remainder");
            stream.write_all(&second).expect("write second frame");
        });

        let mut session =
            Official5188Session::connect(endpoint.to_string(), Duration::from_secs(1))
                .expect("connect fixture session");
        let mut seen = Vec::new();
        let result = session.receive_until_disconnect(|frame| {
            seen.push((frame.kind, frame.metadata, frame.payload.len()));
            Ok(())
        });
        assert!(
            result
                .expect_err("peer close must be surfaced")
                .contains("closed the connection")
        );
        writer.join().expect("join fixture writer");
        assert_eq!(
            seen,
            vec![
                (Official5188Kind::SERVER_DELTA, [1, 2, 3, 4], 32),
                (Official5188Kind::SERVER_META, [5, 6, 7, 8], 12),
            ]
        );
    }

    #[test]
    fn reconnect_authenticated_replaces_stream_and_discards_old_buffer() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let endpoint = listener.local_addr().expect("fixture address");
        let frame = Official5188Frame {
            kind: Official5188Kind::SERVER_DELTA,
            metadata: [9, 8, 7, 6],
            payload: vec![0x33; 4],
        }
        .encode()
        .unwrap();
        let expected = Official5188Frame::decode(&frame).unwrap();
        let frame_for_writer = frame.clone();
        let writer = thread::spawn(move || {
            let (mut first, _) = listener.accept().expect("accept first connection");
            first
                .write_all(&[0x27, 0x04, 0x08])
                .expect("write stale prefix");
            drop(first);
            let (mut second, _) = listener.accept().expect("accept reconnect");
            second
                .write_all(&frame_for_writer)
                .expect("write reconnect frame");
        });

        let mut session =
            Official5188Session::connect(endpoint.to_string(), Duration::from_secs(1))
                .expect("connect first session");
        assert!(
            session
                .read_frames()
                .expect("stale prefix is buffered")
                .is_empty()
        );
        let first_close = session.read_frames();
        assert!(
            first_close.is_err(),
            "closed first connection must be reported"
        );
        session
            .reconnect_authenticated(Duration::from_secs(1), true)
            .expect("reconnect same endpoint");
        let frames = session.read_frames().expect("read reconnect frame");
        assert_eq!(frames, vec![expected]);
        writer.join().expect("join fixture writer");
    }

    #[test]
    fn reconnect_requires_caller_to_resend_ordered_wine_initialization() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture listener");
        let endpoint = listener.local_addr().expect("fixture address");
        let frames = [95usize, 94, 67]
            .into_iter()
            .map(|payload_len| Official5188Frame {
                kind: Official5188Kind::CLIENT_INIT,
                metadata: [0; 4],
                payload: vec![payload_len as u8; payload_len],
            })
            .chain((0..3).map(|index| {
                Official5188ClientSessionEnvelope {
                    word0: index,
                    word1: 2,
                    word2: 3,
                    word3: 4,
                }
                .into_frame([0; 4])
            }))
            .collect::<Vec<_>>();
        let expected = frames.clone();
        let peer = thread::spawn(move || {
            let (first, _) = listener.accept().expect("accept first connection");
            drop(first);
            let (mut second, _) = listener.accept().expect("accept reconnect");
            for expected_frame in expected {
                let mut header = [0u8; HEADER_LEN];
                second.read_exact(&mut header).expect("read frame header");
                let payload_len = usize::from(u16::from_le_bytes([header[2], header[3]]));
                let mut encoded = Vec::from(header);
                encoded.resize(HEADER_LEN + payload_len, 0);
                second
                    .read_exact(&mut encoded[HEADER_LEN..])
                    .expect("read frame payload");
                assert_eq!(
                    Official5188Frame::decode(&encoded).expect("decode client frame"),
                    expected_frame
                );
            }
        });

        let mut session =
            Official5188Session::connect(endpoint.to_string(), Duration::from_secs(1))
                .expect("connect first session");
        assert!(session.read_frames().is_err());
        session
            .reconnect_authenticated(Duration::from_secs(1), true)
            .expect("reconnect same endpoint");
        session
            .send_wine_initialization(&frames)
            .expect("resend ordered initialization");
        drop(session);
        peer.join().expect("join fixture peer");
    }

    #[test]
    fn validates_wine_initialization_shape_without_decoding_payload() {
        let mut handshake = Official5188Handshake::new();
        for kind in [
            Official5188Kind::CLIENT_INIT,
            Official5188Kind::CLIENT_INIT,
            Official5188Kind::CLIENT_INIT,
            Official5188Kind::CLIENT_SESSION,
            Official5188Kind::CLIENT_SESSION,
            Official5188Kind::CLIENT_SESSION,
            Official5188Kind::CLIENT_SUBSCRIBE,
        ] {
            handshake
                .observe_client(&Official5188Frame {
                    kind,
                    metadata: [0; 4],
                    payload: Vec::new(),
                })
                .unwrap();
        }
        assert!(handshake.matches_wine_shape());
        assert_eq!(
            Official5188Frame::decode(&[0x10, 0x36, 0, 0, 0, 0, 3, 0])
                .unwrap()
                .metadata,
            [0, 0, 3, 0]
        );
    }
}
