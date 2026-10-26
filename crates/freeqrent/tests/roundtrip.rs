//! Encode with freeqrent, decode with `rqrr`, compare (SPEC §9).

use freeqrent::{Ecc, EciPolicy, Mask, ModuleRole, QrBuilder, QrCode, QrMatrix, Segment, Version};

/// Decodes the module grid directly, skipping image detection.
fn decode_grid(qr: &QrMatrix) -> (rqrr::MetaData, Vec<u8>) {
    let size = qr.size() as usize;
    let grid = rqrr::Grid::new(rqrr::SimpleGrid::from_func(size, |x, y| {
        qr.is_dark(x as u16, y as u16)
    }));
    let mut bytes = Vec::new();
    let meta = grid.decode_to(&mut bytes).expect("rqrr decodes the grid");
    (meta, bytes)
}

/// Rasterizes with a 4-module quiet zone and decodes from pixels, which also
/// exercises finder detection.
fn decode_image(qr: &QrMatrix, scale: usize) -> Vec<u8> {
    let quiet = 4;
    let side = (qr.size() as usize + 2 * quiet) * scale;
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(side, side, |px, py| {
        let (x, y) = (px / scale, py / scale);
        let inside = (quiet..quiet + qr.size() as usize).contains(&x)
            && (quiet..quiet + qr.size() as usize).contains(&y);
        if inside && qr.is_dark((x - quiet) as u16, (y - quiet) as u16) {
            0
        } else {
            255
        }
    });
    let grids = img.detect_grids();
    assert_eq!(
        grids.len(),
        1,
        "v{} should be detected once",
        qr.version().number()
    );
    let mut bytes = Vec::new();
    grids[0]
        .decode_to(&mut bytes)
        .expect("rqrr decodes the image");
    bytes
}

/// rqrr reports the raw format-information ECC bits.
fn rqrr_ecc(ecc: Ecc) -> u16 {
    match ecc {
        Ecc::L => 1,
        Ecc::M => 0,
        Ecc::Q => 3,
        Ecc::H => 2,
    }
}

fn pseudo_random_bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Largest byte-mode payload for a version and ECC level.
fn max_bytes(version: Version, ecc: Ecc) -> usize {
    let mut lo = 0usize;
    let mut hi = 3000;
    let fits = |n: usize| {
        QrCode::builder()
            .version(version)
            .ecc(ecc)
            .boost_ecc(false)
            .encode_bytes(&vec![0; n])
            .is_ok()
    };
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(mid) { lo = mid } else { hi = mid - 1 }
    }
    lo
}

#[test]
fn every_version_and_ecc_at_full_capacity() {
    for version in Version::all() {
        for ecc in Ecc::ALL {
            let len = max_bytes(version, ecc);
            let data = pseudo_random_bytes(len, version.number() as u64 * 4 + ecc as u64);
            let qr = QrCode::builder()
                .version(version)
                .ecc(ecc)
                .boost_ecc(false)
                .encode_bytes(&data)
                .unwrap();
            let (meta, decoded) = decode_grid(&qr);
            let label = format!("v{} {ecc:?}", version.number());
            assert_eq!(decoded, data, "{label}");
            assert_eq!(meta.version.0, version.number() as usize, "{label}");
            assert_eq!(meta.ecc_level, rqrr_ecc(ecc), "{label}");
            assert_eq!(meta.mask, qr.mask().pattern() as u16, "{label}");
        }
    }
}

#[test]
fn every_forced_mask_decodes() {
    for n in [1, 2, 7, 14, 21, 40] {
        let version = Version::new(n).unwrap();
        for mask in Mask::all() {
            let qr = QrCode::builder()
                .version(version)
                .mask(mask)
                .encode_text("forced mask")
                .unwrap();
            let (meta, decoded) = decode_grid(&qr);
            assert_eq!(decoded, b"forced mask", "v{n} mask {}", mask.pattern());
            assert_eq!(meta.mask, mask.pattern() as u16);
        }
    }
}

