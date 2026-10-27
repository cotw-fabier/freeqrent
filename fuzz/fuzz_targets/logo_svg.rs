//! Arbitrary logo documents: sanitizing never panics, the clean document
//! sanitizes to itself, and a code rendered with it carries nothing the
//! sanitizer is meant to drop.
#![no_main]

use std::sync::OnceLock;

use freeqrent::{Ecc, Logo, QrCode, QrMatrix, RenderError, Style};
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

    // A very wide or tall logo doesn't fit at 0.15; the refusal says the
    // largest scale that does, and the code renders at that.
    let svg = match Style::new().logo(logo.clone().scale(0.15)).render(code()) {
        Err(RenderError::LogoTooLarge { max_scale }) => Style::new()
            .logo(logo.scale(max_scale))
            .render(code())
            .unwrap_or_else(|e| panic!("at its largest scale {max_scale}: {e:?} for {text:?}")),
        other => other.unwrap(),
    };
    // `<` is always escaped in text, so these can only be markup. (Text may
    // say "javascript:" legitimately; attributes carrying it are dropped.)
    for banned in ["<script", "<foreignObject", "<!", "<?", "<a ", "<animate"] {
        assert!(!svg.contains(banned), "{banned} in {svg}");
    }
    assert_eq!(svg.matches("<style>").count(), 1);
});
