use std::io::Read;
use std::{error::Error, fmt};

use crate::{Tdx0547Body, Tdx7709ServerFrame, parse_tdx_0547_body};
use flate2::read::ZlibDecoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tdx0547DeliveryKind {
    SolicitedBatch,
    UnsolicitedUpdate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tdx0547Delivery {
    pub kind: Tdx0547DeliveryKind,
    pub body: Tdx0547Body,
}

#[derive(Clone, Debug, Default)]
pub struct Tdx0547DeliveryDecoder {
    pending: Vec<u8>,
}

impl Tdx0547DeliveryDecoder {
    pub fn push(
        &mut self,
        bytes: &[u8],
    ) -> Result<Vec<Tdx0547Delivery>, Tdx0547DeliveryDecodeError> {
        self.pending.extend_from_slice(bytes);
        let mut consumed = 0usize;
        let mut deliveries = Vec::new();

        while self.pending.len().saturating_sub(consumed) >= 16 {
            let frame_bytes = &self.pending[consumed..];
            if frame_bytes[..4] != [0xb1, 0xcb, 0x74, 0x00] {
                let magic = frame_bytes[..4].try_into().expect("four-byte frame magic");
                self.pending.clear();
                return Err(Tdx0547DeliveryDecodeError::InvalidMagic(magic));
            }
            let body_len = usize::from(u16::from_le_bytes([frame_bytes[12], frame_bytes[13]]));
            let frame_len = 16 + body_len;
            if frame_bytes.len() < frame_len {
                break;
            }
            let frame = Tdx7709ServerFrame {
                op: u16::from_le_bytes([frame_bytes[4], frame_bytes[5]]),
                sub: u16::from_le_bytes([frame_bytes[6], frame_bytes[7]]),
                flags: u16::from_le_bytes([frame_bytes[8], frame_bytes[9]]),
                tag: u16::from_le_bytes([frame_bytes[10], frame_bytes[11]]),
                body: frame_bytes[16..frame_len].to_vec(),
                original_len: u16::from_le_bytes([frame_bytes[14], frame_bytes[15]]),
            };
            if let Some(delivery) = decode_tdx0547_delivery(&frame) {
                deliveries.push(delivery);
            }
            consumed += frame_len;
        }

        if consumed != 0 {
            self.pending.drain(..consumed);
        }
        Ok(deliveries)
    }

    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tdx0547DeliveryDecodeError {
    InvalidMagic([u8; 4]),
}

impl fmt::Display for Tdx0547DeliveryDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic(magic) => write!(formatter, "invalid server16 magic: {magic:02x?}"),
        }
    }
}

impl Error for Tdx0547DeliveryDecodeError {}

pub fn decode_tdx0547_delivery(frame: &Tdx7709ServerFrame) -> Option<Tdx0547Delivery> {
    if frame.tag != 0x0547 {
        return None;
    }

    let (kind, decoded) = if frame.op == 0
        && frame.sub == 0x2900
        && usize::from(frame.original_len) == frame.body.len()
    {
        (Tdx0547DeliveryKind::UnsolicitedUpdate, frame.body.clone())
    } else if frame.op != 0 {
        let mut decoder = ZlibDecoder::new(frame.body.as_slice());
        let mut decoded = Vec::with_capacity(usize::from(frame.original_len));
        decoder.read_to_end(&mut decoded).ok()?;
        if decoded.len() != usize::from(frame.original_len) {
            return None;
        }
        (Tdx0547DeliveryKind::SolicitedBatch, decoded)
    } else {
        return None;
    };

    let body = parse_tdx_0547_body(&decoded);
    if body.records.is_empty() {
        return None;
    }
    Some(Tdx0547Delivery { kind, body })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::{Compression, write::ZlibEncoder};

    use super::{Tdx0547DeliveryDecoder, Tdx0547DeliveryKind, decode_tdx0547_delivery};
    use crate::Tdx7709ServerFrame;

    #[test]
    fn decodes_vendor_unsolicited_single_record_frame_without_zlib() {
        let decoded = [0x01, 0x00, 0x01, b'6', b'0', b'3', b'1', b'9', b'2', 0, 0];
        let frame = Tdx7709ServerFrame {
            op: 0,
            sub: 0x2900,
            flags: 0,
            tag: 0x0547,
            body: decoded.iter().map(|byte| byte ^ 0x93).collect(),
            original_len: decoded.len() as u16,
        };

        let delivery = decode_tdx0547_delivery(&frame).expect("unsolicited quote delivery");
        assert_eq!(delivery.kind, Tdx0547DeliveryKind::UnsolicitedUpdate);
        assert_eq!(delivery.body.records.len(), 1);
        assert_eq!(delivery.body.records[0].market, 1);
        assert_eq!(delivery.body.records[0].code, "603192");
    }

    #[test]
    fn decodes_solicited_zlib_batch_without_misclassifying_raw_replies() {
        let decoded = [0x01, 0x00, 0x00, b'0', b'0', b'0', b'0', b'0', b'1', 0, 0];
        let encoded = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&encoded).expect("compress quote body");
        let compressed = encoder.finish().expect("finish quote body");

        let delivery = decode_tdx0547_delivery(&Tdx7709ServerFrame {
            op: 0x401c,
            sub: 0x2a48,
            flags: 0,
            tag: 0x0547,
            body: compressed,
            original_len: encoded.len() as u16,
        })
        .expect("solicited quote delivery");
        assert_eq!(delivery.kind, Tdx0547DeliveryKind::SolicitedBatch);
        assert_eq!(delivery.body.records[0].code, "000001");

        assert!(
            decode_tdx0547_delivery(&Tdx7709ServerFrame {
                op: 0x400c,
                sub: 0x2a48,
                flags: 0,
                tag: 0x0547,
                body: encoded,
                original_len: decoded.len() as u16,
            })
            .is_none()
        );
    }

    #[test]
    fn incremental_decoder_waits_for_a_complete_unsolicited_frame() {
        let decoded = [0x01, 0x00, 0x01, b'6', b'0', b'3', b'1', b'9', b'2', 0, 0];
        let body = decoded.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>();
        let mut frame = Vec::new();
        frame.extend_from_slice(&[0xb1, 0xcb, 0x74, 0x00]);
        frame.extend_from_slice(&0u16.to_le_bytes());
        frame.extend_from_slice(&0x2900u16.to_le_bytes());
        frame.extend_from_slice(&0u16.to_le_bytes());
        frame.extend_from_slice(&0x0547u16.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u16).to_le_bytes());
        frame.extend_from_slice(&(body.len() as u16).to_le_bytes());
        frame.extend_from_slice(&body);

        let mut decoder = Tdx0547DeliveryDecoder::default();
        assert!(
            decoder
                .push(&frame[..7])
                .expect("first fragment")
                .is_empty()
        );
        assert!(
            decoder
                .push(&frame[7..18])
                .expect("second fragment")
                .is_empty()
        );
        let deliveries = decoder.push(&frame[18..]).expect("final fragment");
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].kind, Tdx0547DeliveryKind::UnsolicitedUpdate);
        assert_eq!(deliveries[0].body.records[0].code, "603192");
        assert_eq!(decoder.pending_bytes(), 0);
    }
}
