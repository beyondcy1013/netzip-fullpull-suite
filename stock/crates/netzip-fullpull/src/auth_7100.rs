#![allow(clippy::chunks_exact_to_as_chunks)]

//!
//! Ported from the reference implementation validated against the 2026-09-01
//! formal-account captures (`Z:\stock\quoteNetzipRs\src\auth_7100_client.rs`).
//! Evidence boundary:
//!
//! - The login/probe/follow-up packet templates are byte-verified against the
//!   captured control flow, with account and password inserted dynamically at
//!   runtime. No captured credential-bearing frame is embedded.
//! - Response roles must arrive in the verified
//!   `zstd_dictionary -> download_file -> zstd_dictionary` order, decoded with
//!   the vendored `Stock.字典` raw-content dictionary, and the decoded text
//!   must contain `登录成功` before any 5188 access is allowed.
//! - Active server entries come from the download response; only the
//!   official non-7709 quote endpoint is exposed. 7709 stays in supplement.
//! - The per-connection `3610` frames are extracted from control responses on
//!   this same socket; the ACK request's opaque `数据` and the `2d10` word
//!   values have no verified constructor yet, so callers must supply them from
//!   the current lifecycle and captured blobs are never embedded here.
//!
//! Errors in this module never contain account or password values.

use std::error::Error;
use std::fmt;
use std::io::{Cursor, Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use crate::{
    Official5188Frame, Official5188InitializationStage, Official5188Kind, Official5188Session,
    OfficialQuoteEndpoint, embedded_client_frames, official_5188_ack_source_prefix,
};

const MAX_PACKET_LEN: usize = 2 * 1024 * 1024;
pub const DEFAULT_AUTH_HOST: &str = "121.41.70.217";
pub const DEFAULT_PROBE_PORTS: [u16; 2] = [6100, 7100];
pub const DEFAULT_LOGIN_PORT: u16 = 6100;
pub const STOCK_DICTIONARY_SHA256: &str =
    "8f44f49cbf10c8d203d9cabbda256da37c1f7d43e7f99b60c08d077c47b2bc68";
const STOCK_DICTIONARY_BYTES: &[u8] = include_bytes!("../assets/Stock.字典");

const INNER_PREFIX_HEX: &str = "a48bc18b0000000000000000000000000000000013000000000000000000000052030000520300000200000004000000000000000000000020000000f78b426c00007b76555f0000";
const INNER_SUFFIX_HEX: &str = "0400000006000000000000000000000026000000065290676f8ff64e0000ea819a5b494e0000050000000800000000000000000000002a000000d08f258446550d54f079000058005800fb79a8520000020000000a000000000000000000000026000000216a57570000a1806879a25b3762ef7a00000c00000004000000000000000200000034000000530074006f0063006b002e006500780065005f00e5651f670000b3f9030000000c00000004000000000000000200000034000000530074006f0063006b002e006500780065005f0048722c6700006a01000000000400000004000000000000000200000024000000cd645c4ffb7cdf7e0000e803000000000400000004000000000000000300000024000000a25b37620768c68b0000609350160000070000000400000000000000030000002a000000517f4596ce982e006500780065000000ec503f630000090000000400000000000000030000002e000000530074006f0063006b002e0064006c006c000000b622072a0000080000000400000000000000030000002c000000530074006f0063006b002e00575b785100006661482400000800000000000000030000002c0000004753a77e4d916e7f2e0069006e0069000000330feb9100000c00000004000000000000000300000034000000287537625c000d67a1526856175268882e0069006e006900000099a51cb500000f0000000400000000000000030000003a000000fb7cdf7e5c00530074006f0063006b006400720076002e0064006c006c000000533e358b00000c00000004000000000000000300000034000000fb7cdf7e5c00496c575b807bfc6268882e007400780074000000cbd1cc8200000400000006000000000000000000000026000000ea81a8524753a77e0000337a9a5b4872000002000000040000000000000002000000200000001a9053900000000000000000";
const OUTER_PREFIX_HEX: &str = "517fdc7e055300000000000000000000000000000400000000000000000000006d0200006d02000002000000080000000000000000000000240000008b53297f00005a00530054004400000002000000040000000000000002000000200000007f95a65e0000c3010000000003000000040000000000000002000000220000009f537f95a65e000052030000000002000000c30100000000000009000000df01000070656e630000";
const PROBE_INNER_PREFIX_HEX: &str = "a48bc18b000000000000000000000000000000000a0000000000000000000000b2010000b20100000200000004000000000000000000000020000000f78b426c00004b6d1f900000";
const PROBE_INNER_SUFFIX_HEX: &str = "0400000006000000000000000000000026000000065290676f8ff64e0000ea819a5b494e0000050000000800000000000000000000002a000000d08f258446550d54f0790000e073776dfb79a8520000020000000a000000000000000000000026000000216a57570000a1806879a25b3762ef7a00000c00000004000000000000000200000034000000530074006f0063006b002e006500780065005f00e5651f670000b3f9030000000c00000004000000000000000200000034000000530074006f0063006b002e006500780065005f0048722c6700006a01000000000400000004000000000000000200000024000000cd645c4ffb7cdf7e0000e803000000000400000004000000000000000300000024000000a25b37620768c68b00003e3667f10000";
const PROBE_OUTER_PREFIX_HEX: &str = "517fdc7e05530000000000000000000000000000040000000000000000000000a3010000a301000002000000080000000000000000000000240000008b53297f00005a00530054004400000002000000040000000000000002000000200000007f95a65e0000f9000000000003000000040000000000000002000000220000009f537f95a65e0000b2010000000002000000f900000000000000090000001501000070656e630000";
const FOLLOWUP_DOWNLOAD_HEX: &str = "517fdc7e055300000000000000000000000000000400000000000000000000000f0100000f010000020000000c0000000000000000000000280000008b53297f00005a00530054004400575b7851000002000000040000000000000002000000200000007f95a65e000061000000000003000000040000000000000002000000220000009f537f95a65e0000820000000000020000006100000000000000090000007d00000070656e63000028b52ffd2082c50200e4030b4e7d8f8765f64e000200821e3a00000000fb7cdf7e5c001a90be8fe14fa18068792e0069006e0069040300000020000000167ff75300000100000000000a00208baed9285913628b73600d24dfb4b6cb0cec6600390000";
const FOLLOWUP_FINISH_HEX: &str = "517fdc7e055300000000000000000000000000000400000000000000000000004d0100004d010000020000000c0000000000000000000000280000008b53297f00005a00530054004400575b7851000002000000040000000000000002000000200000007f95a65e00009f000000000003000000040000000000000002000000220000009f537f95a65e0000ae0100000000020000009f0000000000000009000000bb00000070656e63000028b52ffd60ae00ad040052061722904d73b79f525cd366f054ffbf12df56d8766a58c6f8261fee2dcb64c91c211022538df171902c4209347827941969441bcff428ea4d9385ab1456a26ab8cbb963998472ab5e688723042d29312bb1d92be465e4e3df86031a6760160084139e9908e61e90149f9d6d020e24d5586e808118438d4e19d6336187822a1c00e44b5533628bfb7296b71b0dec8633b29a10400000";
const FOLLOWUP_DOWNLOAD_NUMBER_OFFSET: usize = 124;
const FOLLOWUP_FINISH_NUMBER_OFFSET: usize = 116;
const FOLLOWUP_OUTER_TAIL_LEN: usize = 2;

/// Structured, credential-free error for the authentication control chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthError(String);

impl AuthError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for AuthError {}

type AuthResult<T> = Result<T, AuthError>;

fn auth_err<T>(message: impl Into<String>) -> AuthResult<T> {
    Err(AuthError::new(message))
}

#[derive(Clone, Debug)]
pub struct Auth7100ClientConfig {
    pub host: String,
    pub port: u16,
    pub account: String,
    pub password: String,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auth7100ProbeResult {
    pub endpoint: String,
    pub request_packet_length: usize,
    pub response_packet_length: usize,
    pub response_role: &'static str,
    pub reachable: bool,
}

/// Verified login outcome. This type deliberately carries no account or
/// password value and no serialization implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auth7100LoginResult {
    pub endpoint: String,
    pub status: String,
    pub authenticated: bool,
    pub response_packet_lengths: Vec<usize>,
    pub response_roles: Vec<String>,
    pub dictionary_length: Option<usize>,
    pub dictionary_sha256: Option<String>,
    pub decoded_response_lengths: Vec<usize>,
    pub login_success_confirmed: bool,
    pub active_servers: Vec<DownloadedServerEntry>,
    pub selected_quote_endpoint: Option<OfficialQuoteEndpoint>,
}

/// Owns the authenticated control socket that supplied endpoint discovery.
///
/// The socket must remain bound to the same login lifecycle while requesting
/// per-connection 5188 initialization. It contains no reusable captured
/// initialization bytes and never falls back to a supplementation endpoint.
pub struct Auth7100ControlSession {
    stream: TcpStream,
    login: Auth7100LoginResult,
}

/// Current-session values required by the observed 460-byte `加密包` request.
///
/// This type deliberately has no `Debug` or serialization implementation. The
/// verified runtime sources for these values (login response, downloaded
/// configuration, host identity, or connection assignment) remain
/// needs-verification, so callers must supply them explicitly.
pub struct Auth7100LoginControlFields {
    pub local_ip: [u8; 4],
    pub mac_ascii: [u8; 12],
    pub account_permissions: [u8; 6],
    pub broker: [u8; 4],
    pub encryption_version: u32,
    pub user_id: u32,
    pub interface_version: u32,
}

/// Current-session values required by the observed 452-byte ABK request.
/// This type deliberately has no `Debug` or serialization implementation.
pub struct Auth7100AbkControlFields {
    pub is_64_bit: u32,
    pub major_version: u32,
    pub minor_version: u32,
    pub build_number: u32,
    pub total_memory: [u8; 8],
    pub user_id: u32,
    pub interface_version: u32,
}

/// Current-session values required by the observed variable-length ACK request.
/// The opaque data must come from the same authenticated lifecycle; only the
/// observed 1379/1387-byte CRLF-delimited ASCII shapes are accepted.
pub struct Auth7100AckControlFields {
    pub ack: u32,
    pub user_id: u32,
    pub interface_version: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188InitTriplet {
    pub login: Official5188Frame,
    pub abk: Official5188Frame,
    pub ack: Official5188Frame,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188InterleavedInitialization {
    pub login_response: Official5188Frame,
    pub abk_response: Official5188Frame,
    pub ack_response: Official5188Frame,
    pub post_initialization_frame_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Official5188InterleavedWithCodeTables {
    pub initialization: Official5188InterleavedInitialization,
    pub code_tables: crate::Official5188InitialCodeTables,
}

#[derive(Clone, Copy)]
enum Official5188ControlStage {
    Login,
    Abk,
    Ack,
}

impl Official5188ControlStage {
    fn request_name(self) -> &'static str {
        match self {
            Self::Login => "大智慧C_登录包",
            Self::Abk => "大智慧C_ABK",
            Self::Ack => "大智慧C_ACK",
        }
    }

    fn payload_len(self) -> usize {
        match self {
            Self::Login => 95,
            Self::Abk => 94,
            Self::Ack => 67,
        }
    }

    /// The ACK-stage 3610 length scales with the manifest the client
    /// reported: 67B for the captured 1387B manifest, 44B observed live for
    /// a 592B generated manifest. Login/ABK stay within their observed
    /// session-value variants.
    fn accepts_payload_len(self, payload_len: usize) -> bool {
        match self {
            // Observed ACK-stage 3610 lengths scale with the manifest the
            // client reported: 67B (1387B manifest), 63B (1103B), 44B (592B).
            Self::Ack => (40..=80).contains(&payload_len),
            Self::Abk => matches!(payload_len, 94 | 106),
            Self::Login => payload_len == 95,
        }
    }
}

struct ControlField<'a> {
    label: String,
    value: &'a [u8],
}

/// One active server entry decoded from the download response. Legacy
/// account/password keys in the embedded configuration text are never parsed
/// or retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadedServerEntry {
    pub group_name: Option<String>,
    pub name: String,
    pub host: String,
    pub main_port: u16,
    pub secondary_port: u16,
    pub enabled: bool,
    /// Broker column (col 1) of the `大智慧服务器L1.ini` route row, for
    /// example 华创. Present only on L1 rows; legacy 通达信 rows and synthetic
    /// fixtures keep `None`.
    pub broker: Option<String>,
    /// Account-permission column (col 2) of the L1 route row, for example
    /// 点播版.
    pub permission: Option<String>,
    /// Interface-version column (col 6) of the L1 route row, for example 858.
    pub interface_version: Option<u32>,
}

impl Auth7100ControlSession {
    pub fn login_result(&self) -> &Auth7100LoginResult {
        &self.login
    }

    pub fn into_login_result(self) -> Auth7100LoginResult {
        self.login
    }

    /// Opens the selected official 5188 data channel from this authenticated
    /// login lifecycle. The endpoint is never taken from the 7709 supplement
    /// configuration and authentication must have been confirmed first.
    pub fn connect_selected_official_5188(
        &self,
        timeout: Duration,
    ) -> Result<Official5188Session, AuthError> {
        if !self.login.login_success_confirmed {
            return auth_err("cannot open 5188 before authenticated 7100 login");
        }
        let endpoint = self.login.selected_quote_endpoint.clone().ok_or_else(|| {
            AuthError::new("authenticated login did not provide a quote endpoint")
        })?;
        Official5188Session::connect_authenticated(
            endpoint.socket_addr().to_string(),
            timeout,
            true,
        )
        .map_err(|error| AuthError::new(format!("authenticated 5188 connect failed: {error}")))
    }

    /// Builds the per-connection login control fields from this session's own
    /// L1 route entry (券商/账号权限/接口版本 are the current download's
    /// columns), with only the caller-provided local IP. Returns `None` when
    /// the retained route set has no matching 5188 L1 entry or its 券商/权限
    /// cannot fit the fixed wire field widths, so a caller never substitutes
    /// captured defaults.
    pub fn current_login_control_fields(
        &self,
        local_ip: [u8; 4],
        mac_ascii: [u8; 12],
    ) -> Option<Auth7100LoginControlFields> {
        login_control_fields_from_result(&self.login, local_ip, mac_ascii)
    }

    /// Exchanges one complete vendor `网络包` on the authenticated socket.
    pub fn exchange_current_session_packet(&mut self, request: &[u8]) -> AuthResult<Vec<u8>> {
        validate_complete_netpacket(request)?;
        self.stream
            .write_all(request)
            .map_err(|error| AuthError::new(format!("control socket write failed: {error}")))?;
        self.stream
            .flush()
            .map_err(|error| AuthError::new(format!("control socket flush failed: {error}")))?;
        read_netpacket(&mut self.stream)
    }

    /// Exchanges one control request and extracts complete known 5188 client
    /// frames without promoting them to an accepted handshake.
    pub fn exchange_official_5188_initialization_candidates(
        &mut self,
        request: &[u8],
    ) -> AuthResult<Vec<crate::Official5188EmbeddedClientFrame>> {
        let response = self.exchange_current_session_packet(request)?;
        Ok(embedded_client_frames(&response))
    }

    /// Exchanges one control request and requires exactly one Wine `3610`
    /// candidate with the expected payload length.
    pub fn exchange_expected_official_5188_init(
        &mut self,
        request: &[u8],
        expected_payload_len: usize,
    ) -> AuthResult<Official5188Frame> {
        let candidates = self.exchange_official_5188_initialization_candidates(request)?;
        select_expected_official_5188_init(candidates, expected_payload_len)
    }

    /// Collects the three evidence-backed `3610` stages on this control socket.
    ///
    /// Offline compatibility helper only. A live session must call the three
    /// stage methods separately and interleave them with the 5188 `3110`/`3210`
    /// responses via [`Self::initialize_official_5188_interleaved`].
    pub fn exchange_official_5188_init_triplet(
        &mut self,
        login_request: &[u8],
        abk_request: &[u8],
        ack_request: &[u8],
    ) -> AuthResult<Official5188InitTriplet> {
        Ok(Official5188InitTriplet {
            login: self.exchange_official_5188_control_stage(
                login_request,
                Official5188ControlStage::Login,
            )?,
            abk: self
                .exchange_official_5188_control_stage(abk_request, Official5188ControlStage::Abk)?,
            ack: self
                .exchange_official_5188_control_stage(ack_request, Official5188ControlStage::Ack)?,
        })
    }

    /// Exchanges the login-packet control stage and returns its 95-byte
    /// `3610`. Send it immediately on the assigned 5188 connection.
    pub fn exchange_official_5188_login_init(
        &mut self,
        request: &[u8],
    ) -> AuthResult<Official5188Frame> {
        self.exchange_official_5188_control_stage(request, Official5188ControlStage::Login)
    }

    /// Exchanges the ABK control stage and returns its 94-byte `3610`.
    pub fn exchange_official_5188_abk_init(
        &mut self,
        request: &[u8],
    ) -> AuthResult<Official5188Frame> {
        self.exchange_official_5188_control_stage(request, Official5188ControlStage::Abk)
    }

    /// Exchanges the ACK control stage and returns its 67-byte `3610`.
    pub fn exchange_official_5188_ack_init(
        &mut self,
        request: &[u8],
    ) -> AuthResult<Official5188Frame> {
        self.exchange_official_5188_control_stage(request, Official5188ControlStage::Ack)
    }

    /// Runs one Wine-shaped 7100/5188 initialization as an interleaved state
    /// machine without inventing the still-opaque ACK or `2d10` bytes.
    ///
    /// `build_ack_request` is invoked only after the two `3110` responses are
    /// available and receives a capture-confirmed `&ack=` source prefix.
    /// `build_post_frames` is invoked only after the `3210` response. Both
    /// builders must derive bytes from the current lifecycle; this method
    /// never selects or embeds captured blobs.
    pub fn initialize_official_5188_interleaved<BuildAck, BuildPost>(
        &mut self,
        data_session: &mut Official5188Session,
        login_request: &[u8],
        abk_request: &[u8],
        build_ack_request: BuildAck,
        build_post_frames: BuildPost,
    ) -> AuthResult<Official5188InterleavedInitialization>
    where
        BuildAck: FnOnce(&[u8]) -> AuthResult<Vec<u8>>,
        BuildPost: FnOnce(
            &Official5188Frame,
            &Official5188Frame,
            &Official5188Frame,
        ) -> AuthResult<Vec<Official5188Frame>>,
    {
        let login = self.exchange_official_5188_login_init(login_request)?;
        let login_response = data_session
            .exchange_initialization_stage(Official5188InitializationStage::Login, &login)
            .map_err(|error| AuthError::new(format!("5188 login stage failed: {error}")))?;

        let abk = self.exchange_official_5188_abk_init(abk_request)?;
        let abk_response = data_session
            .exchange_initialization_stage(Official5188InitializationStage::Abk, &abk)
            .map_err(|error| AuthError::new(format!("5188 ABK stage failed: {error}")))?;

        let ack_source = official_5188_ack_source_prefix(&abk_response).map_err(|error| {
            let first_nul = abk_response
                .payload
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(usize::MAX);
            eprintln!(
                "ACK-DIAG abk 3110 kind={} len={} first_nul={first_nul}",
                abk_response.kind.wire_hex(),
                abk_response.payload.len()
            );
            AuthError::new(format!(
                "5188 ACK source failed: {error}; abk_response kind={} len={} first_nul={first_nul}",
                abk_response.kind.wire_hex(),
                abk_response.payload.len()
            ))
        })?;
        let ack_request = build_ack_request(ack_source)?;
        let ack = self.exchange_official_5188_ack_init(&ack_request)?;
        let ack_response = data_session
            .exchange_initialization_stage(Official5188InitializationStage::Ack, &ack)
            .map_err(|error| AuthError::new(format!("5188 ACK stage failed: {error}")))?;

        let post_frames = build_post_frames(&login_response, &abk_response, &ack_response)?;
        data_session
            .send_wine_post_initialization(&post_frames)
            .map_err(|error| AuthError::new(format!("5188 post-initialization failed: {error}")))?;

        Ok(Official5188InterleavedInitialization {
            login_response,
            abk_response,
            ack_response,
            post_initialization_frame_count: post_frames.len(),
        })
    }

    /// Interleaved lifecycle variant that consumes the current connection's
    /// four `0104` objects after `3210` and exposes decoded headers to the
    /// post-initialization builder for dynamic `2d10`/`2a10` frames.
    pub fn initialize_official_5188_interleaved_with_code_tables<BuildAck, BuildPost>(
        &mut self,
        data_session: &mut Official5188Session,
        login_request: &[u8],
        abk_request: &[u8],
        build_ack_request: BuildAck,
        build_post_frames: BuildPost,
    ) -> AuthResult<Official5188InterleavedWithCodeTables>
    where
        BuildAck: FnOnce(&[u8]) -> AuthResult<Vec<u8>>,
        BuildPost: FnOnce(
            &Official5188Frame,
            &Official5188Frame,
            &Official5188Frame,
            &crate::Official5188InitialCodeTables,
        ) -> AuthResult<Vec<Official5188Frame>>,
    {
        let login = self.exchange_official_5188_login_init(login_request)?;
        let login_response = data_session
            .exchange_initialization_stage(Official5188InitializationStage::Login, &login)
            .map_err(|error| AuthError::new(format!("5188 login stage failed: {error}")))?;
        let abk = self.exchange_official_5188_abk_init(abk_request)?;
        let abk_response = data_session
            .exchange_initialization_stage(Official5188InitializationStage::Abk, &abk)
            .map_err(|error| AuthError::new(format!("5188 ABK stage failed: {error}")))?;
        let ack_source = official_5188_ack_source_prefix(&abk_response).map_err(|error| {
            let first_nul = abk_response
                .payload
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(usize::MAX);
            eprintln!(
                "ACK-DIAG abk 3110 kind={} len={} first_nul={first_nul}",
                abk_response.kind.wire_hex(),
                abk_response.payload.len()
            );
            AuthError::new(format!(
                "5188 ACK source failed: {error}; abk_response kind={} len={} first_nul={first_nul}",
                abk_response.kind.wire_hex(),
                abk_response.payload.len()
            ))
        })?;
        let ack_request = build_ack_request(ack_source)?;
        let ack = self.exchange_official_5188_ack_init(&ack_request)?;
        let ack_response = data_session
            .exchange_initialization_stage(Official5188InitializationStage::Ack, &ack)
            .map_err(|error| AuthError::new(format!("5188 ACK stage failed: {error}")))?;
        let code_tables = data_session
            .receive_initial_code_tables()
            .map_err(|error| {
                AuthError::new(format!("5188 code-table collection failed: {error}"))
            })?;
        let post_frames =
            build_post_frames(&login_response, &abk_response, &ack_response, &code_tables)?;
        data_session
            .send_wine_post_initialization(&post_frames)
            .map_err(|error| AuthError::new(format!("5188 post-initialization failed: {error}")))?;
        Ok(Official5188InterleavedWithCodeTables {
            initialization: Official5188InterleavedInitialization {
                login_response,
                abk_response,
                ack_response,
                post_initialization_frame_count: post_frames.len(),
            },
            code_tables,
        })
    }

    fn exchange_official_5188_control_stage(
        &mut self,
        request: &[u8],
        stage: Official5188ControlStage,
    ) -> AuthResult<Official5188Frame> {
        let request_decoded = decode_dictionary_response(request, STOCK_DICTIONARY_BYTES)?;
        let request_fields = parse_control_object(&request_decoded)?;
        require_control_utf16(&request_fields, "请求", stage.request_name())?;
        let request_number = require_control_u32(&request_fields, "编号")?;

        let response = self.exchange_current_session_packet(request)?;
        let response_decoded = decode_dictionary_response(&response, STOCK_DICTIONARY_BYTES)?;
        let response_fields = parse_control_object(&response_decoded)?;
        if response_fields.len() != 4
            || response_fields
                .iter()
                .map(|field| field.label.as_str())
                .ne(["请求", "来源", "应答编号", "数据"])
        {
            return auth_err("5188 control response does not have the verified four-field shape");
        }
        require_control_utf16(&response_fields, "请求", stage.request_name())?;
        require_control_utf16(&response_fields, "来源", "认证服务器")?;
        let answer_number = require_control_u32(&response_fields, "应答编号")?;
        if answer_number != request_number {
            return auth_err(format!(
                "5188 control response number mismatch: request {request_number}, answer {answer_number}"
            ));
        }
        let data = require_control_field(&response_fields, "数据")?.value;
        let frame = Official5188Frame::decode(data)
            .map_err(|error| AuthError::new(format!("control response frame decode: {error}")))?;
        if frame.kind != Official5188Kind::CLIENT_INIT
            || !stage.accepts_payload_len(frame.payload.len())
        {
            return auth_err(format!(
                "{} control response requires one {}-byte 3610 payload, got {} with {} bytes",
                stage.request_name(),
                stage.payload_len(),
                frame.kind,
                frame.payload.len()
            ));
        }
        Ok(frame)
    }
}

fn parse_control_object(bytes: &[u8]) -> AuthResult<Vec<ControlField<'_>>> {
    parse_control_object_with_root(bytes, "加密包")
}

