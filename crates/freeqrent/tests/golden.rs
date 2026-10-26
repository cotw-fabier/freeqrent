//! Output stability: a fingerprint of each symbol in a fixed corpus. Any
//! change to encoding (segmentation, masks, placement) shows up here. If a
//! change is intended, update the values and say why in the wave's LOG entry.

use freeqrent::{Ecc, QrCode};

/// FNV-1a over version, ECC, mask, and every module.
fn fingerprint(text: &str, ecc: Ecc) -> u64 {
    let qr = QrCode::encode(text, ecc).unwrap();
    let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
    let mut eat = |b: u8| {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01B3);
    };
    eat(qr.version().number());
    eat(qr.ecc() as u8);
    eat(qr.mask().pattern());
    for (_, _, dark, _) in qr.modules() {
        eat(dark as u8);
    }
    hash
}

const CORPUS: &[(&str, Ecc)] = &[
    ("", Ecc::L),
    ("https://example.com", Ecc::M),
    ("HTTPS://EXAMPLE.COM/ABC", Ecc::Q),
    ("https://example.com/order/12345678901234567890", Ecc::M),
    ("WIFI:S:home;T:WPA;P:correct horse battery staple;;", Ecc::H),
    ("Grüße, 世界 🌍", Ecc::M),
    ("0123456789012345678901234567890123456789", Ecc::L),
    (
        "BEGIN:VCARD\nVERSION:3.0\nN:Doe;Jane\nTEL:+15555550123\nEND:VCARD",
        Ecc::Q,
    ),
];

#[test]
fn corpus_fingerprints_are_stable() {
    let got: Vec<u64> = CORPUS.iter().map(|&(t, e)| fingerprint(t, e)).collect();
    let expected: &[u64] = &[
        0x41DF28A691A3AE1B,
        0xAC52C7E79EE1E321,
        0x129F8EFFA7C0E367,
        0xCA2D4AEAD93DE242,
        0xA0992443322C085F,
        0xA76068810FAEA651,
        0xB5E9EE40A277320B,
        0xF8764B6D69460A91,
    ];
    assert_eq!(got, expected);
}
