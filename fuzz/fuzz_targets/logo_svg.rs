//! Arbitrary logo documents: sanitizing never panics, the clean document
//! sanitizes to itself, and a code rendered with it carries nothing the
//! sanitizer is meant to drop.
#![no_main]

use std::sync::OnceLock;

use freeqrent::{Ecc, Logo, QrCode, QrMatrix, Style};
use libfuzzer_sys::fuzz_target;

fn code() -> &'static QrMatrix {
    static CODE: OnceLock<QrMatrix> = OnceLock::new();
    CODE.get_or_init(|| QrCode::encode("https://example.com/fuzz", Ecc::H).unwrap())
}

fuzz_target!(|text: &str| {
    let Ok(logo) = Logo::svg(text) else {
        return;
    };
    let clean = logo.sanitized_svg().expect("vector logo");
    let again = Logo::svg(&clean).expect("the clean document is an SVG");
    assert_eq!(again.sanitized_svg().unwrap(), clean, "{text:?}");

    let svg = Style::new().logo(logo.scale(0.15)).render(code()).unwrap();
    // `<` is always escaped in text, so these can only be markup. (Text may
    // say "javascript:" legitimately; attributes carrying it are dropped.)
    for banned in ["<script", "<foreignObject", "<!", "<?", "<a ", "<animate"] {
        assert!(!svg.contains(banned), "{banned} in {svg}");
    }
    assert_eq!(svg.matches("<style>").count(), 1);
});