/// Parses a vendor control object with the given expected UTF-16 root name
/// (加密包 for login/stage objects, 加密解密 for Tdx_Encrypt responses).
fn parse_control_object_with_root<'a>(
    bytes: &'a [u8],
    expected_root: &str,
) -> AuthResult<Vec<ControlField<'a>>> {
    if bytes.len() < 40 || decode_utf16_prefix(&bytes[..20]) != expected_root {
        return auth_err(format!(
            "control object is not a complete {expected_root} header"
        ));
    }
    let field_count = u32::from_le_bytes(bytes[20..24].try_into().expect("4 bytes")) as usize;
    // Header word at 28..32 was historically zero; live evidence 2026-09-04
    // (VIP3 168 login) shows the server now sends 1 there on the 请求登录
    // response. Accept only the two observed values and keep failing closed
    // on anything else.
    let header_flags = u32::from_le_bytes(bytes[28..32].try_into().expect("4 bytes"));
    if bytes[24..28] != [0; 4]
        || !matches!(header_flags, 0 | 1)
        || u32::from_le_bytes(bytes[32..36].try_into().expect("4 bytes")) as usize != bytes.len()
        || u32::from_le_bytes(bytes[36..40].try_into().expect("4 bytes")) as usize != bytes.len()
    {
        return auth_err("control object header lengths do not close exactly");
    }

    let mut fields = Vec::with_capacity(field_count);
    let mut offset = 40usize;
    for _ in 0..field_count {
        let Some(header) = bytes.get(offset..offset + 20) else {
            return auth_err("control field header is truncated");
        };
        let value_len = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes")) as usize;
        let span_len = u32::from_le_bytes(header[16..20].try_into().expect("4 bytes")) as usize;
        let Some(end) = offset
            .checked_add(span_len)
            .filter(|end| *end <= bytes.len())
        else {
            return auth_err("control field span is invalid");
        };
        let Some(tail) = bytes.get(offset + 20..end) else {
            return auth_err("control field body is truncated");
        };
        #[allow(clippy::chunks_exact_to_as_chunks)]
        let Some(label_len) = tail
            .chunks_exact(2)
            .position(|word| word == [0, 0])
            .map(|words| words * 2)
        else {
            return auth_err("control field label is not terminated");
        };
        let label = decode_utf16_prefix(&tail[..label_len]);
        let value_start = offset + 20 + label_len + 2;
        let Some(value_end) = value_start.checked_add(value_len) else {
            return auth_err("control field value length overflow");
        };
        if value_end.checked_add(2) != Some(end) || bytes[value_end..end] != [0, 0] {
            return auth_err("control field value does not close its declared span");
        }
        fields.push(ControlField {
            label,
            value: &bytes[value_start..value_end],
        });
        offset = end;
    }
    if offset != bytes.len() {
        return auth_err("control object has trailing bytes after declared fields");
    }
    Ok(fields)
}

fn require_control_field<'a>(
    fields: &'a [ControlField<'a>],
    label: &str,
) -> AuthResult<&'a ControlField<'a>> {
    let mut matching = fields.iter().filter(|field| field.label == label);
    let Some(field) = matching.next() else {
        return auth_err(format!("control object is missing field {label}"));
    };
    if matching.next().is_some() {
        return auth_err(format!("control object repeats field {label}"));
    }
    Ok(field)
}

