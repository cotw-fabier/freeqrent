//! Known-answer tests against an independent encoder, Nayuki's `qrcodegen`.
//! Given the same segments, version, ECC level, and mask, both must produce
//! identical modules; with mask selection left to each, they must pick the
//! same mask (both score penalties the same way, SPEC §3.1).

use freeqrent::{Ecc, Mask, QrCode, QrMatrix, Segment, Version};
use qrcodegen as nayuki;

fn their_ecc(ecc: Ecc) -> nayuki::QrCodeEcc {
    match ecc {
        Ecc::L => nayuki::QrCodeEcc::Low,
        Ecc::M => nayuki::QrCodeEcc::Medium,
        Ecc::Q => nayuki::QrCodeEcc::Quartile,
        Ecc::H => nayuki::QrCodeEcc::High,
    }
}

fn assert_same(ours: &QrMatrix, theirs: &nayuki::QrCode, label: &str) {
    assert_eq!(ours.size() as i32, theirs.size(), "{label}: size");
    assert_eq!(
        ours.mask().pattern(),
        theirs.mask().value(),
        "{label}: mask"
    );
    for y in 0..ours.size() {
        for x in 0..ours.size() {
            assert_eq!(
                ours.is_dark(x, y),
                theirs.get_module(x as i32, y as i32),
                "{label}: module ({x}, {y})"
            );
        }
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

/// Encodes the same byte payload with both, at a pinned version and ECC.
fn both(data: &[u8], version: Version, ecc: Ecc, mask: Option<Mask>) -> (QrMatrix, nayuki::QrCode) {
    let mut builder = QrCode::builder().version(version).ecc(ecc).boost_ecc(false);
    if let Some(mask) = mask {
        builder = builder.mask(mask);
    }
    let ours = builder.encode_bytes(data).unwrap();
    let v = nayuki::Version::new(version.number());
    let theirs = nayuki::QrCode::encode_segments_advanced(
        &[nayuki::QrSegment::make_bytes(data)],
        their_ecc(ecc),
        v,
        v,
        mask.map(|m| nayuki::Mask::new(m.pattern())),
        false,
    )
    .unwrap();
    (ours, theirs)
}

#[test]
fn identical_modules_for_every_version_ecc_and_mask() {
    for version in Version::all() {
        for ecc in Ecc::ALL {
            // At most 7 bytes fit 1-H; grow with the version.
            let len = 4 + version.number() as usize * 3;
            let data = pseudo_random_bytes(len, version.number() as u64 * 4 + ecc as u64);
            for mask in Mask::all() {
                let (ours, theirs) = both(&data, version, ecc, Some(mask));
                assert_same(
                    &ours,
                    &theirs,
                    &format!("v{} {ecc:?} mask {}", version.number(), mask.pattern()),
                );
            }
        }
    }
}

#[test]
fn same_automatic_mask_choice() {
    for version in Version::all() {
        for ecc in Ecc::ALL {
            for seed in 0..4 {
                let data =
                    pseudo_random_bytes(3 + seed as usize, seed * 1000 + version.number() as u64);
                let (ours, theirs) = both(&data, version, ecc, None);
                assert_same(
                    &ours,
                    &theirs,
                    &format!("v{} {ecc:?} seed {seed}", version.number()),
                );
            }
        }
    }
}

#[test]
fn identical_mixed_segments_and_eci() {
    let ours_segments = [
        Segment::eci(26).unwrap(),
        Segment::alphanumeric("HTTPS://EXAMPLE.COM/").unwrap(),
        Segment::numeric("0123456789012").unwrap(),
        Segment::bytes("/grüße".as_bytes()),
    ];
    let their_segments = [
        nayuki::QrSegment::make_eci(26),
        nayuki::QrSegment::make_alphanumeric("HTTPS://EXAMPLE.COM/"),
        nayuki::QrSegment::make_numeric("0123456789012"),
        nayuki::QrSegment::make_bytes("/grüße".as_bytes()),
    ];
    for ecc in Ecc::ALL {
        let ours = QrCode::builder()
            .ecc(ecc)
            .boost_ecc(true)
            .encode_segments(&ours_segments)
            .unwrap();
        let theirs = nayuki::QrCode::encode_segments_advanced(
            &their_segments,
            their_ecc(ecc),
            nayuki::Version::MIN,
            nayuki::Version::MAX,
            None,
            true,
        )
        .unwrap();
        assert_eq!(ours.version().number(), theirs.version().value());
        assert_same(&ours, &theirs, &format!("mixed {ecc:?}"));
    }
}

#[test]
fn iso_annex_i_symbol() {
    // ISO 18004 Annex I encodes "01234567" at 1-M.
    let ours = QrCode::builder()
        .ecc(Ecc::M)
        .boost_ecc(false)
        .encode_text("01234567")
        .unwrap();
    let theirs = nayuki::QrCode::encode_segments_advanced(
        &[nayuki::QrSegment::make_numeric("01234567")],
        nayuki::QrCodeEcc::Medium,
        nayuki::Version::MIN,
        nayuki::Version::MIN,
        None,
        false,
    )
    .unwrap();
    assert_same(&ours, &theirs, "annex I");
}
