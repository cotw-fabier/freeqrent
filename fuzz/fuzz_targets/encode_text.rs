//! Arbitrary text and options: encoding never panics, and whatever encodes
//! decodes back to the same bytes.
#![no_main]

use arbitrary::Arbitrary;
use freeqrent::{Ecc, EciPolicy, Mask, QrCode, Version};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    text: String,
    ecc: u8,
    min_version: u8,
    max_version: u8,
    mask: Option<u8>,
    eci: u8,
    boost: bool,
}

fuzz_target!(|input: Input| {
    let ecc = Ecc::ALL[input.ecc as usize % 4];
    let mut builder = QrCode::builder()
        .ecc(ecc)
        .boost_ecc(input.boost)
        .min_version(Version::new(input.min_version % 40 + 1).unwrap())
        .max_version(Version::new(input.max_version % 40 + 1).unwrap())
        .eci([EciPolicy::Auto, EciPolicy::Never, EciPolicy::Always][input.eci as usize % 3]);
    if let Some(m) = input.mask {
        builder = builder.mask(Mask::new(m % 8).unwrap());
    }
    let Ok(qr) = builder.encode_text(&input.text) else {
        return;
    };
    assert!(qr.ecc() >= ecc);
    let size = qr.size() as usize;
    let grid = rqrr::Grid::new(rqrr::SimpleGrid::from_func(size, |x, y| {
        qr.is_dark(x as u16, y as u16)
    }));
    let mut decoded = Vec::new();
    grid.decode_to(&mut decoded).expect("decodes");
    assert_eq!(decoded, input.text.as_bytes());
});
