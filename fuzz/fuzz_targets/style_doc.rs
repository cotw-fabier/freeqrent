//! Arbitrary text as a Style document: parsing never panics, and a document
//! that parses can be checked and rendered without panicking (it may still
//! be refused).
#![no_main]

use freeqrent_doc::{Payload, check, parse, render};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let _ = freeqrent_doc::json::parse(text);
    if parse(text).is_ok() {
        let _ = check(Payload::Text("https://example.com"), text);
        let _ = render(Payload::Text("https://example.com"), text);
    }
});
