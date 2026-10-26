//! Arbitrary segment lists built through the public constructors: invalid
//! characters are rejected cleanly, valid lists round-trip.
#![no_main]

use arbitrary::Arbitrary;
use freeqrent::{QrCode, Segment};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
enum Part {
    Numeric(String),
    Alphanumeric(String),
    Bytes(Vec<u8>),
    Eci(u32),
}

fuzz_target!(|parts: Vec<Part>| {
    let mut segments = Vec::new();
    let mut expected = Vec::new();
    for part in &parts {
        let segment = match part {
            Part::Numeric(s) => Segment::numeric(s),
            Part::Alphanumeric(s) => Segment::alphanumeric(s),
            Part::Bytes(b) => Ok(Segment::bytes(b)),
            Part::Eci(v) => Segment::eci(*v),
        };
        let Ok(segment) = segment else { return };
        match part {
            Part::Numeric(s) | Part::Alphanumeric(s) => expected.extend_from_slice(s.as_bytes()),
            Part::Bytes(b) => expected.extend_from_slice(b),
            Part::Eci(_) => {}
        }
        segments.push(segment);
    }
    let Ok(qr) = QrCode::builder().encode_segments(&segments) else {
        return;
    };
    let size = qr.size() as usize;
    let grid = rqrr::Grid::new(rqrr::SimpleGrid::from_func(size, |x, y| {
        qr.is_dark(x as u16, y as u16)
    }));
    let mut decoded = Vec::new();
    grid.decode_to(&mut decoded).expect("decodes");
    assert_eq!(decoded, expected);
});
