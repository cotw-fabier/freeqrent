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

/// Synthetic pictures for halftone tests, `side` pixels square.
pub fn picture(kind: &str, side: u32) -> freeqrent::Rgba8Image {
    let mut img = freeqrent::Rgba8Image::filled(side, side, [0, 0, 0, 255]).unwrap();
    let s = side as f64;
    for y in 0..side {
        for x in 0..side {
            let (u, v) = (x as f64 / s, y as f64 / s);
            let (dx, dy) = (u - 0.5, v - 0.5);
            let r = (dx * dx + dy * dy).sqrt();
            let px: [u8; 4] = match kind {
                // A dark disc on a light page.
                "disc" => {
                    if r < 0.3 {
                        [30, 40, 90, 255]
                    } else {
                        [245, 240, 230, 255]
                    }
                }
                // Left to right, black to white.
                "gradient" => {
                    let g = (u * 255.0) as u8;
                    [g, g, g, 255]
                }
                // Concentric rings, smooth.
                "rings" => {
                    let g = (((r * 40.0).sin() * 0.5 + 0.5) * 255.0) as u8;
                    [g, (g as f64 * 0.8) as u8, 255 - g / 2, 255]
                }
                // Soft blobs, like an out-of-focus photo, in color.
                "photo" => {
                    let a = (u * 7.0).sin() * (v * 5.0 + 1.0).cos();
                    let b = ((u + v) * 4.0).sin();
                    let l = ((a * 0.35 + b * 0.25 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                    [l, (l as f64 * 0.6 + 60.0 * u) as u8, (200.0 * v) as u8, 255]
                }
                // A logo shape on transparency: a dark ring.
                _ => {
                    if (0.25..0.4).contains(&r) {
                        [20, 20, 20, 255]
                    } else {
                        [0, 0, 0, 0]
                    }
                }
            };
            img.set_pixel(x, y, px);
        }
    }
    img
}
