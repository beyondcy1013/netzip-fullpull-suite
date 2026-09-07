#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TdxPacket {
    command: u16,
    category: u16,
    market: u16,
    code: [u8; 6],
    start: u16,
    count: u16,
}

impl TdxPacket {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_command(&mut self, command: u16) {
        self.command = command;
    }

    pub fn command(&self) -> u16 {
        self.command
    }

    pub fn set_params(&mut self, category: u16, market: u16, code: &str, start: u16, count: u16) {
        self.category = category;
        self.market = market;
        self.code = padded_code(code);
        self.start = start;
        self.count = count;
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self.command {
            0x10c => Self::create_security_bars_packet(
                self.category,
                self.market,
                self.code,
                self.start,
                self.count,
            ),
            0x000c => Self::create_security_count_packet(self.market),
            _ => Vec::new(),
        }
    }

    pub fn create_security_bars_packet(
        category: u16,
        market: u16,
        code: impl AsRef<[u8]>,
        start: u16,
        count: u16,
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(30);
        push_u16(&mut packet, 0x10c);
        push_u32(&mut packet, 0x0101_6408);
        push_u16(&mut packet, 0x001c);
        push_u16(&mut packet, 0x001c);
        push_u16(&mut packet, 0x052d);
        push_u16(&mut packet, market);
        packet.extend_from_slice(&padded_bytes::<6>(code.as_ref()));
        push_u16(&mut packet, category);
        push_u16(&mut packet, 1);
        push_u16(&mut packet, start);
        push_u16(&mut packet, count);
        push_u32(&mut packet, 0);
        push_u32(&mut packet, 0);
        push_u16(&mut packet, 0);
        packet
    }

    pub fn create_security_quotes_packet(stocks: &[(u8, String)]) -> Vec<u8> {
        let data_len = (stocks.len() * 7 + 12) as u16;
        let mut packet = Vec::with_capacity(14 + stocks.len() * 7);
        push_u16(&mut packet, 0x10c);
        push_u32(&mut packet, 0x0200_6320);
        push_u16(&mut packet, data_len);
        push_u16(&mut packet, data_len);
        push_u32(&mut packet, 0x0005_053e);
        push_u32(&mut packet, 0);
        push_u16(&mut packet, 0);
        push_u16(&mut packet, stocks.len() as u16);
        for (market, code) in stocks {
            packet.push(*market);
            packet.extend_from_slice(&padded_code(code));
        }
        packet
    }

    pub fn create_security_list_packet(market: u16, start: u16) -> Vec<u8> {
        let mut packet = hex_bytes("0c0118640101060006005004");
        push_u16(&mut packet, market);
        push_u16(&mut packet, start);
        packet
    }

    pub fn create_minute_time_data_packet(market: u16, code: &str) -> Vec<u8> {
        let mut packet = hex_bytes("0c1b080001010e000e001d05");
        push_u16(&mut packet, market);
        packet.extend_from_slice(&padded_code(code));
        push_u32(&mut packet, 0);
        packet
    }

    pub fn create_index_bars_packet(
        category: u16,
        market: u16,
        code: &str,
        start: u16,
        count: u16,
    ) -> Vec<u8> {
        Self::create_security_bars_packet(category, market, padded_code(code), start, count)
    }

    pub fn create_handshake_packet_1() -> Vec<u8> {
        hex_bytes("0c0218930001030003000d0001")
    }

    pub fn create_handshake_packet_2() -> Vec<u8> {
        hex_bytes("0c0218940001030003000d0002")
    }

    pub fn create_handshake_packet_3() -> Vec<u8> {
        hex_bytes(
            "0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002",
        )
    }

    pub fn create_company_info_category_packet(market: u16, code: &str) -> Vec<u8> {
        let mut packet = hex_bytes("0c0f109b00010e000e00cf02");
        push_u16(&mut packet, market);
        packet.extend_from_slice(&padded_code(code));
        push_u32(&mut packet, 0);
        packet
    }

    pub fn create_company_info_content_packet(
        market: u16,
        code: &str,
        filename: &str,
        start: u32,
        length: u32,
    ) -> Vec<u8> {
        let mut packet = hex_bytes("0c07109c000168006800d002");
        push_u16(&mut packet, market);
        packet.extend_from_slice(&padded_code(code));
        push_u16(&mut packet, 0);
        packet.extend_from_slice(&padded_bytes::<80>(filename.as_bytes()));
        push_u32(&mut packet, start);
        push_u32(&mut packet, length);
        push_u32(&mut packet, 0);
        packet
    }

    pub fn create_block_info_meta_packet(block_file: &str) -> Vec<u8> {
        let mut packet = hex_bytes("0c39186900012a002a00c502");
        packet.extend_from_slice(&padded_bytes::<40>(block_file.as_bytes()));
        packet
    }

    pub fn create_block_info_packet(block_file: &str, start: u32, size: u32) -> Vec<u8> {
        let mut packet = hex_bytes("0c37186a00016e006e00b906");
        push_u32(&mut packet, start);
        push_u32(&mut packet, size);
        packet.extend_from_slice(&padded_bytes::<100>(block_file.as_bytes()));
        packet
    }

    pub fn create_finance_info_packet(market: u8, code: &str) -> Vec<u8> {
        let mut packet = hex_bytes("0c1f187600010b000b0010000100");
        packet.push(market);
        packet.extend_from_slice(&padded_code(code));
        packet
    }

    pub fn create_security_count_packet(market: u16) -> Vec<u8> {
        let mut packet = hex_bytes("0c0c186c0001080008004e04");
        push_u16(&mut packet, market);
        packet.extend_from_slice(&hex_bytes("75c73301"));
        packet
    }
}

fn padded_code(code: &str) -> [u8; 6] {
    padded_bytes::<6>(code.as_bytes())
}

fn padded_bytes<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut output = [0_u8; N];
    let len = bytes.len().min(N);
    output[..len].copy_from_slice(&bytes[..len]);
    output
}

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn hex_bytes(input: &str) -> Vec<u8> {
    let clean = input
        .bytes()
        .filter(|value| !value.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let mut output = Vec::with_capacity(clean.len() / 2);
    for pair in clean.as_chunks::<2>().0 {
        output.push((hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]));
    }
    output
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => 10 + byte - b'a',
        b'A'..=b'F' => 10 + byte - b'A',
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::TdxPacket;

    fn hex_string(bytes: &[u8]) -> String {
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push_str(&format!("{byte:02x}"));
        }
        output
    }

    #[test]
    fn handshake_packets_match_cpp_reference() {
        assert_eq!(
            hex_string(&TdxPacket::create_handshake_packet_1()),
            "0c0218930001030003000d0001"
        );
        assert_eq!(
            hex_string(&TdxPacket::create_handshake_packet_2()),
            "0c0218940001030003000d0002"
        );
    }

    #[test]
    fn quote_packet_contains_market_and_code_pairs() {
        let packet = TdxPacket::create_security_quotes_packet(&[
            (0, "000001".to_string()),
            (1, "600000".to_string()),
        ]);
        assert_eq!(&packet[22..29], &[0, b'0', b'0', b'0', b'0', b'0', b'1']);
        assert_eq!(&packet[29..36], &[1, b'6', b'0', b'0', b'0', b'0', b'0']);
    }
}
