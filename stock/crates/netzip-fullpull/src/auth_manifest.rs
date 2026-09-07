//! Protocol-owned ACK manifest constants and formatting helpers.

/// Returns the nine structural market rows observed in the vendor ACK
/// manifest. Dynamic version/status columns remain zero until supplied by the
/// authenticated session.
pub fn default_ack_market_rows() -> Vec<String> {
    [
        (18_515, 20),
        (23_123, 20),
        (9_282, 4),
        (18_003, 20),
        (19_272, 28),
        (22_350, 0),
        (17_999, 4),
        (20_307, 0),
        (19_010, 4),
    ]
    .into_iter()
    .map(|(id, attr)| format!("{id}|{attr}|0|0|0|0|0|0|0|"))
    .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn includes_nine_structural_markets() {
        let rows = super::default_ack_market_rows();
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0], "18515|20|0|0|0|0|0|0|0|");
        assert_eq!(rows[8], "19010|4|0|0|0|0|0|0|0|");
    }
}
