//! Offline forensics for a captured 7100 byte stream (classic pcap).
//!
//! Scans both directions for ZSTD frames, decodes each with the vendored
//! stock dictionary, and prints ONLY filename/keyword matches (day/ini/日线…).
//! All other decoded content stays unprinted. No sockets are opened.

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: probe_7100_files CAPTURE");
    let bytes = std::fs::read(path).expect("read capture");
    for (label, buf) in [
        ("UP", reassemble(&bytes, true)),
        ("DOWN", reassemble(&bytes, false)),
    ] {
        println!("== {label} stream {} bytes", buf.len());
        let mut hits = 0;
        let mut from = 0usize;
        while let Some(rel) = find(&buf[from..], &[0x28, 0xb5, 0x2f, 0xfd]) {
            let start = from + rel;
            from = start + 1;
            let Ok(decoded) =
                netzip_fullpull::auth_7100::decode_7100_zstd_frame_with_dictionary(&buf[start..])
            else {
                continue;
            };
            from = start + 4;
            for key in [
                b"DAY".as_slice(),
                b".ini".as_slice(),
                b"day".as_slice(),
                b"hqd".as_slice(),
                b"lc1".as_slice(),
                "日线".as_bytes(),
                "下载".as_bytes(),
                "文件".as_bytes(),
                "补".as_bytes(),
            ] {
                if let Some(pos) = find(&decoded, key) {
                    let lo = pos.saturating_sub(20);
                    let hi = (pos + 48).min(decoded.len());
                    println!(
                        "  @{start} decoded={} key={} ctx={}",
                        decoded.len(),
                        String::from_utf8_lossy(key),
                        String::from_utf8_lossy(&decoded[lo..hi]).replace('\0', " ")
                    );
                    hits += 1;
                    break;
                }
            }
        }
        println!("  hits: {hits}");
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Reassembles one direction of a classic pcap into a single TCP stream.
fn reassemble(bytes: &[u8], uplink: bool) -> Vec<u8> {
    let mut segments: Vec<(u32, Vec<u8>)> = Vec::new();
    if bytes.len() < 24 {
        return Vec::new();
    }
    let mut off = 24usize;
    while off + 16 <= bytes.len() {
        let _ts = u32::from_le_bytes(bytes[off..off + 4].try_into().expect("ts"));
        let _tus = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().expect("tus"));
        let incl = u32::from_le_bytes(bytes[off + 8..off + 12].try_into().expect("incl"));
        off += 16;
        let Some(end) = off.checked_add(incl as usize) else {
            break;
        };
        if end > bytes.len() {
            break;
        }
        let raw = &bytes[off..end];
        off = end;
        if raw.len() < 34 || (raw[0] >> 4) != 4 || raw[9] != 6 {
            continue;
        }
        let ihl = usize::from((raw[0] & 0x0f) * 4);
        if raw.len() < ihl + 20 {
            continue;
        }
        let tcp = &raw[ihl..];
        let sport = u16::from_be_bytes([tcp[0], tcp[1]]);
        let dport = u16::from_be_bytes([tcp[2], tcp[3]]);
        let seq = u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]);
        let data_offset = usize::from((tcp[12] >> 4) * 4);
        let payload = &tcp[data_offset.min(tcp.len())..];
        if payload.is_empty() {
            continue;
        }
        let matches = if uplink { dport == 7100 } else { sport == 7100 };
        if matches {
            segments.push((seq, payload.to_vec()));
        }
    }
    segments.sort_by_key(|(seq, _)| *seq);
    let mut stream = Vec::new();
    for (_seq, payload) in segments {
        stream.extend_from_slice(&payload);
    }
    stream
}
