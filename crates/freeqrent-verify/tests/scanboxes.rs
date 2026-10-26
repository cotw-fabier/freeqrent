//! Round trips for the scan-box styles (SPEC §7.2): finder rings and eyes,
//! per-corner overrides, alignment shapes and emphasis, timing styles.

mod common;

use common::assert_scans;
use freeqrent::{
    AlignmentShape, AlignmentStyle, Corner, Ecc, Emphasis, EyeShape, FinderStyle, ModuleShape,
    Palette, QrCode, RingShape, Style, TimingStyle, Version,
};

const RINGS: [RingShape; 6] = [
    RingShape::Square,
    RingShape::Rounded(0.5),
    RingShape::Rounded(1.0),
    RingShape::Rounded(2.5),
    RingShape::Squircle,
    RingShape::Leaf,
];

const EYES: [EyeShape; 6] = [
    EyeShape::Square,
    EyeShape::Rounded(0.5),
    EyeShape::Rounded(1.5),
    EyeShape::Circle,
    EyeShape::Squircle,
    EyeShape::Leaf,
];

#[test]
fn every_ring_and_eye_scans() {
    for ring in RINGS {
        for eye in EYES {
            let style = Style::new().finders(FinderStyle::new(ring, eye));
            assert_scans(&style, &format!("{ring:?} + {eye:?}"));
        }
    }
}

#[test]
fn per_corner_overrides_scan() {
    let finders = FinderStyle::new(RingShape::Squircle, EyeShape::Circle)
        .corner(Corner::TopRight, RingShape::Leaf, EyeShape::Leaf)
        .corner(Corner::BottomLeft, RingShape::Square, EyeShape::Squircle);
    let palette = Palette::from_tokens([
        ("finder.top-left.eye", "#b03a2e"),
        ("finder.bottom-left.outer", "#1f4e79"),
    ])
    .unwrap();
    assert_scans(
        &Style::new().finders(finders).palette(palette),
        "mixed corners",
    );
}

#[test]
fn alignment_styles_scan() {
    let shapes = [
        AlignmentShape::Match,
        AlignmentShape::Own {
            outer: RingShape::Square,
            eye: EyeShape::Square,
        },
        AlignmentShape::Own {
            outer: RingShape::Rounded(2.5),
            eye: EyeShape::Circle,
        },
        AlignmentShape::Own {
            outer: RingShape::Rounded(2.5),
            eye: EyeShape::Leaf,
        },
        AlignmentShape::Own {
            outer: RingShape::Squircle,
            eye: EyeShape::Squircle,
        },
    ];
    let finder_styles = [
        FinderStyle::SQUARE,
        FinderStyle::new(RingShape::Rounded(2.5), EyeShape::Circle),
        FinderStyle::new(RingShape::Leaf, EyeShape::Leaf),
        FinderStyle::new(RingShape::Rounded(2.0), EyeShape::Rounded(1.0)),
    ];
    let palette = Palette::from_tokens([("alignment", "#7a2e8f")]).unwrap();
    for finders in finder_styles {
        for shape in shapes {
            for emphasis in [Emphasis::None, Emphasis::Accent] {
                let style = Style::new()
                    .finders(finders)
                    .palette(palette.clone())
                    .alignment(AlignmentStyle::new(shape, emphasis));
                assert_scans(&style, &format!("{finders:?} {shape:?} {emphasis:?}"));
            }
        }
    }
}

#[test]
fn outline_emphasis_scans_within_budget() {
    // Outline clears real data modules, so only codes within the damage
    // budget render. Every one that does must decode.
    let style = Style::new().alignment(AlignmentStyle::new(
        AlignmentShape::Match,
        Emphasis::Outline,
    ));
    let mut rendered = 0;
    for version in [2, 3, 5, 7, 10, 14, 20, 25, 30, 40] {
        for ecc in Ecc::ALL {
            let qr = QrCode::builder()
                .ecc(ecc)
                .boost_ecc(false)
                .version(Version::new(version).unwrap())
                .encode_text("HTTPS://EXAMPLE.COM")
                .unwrap();
            if !style.check(&qr).ok() {
                continue;
            }
            rendered += 1;
            if let Err(e) = freeqrent_verify::verify_style(&style, &qr, "HTTPS://EXAMPLE.COM") {
                panic!("outline v{version} {ecc:?}: {e}");
            }
        }
    }
    assert!(
        rendered >= 10,
        "only {rendered} outlined codes fit the budget"
    );
}

#[test]
fn timing_styles_scan() {
    for timing in [TimingStyle::Plain, TimingStyle::Dotted, TimingStyle::Dashed] {
        for shape in [
            ModuleShape::SQUARE,
            ModuleShape::Dots { size: 0.7 },
            ModuleShape::Rounded { radius: 0.5 },
        ] {
            let style = Style::new().timing(timing).modules(shape);
            assert_scans(&style, &format!("{timing:?} {shape:?}"));
        }
    }
}

#[test]
fn styled_combinations_scan() {
    let style = Style::new()
        .modules(ModuleShape::Dots { size: 0.8 })
        .finders(FinderStyle::new(RingShape::Squircle, EyeShape::Squircle))
        .timing(TimingStyle::Dotted)
        .palette(
            Palette::from_tokens([
                ("bg", "oklch(97% .015 85)"),
                (
                    "fg",
                    "linear-gradient(135deg, oklch(30% .1 265), oklch(28% .12 320))",
                ),
                ("finder.eye", "oklch(42% .16 30)"),
            ])
            .unwrap(),
        );
    assert_scans(&style, "dots + squircles + gradient");
}

#[test]
fn rounded_rings_hold_across_payloads() {
    // Round rings are marginal at large versions (a circle missed about 1
    // in 200 clean renders, which is why it isn't offered), so sweep
    // payloads at the roundest shapes that ship.
    for ring in [
        RingShape::Rounded(2.5),
        RingShape::Squircle,
        RingShape::Leaf,
    ] {
        let style = Style::new().finders(FinderStyle::new(ring, EyeShape::Rounded(0.5)));
        for version in [15, 25, 40] {
            for seed in 0..6u32 {
                let text = format!("https://example.com/p/{}", seed * 7919 + version * 31);
                let qr = QrCode::builder()
                    .ecc(Ecc::M)
                    .boost_ecc(false)
                    .version(Version::new(version as u8).unwrap())
                    .encode_text(&text)
                    .unwrap();
                if let Err(e) = freeqrent_verify::verify_style(&style, &qr, &text) {
                    panic!("{ring:?} v{version} {text}: {e}");
                }
            }
        }
    }
}
