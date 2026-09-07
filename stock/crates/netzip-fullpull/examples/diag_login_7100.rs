//! Forensic diagnostic for the 7100 login-response object layout.
//!
//! Replays the first login exchange (19-field 请求登录) on a raw socket and
//! prints STRUCTURAL information only: packet lengths, the decoded object's
//! root label, field count, the four header length words, and each field's
//! label plus byte spans. Field values are never printed, so no credentials
//! or session tokens leak.

use std::io::{Read, Write};
use std::net::TcpStream;

use netzip_fullpull::auth_7100::{build_real_login_packet, decode_stock_dictionary_netpacket};

fn read_netpacket_diag(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut header = vec![0u8; 74];
    stream
        .read_exact(&mut header)
        .map_err(|error| format!("read header: {error}"))?;
    let packet_len = u32::from_le_bytes(header[32..36].try_into().unwrap()) as usize;
    println!("D\theader_len_word\t{packet_len}");
    if !(74..=1 << 20).contains(&packet_len) {
        return Err(format!("invalid packet length {packet_len}"));
    }
    let mut packet = header;
    packet.resize(packet_len, 0);
    stream
        .read_exact(&mut packet[74..])
        .map_err(|error| format!("read body: {error}"))?;
    Ok(packet)
}

fn utf16_prefix(bytes: &[u8]) -> String {
    let mut text = String::new();
    for chunk in bytes.as_chunks::<2>().0 {
        let word = u16::from_le_bytes([chunk[0], chunk[1]]);
        if word == 0 {
            break;
        }
        text.push(char::from_u32(u32::from(word)).unwrap_or_default());
    }
    text
}

fn main() {
    let account = std::env::var("NETZIP_TEST_ACCOUNT").expect("NETZIP_TEST_ACCOUNT");
    let password = std::env::var("NETZIP_TEST_PASSWORD").expect("NETZIP_TEST_PASSWORD");
    let host = std::env::var("DIAG_AUTH_HOST").unwrap_or_else(|_| "121.41.70.217".into());
    let port: u16 = std::env::var("DIAG_AUTH_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(7100);

    let request = build_real_login_packet(&account, &password).expect("login request");
    println!("D\trequest_len\t{}", request.len());

    let mut stream = TcpStream::connect((host.as_str(), port)).expect("connect auth server");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(8)))
        .unwrap();
    stream.write_all(&request).expect("write login");
    stream.flush().expect("flush login");

    let response = read_netpacket_diag(&mut stream).expect("read response");
    println!("D\tresponse_len\t{}", response.len());

    let decoded = match decode_stock_dictionary_netpacket(&response) {
        Ok(decoded) => decoded,
        Err(_) => {
            let magic = response
                .windows(4)
                .position(|window| window == [0x28, 0xb5, 0x2f, 0xfd]);
            println!("D\tzstd_magic_offset\t{:?}", magic);
            println!("F\tdictionary_decode_failed");
            return;
        }
    };
    println!("D\tdecoded_len\t{}", decoded.len());
    if decoded.len() < 40 {
        println!("F\tdecoded_too_short");
        return;
    }
    println!("D\troot\t{}", utf16_prefix(&decoded[..20]));
    println!(
        "D\tfield_count\t{}",
        u32::from_le_bytes(decoded[20..24].try_into().unwrap())
    );
    println!("D\twords_24_32\t{:02x?}", &decoded[24..32]);
    println!(
        "D\tlen_word_32\t{}",
        u32::from_le_bytes(decoded[32..36].try_into().unwrap())
    );
    println!(
        "D\tlen_word_36\t{}",
        u32::from_le_bytes(decoded[36..40].try_into().unwrap())
    );

    let field_count = u32::from_le_bytes(decoded[20..24].try_into().unwrap()) as usize;
    let mut offset = 40usize;
    for index in 0..field_count {
        let Some(header) = decoded.get(offset..offset + 20) else {
            println!("F\tfield_truncated\t{index}\t{offset}");
            return;
        };
        let value_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let span_len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
        if span_len < 20 || offset + span_len > decoded.len() {
            println!(
                "F\tfield_span_out_of_bounds\t{index}\toffset={offset}\tspan_len={span_len}\tdecoded_len={}",
                decoded.len()
            );
            return;
        }
        let tail = &decoded[offset + 20..offset + span_len];
        let label = utf16_prefix(tail);
        println!(
            "I\tfield\t{index}\tlabel={label}\ttype_id={}\tvalue_len={value_len}\tspan_len={span_len}\tspan_end={}",
            u32::from_le_bytes(header[0..4].try_into().unwrap()),
            offset + span_len
        );
        if label == "错误" || label == "提示信息" {
            let value_start = offset + 20 + label.chars().count() * 2 + 2;
            if value_start + value_len > decoded.len() {
                println!(
                    "F\ttext_value_out_of_bounds\t{label}\tvalue_start={value_start}\tvalue_len={value_len}"
                );
                return;
            }
            let value = &decoded[value_start..value_start + value_len];
            let mut text = String::new();
            for chunk in value.as_chunks::<2>().0 {
                let word = u16::from_le_bytes([chunk[0], chunk[1]]);
                if word == 0 {
                    break;
                }
                text.push(char::from_u32(u32::from(word)).unwrap_or('?'));
            }
            let safe = text.replace(&account, "<acct>").replace(&password, "<pw>");
            println!("I\ttext\t{label}\t{safe}");
        }
        if label == "登录方式" || label == "应答编号" {
            let value_start = offset + 20 + label.chars().count() * 2 + 2;
            if value_len == 4 && value_start + 4 <= decoded.len() {
                println!(
                    "I\tu32\t{label}\t{}",
                    u32::from_le_bytes(decoded[value_start..value_start + 4].try_into().unwrap())
                );
            }
        }
        offset += span_len;
    }
    println!("D\tfinal_offset\t{offset}");
    let _ = std::io::Write::flush(&mut std::io::stdout());
}
