//! Shared round-trip helpers.

// Each test binary uses its own subset.
#![allow(dead_code)]

use freeqrent::{Ecc, QrCode, Style, Version};

/// Payloads pinned to versions with 0, 1, 6 and 22 alignment patterns.
pub const PAYLOADS: [(&str, u8); 4] = [
    ("HELLO", 1),
    ("https://example.com/menu", 5),
    (
        "https://example.com/order/12345678901234567890?ref=freeqrent",
        10,
    ),
    (
        "A longer payload that needs a larger symbol to hold it; version 25 at ECC M.",
        25,
    ),
];

/// Renders `style` for every payload in `PAYLOADS` and checks it against
/// the style's own bar (`Bar::for_report`).
pub fn assert_scans(style: &Style, label: &str) {
    assert_scans_at(style, label, &PAYLOADS, Ecc::M);
}

/// [`assert_scans`] for chosen payloads and ECC level.
pub fn assert_scans_at(style: &Style, label: &str, payloads: &[(&str, u8)], ecc: Ecc) {
    for &(text, version) in payloads {
        let qr = QrCode::builder()
            .ecc(ecc)
            .boost_ecc(false)
            .version(Version::new(version).unwrap())
            .encode_text(text)
            .unwrap();
        let result = freeqrent_verify::verify_style(style, &qr, text);
        if let Err(e) = result {
            panic!("{label} v{version}: {e}");
        }
    }
}
