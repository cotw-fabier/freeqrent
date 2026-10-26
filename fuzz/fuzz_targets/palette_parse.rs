//! Arbitrary CSS and HTML text: palette extraction never panics, and any
//! palette it returns keeps every painted token at the extraction contrast,
//! so a code drawn with it renders without a contrast warning.
#![no_main]

use freeqrent::{EXTRACT_CONTRAST, Palette, Token};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    for result in [Palette::from_css(text), Palette::from_html(text)] {
        let Ok((palette, report)) = result else {
            continue;
        };
        assert!(!report.swatches.is_empty());
        for choice in &report.choices {
            assert_eq!(palette.get(choice.token), Some(&choice.color.into()));
        }
        for token in [Token::Fg, Token::FinderEye] {
            let c = palette.contrast(token);
            assert!(!c.inverted && c.delta_l >= EXTRACT_CONTRAST, "{token}: {c:?}");
        }
    }
});
