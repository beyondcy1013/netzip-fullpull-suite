use std::{error::Error, time::Duration};

use crate::{
    tdx_0547::normalize_quote_head_by_decimal_point,
    tdx7709::{Tdx7709Config, Tdx7709QuoteRequestItem, Tdx7709Session},
};

pub type NativeDriverConfig = Tdx7709Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NativeMarket {
    Shenzhen = 0,
    Shanghai = 1,
    Beijing = 2,
}

impl NativeMarket {
    #[must_use]
    pub const fn flag(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for NativeMarket {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Shenzhen),
            1 => Ok(Self::Shanghai),
            2 => Ok(Self::Beijing),
            _ => Err("unsupported native market flag"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeInstrument {
    pub market: NativeMarket,
    pub code: String,
    pub decimal_point: u8,
    pub renewal_token: u32,
}

impl NativeInstrument {
    #[must_use]
    pub fn equity(market: NativeMarket, code: impl Into<String>) -> Self {
        Self {
            market,
            code: code.into(),
            decimal_point: 2,
            renewal_token: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeQuote {
    pub market: NativeMarket,
    pub code: String,
    pub source_time_hhmmss: Option<u32>,
    pub renewal_token: Option<u32>,
    pub last_price: f64,
    pub previous_close: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub volume: Option<f64>,
    pub turnover: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverProbe {
    pub endpoint: String,
    pub bootstrap_frames: usize,
}

pub struct NativeSession {
    inner: Tdx7709Session,
}

impl NativeSession {
    /// Opens a quote-only 7709 session through the authoritative native implementation.
    ///
    /// # Errors
    ///
    /// Returns connection, bootstrap, framing, or configuration errors from the native session.
    pub fn connect(config: NativeDriverConfig) -> Result<(Self, DriverProbe), Box<dyn Error>> {
        let endpoint = format!("{}:{}", config.host, config.port);
        let inner = Tdx7709Session::open_quote_only(&config)?;
        let bootstrap_frames = inner.sync_result().frames.len();
        Ok((
            Self { inner },
            DriverProbe {
                endpoint,
                bootstrap_frames,
            },
        ))
    }

    /// Fetches normalized quotes while retaining the complete shared 0547 parser underneath.
    ///
    /// # Errors
    ///
    /// Returns validation, transport, decompression, or response framing errors.
    pub fn fetch_quotes(
        &mut self,
        instruments: &[NativeInstrument],
    ) -> Result<Vec<NativeQuote>, Box<dyn Error>> {
        let requests = instruments
            .iter()
            .map(|instrument| Tdx7709QuoteRequestItem {
                market: instrument.market.flag(),
                code: instrument.code.clone(),
                token: instrument.renewal_token,
            })
            .collect::<Vec<_>>();
        let result = self.inner.request_live_quotes(&requests)?;
        let mut quotes = Vec::new();
        for record in result.quote_bodies.iter().flat_map(|body| &body.records) {
            let Some(instrument) = instruments.iter().find(|instrument| {
                instrument.market.flag() == record.market && instrument.code == record.code
            }) else {
                continue;
            };
            let Some(head) = record.quote_head.as_ref().and_then(|head| {
                normalize_quote_head_by_decimal_point(head, instrument.decimal_point)
            }) else {
                continue;
            };
            quotes.push(NativeQuote {
                market: instrument.market,
                code: instrument.code.clone(),
                source_time_hhmmss: record.time_hhmmss_raw,
                renewal_token: record.renewal_token_raw,
                last_price: head.price,
                previous_close: head.last_close,
                open: head.open,
                high: head.high,
                low: head.low,
                volume: record.volume,
                turnover: record.amount,
            });
        }
        if quotes.is_empty() {
            return Err("native response contains no normalized quotes".into());
        }
        Ok(quotes)
    }

    /// Sends one renewal request for an existing quote subscription.
    ///
    /// # Errors
    ///
    /// Returns validation or socket write errors from the shared native session.
    pub fn renew(&mut self, instruments: &[NativeInstrument]) -> Result<(), Box<dyn Error>> {
        let requests = instruments
            .iter()
            .map(|instrument| Tdx7709QuoteRequestItem {
                market: instrument.market.flag(),
                code: instrument.code.clone(),
                token: instrument.renewal_token,
            })
            .collect::<Vec<_>>();
        self.inner.send_live_quote_renewal(&requests)
    }

    /// Collects decoded solicited and unsolicited 0547 deliveries for a bounded duration.
    ///
    /// # Errors
    ///
    /// Returns socket, framing, or partial-delivery errors from the shared native session.
    pub fn collect_deliveries(
        &mut self,
        duration: Duration,
    ) -> Result<Vec<crate::tdx_0547_delivery::Tdx0547Delivery>, Box<dyn Error>> {
        self.inner.collect_quote_deliveries(duration)
    }
}

/// Probes the shared native endpoint without retaining the session.
///
/// # Errors
///
/// Returns connection or bootstrap errors from [`NativeSession::connect`].
pub fn probe(config: NativeDriverConfig) -> Result<DriverProbe, Box<dyn Error>> {
    NativeSession::connect(config).map(|(_, probe)| probe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn market_flags_match_the_extracted_protocol_contract() {
        assert_eq!(NativeMarket::Shenzhen.flag(), 0);
        assert_eq!(NativeMarket::Shanghai.flag(), 1);
        assert_eq!(NativeMarket::Beijing.flag(), 2);
        assert!(NativeMarket::try_from(3).is_err());
    }

    #[test]
    fn equity_defaults_to_two_decimal_places_and_zero_token() {
        let instrument = NativeInstrument::equity(NativeMarket::Shanghai, "600000");
        assert_eq!(instrument.code, "600000");
        assert_eq!(instrument.decimal_point, 2);
        assert_eq!(instrument.renewal_token, 0);
    }
}