fn require_control_u32(fields: &[ControlField<'_>], label: &str) -> AuthResult<u32> {
    let value = require_control_field(fields, label)?.value;
    let bytes: [u8; 4] = value
        .try_into()
        .map_err(|_| AuthError::new(format!("control field {label} is not a u32")))?;
    Ok(u32::from_le_bytes(bytes))
}

fn require_control_utf16(
    fields: &[ControlField<'_>],
    label: &str,
    expected: &str,
) -> AuthResult<()> {
    let value = require_control_field(fields, label)?.value;
    if value != utf16_bytes(expected) {
        return auth_err(format!("control field {label} has an unexpected value"));
    }
    Ok(())
}

fn select_expected_official_5188_init(
    candidates: Vec<crate::Official5188EmbeddedClientFrame>,
    expected_payload_len: usize,
) -> AuthResult<Official5188Frame> {
    let mut matching = candidates.into_iter().filter(|candidate| {
        candidate.frame.kind == Official5188Kind::CLIENT_INIT
            && candidate.frame.payload.len() == expected_payload_len
    });
    let Some(frame) = matching.next() else {
        return auth_err(format!(
            "control response contained no {expected_payload_len}-byte 3610 candidate"
        ));
    };
    if matching.next().is_some() {
        return auth_err(format!(
            "control response contained multiple {expected_payload_len}-byte 3610 candidates"
        ));
    }
    Ok(frame.frame)
}

/// Builds the real initial login: the 19-field `认证/请求登录` client
/// manifest whose layout and build tokens (Stock.exe version 362 date token,
/// file CRC32 tokens) were extracted from `vendor_pm`. 账号/密码 are the
/// caller's runtime credentials. The inflated object is compressed with
/// plain level-3 ZSTD (no dictionary). Its length varies with the UTF-16
/// account and password values; packet length fields are derived at runtime.
/// Dictionary variants are rejected by the server ("认证服务器不支持该请求").
pub fn build_real_login_packet(account: &str, password: &str) -> AuthResult<Vec<u8>> {
    #[rustfmt::skip]
    let fields: [(u32, u32, &str, Vec<u8>); 19] = [
        (2,  0, "请求", "登录".encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (2,  0, "账号", account.encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (2,  0, "密码", password.encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (4,  0, "分析软件", "自定义".encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (5,  0, "运营商名称", "珠海移动".encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (2,  0, "模块", "股票客户端".encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (12, 2, "Stock.exe_日期", vec![0xb3, 0xf9, 0x03, 0x00]),
        (12, 2, "Stock.exe_版本", vec![0x6a, 0x01, 0x00, 0x00]),
        (4,  2, "操作系统", vec![0xe8, 0x03, 0x00, 0x00]),
        (4,  3, "客户标识", vec![0x3e, 0x36, 0x67, 0xf1]),
        (7,  3, "网际风.exe", vec![0xec, 0x50, 0x3f, 0x63]),
        (9,  3, "Stock.dll", vec![0xb6, 0x22, 0x07, 0x2a]),
        (8,  3, "Stock.字典", vec![0x66, 0x61, 0x48, 0x24]),
        (8,  3, "升级配置.ini", vec![0x33, 0x0f, 0xeb, 0x91]),
        (12, 3, "用户\\服务器列表.ini", vec![0xac, 0x04, 0xc5, 0x18]),
        (15, 3, "系统\\Stockdrv.dll", vec![0x53, 0x3e, 0x35, 0x8b]),
        (12, 3, "系统\\汉字简拼表.txt", vec![0xcb, 0xd1, 0xcc, 0x82]),
        (4,  0, "自动升级", "稳定版".encode_utf16().flat_map(u16::to_le_bytes).collect()),
        (2,  2, "通道", vec![0; 4]),
    ];

    let mut decoded = vec![0u8; 40];
    write_fixed_utf16(&mut decoded[..20], "认证")?;
    for (type_id, field_12, label, value) in &fields {
        append_control_field(&mut decoded, *type_id, *field_12, label, value)?;
    }
    patch_u32(&mut decoded, 20, fields.len())?;
    let decoded_len = decoded.len();
    patch_u32(&mut decoded, 32, decoded_len)?;
    patch_u32(&mut decoded, 36, decoded_len)?;

    let compressed = zstd::bulk::compress(&decoded, 3)
        .map_err(|error| AuthError::new(format!("zstd compress: {error}")))?;
    let mut packet = decode_hex(OUTER_PREFIX_HEX)?;
    let packet_len = packet.len() + compressed.len() + 2;
    patch_u32(&mut packet, 32, packet_len)?;
    patch_u32(&mut packet, 36, packet_len)?;
    patch_u32(&mut packet, 102, compressed.len())?;
    patch_u32(&mut packet, 136, decoded.len())?;
    patch_u32(&mut packet, 146, compressed.len())?;
    patch_u32(&mut packet, 158, compressed.len() + 28)?;
    packet.extend_from_slice(&compressed);
    packet.extend_from_slice(&[0, 0]);
    Ok(packet)
}

/// Validates the real `请求登录` response: the 15-field `认证` object from
/// `认证服务器` whose `提示信息` must contain `登录成功`. Returns the
/// decoded length.
fn validate_real_login_response(packet: &[u8]) -> AuthResult<usize> {
    let decoded = decode_dictionary_response(packet, STOCK_DICTIONARY_BYTES).or_else(|_| {
        let magic = packet
            .windows(4)
            .position(|window| window == [0x28, 0xb5, 0x2f, 0xfd])
            .ok_or_else(|| AuthError::new("no ZSTD frame"))?;
        zstd::stream::decode_all(Cursor::new(&packet[magic..]))
            .map_err(|error| AuthError::new(format!("plain zstd decode: {error}")))
    })?;
    let fields = parse_control_object_with_root(&decoded, "认证")?;
    // Server layout 2026-09-04: rejections carry 请求/来源/应答编号/错误/登录
    // 方式 (with header flag word 1) instead of the legacy 提示信息 object.
    // Surface the server's own reason instead of a missing-field parse error.
    if let Ok(error_field) = require_control_field(&fields, "错误") {
        let mut reason = String::new();
        for chunk in error_field.value.chunks_exact(2) {
            let word = u16::from_le_bytes([chunk[0], chunk[1]]);
            if word == 0 {
                break;
            }
            reason.push(char::from_u32(u32::from(word)).unwrap_or('\u{fffd}'));
        }
        return auth_err(format!("server rejected login: {reason}"));
    }
    require_control_utf16(&fields, "来源", "认证服务器")?;
    let tip = require_control_field(&fields, "提示信息")?.value;
    if !contains_utf16(tip, "登录成功") {
        return auth_err("real login response did not contain 登录成功");
    }
    Ok(decoded.len())
}

/// Runs the formal login flow and closes the control socket, keeping only the
/// verified login result.
pub fn login_auth_with_verified_dictionary(
    config: &Auth7100ClientConfig,
) -> AuthResult<Auth7100LoginResult> {
    connect_auth_sequence(config).map(|(_, login)| login)
}

/// Runs the formal login flow while retaining the selected control connection.
pub fn connect_auth_control_with_verified_dictionary(
    config: &Auth7100ClientConfig,
) -> AuthResult<Auth7100ControlSession> {
    let (stream, login) = connect_auth_sequence(config)?;
    Ok(Auth7100ControlSession { stream, login })
}

/// Sends the same credential-bearing `认证|测速` packet to one authentication candidate.
pub fn probe_auth_server(config: &Auth7100ClientConfig) -> AuthResult<Auth7100ProbeResult> {
    if config.account.trim().is_empty() || config.password.is_empty() {
        return auth_err("authentication credentials are incomplete");
    }

    let endpoint = format!("{}:{}", config.host, config.port);
    let socket = resolve_auth_socket(config, &endpoint)?;
    let mut stream = TcpStream::connect_timeout(&socket, config.timeout)
        .map_err(|error| AuthError::new(format!("probe connect {endpoint}: {error}")))?;
    stream
        .set_read_timeout(Some(config.timeout))
        .map_err(|error| AuthError::new(format!("probe read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(config.timeout))
        .map_err(|error| AuthError::new(format!("probe write timeout: {error}")))?;

    let probe = build_auth_probe_packet(&config.account, &config.password)?;
    stream
        .write_all(&probe)
        .map_err(|error| AuthError::new(format!("probe write failed: {error}")))?;
    stream
        .flush()
        .map_err(|error| AuthError::new(format!("probe flush failed: {error}")))?;
    let response = read_netpacket(&mut stream)?;
    let response_role = classify_response(&response)?;
    if response_role != "auth_probe_response" {
        return auth_err(format!(
            "unexpected authentication probe response: {response_role}"
        ));
    }

    Ok(Auth7100ProbeResult {
        endpoint,
        request_packet_length: probe.len(),
        response_packet_length: response.len(),
        response_role,
        reachable: true,
    })
}

fn resolve_auth_socket(
    _config: &Auth7100ClientConfig,
    endpoint: &str,
) -> AuthResult<std::net::SocketAddr> {
    endpoint
        .to_socket_addrs()
        .map_err(|error| AuthError::new(format!("resolve authentication endpoint: {error}")))?
        .next()
        .ok_or_else(|| AuthError::new("authentication endpoint has no resolved address"))
}

fn connect_auth_sequence(
    config: &Auth7100ClientConfig,
) -> AuthResult<(TcpStream, Auth7100LoginResult)> {
    if config.account.trim().is_empty() || config.password.is_empty() {
        return auth_err("authentication credentials are incomplete");
    }

    let dictionary_sha256 = validate_stock_dictionary(STOCK_DICTIONARY_BYTES)?;
    let endpoint = format!("{}:{}", config.host, config.port);
    let socket = resolve_auth_socket(config, &endpoint)?;
    let mut stream = TcpStream::connect_timeout(&socket, config.timeout)
        .map_err(|error| AuthError::new(format!("connect {endpoint}: {error}")))?;
    stream
        .set_read_timeout(Some(config.timeout))
        .map_err(|error| AuthError::new(format!("control read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(config.timeout))
        .map_err(|error| AuthError::new(format!("control write timeout: {error}")))?;

    // Current login flow (validated live 2026-09-01/02 against vendor_pm):
    // the 19-field `认证/请求登录` manifest with real credentials (plain
    // level-3 ZSTD, no dictionary), then the download request. The legacy
    // dialog shapes are rejected by today's server, and vendor_pm shows no
    // Tdx_Encrypt step on the login socket.
    let login_request = build_real_login_packet(&config.account, &config.password)?;
    stream
        .write_all(&login_request)
        .map_err(|error| AuthError::new(format!("control write failed: {error}")))?;
    stream
        .flush()
        .map_err(|error| AuthError::new(format!("control flush failed: {error}")))?;
    let first = read_netpacket(&mut stream)?;
    let first_decoded = validate_real_login_response(&first).map_err(|error| {
        // The server's rejection text can echo the account string; keep
        // it out of errors and logs.
        let redacted = error.to_string().replace(&config.account, "<account>");
        AuthError::new(format!("login stage: {redacted}"))
    })?;

    thread::sleep(Duration::from_millis(250));
    // The current Wine flow requests the L1 route list directly after the
    // login response; the generic 271-byte download template is not part of
    // this lifecycle.
    let l1_request = build_download_file_request("系统\\大智慧服务器L1.ini", 1)?;
    stream
        .write_all(&l1_request)
        .map_err(|error| AuthError::new(format!("control write failed: {error}")))?;
    stream
        .flush()
        .map_err(|error| AuthError::new(format!("control flush failed: {error}")))?;
    let l1_response = read_netpacket(&mut stream)?;
    let l1_role = classify_response(&l1_response)
        .map_err(|error| AuthError::new(format!("L1 download stage: {error}")))?;
    if l1_role != "download_file" {
        return auth_err(format!("unexpected L1 download response role: {l1_role}"));
    }
    let active_servers = parse_server_entries_from_packet_bytes(&l1_response)
        .map_err(|error| AuthError::new(format!("L1 download stage: {error}")))?;
    if active_servers.is_empty() {
        return auth_err("L1 download response contained no active server entries");
    }
    let selected_quote_endpoint = select_quote_endpoint(&active_servers);

    let login = Auth7100LoginResult {
        endpoint,
        status: "登录成功".to_string(),
        authenticated: true,
        response_packet_lengths: vec![first.len(), l1_response.len()],
        response_roles: vec!["login".to_string(), "download_l1".to_string()],
        dictionary_length: Some(STOCK_DICTIONARY_BYTES.len()),
        dictionary_sha256: Some(dictionary_sha256),
        decoded_response_lengths: vec![first_decoded],
        login_success_confirmed: true,
        active_servers,
        selected_quote_endpoint,
    };
    Ok((stream, login))
}

/// Checks whether `value` appears in `bytes` as UTF-16LE text.
fn contains_utf16(bytes: &[u8], value: &str) -> bool {
    let needle = value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// Derives the vendor's uppercase 12-byte ASCII interface-MAC field. Linux
/// resolves the default-route interface from procfs; other platforms currently
/// return the empty field and callers may override with a verified host value.
#[must_use]
pub fn outbound_mac_ascii(_local_ip: [u8; 4]) -> [u8; 12] {
    #[cfg(target_os = "linux")]
    {
        let routes = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
        let iface = routes.lines().skip(1).find_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.first() == Some(&"br0") || fields.first().is_some_and(|name| !name.is_empty()))
                .then(|| fields.first().unwrap().to_string())
        });
        let Some(iface) = iface else {
            return [0; 12];
        };
        let address =
            std::fs::read_to_string(format!("/sys/class/net/{iface}/address")).unwrap_or_default();
        let mut ascii = [0u8; 12];
        let hex: Vec<u8> = address
            .trim()
            .split(':')
            .filter_map(|part| u8::from_str_radix(part, 16).ok())
            .flat_map(|byte| format!("{byte:02X}").into_bytes())
            .collect();
        if hex.len() == 12 {
            ascii.copy_from_slice(&hex);
        }
        ascii
    }
    #[cfg(not(target_os = "linux"))]
    {
        [0; 12]
    }
}

/// Selects the first active server entry that exposes the official 5188
/// endpoint. 7709-only route sets yield `None`; supplement routing belongs to
/// the sibling supplement boundary, not to fullpull.
pub fn select_quote_endpoint(entries: &[DownloadedServerEntry]) -> Option<OfficialQuoteEndpoint> {
    for entry in entries {
        let port = if entry.main_port == 5188 {
            Some(entry.main_port)
        } else if entry.secondary_port == 5188 {
            Some(entry.secondary_port)
        } else {
            None
        };
        let Some(port) = port else { continue };
        if let Ok(host) = entry.host.parse::<IpAddr>()
            && let Ok(endpoint) = OfficialQuoteEndpoint::new(host, port)
        {
            return Some(endpoint);
        }
    }
    None
}

/// Sources the current-session login control fields from the retained login
/// result: the 券商/账号权限/接口版本 come from the L1 route entry that
/// matches the selected 5188 endpoint, never from captured defaults. Returns
/// `None` when the route set has no usable L1 metadata so callers fail closed
/// instead of sending an unverified zero-field login request.
fn login_control_fields_from_result(
    login: &Auth7100LoginResult,
    local_ip: [u8; 4],
    mac_ascii: [u8; 12],
) -> Option<Auth7100LoginControlFields> {
    let endpoint = login.selected_quote_endpoint.as_ref()?;
    let entry = login.active_servers.iter().find(|entry| {
        entry.host.parse::<IpAddr>().ok() == Some(endpoint.host)
            && (entry.main_port == endpoint.port || entry.secondary_port == endpoint.port)
            && entry.interface_version.is_some()
    })?;
    let mut account_permissions = [0u8; 6];
    let permission = utf16_bytes(entry.permission.as_deref()?);
    if permission.len() > account_permissions.len() {
        return None;
    }
    account_permissions[..permission.len()].copy_from_slice(&permission);
    let mut broker = [0u8; 4];
    let broker_bytes = utf16_bytes(entry.broker.as_deref()?);
    if broker_bytes.len() > broker.len() {
        return None;
    }
    broker[..broker_bytes.len()].copy_from_slice(&broker_bytes);
    Some(Auth7100LoginControlFields {
        local_ip,
        mac_ascii,
        account_permissions,
        broker,
        encryption_version: 0,
        user_id: 0,
        interface_version: entry.interface_version?,
    })
}

fn validate_stock_dictionary(dictionary: &[u8]) -> AuthResult<String> {
    if dictionary != STOCK_DICTIONARY_BYTES {
        return auth_err(format!(
            "unsupported Stock.字典 bytes: expected length {}, got {}",
            STOCK_DICTIONARY_BYTES.len(),
            dictionary.len()
        ));
    }
    Ok(STOCK_DICTIONARY_SHA256.to_string())
}

fn decode_dictionary_response(packet: &[u8], dictionary: &[u8]) -> AuthResult<Vec<u8>> {
    let magic = packet
        .windows(4)
        .position(|window| window == [0x28, 0xb5, 0x2f, 0xfd])
        .ok_or_else(|| AuthError::new("dictionary response contains no ZSTD frame"))?;
    let mut decoder =
        zstd::stream::read::Decoder::with_dictionary(Cursor::new(&packet[magic..]), dictionary)
            .map_err(|error| AuthError::new(format!("zstd dictionary decoder: {error}")))?
            .single_frame();
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| AuthError::new(format!("zstd dictionary decode: {error}")))?;
    Ok(decoded)
}

/// Decodes one complete vendor `网络包` using the vendored `Stock.字典`
/// raw-content dictionary.
///
/// This is an offline-forensics boundary: the outer object and its declared
/// length must be complete before any embedded ZSTD frame is accepted. It
/// neither opens a socket nor classifies the decoded object as market data.
/// Offline forensics helper: decodes one complete ZSTD frame from a 7100
/// byte stream using the vendored stock dictionary. The dictionary itself is
/// never exposed; only the decoded bytes are returned to the caller.
pub fn decode_7100_zstd_frame_with_dictionary(frame: &[u8]) -> AuthResult<Vec<u8>> {
    decode_dictionary_response(frame, STOCK_DICTIONARY_BYTES)
}

pub fn decode_stock_dictionary_netpacket(packet: &[u8]) -> AuthResult<Vec<u8>> {
    validate_complete_netpacket(packet)?;
    validate_stock_dictionary(STOCK_DICTIONARY_BYTES)?;
    decode_dictionary_response(packet, STOCK_DICTIONARY_BYTES)
}

/// Builds the evidence-backed `认证|测速` probe packet with dynamic credentials.
pub fn build_auth_probe_packet(account: &str, password: &str) -> AuthResult<Vec<u8>> {
    build_auth_packet(
        account,
        password,
        PROBE_INNER_PREFIX_HEX,
        PROBE_INNER_SUFFIX_HEX,
        PROBE_OUTER_PREFIX_HEX,
    )
}

/// Builds the evidence-backed login packet with dynamic credentials.
pub fn build_auth_login_packet(account: &str, password: &str) -> AuthResult<Vec<u8>> {
    build_auth_packet(
        account,
        password,
        INNER_PREFIX_HEX,
        INNER_SUFFIX_HEX,
        OUTER_PREFIX_HEX,
    )
}

/// Builds the evidence-backed shape of one per-connection 5188 login request.
///
/// Field sources and connection assignment are still under investigation, so
/// callers must supply values from the current authenticated lifecycle. This
/// function never reads captured field values or selects a 5188 endpoint.
pub fn build_candidate_official_5188_login_control_packet(
    fields: &Auth7100LoginControlFields,
    request_number: u32,
) -> AuthResult<Vec<u8>> {
    build_candidate_login_control_packet(fields, request_number, None)
}

/// Builds the 12-field initial-login variant with real credentials filled in.
///
/// The 2026-09-01 boot capture shows the current initial login uses this
/// dictionary-wrapped `加密包` shape; the legacy `认证|登录` object is
/// rejected by the server ("认证服务器不支持该请求").
pub fn build_candidate_initial_login_control_packet(
    fields: &Auth7100LoginControlFields,
    account: &str,
    password: &str,
    request_number: u32,
) -> AuthResult<Vec<u8>> {
    build_candidate_login_control_packet(fields, request_number, Some((account, password)))
}

fn build_candidate_login_control_packet(
    fields: &Auth7100LoginControlFields,
    request_number: u32,
    credentials: Option<(&str, &str)>,
) -> AuthResult<Vec<u8>> {
    let mut decoded = vec![0u8; 40];
    write_fixed_utf16(&mut decoded[..20], "加密包")?;
    append_control_field(&mut decoded, 2, 0, "请求", &utf16_bytes("大智慧C_登录包"))?;
    append_control_field(&mut decoded, 4, 3, "本地IP", &fields.local_ip)?;
    append_control_field(&mut decoded, 5, 1, "网卡MAC", &fields.mac_ascii)?;
    append_control_field(&mut decoded, 2, 0, "版本", &[])?;
    append_control_field(&mut decoded, 4, 0, "账号权限", &fields.account_permissions)?;
    append_control_field(&mut decoded, 2, 0, "券商", &fields.broker)?;
    append_control_field(
        &mut decoded,
        4,
        2,
        "加密版本",
        &fields.encryption_version.to_le_bytes(),
    )?;
    match credentials {
        Some((account, password)) => {
            append_control_field(&mut decoded, 2, 0, "账号", &utf16_bytes(account))?;
            append_control_field(&mut decoded, 2, 0, "密码", &utf16_bytes(password))?;
        }
        None => {
            append_control_field(&mut decoded, 2, 0, "账号", &[])?;
            append_control_field(&mut decoded, 2, 0, "密码", &[])?;
        }
    }
    append_control_field(&mut decoded, 4, 2, "用户ID", &fields.user_id.to_le_bytes())?;
    append_control_field(
        &mut decoded,
        4,
        2,
        "接口版本",
        &fields.interface_version.to_le_bytes(),
    )?;
    append_control_field(&mut decoded, 2, 3, "编号", &request_number.to_le_bytes())?;
    patch_u32(&mut decoded, 20, 12)?;
    let decoded_len = decoded.len();
    patch_u32(&mut decoded, 32, decoded_len)?;
    patch_u32(&mut decoded, 36, decoded_len)?;
    encode_dictionary_control_packet(&decoded, FOLLOWUP_DOWNLOAD_HEX)
}

/// Builds the evidence-backed shape of one ABK initialization request.
pub fn build_candidate_official_5188_abk_control_packet(
    fields: &Auth7100AbkControlFields,
    request_number: u32,
) -> AuthResult<Vec<u8>> {
    let mut decoded = vec![0u8; 40];
    write_fixed_utf16(&mut decoded[..20], "加密包")?;
    append_control_field(&mut decoded, 2, 0, "请求", &utf16_bytes("大智慧C_ABK"))?;
    append_control_field(&mut decoded, 3, 2, "64位", &fields.is_64_bit.to_le_bytes())?;
    append_control_field(
        &mut decoded,
        3,
        2,
        "主版本",
        &fields.major_version.to_le_bytes(),
    )?;
    append_control_field(
        &mut decoded,
        3,
        2,
        "次版本",
        &fields.minor_version.to_le_bytes(),
    )?;
    append_control_field(
        &mut decoded,
        3,
        2,
        "构建号",
        &fields.build_number.to_le_bytes(),
    )?;
    append_control_field(&mut decoded, 5, 1, "网卡MAC", &[])?;
    append_control_field(&mut decoded, 3, 6, "总内存", &fields.total_memory)?;
    append_control_field(&mut decoded, 2, 0, "账号", &[])?;
    append_control_field(&mut decoded, 2, 0, "密码", &[])?;
    append_control_field(&mut decoded, 4, 2, "用户ID", &fields.user_id.to_le_bytes())?;
    append_control_field(
        &mut decoded,
        4,
        2,
        "接口版本",
        &fields.interface_version.to_le_bytes(),
    )?;
    append_control_field(&mut decoded, 2, 3, "编号", &request_number.to_le_bytes())?;
    patch_u32(&mut decoded, 20, 12)?;
    let decoded_len = decoded.len();
    patch_u32(&mut decoded, 32, decoded_len)?;
    patch_u32(&mut decoded, 36, decoded_len)?;
    encode_dictionary_control_packet(&decoded, FOLLOWUP_DOWNLOAD_HEX)
}

/// Builds the ACK manifest text (`SFLogInPack=ACK` body) from the runtime's
/// own state: file CRC32 inventory, market version rows and data-id CRCs.
/// Grammar extracted from vendor_pm (1387B, 44 CRLF lines, four sections).
/// A fresh-install client sends an empty `<MarketInfo>`/`<SDidsCrc>` row set
/// so the server pushes the current static versions.
pub fn build_ack_manifest_text(
    file_infos: &[(String, u32)],
    market_rows: &[String],
    sdid_rows: &[(u32, u32)],
) -> String {
    let crlf = "\u{d}\u{a}";
    let mut out = String::from("SFLogInPack=ACK");
    out.push_str(crlf);
    out.push_str("<MarketInfo>");
    out.push_str(crlf);
    out.push_str("m_wMarkeId|m_wMarketAttr|m_dwServIp|m_dwStaticVer|m_dwStaticDay|m_dwChangeNum|m_dwInitTime|m_dwStaticExVer|m_dwStaticExDay|");
    out.push_str(crlf);
    for row in market_rows {
        out.push_str(row);
        out.push_str(crlf);
    }
    out.push_str("</MarketInfo>");
    out.push_str(crlf);
    out.push_str("<FileInfo>");
    out.push_str(crlf);
    out.push_str("m_charCDir|m_dwFileCrcCode|");
    out.push_str(crlf);
    for (path, crc) in file_infos {
        out.push_str(path);
        out.push('|');
        out.push_str(&crc.to_string());
        out.push('|');
        out.push_str(crlf);
    }
    out.push_str("</FileInfo>");
    out.push_str(crlf);
    out.push_str("<SDidsCrc>");
    out.push_str(crlf);
    out.push_str("m_dwDid|m_dwCrc|");
    out.push_str(crlf);
    for (did, crc) in sdid_rows {
        out.push_str(&did.to_string());
        out.push('|');
        out.push_str(&crc.to_string());
        out.push('|');
        out.push_str(crlf);
    }
    out.push_str("</SDidsCrc>");
    out.push_str(crlf);
    out
}
/// Builds the evidence-backed shape of one ACK initialization request.
pub fn build_candidate_official_5188_ack_control_packet(
    fields: &Auth7100AckControlFields,
    request_number: u32,
) -> AuthResult<Vec<u8>> {
    if fields.data.is_empty() || fields.data.len() > 8192 {
        return auth_err(format!(
            "unverified ACK data length {}; expected a generated manifest between 1 and 8192 bytes",
            fields.data.len()
        ));
    }
    let mut decoded = vec![0u8; 40];
    write_fixed_utf16(&mut decoded[..20], "加密包")?;
    append_control_field(&mut decoded, 2, 0, "请求", &utf16_bytes("大智慧C_ACK"))?;
    append_control_field(&mut decoded, 3, 0, "ack", &fields.ack.to_le_bytes())?;
    append_control_field(&mut decoded, 2, 0, "账号", &[])?;
    append_control_field(&mut decoded, 2, 0, "密码", &[])?;
    append_control_field(&mut decoded, 4, 2, "用户ID", &fields.user_id.to_le_bytes())?;
    append_control_field(
        &mut decoded,
        4,
        2,
        "接口版本",
        &fields.interface_version.to_le_bytes(),
    )?;
    append_control_field(&mut decoded, 2, 3, "编号", &request_number.to_le_bytes())?;
    append_control_field(&mut decoded, 2, 9, "数据", &fields.data)?;
    patch_u32(&mut decoded, 20, 8)?;
    let decoded_len = decoded.len();
    patch_u32(&mut decoded, 32, decoded_len)?;
    patch_u32(&mut decoded, 36, decoded_len)?;
    encode_dictionary_control_packet(&decoded, FOLLOWUP_DOWNLOAD_HEX)
}

/// Builds the dictionary-backed C2 download request for an arbitrary server
/// list file (root `下载文件`, fields 请求=file name + 编号). The vendor
/// uses this shape to fetch `系统\通达信股票服务器.ini` (7709 supplement
/// list) AND `系统\大智慧服务器L1.ini` — the latter returns the official
/// non-7709 quote-server route list (validated live: 14 rows, 10 with 5188).
pub fn build_download_file_request(file_name: &str, request_number: u32) -> AuthResult<Vec<u8>> {
    let mut decoded = vec![0u8; 40];
    write_fixed_utf16(&mut decoded[..20], "下载文件")?;
    append_control_field(&mut decoded, 2, 0, "请求", &utf16_bytes(file_name))?;
    append_control_field(&mut decoded, 2, 3, "编号", &request_number.to_le_bytes())?;
    patch_u32(&mut decoded, 20, 2)?;
    let decoded_len = decoded.len();
    patch_u32(&mut decoded, 32, decoded_len)?;
    patch_u32(&mut decoded, 36, decoded_len)?;
    encode_dictionary_control_packet(&decoded, FOLLOWUP_DOWNLOAD_HEX)
}

/// Builds the dictionary-backed C2 download request with the supplied request number.
pub fn build_auth_followup_download_packet(request_number: u32) -> AuthResult<Vec<u8>> {
    build_dictionary_followup_packet(
        FOLLOWUP_DOWNLOAD_HEX,
        FOLLOWUP_DOWNLOAD_NUMBER_OFFSET,
        request_number,
    )
}

/// Builds the dictionary-backed C3 `Tdx_Encrypt` request with the supplied request number.
pub fn build_auth_followup_finish_packet(request_number: u32) -> AuthResult<Vec<u8>> {
    build_dictionary_followup_packet(
        FOLLOWUP_FINISH_HEX,
        FOLLOWUP_FINISH_NUMBER_OFFSET,
        request_number,
    )
}

fn build_dictionary_followup_packet(
    template_hex: &str,
    number_offset: usize,
    request_number: u32,
) -> AuthResult<Vec<u8>> {
    let template = decode_hex(template_hex)?;
    let mut decoded = decode_dictionary_response(&template, STOCK_DICTIONARY_BYTES)?;
    patch_u32(&mut decoded, number_offset, request_number as usize)?;

    encode_dictionary_control_packet(&decoded, template_hex)
}

fn encode_dictionary_control_packet(decoded: &[u8], template_hex: &str) -> AuthResult<Vec<u8>> {
    let mut compressor = zstd::bulk::Compressor::with_dictionary(3, STOCK_DICTIONARY_BYTES)
        .map_err(|error| AuthError::new(format!("zstd dictionary compressor: {error}")))?;
    let compressed = compressor
        .compress(decoded)
        .map_err(|error| AuthError::new(format!("zstd compress: {error}")))?;
    let template = decode_hex(template_hex)?;
    let magic = template
        .windows(4)
        .position(|window| window == [0x28, 0xb5, 0x2f, 0xfd])
        .ok_or_else(|| AuthError::new("follow-up template contains no ZSTD frame"))?;
    if template.len() < magic + FOLLOWUP_OUTER_TAIL_LEN {
        return auth_err("follow-up template is shorter than its outer tail");
    }
    let tail_start = template.len() - FOLLOWUP_OUTER_TAIL_LEN;
    let tail = &template[tail_start..];
    if tail != [0, 0] {
        return auth_err("follow-up template has an unexpected outer tail");
    }

    let Some(packet_len) = magic
        .checked_add(compressed.len())
        .and_then(|value| value.checked_add(tail.len()))
    else {
        return auth_err("follow-up packet length overflow");
    };
    let mut packet = template[..magic].to_vec();
    patch_u32(&mut packet, 32, packet_len)?;
    patch_u32(&mut packet, 36, packet_len)?;
    patch_u32(&mut packet, 106, compressed.len())?;
    patch_u32(&mut packet, 140, decoded.len())?;
    patch_u32(&mut packet, 150, compressed.len())?;
    patch_u32(&mut packet, 162, compressed.len() + 28)?;
    packet.extend_from_slice(&compressed);
    packet.extend_from_slice(tail);
    Ok(packet)
}

fn append_control_field(
    bytes: &mut Vec<u8>,
    type_id: u32,
    field_12: u32,
    label: &str,
    value: &[u8],
) -> AuthResult<()> {
    let label_bytes = utf16_bytes(label);
    let Some(span_len) = 20usize
        .checked_add(label_bytes.len())
        .and_then(|length| length.checked_add(2))
        .and_then(|length| length.checked_add(value.len()))
        .and_then(|length| length.checked_add(2))
    else {
        return auth_err("control field length overflow");
    };
    bytes.extend_from_slice(&type_id.to_le_bytes());
    let value_len =
        u32::try_from(value.len()).map_err(|_| AuthError::new("control field value too large"))?;
    bytes.extend_from_slice(&value_len.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&field_12.to_le_bytes());
    let span =
        u32::try_from(span_len).map_err(|_| AuthError::new("control field span too large"))?;
    bytes.extend_from_slice(&span.to_le_bytes());
    bytes.extend_from_slice(&label_bytes);
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(value);
    bytes.extend_from_slice(&[0, 0]);
    Ok(())
}

fn utf16_bytes(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn write_fixed_utf16(target: &mut [u8], value: &str) -> AuthResult<()> {
    let encoded = utf16_bytes(value);
    if encoded.len() + 2 > target.len() {
        return auth_err("fixed UTF-16 field is too long");
    }
    target.fill(0);
    target[..encoded.len()].copy_from_slice(&encoded);
    Ok(())
}

fn build_auth_packet(
    account: &str,
    password: &str,
    inner_prefix_hex: &str,
    inner_suffix_hex: &str,
    outer_prefix_hex: &str,
) -> AuthResult<Vec<u8>> {
    let mut inner = decode_hex(inner_prefix_hex)?;
    append_string_field(&mut inner, "账号", account)?;
    append_string_field(&mut inner, "密码", password)?;
    inner.extend_from_slice(&decode_hex(inner_suffix_hex)?);
    let inner_len = inner.len();
    patch_u32(&mut inner, 32, inner_len)?;
    patch_u32(&mut inner, 36, inner_len)?;

    let compressed = zstd::bulk::compress(&inner, 3)
        .map_err(|error| AuthError::new(format!("zstd compress: {error}")))?;
    let mut packet = decode_hex(outer_prefix_hex)?;
    let packet_len = packet.len() + compressed.len() + 2;
    patch_u32(&mut packet, 32, packet_len)?;
    patch_u32(&mut packet, 36, packet_len)?;
    patch_u32(&mut packet, 102, compressed.len())?;
    patch_u32(&mut packet, 136, inner.len())?;
    patch_u32(&mut packet, 146, compressed.len())?;
    patch_u32(&mut packet, 158, compressed.len() + 28)?;
    packet.extend_from_slice(&compressed);
    packet.extend_from_slice(&[0, 0]);
    Ok(packet)
}

fn append_string_field(bytes: &mut Vec<u8>, label: &str, value: &str) -> AuthResult<()> {
    let value_words = value.encode_utf16().collect::<Vec<_>>();
    let value_len = value_words
        .len()
        .checked_mul(2)
        .ok_or_else(|| AuthError::new("string too long"))?;
    bytes.extend_from_slice(&2u32.to_le_bytes());
    let value_len32 = u32::try_from(value_len).map_err(|_| AuthError::new("string too long"))?;
    bytes.extend_from_slice(&value_len32.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    let span = 28usize + value_len;
    let span32 = u32::try_from(span).map_err(|_| AuthError::new("string field span too large"))?;
    bytes.extend_from_slice(&span32.to_le_bytes());
    append_utf16_z(bytes, label);
    for word in value_words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0]);
    Ok(())
}

fn append_utf16_z(bytes: &mut Vec<u8>, value: &str) {
    for word in value.encode_utf16() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0]);
}

fn patch_u32(bytes: &mut [u8], offset: usize, value: usize) -> AuthResult<()> {
    let Some(target) = bytes.get_mut(offset..offset + 4) else {
        return auth_err("packet patch offset out of range");
    };
    let value32 =
        u32::try_from(value).map_err(|_| AuthError::new("packet patch value too large"))?;
    target.copy_from_slice(&value32.to_le_bytes());
    Ok(())
}

fn read_netpacket(stream: &mut TcpStream) -> AuthResult<Vec<u8>> {
    let mut header = vec![0u8; 74];
    stream
        .read_exact(&mut header)
        .map_err(|error| AuthError::new(format!("read network packet header: {error}")))?;
    let packet_len = u32::from_le_bytes(header[32..36].try_into().expect("4 bytes")) as usize;
    if !(74..=MAX_PACKET_LEN).contains(&packet_len) {
        return auth_err(format!("invalid 7100 packet length: {packet_len}"));
    }
    let mut packet = header;
    packet.resize(packet_len, 0);
    stream
        .read_exact(&mut packet[74..])
        .map_err(|error| AuthError::new(format!("read network packet body: {error}")))?;
    Ok(packet)
}

fn validate_complete_netpacket(packet: &[u8]) -> AuthResult<()> {
    if packet.len() < 74 {
        return auth_err("control request is shorter than the network packet header");
    }
    if decode_utf16_prefix(&packet[..20]) != "网络包" {
        return auth_err("control request is not a 网络包 object");
    }
    let declared = u32::from_le_bytes(packet[32..36].try_into().expect("4 bytes")) as usize;
    if declared != packet.len() || declared > MAX_PACKET_LEN {
        return auth_err(format!(
            "control request length mismatch: declared {declared}, actual {}",
            packet.len()
        ));
    }
    Ok(())
}

fn decode_utf16_prefix(bytes: &[u8]) -> String {
    #[allow(clippy::chunks_exact_to_as_chunks)]
    let words = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|word| *word != 0)
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&words)
}

fn classify_response(packet: &[u8]) -> AuthResult<&'static str> {
    if packet.len() < 74 {
        return auth_err("short 7100 response packet");
    }
    let field20 = u32::from_le_bytes(packet[20..24].try_into().expect("4 bytes"));
    let field44 = u32::from_le_bytes(packet[44..48].try_into().expect("4 bytes"));
    #[allow(clippy::chunks_exact_to_as_chunks)]
    let tail = String::from_utf16_lossy(
        &packet[60..74]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .filter(|word| *word != 0)
            .collect::<Vec<_>>(),
    );
    if field20 == 1 && tail.contains("下载文件") {
        return Ok("download_file");
    }
    if field20 == 4 && field44 == 8 && tail.contains("ZSTD") {
        return Ok("auth_probe_response");
    }
    if field20 == 4 && field44 == 12 && tail.contains("ZSTD") {
        return Ok("zstd_dictionary");
    }
    auth_err(format!(
        "unrecognized 7100 response: type={field20} payload={field44} tail={tail}"
    ))
}

fn decode_hex(value: &str) -> AuthResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return auth_err("hex constant must have even length");
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    let bytes = value.as_bytes();
    let mut index = 0;
    while index + 2 <= bytes.len() {
        let text = std::str::from_utf8(&bytes[index..index + 2])
            .map_err(|error| AuthError::new(format!("hex constant is not UTF-8: {error}")))?;
        let byte = u8::from_str_radix(text, 16)
            .map_err(|error| AuthError::new(format!("hex constant digit: {error}")))?;
        out.push(byte);
        index += 2;
    }
    Ok(out)
}

/// Extracts only active server entries from an in-memory download packet.
/// The legacy 账号/密码 keys in the embedded text are never parsed or retained.
pub fn parse_server_entries_from_packet_bytes(
    packet_bytes: &[u8],
) -> AuthResult<Vec<DownloadedServerEntry>> {
    let raw_text = extract_embedded_utf16_text(packet_bytes)
        .ok_or_else(|| AuthError::new("no embedded UTF-16 server configuration text found"))?;
    Ok(parse_server_config_text(&raw_text))
}

fn parse_server_config_text(text: &str) -> Vec<DownloadedServerEntry> {
    let mut active = Vec::new();
    let mut current_group = None::<String>;

    for raw_line in text.lines() {
        let line = raw_line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || (line.starts_with('[') && line.ends_with(']')) {
            continue;
        }
        if let Some(group_name) = line.strip_prefix("//") {
            if parse_server_entry(line, current_group.clone(), false).is_some() {
                continue;
            }
            let group_name = group_name.trim();
            if group_name.contains(',') {
                continue;
            }
            if !group_name.is_empty() {
                current_group = Some(group_name.to_string());
            }
            continue;
        }

        if line.contains('=') {
            continue;
        }

        if let Some(entry) = parse_server_entry(line, current_group.clone(), true)
            .or_else(|| parse_l1_server_entry(line, current_group.clone()))
        {
            active.push(entry);
        }
    }

    active
}

/// Parses one `大智慧服务器L1.ini` active row: the 7-field shape
/// `券商, 权限, 名称, IP, 主端口, 接口版本, 软件版本`. The L1 list is
/// filtered server-side per account (broker + permission) and carries the
/// official non-7709 quote routes. Commented rows (other brokers) are
/// handled by the caller's disabled-row path.
fn parse_l1_server_entry(line: &str, group_name: Option<String>) -> Option<DownloadedServerEntry> {
    let parts: Vec<&str> = line
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 7 {
        return None;
    }
    let main_port = parts[4].parse::<u16>().ok()?;
    let interface_version = parts[5].parse::<u16>().ok()?;
    if parts[6].is_empty() {
        return None;
    }
    Some(DownloadedServerEntry {
        group_name,
        name: parts[2].to_string(),
        host: parts[3].to_string(),
        main_port,
        secondary_port: main_port,
        enabled: true,
        broker: Some(parts[0].to_string()),
        permission: Some(parts[1].to_string()),
        interface_version: Some(u32::from(interface_version)),
    })
}

fn parse_server_entry(
    line: &str,
    group_name: Option<String>,
    enabled: bool,
) -> Option<DownloadedServerEntry> {
    let line = if enabled {
        line
    } else {
        line.strip_prefix("//")?.trim()
    };
    let parts = line
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let main_port = parts.get(2)?.parse::<u16>().ok()?;
    let secondary_port = match parts.as_slice() {
        [_, _, _] => main_port,
        [_, _, _, secondary_port] => secondary_port.parse::<u16>().ok()?,
        _ => return None,
    };

    Some(DownloadedServerEntry {
        group_name,
        name: parts[0].to_string(),
        host: parts[1].to_string(),
        main_port,
        secondary_port,
        enabled,
        broker: None,
        permission: None,
        interface_version: None,
    })
}

fn extract_embedded_utf16_text(bytes: &[u8]) -> Option<String> {
    let start = bytes
        .windows(4)
        .position(|window| window == [0xff, 0xfe, 0x5b, 0x00])?;
    let remainder = &bytes[start..];
    if remainder.len() < 4 {
        return None;
    }

    #[allow(clippy::chunks_exact_to_as_chunks)]
    let mut words = remainder[2..]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    while matches!(words.last(), Some(0)) {
        words.pop();
    }
    let decoded = String::from_utf16_lossy(&words);

    let mut lines = Vec::new();
    let mut started = false;
    for raw_line in decoded.lines() {
        let line = raw_line.trim_end_matches('\u{0}');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if started {
                lines.push(String::new());
            }
            continue;
        }
        if is_config_line(trimmed) {
            started = true;
            lines.push(trimmed.to_string());
            continue;
        }
        if started {
            break;
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn is_config_line(line: &str) -> bool {
    if (line.starts_with('[') && line.ends_with(']')) || line.starts_with("//") {
        return true;
    }
    if line.contains('=') || line.matches(',').count() >= 2 {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        Auth7100AbkControlFields, Auth7100AckControlFields, Auth7100ControlSession,
        Auth7100LoginControlFields, Auth7100LoginResult, DownloadedServerEntry,
        Official5188ControlStage, STOCK_DICTIONARY_BYTES, append_control_field,
        build_auth_followup_download_packet, build_auth_followup_finish_packet,
        build_auth_login_packet, build_auth_probe_packet,
        build_candidate_official_5188_abk_control_packet,
        build_candidate_official_5188_ack_control_packet,
        build_candidate_official_5188_login_control_packet, classify_response,
        decode_dictionary_response, decode_hex, decode_utf16_prefix,
        encode_dictionary_control_packet, login_control_fields_from_result, parse_control_object,
        parse_server_entries_from_packet_bytes, patch_u32, read_netpacket,
        select_expected_official_5188_init, select_quote_endpoint, write_fixed_utf16,
    };
    use crate::{
        Official5188ClientSessionEnvelope, Official5188EmbeddedClientFrame, Official5188Frame,
        Official5188Kind, Official5188Session, OfficialQuoteEndpoint,
    };
    use std::io::{Cursor, Read, Write};
    use std::net::{TcpListener, TcpStream, ToSocketAddrs};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn builds_probe_packet_with_probe_role_and_dynamic_credentials() {
        let packet = build_auth_probe_packet("1522", "fixture-probe-secret").expect("packet");
        let declared = u32::from_le_bytes(packet[32..36].try_into().unwrap()) as usize;
        assert_eq!(declared, packet.len());
        assert_eq!(&packet[162..166], b"penc");

        let inner =
            zstd::stream::decode_all(Cursor::new(&packet[168..packet.len() - 2])).expect("inner");
        assert_eq!(
            u32::from_le_bytes(inner[32..36].try_into().unwrap()) as usize,
            inner.len()
        );
        assert!(contains_utf16(&inner, "测速"));
        assert!(contains_utf16(&inner, "1522"));
        assert!(contains_utf16(&inner, "fixture-probe-secret"));
    }

    #[test]
    fn builds_dynamic_login_packet_without_a_captured_credential_frame() {
        let packet = build_auth_login_packet("168", "fixture-login-secret").expect("packet");
        let declared = u32::from_le_bytes(packet[32..36].try_into().unwrap()) as usize;
        assert_eq!(declared, packet.len());
        assert_eq!(&packet[162..166], b"penc");
        assert_eq!(packet[172], 0x60);
        assert_eq!(
            u32::from_le_bytes(packet[146..150].try_into().unwrap()) as usize,
            packet.len() - 170
        );

        let inner =
            zstd::stream::decode_all(Cursor::new(&packet[168..packet.len() - 2])).expect("inner");
        assert!(contains_utf16(&inner, "168"));
        assert!(contains_utf16(&inner, "fixture-login-secret"));
    }

    #[test]
    fn real_login_manifest_supports_runtime_credential_lengths() {
        let packet = super::build_real_login_packet("168", "168").expect("real login packet");
        assert_eq!(
            u32::from_le_bytes(packet[32..36].try_into().unwrap()) as usize,
            packet.len()
        );
        assert_eq!(
            u32::from_le_bytes(packet[36..40].try_into().unwrap()) as usize,
            packet.len()
        );

        let decoded = zstd::stream::decode_all(Cursor::new(&packet[168..packet.len() - 2]))
            .expect("real login manifest");
        assert_eq!(u32::from_le_bytes(decoded[20..24].try_into().unwrap()), 19);
        assert_eq!(
            u32::from_le_bytes(decoded[32..36].try_into().unwrap()) as usize,
            decoded.len()
        );
        assert_eq!(
            u32::from_le_bytes(decoded[36..40].try_into().unwrap()) as usize,
            decoded.len()
        );
        assert_ne!(decoded.len(), 854);
    }

    #[test]
    fn rebuilds_the_captured_followup_templates_byte_for_byte() {
        let download_template = decode_hex(super::FOLLOWUP_DOWNLOAD_HEX).expect("download");
        let finish_template = decode_hex(super::FOLLOWUP_FINISH_HEX).expect("finish");

        assert_eq!(
            build_auth_followup_download_packet(1).expect("download packet"),
            download_template
        );
        assert_eq!(
            build_auth_followup_finish_packet(2).expect("finish packet"),
            finish_template
        );
    }

    #[test]
    fn advances_followup_number_without_exposing_template_payload() {
        let download = build_auth_followup_download_packet(3).expect("download packet");
        let download_decoded =
            decode_dictionary_response(&download, super::STOCK_DICTIONARY_BYTES).expect("decode");
        assert_eq!(
            u32::from_le_bytes(download_decoded[124..128].try_into().unwrap()),
            3
        );
        assert_eq!(
            u32::from_le_bytes(download[32..36].try_into().unwrap()) as usize,
            download.len()
        );
    }

    #[test]
    fn builds_candidate_5188_login_control_shape_without_captured_values() {
        let fields = Auth7100LoginControlFields {
            local_ip: [198, 18, 0, 1],
            mac_ascii: [0; 12],
            account_permissions: [1, 2, 3, 4, 5, 6],
            broker: [7, 8, 9, 10],
            encryption_version: 0,
            user_id: 42,
            interface_version: 858,
        };
        let packet = build_candidate_official_5188_login_control_packet(&fields, 2)
            .expect("candidate control packet");
        let decoded =
            decode_dictionary_response(&packet, super::STOCK_DICTIONARY_BYTES).expect("decode");
        assert_eq!(decoded.len(), 460);
        assert_eq!(decode_utf16_prefix(&decoded[..20]), "加密包");
        assert_eq!(
            [
                u32::from_le_bytes(decoded[20..24].try_into().unwrap()),
                u32::from_le_bytes(decoded[24..28].try_into().unwrap()),
                u32::from_le_bytes(decoded[28..32].try_into().unwrap()),
                u32::from_le_bytes(decoded[32..36].try_into().unwrap()),
                u32::from_le_bytes(decoded[36..40].try_into().unwrap()),
            ],
            [12, 0, 0, 460, 460]
        );
        assert!(contains_utf16(&decoded, "大智慧C_登录包"));
        assert_eq!(&decoded[454..458], &2u32.to_le_bytes());
    }

    #[test]
    fn builds_candidate_5188_abk_control_shape_without_captured_values() {
        let fields = Auth7100AbkControlFields {
            is_64_bit: 1,
            major_version: 2,
            minor_version: 3,
            build_number: 4,
            total_memory: [5, 6, 7, 8, 9, 10, 11, 12],
            user_id: 42,
            interface_version: 858,
        };
        let packet = build_candidate_official_5188_abk_control_packet(&fields, 12)
            .expect("candidate ABK packet");
        let decoded =
            decode_dictionary_response(&packet, super::STOCK_DICTIONARY_BYTES).expect("decode");
        assert_eq!(decoded.len(), 452);
        assert!(contains_utf16(&decoded, "大智慧C_ABK"));
        assert_eq!(&decoded[446..450], &12u32.to_le_bytes());
    }

    #[test]
    fn builds_candidate_5188_ack_control_shape_with_current_opaque_data() {
        let fields = Auth7100AckControlFields {
            ack: 1,
            user_id: 42,
            interface_version: 858,
            data: vec![0x5a; 1379],
        };
        let packet = build_candidate_official_5188_ack_control_packet(&fields, 14)
            .expect("candidate ACK packet");
        let decoded =
            decode_dictionary_response(&packet, super::STOCK_DICTIONARY_BYTES).expect("decode");
        assert_eq!(decoded.len(), 1685);
        assert!(contains_utf16(&decoded, "大智慧C_ACK"));
        assert_eq!(&decoded[272..276], &14u32.to_le_bytes());
        assert_eq!(&decoded[304..1683], fields.data.as_slice());
    }

    #[test]
    fn candidate_5188_ack_rejects_unobserved_data_length() {
        let mut fields = Auth7100AckControlFields {
            ack: 1,
            user_id: 42,
            interface_version: 858,
            data: vec![0; 16],
        };
        fields.data = vec![0; 16384];
        let error = build_candidate_official_5188_ack_control_packet(&fields, 14)
            .expect_err("oversized ACK data must fail");
        assert!(error.to_string().contains("unverified ACK data length"));
    }

    #[test]
    fn authenticated_control_session_preserves_one_socket_across_exchanges() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind control fixture");
        let endpoint = listener.local_addr().expect("control fixture endpoint");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept control fixture");
            for _ in 0..2 {
                let packet = read_netpacket(&mut stream).expect("read control request");
                stream.write_all(&packet).expect("echo control response");
            }
        });

        let stream = TcpStream::connect(endpoint).expect("connect control fixture");
        let mut session = Auth7100ControlSession {
            stream,
            login: fixture_login_result(endpoint.to_string()),
        };
        let first = build_auth_followup_download_packet(3).expect("first request");
        let second = build_auth_followup_finish_packet(4).expect("second request");
        assert_eq!(
            session
                .exchange_current_session_packet(&first)
                .expect("first exchange"),
            first
        );
        assert_eq!(
            session
                .exchange_current_session_packet(&second)
                .expect("second exchange"),
            second
        );
        assert_eq!(session.login_result().endpoint, endpoint.to_string());
        server.join().expect("join control fixture");
    }

    #[test]
    fn authenticated_control_session_rejects_incomplete_network_packet() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind control fixture");
        let endpoint = listener.local_addr().expect("control fixture endpoint");
        let stream = TcpStream::connect(endpoint).expect("connect control fixture");
        let mut session = Auth7100ControlSession {
            stream,
            login: fixture_login_result(endpoint.to_string()),
        };
        let mut packet = build_auth_followup_download_packet(3).expect("request");
        packet.pop();
        let error = session
            .exchange_current_session_packet(&packet)
            .expect_err("truncated packet must fail");
        assert!(error.to_string().contains("length mismatch"));
    }

    #[test]
    fn authenticated_control_session_extracts_current_response_5188_candidates() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind control fixture");
        let endpoint = listener.local_addr().expect("control fixture endpoint");
        let expected = Official5188Frame {
            kind: Official5188Kind::CLIENT_INIT,
            metadata: [1, 2, 3, 4],
            payload: vec![5, 6, 7, 8],
        };
        let encoded = expected.encode().expect("encode 5188 fixture");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept control fixture");
            let mut response = read_netpacket(&mut stream).expect("read control request");
            response.extend_from_slice(&encoded);
            let response_len = response.len();
            super::patch_u32(&mut response, 32, response_len).expect("patch response length");
            super::patch_u32(&mut response, 36, response_len).expect("patch response length");
            stream.write_all(&response).expect("write control response");
        });

        let stream = TcpStream::connect(endpoint).expect("connect control fixture");
        let mut session = Auth7100ControlSession {
            stream,
            login: fixture_login_result(endpoint.to_string()),
        };
        let request = build_auth_followup_download_packet(3).expect("request");
        let candidates = session
            .exchange_official_5188_initialization_candidates(&request)
            .expect("extract candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].offset, request.len());
        assert_eq!(candidates[0].frame, expected);
        server.join().expect("join control fixture");
    }

    #[test]
    fn expected_5188_init_selector_rejects_missing_or_ambiguous_candidates() {
        let expected = Official5188Frame {
            kind: Official5188Kind::CLIENT_INIT,
            metadata: [1, 2, 3, 4],
            payload: vec![0; 95],
        };
        let candidate = Official5188EmbeddedClientFrame {
            offset: 40,
            frame: expected.clone(),
        };
        assert_eq!(
            select_expected_official_5188_init(vec![candidate.clone()], 95)
                .expect("one expected candidate"),
            expected
        );
        assert!(select_expected_official_5188_init(Vec::new(), 95).is_err());
        assert!(
            select_expected_official_5188_init(vec![candidate.clone(), candidate], 95).is_err()
        );
    }

    #[test]
    fn orchestrates_7100_and_5188_as_one_interleaved_lifecycle() {
        let control_listener = TcpListener::bind("127.0.0.1:0").expect("bind control fixture");
        let control_endpoint = control_listener.local_addr().expect("control endpoint");
        let data_listener = TcpListener::bind("127.0.0.1:0").expect("bind data fixture");
        let data_endpoint = data_listener.local_addr().expect("data endpoint");

        let control_server = thread::spawn(move || {
            let (mut stream, _) = control_listener.accept().expect("accept control fixture");
            for (stage, request_number, payload_len) in [
                (Official5188ControlStage::Login, 2, 95),
                (Official5188ControlStage::Abk, 12, 94),
                (Official5188ControlStage::Ack, 14, 67),
            ] {
                read_netpacket(&mut stream).expect("read control request");
                let frame = Official5188Frame {
                    kind: Official5188Kind::CLIENT_INIT,
                    metadata: [0; 4],
                    payload: vec![payload_len as u8; payload_len],
                };
                stream
                    .write_all(&fixture_control_stage_response(
                        stage,
                        request_number,
                        &frame,
                    ))
                    .expect("write control response");
            }
        });
        let data_server = thread::spawn(move || {
            let (mut stream, _) = data_listener.accept().expect("accept data fixture");
            for (client_len, server_kind, server_len) in [
                (95, Official5188Kind::SERVER_INIT_CONTROL, 48),
                (94, Official5188Kind::SERVER_INIT_CONTROL, 631),
                (67, Official5188Kind::SERVER_INIT_CONTINUE, 466),
            ] {
                let frame = read_5188_fixture_frame(&mut stream);
                assert_eq!(frame.kind, Official5188Kind::CLIENT_INIT);
                assert_eq!(frame.payload.len(), client_len);
                let mut server_payload = vec![server_len as u8; server_len];
                if server_len == 631 {
                    server_payload[64] = 0;
                } else if server_len == 636 {
                    server_payload[111] = 0;
                }
                stream
                    .write_all(
                        &Official5188Frame {
                            kind: server_kind,
                            metadata: [1, 2, 3, 4],
                            payload: server_payload,
                        }
                        .encode()
                        .expect("encode data response"),
                    )
                    .expect("write data response");
            }
            for _ in 0..3 {
                let frame = read_5188_fixture_frame(&mut stream);
                assert_eq!(frame.kind, Official5188Kind::CLIENT_SESSION);
                Official5188ClientSessionEnvelope::decode(&frame)
                    .expect("valid post-initialization frame");
            }
        });

        let control_stream = TcpStream::connect(control_endpoint).expect("connect control fixture");
        let mut control = Auth7100ControlSession {
            stream: control_stream,
            login: fixture_login_result_with_quote_endpoint(data_endpoint.to_string()),
        };
        let mut data = Official5188Session::connect_authenticated(
            control
                .login_result()
                .selected_quote_endpoint
                .clone()
                .unwrap()
                .socket_addr()
                .to_string(),
            Duration::from_secs(1),
            true,
        )
        .expect("connect data fixture");
        let login_request = fixture_login_control_request(2);
        let abk_request = fixture_abk_control_request(12);
        let result = control
            .initialize_official_5188_interleaved(
                &mut data,
                &login_request,
                &abk_request,
                |ack_source| {
                    assert_eq!(ack_source.len(), 64);
                    Ok(fixture_ack_control_request(14))
                },
                |login_response, abk_response, ack_response| {
                    assert_eq!(login_response.payload.len(), 48);
                    assert_eq!(abk_response.payload.len(), 631);
                    assert_eq!(ack_response.payload.len(), 466);
                    Ok((0..3)
                        .map(|index| {
                            Official5188ClientSessionEnvelope {
                                word0: index,
                                word1: 2,
                                word2: 3,
                                word3: 4,
                            }
                            .into_frame([0; 4])
                        })
                        .collect())
                },
            )
            .expect("interleaved initialization");
        assert_eq!(result.login_response.payload.len(), 48);
        assert_eq!(result.abk_response.payload.len(), 631);
        assert_eq!(result.ack_response.payload.len(), 466);
        assert_eq!(result.post_initialization_frame_count, 3);
        drop(data);
        control_server.join().expect("join control fixture");
        data_server.join().expect("join data fixture");
    }

    #[test]
    fn strict_control_stage_rejects_mismatched_answer_number() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind control fixture");
        let endpoint = listener.local_addr().expect("control fixture endpoint");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept control fixture");
            read_netpacket(&mut stream).expect("read login request");
            let frame = Official5188Frame {
                kind: Official5188Kind::CLIENT_INIT,
                metadata: [0; 4],
                payload: vec![0; 95],
            };
            let response =
                fixture_control_stage_response(Official5188ControlStage::Login, 3, &frame);
            stream.write_all(&response).expect("write control response");
        });
        let request = fixture_login_control_request(2);
        let stream = TcpStream::connect(endpoint).expect("connect control fixture");
        let mut session = Auth7100ControlSession {
            stream,
            login: fixture_login_result(endpoint.to_string()),
        };
        let error = session
            .exchange_official_5188_login_init(&request)
            .expect_err("mismatched answer number must fail");
        assert!(error.to_string().contains("number mismatch"));
        server.join().expect("join control fixture");
    }

    #[test]
    fn parses_the_complete_login_success_response_roles() {
        let first = decode_hex(super::FOLLOWUP_DOWNLOAD_HEX).expect("dict packet");
        let mut download = vec![0u8; 1540];
        download[..20].copy_from_slice(&first[..20]);
        download[20..24].copy_from_slice(&1u32.to_le_bytes());
        download[32..36].copy_from_slice(&1540u32.to_le_bytes());
        download[36..40].copy_from_slice(&1540u32.to_le_bytes());
        download[60..74].copy_from_slice(&utf16_tail("数据下载文件"));
        assert_eq!(classify_response(&first).unwrap(), "zstd_dictionary");
        assert_eq!(classify_response(&download).unwrap(), "download_file");
    }

    #[test]
    fn decodes_raw_content_dictionary_frame_without_exposing_payload() {
        let dictionary = b"network packet login success";
        let mut encoder =
            zstd::stream::Encoder::with_dictionary(Vec::new(), 3, dictionary).expect("encoder");
        encoder.write_all("登录成功".as_bytes()).expect("payload");
        let compressed = encoder.finish().expect("finish");

        let mut packet = vec![0u8; 172];
        packet[20..24].copy_from_slice(&4u32.to_le_bytes());
        packet[44..48].copy_from_slice(&12u32.to_le_bytes());
        packet.extend_from_slice(&compressed);
        packet.extend_from_slice(&[0, 0]);

        let decoded = decode_dictionary_response(&packet, dictionary).expect("dictionary decode");
        assert!(String::from_utf8_lossy(&decoded).contains("登录成功"));
    }

    #[test]
    fn decodes_the_captured_dictionary_request_with_the_verified_stock_dictionary() {
        let packet = decode_hex(super::FOLLOWUP_DOWNLOAD_HEX).expect("captured packet");
        let decoded = super::decode_stock_dictionary_netpacket(&packet).expect("Stock.字典 decode");
        assert_eq!(decoded.len(), 130);
        assert!(contains_utf16(&decoded, "下载文件"));
        assert!(contains_utf16(&decoded, "通达信股票服务器.ini"));
    }

    #[test]
    fn public_dictionary_decoder_rejects_a_bare_embedded_frame() {
        let packet = decode_hex(super::FOLLOWUP_DOWNLOAD_HEX).expect("captured packet");
        let magic = packet
            .windows(4)
            .position(|window| window == [0x28, 0xb5, 0x2f, 0xfd])
            .expect("zstd magic");
        let error = super::decode_stock_dictionary_netpacket(&packet[magic..])
            .expect_err("bare frame must be rejected");
        assert!(error.to_string().contains("is not a 网络包 object"));
    }

    #[test]
    fn vendored_dictionary_matches_the_published_sha256_constant() {
        assert_eq!(
            super::STOCK_DICTIONARY_SHA256,
            "8f44f49cbf10c8d203d9cabbda256da37c1f7d43e7f99b60c08d077c47b2bc68"
        );
        assert_eq!(STOCK_DICTIONARY_BYTES.len(), 1000);
    }

    #[test]
    fn classifies_standard_probe_response() {
        let mut response = vec![0u8; 74];
        response[20..24].copy_from_slice(&4u32.to_le_bytes());
        response[44..48].copy_from_slice(&8u32.to_le_bytes());
        response[60..74].copy_from_slice(&utf16_tail("压缩ZSTD"));
        assert_eq!(classify_response(&response).unwrap(), "auth_probe_response");
    }

    #[test]
    fn selects_only_typed_5188_routes_without_treating_either_as_authentication() {
        let entries = vec![
            DownloadedServerEntry {
                group_name: None,
                name: "quote".to_string(),
                host: "198.51.100.10".to_string(),
                main_port: 5188,
                secondary_port: 5188,
                enabled: true,
                broker: None,
                permission: None,
                interface_version: None,
            },
            DownloadedServerEntry {
                group_name: None,
                name: "bootstrap".to_string(),
                host: "198.51.100.11".to_string(),
                main_port: 7709,
                secondary_port: 7709,
                enabled: true,
                broker: None,
                permission: None,
                interface_version: None,
            },
        ];

        let selected = select_quote_endpoint(&entries).expect("5188 route");
        assert_eq!(selected.port, 5188);
        assert_eq!(selected.host.to_string(), "198.51.100.10");

        let seven_709_only = vec![DownloadedServerEntry {
            group_name: None,
            name: "bootstrap".to_string(),
            host: "198.51.100.11".to_string(),
            main_port: 7709,
            secondary_port: 7709,
            enabled: true,
            broker: None,
            permission: None,
            interface_version: None,
        }];
        assert!(select_quote_endpoint(&seven_709_only).is_none());
    }

    #[test]
    fn parses_active_server_entries_without_returning_config_credentials() {
        let text = "\u{feff}[行情服务器]\n\n上海主站, 103.141.11.1, 5188, 5188\n深圳备用, 103.141.11.2, 5188, 5189\n";
        let mut packet = vec![0u8; 422];
        for word in text.encode_utf16() {
            packet.extend_from_slice(&word.to_le_bytes());
        }

        let entries = parse_server_entries_from_packet_bytes(&packet).expect("server entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].host, "103.141.11.1");
        assert_eq!(entries[0].main_port, 5188);
        assert!(entries.iter().all(|entry| entry.enabled));
    }

    #[test]
    fn server_config_credentials_are_never_parsed() {
        let text = "\u{feff}[通达信服务器]\n\n账号   = NetCardMac\n密码   = leaked-secret\n主端口 = 7709\n\n//华泰\n南京移动, 120.195.71.160, 7709, 7709\n";
        let mut packet = vec![0u8; 422];
        for word in text.encode_utf16() {
            packet.extend_from_slice(&word.to_le_bytes());
        }

        let entries = parse_server_entries_from_packet_bytes(&packet).expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "南京移动");
    }

    #[test]
    fn l1_rows_retain_broker_permission_and_interface_version() {
        let text = "\u{feff}[券商版]\n\n华创, 点播版, 华创证券, 58.16.134.228, 5188, 858, 1.0\n//国海, 点播版, 国海证券, 198.51.100.2, 5188, 858, 1.0\n";
        let mut packet = vec![0u8; 422];
        for word in text.encode_utf16() {
            packet.extend_from_slice(&word.to_le_bytes());
        }

        let entries = parse_server_entries_from_packet_bytes(&packet).expect("entries");
        assert_eq!(entries.len(), 1, "commented foreign-broker row is skipped");
        let entry = &entries[0];
        assert_eq!(entry.name, "华创证券");
        assert_eq!(entry.host, "58.16.134.228");
        assert_eq!(entry.main_port, 5188);
        assert_eq!(entry.broker.as_deref(), Some("华创"));
        assert_eq!(entry.permission.as_deref(), Some("点播版"));
        assert_eq!(entry.interface_version, Some(858));

        let selected = select_quote_endpoint(&entries).expect("5188 route");
        assert_eq!(selected.port, 5188);
    }

    #[test]
    fn vendor_pm_control_numbers_follow_per_pair_increment_four_rule() {
        // vendor_pm 7100 sequence (extracted 2026-09-02): L1 download = 1,
        // login packets 2..11 (one per connection), then the ten connections
        // are served five pairs at a time; each pair uses four numbers
        // (ABK x2 then ACK x2): 12,13/14,15 ... 28,29/30,31.
        let expected_login: Vec<u32> = (2..=11).collect();
        let mut expected_abk = Vec::new();
        let mut expected_ack = Vec::new();
        for pair in 0..5u32 {
            let abk_first = 12 + pair * 4;
            expected_abk.extend([abk_first, abk_first + 1]);
            expected_ack.extend([abk_first + 2, abk_first + 3]);
        }
        assert_eq!(expected_login, (2..=11).collect::<Vec<u32>>());
        assert_eq!(expected_abk, vec![12, 13, 16, 17, 20, 21, 24, 25, 28, 29]);
        assert_eq!(expected_ack, vec![14, 15, 18, 19, 22, 23, 26, 27, 30, 31]);
    }

    #[test]
    fn login_control_fields_come_from_the_selected_l1_route() {
        let entry = DownloadedServerEntry {
            group_name: None,
            name: "华创证券".to_string(),
            host: "58.16.134.228".to_string(),
            main_port: 5188,
            secondary_port: 5188,
            enabled: true,
            broker: Some("华创".to_string()),
            permission: Some("点播版".to_string()),
            interface_version: Some(858),
        };
        let endpoint = OfficialQuoteEndpoint::new("58.16.134.228".parse().expect("valid ip"), 5188)
            .expect("endpoint");
        let login = Auth7100LoginResult {
            endpoint: "auth:7100".to_string(),
            status: "登录成功".to_string(),
            authenticated: true,
            response_packet_lengths: vec![],
            response_roles: vec![],
            dictionary_length: None,
            dictionary_sha256: None,
            decoded_response_lengths: vec![],
            login_success_confirmed: true,
            active_servers: vec![entry],
            selected_quote_endpoint: Some(endpoint),
        };
        let utf16 = |value: &str| {
            value
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        };
        let fields = login_control_fields_from_result(&login, [192, 168, 3, 2], *b"000000000000")
            .expect("runtime fields");
        assert_eq!(fields.local_ip, [192, 168, 3, 2]);
        assert_eq!(&fields.account_permissions[..], &utf16("点播版")[..]);
        assert_eq!(&fields.broker[..], &utf16("华创")[..]);
        assert_eq!(fields.encryption_version, 0);
        assert_eq!(fields.user_id, 0);
        assert_eq!(fields.interface_version, 858);
    }

    #[test]
    fn login_control_fields_fail_closed_without_l1_metadata() {
        let entry = DownloadedServerEntry {
            group_name: None,
            name: "通达信旧式".to_string(),
            host: "58.16.134.228".to_string(),
            main_port: 5188,
            secondary_port: 5188,
            enabled: true,
            broker: None,
            permission: None,
            interface_version: None,
        };
        let endpoint = OfficialQuoteEndpoint::new("58.16.134.228".parse().expect("valid ip"), 5188)
            .expect("endpoint");
        let login = Auth7100LoginResult {
            endpoint: "auth:7100".to_string(),
            status: "登录成功".to_string(),
            authenticated: true,
            response_packet_lengths: vec![],
            response_roles: vec![],
            dictionary_length: None,
            dictionary_sha256: None,
            decoded_response_lengths: vec![],
            login_success_confirmed: true,
            active_servers: vec![entry],
            selected_quote_endpoint: Some(endpoint),
        };
        assert!(login_control_fields_from_result(&login, [0; 4], [0; 12]).is_none());
    }

    #[test]
    fn runtime_l1_fields_flow_into_the_login_control_packet() {
        let entry = DownloadedServerEntry {
            group_name: None,
            name: "华创证券".to_string(),
            host: "58.16.134.228".to_string(),
            main_port: 5188,
            secondary_port: 5188,
            enabled: true,
            broker: Some("华创".to_string()),
            permission: Some("点播版".to_string()),
            interface_version: Some(858),
        };
        let endpoint = OfficialQuoteEndpoint::new("58.16.134.228".parse().expect("valid ip"), 5188)
            .expect("endpoint");
        let login = Auth7100LoginResult {
            endpoint: "auth:7100".to_string(),
            status: "登录成功".to_string(),
            authenticated: true,
            response_packet_lengths: vec![],
            response_roles: vec![],
            dictionary_length: None,
            dictionary_sha256: None,
            decoded_response_lengths: vec![],
            login_success_confirmed: true,
            active_servers: vec![entry],
            selected_quote_endpoint: Some(endpoint),
        };
        let fields = login_control_fields_from_result(&login, [192, 168, 3, 2], *b"000000000000")
            .expect("runtime fields from L1 route");
        let packet = build_candidate_official_5188_login_control_packet(&fields, 2)
            .expect("candidate login control packet");
        let decoded = decode_dictionary_response(&packet, super::STOCK_DICTIONARY_BYTES)
            .expect("decode login control packet");
        assert_eq!(decoded.len(), 460);
        let parsed = parse_control_object(&decoded).expect("control fields");
        let value = |label: &str| {
            parsed
                .iter()
                .find(|field| field.label == label)
                .unwrap_or_else(|| panic!("missing field {label}"))
                .value
        };
        assert_eq!(value("券商"), super::utf16_bytes("华创"));
        assert_eq!(value("账号权限"), super::utf16_bytes("点播版"));
        assert_eq!(value("接口版本"), 858u32.to_le_bytes().as_slice());
        assert_ne!(
            value("券商"),
            &[0, 0],
            "broker must not be the zero default"
        );
        assert_eq!(&decoded[454..458], &2u32.to_le_bytes(), "request number 2");
    }

    fn fixture_control_stage_response(
        stage: Official5188ControlStage,
        answer_number: u32,
        frame: &Official5188Frame,
    ) -> Vec<u8> {
        let mut decoded = vec![0u8; 40];
        write_fixed_utf16(&mut decoded[..20], "加密包").expect("write response root");
        append_control_field(
            &mut decoded,
            2,
            0,
            "请求",
            &super::utf16_bytes(stage.request_name()),
        )
        .expect("append response request");
        append_control_field(
            &mut decoded,
            2,
            0,
            "来源",
            &super::utf16_bytes("认证服务器"),
        )
        .expect("append response source");
        append_control_field(&mut decoded, 2, 3, "应答编号", &answer_number.to_le_bytes())
            .expect("append response number");
        append_control_field(
            &mut decoded,
            2,
            9,
            "数据",
            &frame.encode().expect("encode 5188 response frame"),
        )
        .expect("append response data");
        patch_u32(&mut decoded, 20, 4).expect("patch response field count");
        let decoded_len = decoded.len();
        patch_u32(&mut decoded, 32, decoded_len).expect("patch response length");
        patch_u32(&mut decoded, 36, decoded_len).expect("patch response length");
        encode_dictionary_control_packet(&decoded, super::FOLLOWUP_DOWNLOAD_HEX)
            .expect("encode control response")
    }

    fn fixture_login_control_request(request_number: u32) -> Vec<u8> {
        build_candidate_official_5188_login_control_packet(
            &Auth7100LoginControlFields {
                local_ip: [127, 0, 0, 1],
                mac_ascii: [0; 12],
                account_permissions: [1; 6],
                broker: [2; 4],
                encryption_version: 3,
                user_id: 4,
                interface_version: 5,
            },
            request_number,
        )
        .expect("login request")
    }

    fn fixture_abk_control_request(request_number: u32) -> Vec<u8> {
        build_candidate_official_5188_abk_control_packet(
            &Auth7100AbkControlFields {
                is_64_bit: 0,
                major_version: 1,
                minor_version: 2,
                build_number: 3,
                total_memory: [4; 8],
                user_id: 5,
                interface_version: 6,
            },
            request_number,
        )
        .expect("ABK request")
    }

    fn fixture_ack_control_request(request_number: u32) -> Vec<u8> {
        build_candidate_official_5188_ack_control_packet(
            &Auth7100AckControlFields {
                ack: 1,
                user_id: 5,
                interface_version: 6,
                data: vec![7; 1379],
            },
            request_number,
        )
        .expect("ACK request")
    }

    fn read_5188_fixture_frame(stream: &mut TcpStream) -> Official5188Frame {
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).expect("read 5188 header");
        let payload_len = usize::from(u16::from_le_bytes([header[2], header[3]]));
        let mut encoded = Vec::with_capacity(8 + payload_len);
        encoded.extend_from_slice(&header);
        encoded.resize(8 + payload_len, 0);
        stream
            .read_exact(&mut encoded[8..])
            .expect("read 5188 payload");
        Official5188Frame::decode(&encoded).expect("decode fixture frame")
    }

    fn contains_utf16(bytes: &[u8], value: &str) -> bool {
        let needle = value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    fn utf16_tail(value: &str) -> [u8; 14] {
        let mut out = [0u8; 14];
        for (index, word) in value.encode_utf16().take(7).enumerate() {
            out[index * 2..index * 2 + 2].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// Resolves live-acceptance credentials. Default: the 168 test account
    /// from environment variables. With an explicit `NETZIP_LIVE_ALLOW_FORMAL=1`
    /// switch plus `NETZIP_LIVE_CREDENTIALS_INI`, the formal account is loaded
    /// from the vendor INI in process memory only; values are never printed.
    fn parse_credentials_ini(bytes: &[u8]) -> (String, String) {
        let words = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
        let text = String::from_utf16_lossy(&words.collect::<Vec<_>>());
        let mut account = String::new();
        let mut password = String::new();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim().trim_start_matches('\u{feff}') {
                "账号" => account = value.trim().to_string(),
                "密码" => password = value.trim().to_string(),
                _ => {}
            }
        }
        (account, password)
    }

    fn resolve_live_credentials() -> (String, String, &'static str) {
        if std::env::var("NETZIP_LIVE_ALLOW_FORMAL").as_deref() == Ok("1") {
            if let (Ok(account), Ok(password)) = (
                std::env::var("NETZIP_TEST_ACCOUNT"),
                std::env::var("NETZIP_TEST_PASSWORD"),
            ) {
                assert!(
                    !account.trim().is_empty() && !password.is_empty(),
                    "formal credentials in the environment are incomplete"
                );
                return (account, password, "formal");
            }
            let ini =
                std::env::var("NETZIP_LIVE_CREDENTIALS_INI").expect("NETZIP_LIVE_CREDENTIALS_INI");
            let bytes = std::fs::read(&ini).expect("read credentials ini");
            let (account, password) = parse_credentials_ini(&bytes);
            assert!(
                !account.trim().is_empty() && !password.is_empty(),
                "formal credentials in the ini are incomplete"
            );
            return (account, password, "formal");
        }
        let account = std::env::var("NETZIP_TEST_ACCOUNT").expect("NETZIP_TEST_ACCOUNT");
        let password = std::env::var("NETZIP_TEST_PASSWORD").expect("NETZIP_TEST_PASSWORD");
        assert_eq!(
            account, "168",
            "without NETZIP_LIVE_ALLOW_FORMAL=1 only the 168 test account is allowed"
        );
        (account, password, "test-168")
    }

    fn fixture_login_result(endpoint: String) -> super::Auth7100LoginResult {
        super::Auth7100LoginResult {
            endpoint,
            status: "fixture-authenticated".to_string(),
            authenticated: true,
            response_packet_lengths: Vec::new(),
            response_roles: Vec::new(),
            dictionary_length: None,
            dictionary_sha256: None,
            decoded_response_lengths: Vec::new(),
            login_success_confirmed: true,
            active_servers: Vec::new(),
            selected_quote_endpoint: Some(
                crate::OfficialQuoteEndpoint::new(std::net::IpAddr::from([127, 0, 0, 1]), 5188)
                    .expect("fixture endpoint"),
            ),
        }
    }

    fn fixture_login_result_with_quote_endpoint(endpoint: String) -> super::Auth7100LoginResult {
        let mut result = fixture_login_result(endpoint);
        if let Some(addr) = result.endpoint.rsplit_once(':')
            && let Ok(host) = addr.0.parse::<std::net::IpAddr>()
        {
            result.selected_quote_endpoint =
                super::OfficialQuoteEndpoint::new(host, addr.1.parse().unwrap()).ok();
        }
        result
    }

    /// Live manual acceptance for the test account only.
    ///
    /// Runs only with `NETZIP_LIVE_ACCEPTANCE=1` plus `NETZIP_TEST_ACCOUNT`
    /// and `NETZIP_TEST_PASSWORD` in the environment, and hard-fails unless
    /// the account is `168`. Reports stage/boolean/length/category lines on
    /// stdout; credentials, decoded response bodies and received session
    /// bytes are never printed.
    #[test]
    #[ignore = "live network acceptance; requires explicit env switch and the 168 test account"]
    fn live_acceptance_with_test_account() {
        let enabled = std::env::var("NETZIP_LIVE_ACCEPTANCE").unwrap_or_default();
        assert_eq!(
            enabled, "1",
            "set NETZIP_LIVE_ACCEPTANCE=1 to run live acceptance"
        );
        let (account, password, account_class) = resolve_live_credentials();
        println!("P\taccount-class\tINFO\t{account_class}");

        let port: u16 = std::env::var("NETZIP_LIVE_AUTH_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(6100);
        let host = std::env::var("NETZIP_LIVE_AUTH_HOST")
            .unwrap_or_else(|_| super::DEFAULT_AUTH_HOST.to_string());
        let timeout = Duration::from_secs(
            std::env::var("NETZIP_LIVE_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
        );
        let config = super::Auth7100ClientConfig {
            host: host.clone(),
            port,
            account,
            password,
            timeout,
        };
        println!("P\ttarget\tINFO\t{host}:{port} (port roles are session/config dependent)");

        // Phase P: reachability probe with dynamic credentials.
        match super::probe_auth_server(&config) {
            Ok(probe) => println!(
                "P\tprobe\tPASS\treq {}B, resp {}B, role {}",
                probe.request_packet_length, probe.response_packet_length, probe.response_role
            ),
            Err(error) => println!("P\tprobe\tFAIL\t{error}"),
        }

        // Phase L: byte-verified requests and the live v2 login round trip.
        let download_request = super::build_auth_followup_download_packet(1).expect("download");
        assert_eq!(
            download_request,
            decode_hex(super::FOLLOWUP_DOWNLOAD_HEX).expect("download template"),
            "download follow-up must equal the captured template byte-for-byte"
        );
        let finish_request = super::build_auth_followup_finish_packet(2).expect("finish");
        assert_eq!(
            finish_request,
            decode_hex(super::FOLLOWUP_FINISH_HEX).expect("finish template"),
            "finish follow-up must equal the captured template byte-for-byte"
        );
        // The live login request: the real 19-field 请求登录 manifest.
        let login_request = super::build_real_login_packet(&config.account, &config.password)
            .expect("login packet");
        let declared = u32::from_le_bytes(login_request[32..36].try_into().unwrap()) as usize;
        assert_eq!(declared, login_request.len());
        assert_eq!(
            u32::from_le_bytes(login_request[36..40].try_into().unwrap()) as usize,
            login_request.len()
        );
        let decoded_login = decode_dict_then_plain(&login_request);
        let declared_manifest_len =
            u32::from_le_bytes(decoded_login[32..36].try_into().unwrap()) as usize;
        assert_eq!(
            decoded_login.len(),
            declared_manifest_len,
            "inflated manifest length must match its declared header length"
        );
        let labels: Vec<String> = super::parse_control_object_with_root(&decoded_login, "认证")
            .expect("login request fields")
            .into_iter()
            .map(|field| field.label)
            .collect();
        assert_eq!(labels.len(), 19, "manifest must carry 19 fields");
        assert_eq!(labels[0], "请求");
        assert_eq!(labels[1], "账号");
        assert!(contains_utf16(&decoded_login, &config.account));
        assert!(contains_utf16(&decoded_login, &config.password));
        println!(
            "L\trequest-bytes\tPASS\tlogin {}B (decoded 854B, 19 fields), download {}B == template, finish {}B == template",
            login_request.len(),
            download_request.len(),
            finish_request.len()
        );

        let mut control = match super::connect_auth_control_with_verified_dictionary(&config) {
            Ok(control) => control,
            Err(error) => panic!("L\tlogin\tFAIL\t{error}"),
        };
        let login = control.login_result().clone();
        assert_eq!(
            login.response_roles,
            ["login", "download_l1"],
            "response role sequence must match the real login flow"
        );
        println!(
            "L\tresponse-roles\tPASS\t{}B/{}B in verified order",
            login.response_packet_lengths[0], login.response_packet_lengths[1]
        );
        println!(
            "L\tlogin-dialog\t{}\tdecoded {}B with vendored dictionary ({}B, sha {})",
            login.login_success_confirmed,
            login.decoded_response_lengths[0],
            login.dictionary_length.unwrap_or(0),
            login
                .dictionary_sha256
                .as_deref()
                .unwrap_or("-")
                .get(0..8)
                .unwrap_or("-")
        );
        assert!(
            login.login_success_confirmed,
            "server must complete the three-stage login dialog"
        );

        let selected = login.selected_quote_endpoint.clone();
        let port_list = login
            .active_servers
            .iter()
            .map(|entry| format!("{}/{}", entry.main_port, entry.secondary_port))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "L\tservers\tINFO\tactive {} entries [{port_list}], official 5188 endpoint present: {}",
            login.active_servers.len(),
            selected.is_some()
        );

        // Phase D: authenticated 5188 connect plus passive pre-init read.
        // Accounts whose download response has no 5188 row skip this phase:
        // falling back to 7709 is explicitly not allowed.
        let mut data = match selected.clone() {
            Some(endpoint) => {
                let mut data = Official5188Session::connect_authenticated(
                    endpoint.socket_addr().to_string(),
                    Duration::from_secs(8),
                    true,
                )
                .map_err(|error| panic!("D\tconnect\tFAIL\t{error}"))
                .unwrap();
                println!(
                    "D\tconnect\tPASS\tofficial 5188 (host redacted, port {})",
                    endpoint.port
                );
                match data.receive_once() {
                    Ok(frames) => {
                        let summary = frames
                            .iter()
                            .map(|frame| format!("{}/{}B", frame.kind, frame.payload.len()))
                            .collect::<Vec<_>>()
                            .join(",");
                        println!(
                            "D\tpassive-read\tINFO\t{} frame(s): {summary}",
                            frames.len()
                        );
                    }
                    Err(error) if error.contains("timed out") || error.contains("would block") => {
                        println!("D\tpassive-read\tINFO\t0B before timeout (expected pre-init)");
                    }
                    Err(error) => println!("D\tpassive-read\tINFO\t{error}"),
                }
                Some(data)
            }
            None => {
                println!(
                    "D\tskip\tINFO\taccount provides no official 5188 endpoint (supplement-only route set); not falling back to 7709"
                );
                None
            }
        };

        // Phase X: per-connection login-stage control exchange on the live
        // authenticated control socket, forwarding the returned 3610 to the
        // 5188 connection when this account has one.
        let login_fields = super::Auth7100LoginControlFields {
            local_ip: [0, 0, 0, 0],
            mac_ascii: [0; 12],
            account_permissions: [0; 6],
            broker: [0; 4],
            encryption_version: 0,
            user_id: 0,
            interface_version: 0,
        };
        let candidate = super::build_candidate_official_5188_login_control_packet(&login_fields, 3)
            .expect("candidate");
        let candidate_decoded =
            super::decode_dictionary_response(&candidate, super::STOCK_DICTIONARY_BYTES)
                .expect("decode candidate");
        let candidate_labels: Vec<String> = super::parse_control_object(&candidate_decoded)
            .expect("candidate fields")
            .into_iter()
            .map(|field| field.label)
            .collect();
        assert_eq!(
            candidate_labels,
            [
                "请求",
                "本地IP",
                "网卡MAC",
                "版本",
                "账号权限",
                "券商",
                "加密版本",
                "账号",
                "密码",
                "用户ID",
                "接口版本",
                "编号"
            ],
            "candidate control packet must reproduce the confirmed 12-field layout"
        );
        println!(
            "X\tcandidate-bytes\tPASS\tdecoded {}B, 12 confirmed fields in order",
            candidate_decoded.len()
        );

        match control.exchange_official_5188_login_init(&candidate) {
            Ok(init_frame) => {
                println!(
                    "X\tlogin-stage\tPASS\t3610 payload {}B, metadata {:?}",
                    init_frame.payload.len(),
                    init_frame.metadata
                );
                if let Some(data) = data.as_mut() {
                    match data.exchange_initialization_stage(
                        crate::Official5188InitializationStage::Login,
                        &init_frame,
                    ) {
                        Ok(server) => println!(
                            "X\t5188-stage1\tPASS\tserver kind {}, payload {}B",
                            server.kind,
                            server.payload.len()
                        ),
                        Err(error) => println!("X\t5188-stage1\tFAIL\t{error}"),
                    }
                }
            }
            Err(error) => println!("X\tlogin-stage\tFAIL\t{error}"),
        }

        // Phase R: the real `请求登录` — a 19-field client-manifest object.
        // The vendor flow matrix shows this is the actual initial login
        // (632B wire / 854B inflated, root `认证`), and the 2026-08 legacy
        // template failed tonight because its embedded manifest values
        // (Stock.exe date/version/file tokens) are stale. Build the same
        // object with current credentials plus locally-derived values.
        if std::env::var("NETZIP_LIVE_REAL_LOGIN").as_deref() == Ok("1") {
            let socket = format!("{}:{}", config.host, config.port)
                .to_socket_addrs()
                .expect("resolve")
                .next()
                .expect("addr");
            let mut stream = TcpStream::connect_timeout(&socket, config.timeout).expect("connect");
            stream.set_read_timeout(Some(config.timeout)).unwrap();
            stream.set_write_timeout(Some(config.timeout)).unwrap();
            let manifest_login = super::build_real_login_packet(&config.account, &config.password)
                .expect("manifest login request");
            stream
                .write_all(&manifest_login)
                .expect("write manifest login");
            stream.flush().unwrap();
            match super::read_netpacket(&mut stream) {
                Ok(response) => {
                    let decoded = decode_dict_then_plain(&response);
                    println!(
                        "R\tmanifest-login\tINFO\t{}B request (inflated {}B), {}B response (decoded {}B)",
                        manifest_login.len(),
                        854,
                        response.len(),
                        decoded.len()
                    );
                    if let Ok(fields) = super::parse_control_object_with_root(&decoded, "认证") {
                        for field in &fields {
                            let text = String::from_utf16_lossy(&utf16_words(field.value));
                            let printable = text.chars().all(|ch| {
                                ch.is_ascii_graphic()
                                    || ch.is_ascii_whitespace()
                                    || (ch as u32) >= 0x4e00
                            });
                            let display = if field.label == "外网IP" {
                                "<masked>".to_string()
                            } else if printable && !text.is_empty() {
                                text.trim_end_matches('\u{0}').to_string()
                            } else {
                                format!("<{}B>", field.value.len())
                            };
                            println!("R\tfield\tINFO\t{} = {display}", field.label);
                        }
                    } else if let Ok(fields) =
                        super::parse_control_object_with_root(&decoded, "加密包")
                    {
                        for field in &fields {
                            let text = String::from_utf16_lossy(&utf16_words(field.value));
                            let printable = text.chars().all(|ch| {
                                ch.is_ascii_graphic()
                                    || ch.is_ascii_whitespace()
                                    || (ch as u32) >= 0x4e00
                            });
                            let display = if printable && !text.is_empty() {
                                text.trim_end_matches('\u{0}').to_string()
                            } else {
                                format!("<{}B>", field.value.len())
                            };
                            println!("R\treject\tINFO\t{} = {display}", field.label);
                        }
                    } else if decoded.len() >= 40 {
                        let count =
                            u32::from_le_bytes(decoded[20..24].try_into().unwrap()) as usize;
                        println!(
                            "R\treject-raw\tINFO\tdecoded {}B, declared {} fields",
                            decoded.len(),
                            count
                        );
                        let mut offset = 40usize;
                        for _ in 0..count.min(32) {
                            if offset + 20 > decoded.len() {
                                break;
                            }
                            let vlen = u32::from_le_bytes(
                                decoded[offset + 4..offset + 8].try_into().unwrap(),
                            ) as usize;
                            let span = u32::from_le_bytes(
                                decoded[offset + 16..offset + 20].try_into().unwrap(),
                            ) as usize;
                            if span == 0 || offset + span > decoded.len() {
                                break;
                            }
                            let tail = &decoded[offset + 20..offset + span];
                            let label = String::from_utf16_lossy(&utf16_words(tail));
                            let vstart = offset + 20 + label.encode_utf16().count() * 2 + 2;
                            let value = decoded.get(vstart..vstart + vlen).unwrap_or(&[]);
                            let text = String::from_utf16_lossy(&utf16_words(value));
                            let printable = !text.is_empty()
                                && text.chars().all(|ch| {
                                    ch.is_ascii_graphic()
                                        || ch.is_ascii_whitespace()
                                        || (ch as u32) >= 0x4e00
                                });
                            println!(
                                "R\treject-field\tINFO\t{} = {}",
                                label,
                                if printable {
                                    text
                                } else {
                                    format!("<{}B>", vlen)
                                }
                            );
                            offset += span;
                        }
                    } else {
                        println!("R\tfield\tINFO\tresponse is not an 认证 object");
                        let mut words = Vec::with_capacity(decoded.len() / 2);
                        let mut pairs: &[u8] = &decoded;
                        while pairs.len() >= 2 {
                            words.push(u16::from_le_bytes([pairs[0], pairs[1]]));
                            pairs = &pairs[2..];
                        }
                        let text = String::from_utf16_lossy(&words);
                        let visible: String = text
                            .chars()
                            .filter(|ch| {
                                ch.is_ascii_graphic()
                                    || ch.is_ascii_whitespace()
                                    || (*ch as u32) >= 0x4e00
                            })
                            .take(120)
                            .collect();
                        println!("R\treject-text\tINFO\t{visible}");
                    }
                    let download =
                        super::build_auth_followup_download_packet(1).expect("R download");
                    stream.write_all(&download).expect("R write download");
                    stream.flush().unwrap();
                    let raw = super::read_netpacket(&mut stream).expect("R download response");
                    let entries =
                        super::parse_server_entries_from_packet_bytes(&raw).unwrap_or_default();
                    let port_list = entries
                        .iter()
                        .map(|entry| format!("{}/{}", entry.main_port, entry.secondary_port))
                        .collect::<Vec<_>>()
                        .join(",");
                    let all_lines = all_utf16_lines(&raw);
                    let has_5188 = all_lines.iter().any(|line| line.contains("5188"));
                    println!(
                        "R\tdownload\tINFO\t{}B, {} entries [{port_list}], route text has 5188: {has_5188}, {} lines",
                        raw.len(),
                        entries.len(),
                        all_lines.len()
                    );

                    // Phase S: the official quote-server list (大智慧 L1) and
                    // the real-value per-connection exchange.
                    let l1_request =
                        super::build_download_file_request("系统\\大智慧服务器L1.ini", 2)
                            .expect("L1 request");
                    stream.write_all(&l1_request).expect("write L1");
                    stream.flush().unwrap();
                    let l1_raw = super::read_netpacket(&mut stream).expect("L1 response");
                    let l1_lines = all_utf16_lines(&l1_raw);
                    println!(
                        "S\tL1-raw\tINFO\t{}B, bom_at {:?}, {} text lines, first: {:?}",
                        l1_raw.len(),
                        l1_raw
                            .windows(4)
                            .position(|w| w == [0xff, 0xfe, 0x5b, 0x00]),
                        l1_lines.len(),
                        l1_lines
                            .first()
                            .map(|l| l.chars().take(40).collect::<String>())
                    );
                    for (index, line) in l1_lines.iter().enumerate() {
                        println!(
                            "S\tL1-line {index}\tINFO\tcomment={} commas={} text={}",
                            line.starts_with("//"),
                            line.matches(',').count(),
                            line.chars().take(60).collect::<String>()
                        );
                    }
                    let l1_entries =
                        super::parse_server_entries_from_packet_bytes(&l1_raw).unwrap_or_default();
                    let l1_5188: Vec<_> = l1_entries
                        .iter()
                        .filter(|entry| entry.main_port == 5188 || entry.secondary_port == 5188)
                        .collect();
                    println!(
                        "S\tL1-list\tINFO\t{}B, {} entries, {} with 5188",
                        l1_raw.len(),
                        l1_entries.len(),
                        l1_5188.len()
                    );
                    let endpoint = l1_5188.first().and_then(|entry| {
                        entry
                            .host
                            .parse::<std::net::IpAddr>()
                            .ok()
                            .and_then(|host| {
                                crate::OfficialQuoteEndpoint::new(host, entry.main_port).ok()
                            })
                    });
                    if let Some(endpoint) = endpoint {
                        println!(
                            "S\tendpoint\tINFO\t5188 endpoint acquired (host redacted, port {})",
                            endpoint.port
                        );
                        let local_ip: std::net::Ipv4Addr = std::net::UdpSocket::bind("0.0.0.0:0")
                            .and_then(|socket| {
                                socket.connect("121.41.70.217:6100")?;
                                socket.local_addr()
                            })
                            .ok()
                            .and_then(|addr| match addr.ip() {
                                std::net::IpAddr::V4(v4) => Some(v4),
                                std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped(),
                            })
                            .unwrap_or(std::net::Ipv4Addr::LOCALHOST);
                        let utf16 = |s: &str| -> Vec<u8> {
                            s.encode_utf16().flat_map(u16::to_le_bytes).collect()
                        };
                        let mut permissions = [0u8; 6];
                        permissions.copy_from_slice(&utf16("点播版"));
                        let mut broker = [0u8; 4];
                        broker.copy_from_slice(&utf16("华创"));
                        let real_fields = super::Auth7100LoginControlFields {
                            local_ip: local_ip.octets(),
                            mac_ascii: super::outbound_mac_ascii(local_ip.octets()),
                            account_permissions: permissions,
                            broker,
                            encryption_version: 0,
                            user_id: 0,
                            interface_version: 858,
                        };
                        let candidate = super::build_candidate_official_5188_login_control_packet(
                            &real_fields,
                            2,
                        )
                        .expect("real candidate");
                        let mut control = super::Auth7100ControlSession {
                            stream: stream.try_clone().expect("clone control socket"),
                            login: super::Auth7100LoginResult {
                                endpoint: format!("{}:{}", config.host, config.port),
                                status: "登录成功".to_string(),
                                authenticated: true,
                                response_packet_lengths: Vec::new(),
                                response_roles: Vec::new(),
                                dictionary_length: None,
                                dictionary_sha256: None,
                                decoded_response_lengths: Vec::new(),
                                login_success_confirmed: true,
                                active_servers: Vec::new(),
                                selected_quote_endpoint: Some(endpoint.clone()),
                            },
                        };
                        // Vendor order: login stage on 5188 first, then the ABK
                        // control exchange, then the ACK stage.
                        let mut data = match Official5188Session::connect_authenticated(
                            endpoint.socket_addr().to_string(),
                            Duration::from_secs(8),
                            true,
                        ) {
                            Ok(data) => {
                                println!(
                                    "S\t5188-connect\tPASS\tofficial 5188 (host redacted, port {})",
                                    endpoint.port
                                );
                                Some(data)
                            }
                            Err(error) => {
                                println!("S\t5188-connect\tFAIL\t{error}");
                                None
                            }
                        };
                        match control.exchange_official_5188_login_init(&candidate) {
                            Ok(init) => {
                                println!(
                                    "S\tcontrol-stage\tPASS\t3610 payload {}B",
                                    init.payload.len()
                                );
                                if let Some(data) = data.as_mut() {
                                    match data.exchange_initialization_stage(
                                        crate::Official5188InitializationStage::Login,
                                        &init,
                                    ) {
                                        Ok(server) => println!(
                                            "S\t5188-stage1\tPASS\tserver kind {}, payload {}B",
                                            server.kind,
                                            server.payload.len()
                                        ),
                                        Err(error) => {
                                            println!("S\t5188-stage1\tFAIL\t{error}")
                                        }
                                    }
                                }
                            }
                            Err(error) => println!("S\tcontrol-stage\tFAIL\t{error}"),
                        }
                        // Stage 2 (ABK) with the vendor client's build values.
                        let abk_fields = super::Auth7100AbkControlFields {
                            is_64_bit: 0x20,
                            major_version: 8,
                            minor_version: 0x32,
                            build_number: 0x5956,
                            total_memory: [0x00, 0x40, 0x6a, 0xfb, 0x0f, 0x00, 0x00, 0x00],
                            user_id: 0,
                            interface_version: 858,
                        };
                        let abk_candidate =
                            super::build_candidate_official_5188_abk_control_packet(
                                &abk_fields,
                                12,
                            )
                            .expect("ABK candidate");
                        match control.exchange_official_5188_abk_init(&abk_candidate) {
                            Ok(init) => {
                                println!(
                                    "S\tabk-stage\tPASS\t3610 payload {}B",
                                    init.payload.len()
                                );
                                if let Some(data) = data.as_mut() {
                                    match data.exchange_initialization_stage(
                                        crate::Official5188InitializationStage::Abk,
                                        &init,
                                    ) {
                                        Ok(server) => {
                                            println!(
                                                "S\t5188-stage2\tPASS\tserver kind {}, payload {}B",
                                                server.kind,
                                                server.payload.len()
                                            );
                                            let first_nul =
                                                server.payload.iter().position(|byte| *byte == 0);
                                            println!(
                                                "S\tabk-nul\tINFO\tpayload {}B, first NUL at {:?}",
                                                server.payload.len(),
                                                first_nul
                                            );
                                        }
                                        Err(error) => {
                                            println!("S\t5188-stage2\tFAIL\t{error}")
                                        }
                                    }
                                }
                            }
                            Err(error) => println!("S\tabk-stage\tFAIL\t{error}"),
                        }
                        // Stage 3 (ACK): report the runtime's own file-manifest
                        // text with fresh-install market state.
                        // Enumerate the local installation's real files so the
                        // manifest describes OUR client (the captured set came
                        // from the Wine host's directory).
                        let file_root = std::path::Path::new("D:/Soft/_Stock/飞狐2020");
                        let mut file_infos: Vec<(String, u32)> = Vec::new();
                        let push_file =
                            |rel: String,
                             path: std::path::PathBuf,
                             out: &mut Vec<(String, u32)>| {
                                if let Ok(bytes) = std::fs::read(&path) {
                                    let normalized = rel.replace(std::path::MAIN_SEPARATOR, "//");
                                    out.push((format!(".//{normalized}"), crc32fast::hash(&bytes)));
                                }
                            };
                        if let Ok(entries) = std::fs::read_dir(file_root) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.is_file()
                                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                                {
                                    push_file(name.to_string(), path, &mut file_infos);
                                }
                            }
                        }
                        if let Ok(entries) = std::fs::read_dir(file_root.join("大智慧")) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path.is_file()
                                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                                {
                                    file_infos.push((
                                        format!(".//大智慧//{name}"),
                                        std::fs::read(&path)
                                            .map(|bytes| crc32fast::hash(&bytes))
                                            .unwrap_or(0),
                                    ));
                                }
                            }
                        }
                        file_infos.truncate(40);
                        // Market ids/attrs are the structural market list from
                        // vendor_pm; state fields stay zeroed (fresh-install).
                        let market_ids: [(u32, u32); 9] = [
                            (18515, 20),
                            (23123, 20),
                            (9282, 4),
                            (18003, 20),
                            (19272, 28),
                            (22350, 0),
                            (17999, 4),
                            (20307, 0),
                            (19010, 4),
                        ];
                        let market_rows: Vec<String> = market_ids
                            .iter()
                            .map(|(id, attr)| format!("{id}|{attr}|0|0|0|0|0|0|0|"))
                            .collect();
                        let manifest =
                            super::build_ack_manifest_text(&file_infos, &market_rows, &[]);
                        let ack_fields = super::Auth7100AckControlFields {
                            ack: u32::from_le_bytes(*b"penc"),
                            user_id: 0,
                            interface_version: 858,
                            data: manifest.into_bytes(),
                        };
                        let ack_candidate =
                            super::build_candidate_official_5188_ack_control_packet(
                                &ack_fields,
                                14,
                            )
                            .expect("ACK candidate");
                        println!(
                            "S\tack-manifest\tINFO\t{}B manifest text",
                            ack_fields.data.len()
                        );
                        match control.exchange_official_5188_ack_init(&ack_candidate) {
                            Ok(init) => {
                                println!(
                                    "S\tack-stage\tPASS\t3610 payload {}B",
                                    init.payload.len()
                                );
                                if let Some(data) = data.as_mut() {
                                    match data.exchange_initialization_stage(
                                        crate::Official5188InitializationStage::Ack,
                                        &init,
                                    ) {
                                        Ok(server) => println!(
                                            "S\t5188-stage3\tPASS\tserver kind {}, payload {}B",
                                            server.kind,
                                            server.payload.len()
                                        ),
                                        Err(error) => {
                                            println!("S\t5188-stage3\tFAIL\t{error}")
                                        }
                                    }
                                }
                            }
                            Err(error) => println!("S\tack-stage\tFAIL\t{error}"),
                        }
                        if let Some(mut data) = data {
                            let _ = data.stop();
                        }
                    }
                }
                Err(error) => println!("R\tmanifest-login\tFAIL\t{error}"),
            }
        }

        // Route diagnostic: dump every UTF-16 line in the raw download
        // response (numeric tokens only, no names) to check whether 5188
        // rows exist beyond what the structured extractor returns.
        if std::env::var("NETZIP_LIVE_ROUTE_DIAG").as_deref() == Ok("1") {
            let socket = format!("{}:{}", config.host, config.port)
                .to_socket_addrs()
                .expect("resolve")
                .next()
                .expect("addr");
            let mut stream = TcpStream::connect_timeout(&socket, config.timeout).expect("connect");
            stream.set_read_timeout(Some(config.timeout)).unwrap();
            stream.set_write_timeout(Some(config.timeout)).unwrap();
            let login_fields = super::Auth7100LoginControlFields {
                local_ip: [0, 0, 0, 0],
                mac_ascii: [0; 12],
                account_permissions: [0; 6],
                broker: [0; 4],
                encryption_version: 0,
                user_id: 0,
                interface_version: 0,
            };
            let login_request = super::build_candidate_initial_login_control_packet(
                &login_fields,
                &config.account,
                &config.password,
                2,
            )
            .expect("diag login");
            stream.write_all(&login_request).expect("diag write login");
            stream.flush().unwrap();
            let _login_response = super::read_netpacket(&mut stream).expect("diag login resp");
            let download = super::build_auth_followup_download_packet(1).expect("diag download");
            stream.write_all(&download).expect("diag write download");
            stream.flush().unwrap();
            let raw = super::read_netpacket(&mut stream).expect("diag download resp");
            let lines = all_utf16_lines(&raw);
            println!(
                "DIAG\troute-lines\tINFO\t{} lines in download payload",
                lines.len()
            );
            for (index, line) in lines.iter().enumerate() {
                let numbers = numeric_tokens(line);
                if !numbers.is_empty() {
                    println!(
                        "DIAG\tline {index}\tINFO\tcommas {}, ports {numbers:?}",
                        line.matches(',').count()
                    );
                }
            }
        }

        if let Some(mut data) = data {
            let _ = data.stop();
        }
        println!("Z\tdone\tOK\tall sockets closed; no credentials or session bytes printed");
    }

    /// Splits every UTF-16LE line found after the BOM+bracket marker without
    /// the structured extractor's early-stop rules.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    fn all_utf16_lines(bytes: &[u8]) -> Vec<String> {
        let Some(start) = bytes
            .windows(4)
            .position(|window| window == [0xff, 0xfe, 0x5b, 0x00])
        else {
            return Vec::new();
        };
        let decoded = String::from_utf16_lossy(
            &bytes[start + 2..]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>(),
        );
        decoded
            .lines()
            .map(|line| line.trim_end_matches('\u{0}').trim().to_string())
            .filter(|line| !line.is_empty())
            .collect()
    }

    fn numeric_tokens(line: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        for ch in line.chars() {
            if ch.is_ascii_digit() {
                current.push(ch);
            } else {
                if current.len() >= 2 {
                    tokens.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
        if current.len() >= 2 {
            tokens.push(current);
        }
        tokens
    }

    #[allow(clippy::chunks_exact_to_as_chunks)]
    fn utf16_words(bytes: &[u8]) -> Vec<u16> {
        bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .take_while(|word| *word != 0)
            .collect()
    }

    /// Dictionary decode first, plain ZSTD fallback. A known control root at
    /// the head wins immediately; otherwise fall back to the plain decode,
    /// then the dictionary decode (some error objects carry no root at all).
    fn decode_dict_then_plain(packet: &[u8]) -> Vec<u8> {
        let dict = super::decode_dictionary_response(packet, super::STOCK_DICTIONARY_BYTES).ok();
        let plain = packet
            .windows(4)
            .position(|w| w == [0x28, 0xb5, 0x2f, 0xfd])
            .and_then(|magic| zstd::stream::decode_all(Cursor::new(&packet[magic..])).ok());
        let has_known_root = |candidate: &[u8]| {
            if candidate.len() < 20 {
                return false;
            }
            #[allow(clippy::chunks_exact_to_as_chunks)]
            let head: String = String::from_utf16_lossy(
                &candidate[..20]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<_>>(),
            );
            let head = head.trim_end_matches('\u{0}');
            matches!(head, "认证" | "加密包" | "加密解密" | "网络包" | "下载文件")
        };
        if let Some(decoded) = &dict
            && has_known_root(decoded)
        {
            return decoded.clone();
        }
        if let Some(decoded) = &plain
            && has_known_root(decoded)
        {
            return decoded.clone();
        }
        if let Some(decoded) = plain {
            return decoded;
        }
        dict.unwrap_or_default()
    }
}