#[test]
fn modes_and_mixed_segments() {
    let cases: &[(&str, &[Segment])] = &[
        ("0123456789", &[Segment::numeric("0123456789").unwrap()]),
        (
            "HELLO WORLD $%*+-./:",
            &[Segment::alphanumeric("HELLO WORLD $%*+-./:").unwrap()],
        ),
        (
            "ABC123456789xyz",
            &[
                Segment::alphanumeric("ABC").unwrap(),
                Segment::numeric("123456789").unwrap(),
                Segment::bytes(b"xyz"),
            ],
        ),
    ];
    for &(expected, segments) in cases {
        let qr = QrCode::builder().encode_segments(segments).unwrap();
        assert_eq!(decode_grid(&qr).1, expected.as_bytes());
    }
}

#[test]
fn utf8_text_with_and_without_eci() {
    let text = "Grüße, 世界 🌍";
    for policy in [EciPolicy::Auto, EciPolicy::Never, EciPolicy::Always] {
        let qr = QrCode::builder().eci(policy).encode_text(text).unwrap();
        // rqrr skips the ECI header and passes the bytes through.
        assert_eq!(decode_grid(&qr).1, text.as_bytes(), "{policy:?}");
    }
    let with = QrCode::builder()
        .eci(EciPolicy::Auto)
        .encode_text(text)
        .unwrap();
    let without = QrCode::builder()
        .eci(EciPolicy::Never)
        .encode_text(text)
        .unwrap();
    assert_eq!(
        with.capacity_report().data_bits_used,
        without.capacity_report().data_bits_used + 12
    );
}

#[test]
fn empty_and_typical_text() {
    for text in [
        "",
        "https://example.com",
        "HTTPS://EXAMPLE.COM/A",
        "WIFI:S:home;T:WPA;P:secret;;",
    ] {
        let qr = QrCode::encode(text, Ecc::M).unwrap();
        assert_eq!(decode_grid(&qr).1, text.as_bytes(), "{text:?}");
    }
}

#[test]
fn detected_from_pixels() {
    for n in [1, 4, 7, 14, 25, 40] {
        let version = Version::new(n).unwrap();
        let data = pseudo_random_bytes(max_bytes(version, Ecc::M) / 2, n as u64);
        let qr = QrBuilder::default()
            .version(version)
            .encode_bytes(&data)
            .unwrap();
        assert_eq!(decode_image(&qr, 3), data, "v{n}");
    }
}

#[test]
fn roles_account_for_every_codeword_bit() {
    for version in Version::all() {
        for ecc in Ecc::ALL {
            let qr = QrCode::builder()
                .version(version)
                .ecc(ecc)
                .boost_ecc(false)
                .encode_text("x")
                .unwrap();
            let report = qr.capacity_report();
            let count = |role| qr.modules().filter(|m| m.3 == role).count();
            assert_eq!(count(ModuleRole::Data), report.data_codewords * 8);
            assert_eq!(count(ModuleRole::Ecc), report.ecc_codewords * 8);
            assert!(count(ModuleRole::Remainder) <= 7);
            assert_eq!(count(ModuleRole::DarkModule), 1);
            assert_eq!(count(ModuleRole::Format), 30);
            assert_eq!(
                count(ModuleRole::Version),
                if version.number() >= 7 { 36 } else { 0 }
            );
        }
    }
}

#[test]
fn mixed_mode_text_decodes_and_is_smaller() {
    let texts = [
        "https://example.com/order/12345678901234567890",
        "Invoice INV-2026-000123 total 4999.00 EUR",
        "tel:+15555550123",
        "Ünïcode 12345678 ABCDEFGHIJ xyz 99",
        "0123456789012345678901234567890123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabc",
    ];
    for text in texts {
        let qr = QrCode::builder()
            .boost_ecc(false)
            .encode_text(text)
            .unwrap();
        assert_eq!(decode_grid(&qr).1, text.as_bytes(), "{text:?}");
        let bytes_only = QrCode::builder()
            .boost_ecc(false)
            .encode_bytes(text.as_bytes())
            .unwrap();
        assert!(
            qr.capacity_report().data_bits_used <= bytes_only.capacity_report().data_bits_used + 12,
            "{text:?}"
        );
    }
}

#[test]
fn segmentation_follows_the_version_band() {
    // Long enough to need version 10+, where count fields are wider.
    let text: String = (0..60)
        .map(|i| if i % 2 == 0 { "123456" } else { "abc" })
        .collect();
    let qr = QrCode::builder().ecc(Ecc::H).encode_text(&text).unwrap();
    assert!(qr.version().number() >= 10);
    assert_eq!(decode_grid(&qr).1, text.as_bytes());
}
