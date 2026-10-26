//! Arbitrary CSS and HTML text: palette extraction never panics, and any
//! palette it returns keeps every painted token at the extraction contrast,
//! so a code drawn with it renders without a contrast warning. With
//! inverted palettes allowed, a dark palette keeps the inverted contrast,
//! with every token on the same side of `bg`.
#![no_main]

use freeqrent::{EXTRACT_CONTRAST, INVERTED_CONTRAST, Palette, PaletteOpts, Token};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let inverted = PaletteOpts::new().allow_inverted(true);
    for (result, allowed) in [
        (Palette::from_css(text), false),
        (Palette::from_html(text), false),
        (Palette::from_css_with(text, inverted), true),
        (Palette::from_html_with(text, inverted), true),
    ] {
        let Ok((palette, report)) = result else {
            continue;
        };
        assert!(!report.swatches.is_empty());
        for choice in &report.choices {
            assert_eq!(palette.get(choice.token), Some(&choice.color.into()));
        }
        let dark = palette.contrast(Token::Fg).inverted;
        assert!(allowed || !dark);
        let floor = if dark {
            INVERTED_CONTRAST
        } else {
            EXTRACT_CONTRAST
        };
        for token in [Token::Fg, Token::FinderEye, Token::LogoFrame] {
            let c = palette.contrast(token);
            assert!(c.inverted == dark && c.delta_l >= floor, "{token}: {c:?}");
        }
    }
});
