//! Style presets (SPEC §5): a curated set of named styles.

use crate::{
    Axis, EyeShape, FinderStyle, Frame, FrameRing, ModuleShape, Palette, RingShape, Style,
};

/// The preset names, in gallery order.
const NAMES: [&str; 8] = [
    "classic", "ink", "candy", "terminal", "sunset", "bubble", "ribbon", "leaf",
];

impl Style {
    /// The style named `name`, or `None` for an unknown name (the names are
    /// in [`Style::presets`]). A preset is an ordinary style: the builder
    /// methods change any part of it, and a logo, picture or intro goes on
    /// top. Every preset is verified to scan across module shapes and
    /// frames (SPEC §9).
    ///
    /// - `classic`: black squares on white, the default style.
    /// - `ink`: dark blue dots with round eyes on warm paper.
    /// - `candy`: liquid modules in deep berry, squircle finders.
    /// - `terminal`: phosphor green on black (inverted; some older
    ///   scanners miss light-on-dark codes, `Warning::Inverted`).
    /// - `sunset`: rounded modules in a red-to-violet gradient.
    /// - `bubble`: a round code of dots, with a ring.
    /// - `ribbon`: horizontal bars in teal, rounded eyes.
    /// - `leaf`: rounded modules in forest green, leaf finders.
    ///
    /// ```
    /// use freeqrent::{ModuleShape, QrCode, Ecc, Style};
    ///
    /// let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
    /// let style = Style::preset("ink").unwrap().modules(ModuleShape::Dots { size: 0.9 });
    /// assert!(style.check(&qr).ok());
    /// assert!(Style::preset("nope").is_none());
    /// ```
    pub fn preset(name: &str) -> Option<Style> {
        let style = match name {
            "classic" => Style::new(),
            "ink" => Style::new()
                .modules(ModuleShape::Dots { size: 0.86 })
                .finders(FinderStyle::new(RingShape::Square, EyeShape::Circle))
                .palette(palette(&[("bg", "#f7f3ec"), ("fg", "oklch(30% .07 265)")])),
            "candy" => Style::new()
                .modules(ModuleShape::Liquid { smoothness: 0.8 })
                .finders(FinderStyle::new(RingShape::Squircle, EyeShape::Circle))
                .palette(palette(&[
                    ("bg", "#fff3f7"),
                    ("fg", "oklch(34% .14 350)"),
                    ("finder.eye", "oklch(32% .16 15)"),
                ])),
            "terminal" => Style::new()
                .modules(ModuleShape::Square { gap: 0.12 })
                .palette(palette(&[("bg", "#0d1b12"), ("fg", "#7dffa8")]))
                .allow_inverted(true),
            "sunset" => Style::new()
                .modules(ModuleShape::Rounded { radius: 0.4 })
                .finders(FinderStyle::new(RingShape::Square, EyeShape::Rounded(0.6)))
                .palette(palette(&[
                    ("bg", "#fff8f0"),
                    (
                        "fg",
                        "linear-gradient(135deg, oklch(36% .15 25), oklch(31% .14 305))",
                    ),
                ])),
            "bubble" => Style::new()
                .modules(ModuleShape::Dots { size: 0.82 })
                .finders(FinderStyle::new(RingShape::Rounded(1.5), EyeShape::Circle))
                .frame(Frame::Circle {
                    filler_density: 0.5,
                    ring: FrameRing::Solid,
                })
                .palette(palette(&[
                    ("bg", "#f1f6ff"),
                    ("fg", "oklch(30% .1 255)"),
                    ("filler", "oklch(30% .1 255 / .35)"),
                ])),
            "ribbon" => Style::new()
                .modules(ModuleShape::Bars {
                    axis: Axis::Horizontal,
                    radius: 0.5,
                })
                .finders(FinderStyle::new(RingShape::Square, EyeShape::Rounded(1.0)))
                .palette(palette(&[("bg", "#f2fbf9"), ("fg", "oklch(31% .07 195)")])),
            "leaf" => Style::new()
                .modules(ModuleShape::Rounded { radius: 0.5 })
                .finders(FinderStyle::new(RingShape::Leaf, EyeShape::Leaf))
                .palette(palette(&[("bg", "#f6f4e8"), ("fg", "oklch(32% .08 150)")])),
            _ => return None,
        };
        Some(style)
    }

    /// The names [`Style::preset`] knows, in gallery order.
    pub fn presets() -> &'static [&'static str] {
        &NAMES
    }
}

/// A palette from constant tokens; the preset tests parse every one.
fn palette(tokens: &[(&str, &str)]) -> Palette {
    Palette::from_tokens(tokens.iter().copied()).expect("preset palettes parse")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, QrCode, Warning};

    #[test]
    fn every_preset_name_is_a_preset() {
        for name in Style::presets() {
            assert!(Style::preset(name).is_some(), "{name}");
        }
        assert_eq!(Style::preset("classic"), Some(Style::new()));
        assert_eq!(Style::preset("Ink"), None);
    }

    #[test]
    fn presets_check_clean() {
        let qr = QrCode::encode("https://example.com/presets", Ecc::M).unwrap();
        for name in Style::presets() {
            let report = Style::preset(name).unwrap().check(&qr);
            assert!(report.ok(), "{name}: {:?}", report.errors);
            match *name {
                // Every token is light on the dark background.
                "terminal" => assert!(
                    !report.warnings.is_empty()
                        && report
                            .warnings
                            .iter()
                            .all(|w| matches!(w, Warning::Inverted { .. })),
                    "{name}: {:?}",
                    report.warnings
                ),
                "candy" | "bubble" | "leaf" => {
                    assert_eq!(report.warnings, [Warning::RoundedFinder], "{name}")
                }
                _ => assert_eq!(report.warnings, [], "{name}"),
            }
        }
    }
}
