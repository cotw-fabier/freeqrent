//! Arbitrary paint strings: parsing never panics, and whatever parses
//! serializes to a canonical form that parses back to the same value.
#![no_main]

use freeqrent::Paint;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let Ok(paint) = Paint::parse(text) else {
        return;
    };
    let canonical = paint.to_string();
    let again = Paint::parse(&canonical).expect("canonical form parses");
    assert_eq!(again, paint, "{text:?} -> {canonical:?}");
    assert_eq!(again.to_string(), canonical);
});
