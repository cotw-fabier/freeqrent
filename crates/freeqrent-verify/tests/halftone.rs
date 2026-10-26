//! Halftone codes (SPEC §7.7 B): the picture drawn into 3 × 3 sub-cells
//! whose centers carry the code. They must scan at the full bar, alone and
//! with every other style option.

mod common;

use common::picture;
use freeqrent::{
    AlignmentShape, AlignmentStyle, Clarity, Ecc, Emphasis, Fit, Frame, FrameRing, Halftone, Ink,
    Intro, LayerRole, Logo, Palette, Preset, QrCode, QrMatrix, RenderError, Rgba8Image, Style,
    TimingStyle, Token, Version, Warning,
};
use freeqrent_verify::{Render, rasterize, verify_one, verify_style};

const TEXT: &str = "https://example.com";
const PICTURES: [&str; 5] = ["disc", "gradient", "rings", "photo", "logo"];

fn fitted(style: &Style, version: u8, ecc: Ecc) -> QrMatrix {
    QrCode::builder()
        .ecc(ecc)
        .min_version(Version::new(version).unwrap())
        .encode_text_for(TEXT, style)
        .unwrap()
}

fn assert_verifies(style: &Style, qr: &QrMatrix, label: &str) {
    let report = style.check(qr);
    assert!(report.ok(), "{label}: {report:?}");
    assert!(report.warnings.is_empty(), "{label}: {:?}", report.warnings);
    if let Err(e) = verify_style(style, qr, TEXT) {
        panic!("{label} v{}: {e}", qr.version().number());
    }
}

#[test]
fn halftone_codes_scan() {
    for kind in PICTURES {
        let image = picture(kind, 200);
        for ink in [Ink::Mono, Ink::Color { colors: 6 }] {
            for version in [5u8, 14, 27] {
                for ecc in [Ecc::L, Ecc::M, Ecc::Q] {
                    let style = Style::new().halftone(Halftone::new(image.clone()).ink(ink));
                    let qr = fitted(&style, version, ecc);
                    assert_verifies(&style, &qr, &format!("{kind} {ink:?} {ecc:?}"));
                }
            }
        }
    }
}

#[test]
fn dark_halftones_scan() {
    // Light ink on a dark twin: the picture is traced from its light
    // parts, and the code reads at the full bar, inverted. Clarity 0.5 and
    // a logo too, as the gallery's picture cards use them.
    let dark = Palette::new().dark_twin();
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#c33"/></svg>"##)
            .unwrap();
    for kind in PICTURES {
        let image = picture(kind, 200);
        for ink in [Ink::Mono, Ink::Color { colors: 6 }] {
            for clarity in [0.0, 0.5] {
                for version in [5u8, 14, 27] {
                    for ecc in [Ecc::L, Ecc::M, Ecc::Q] {
                        let picture = Halftone::new(image.clone())
                            .ink(ink)
                            .clarity(Clarity::Uniform(clarity));
                        let mut style = Style::new()
                            .halftone(picture)
                            .palette(dark.clone())
                            .allow_inverted(true);
                        if version == 14 {
                            style = style.logo(logo.clone().scale(0.12));
                        }
                        let qr = fitted(&style, version, ecc);
                        let label = format!("{kind} {ink:?} {clarity} {ecc:?}");
                        let report = style.check(&qr);
                        assert!(report.ok(), "{label}: {report:?}");
                        assert!(
                            report
                                .warnings
                                .iter()
                                .all(|w| matches!(w, Warning::Inverted { .. })),
                            "{label}: {report:?}"
                        );
                        if let Err(e) = verify_style(&style, &qr, TEXT) {
                            panic!("{label} v{}: {e}", qr.version().number());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn logos_on_pictures_scan_at_every_size() {
    // rqrr samples one pixel through a perspective fitted to a single
    // alignment pattern, so it lands off-center: the reason for the 0.6
    // center and the smaller damage budget. Every size from 4 to 12 px.
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#c33"/></svg>"##)
            .unwrap();
    for scale in [0.12, 0.18] {
        for kind in ["disc", "photo", "gradient"] {
            for version in [8u8, 13, 20] {
                let style = Style::new()
                    .halftone(Halftone::new(picture(kind, 200)))
                    .logo(logo.clone().scale(scale));
                let qr = fitted(&style, version, Ecc::M);
                let svg = style.render(&qr).unwrap();
                for px in 4..=12 {
                    let render = Render {
                        px_per_module: px,
                        blur: false,
                        noise: false,
                    };
                    if let Err(e) = verify_one(&svg, TEXT, render, true) {
                        panic!("{scale} {kind} v{}: {e}", qr.version().number());
                    }
                }
            }
        }
    }
}

#[test]
fn halftone_combines_with_other_styles() {
    let mut clear_left = Rgba8Image::filled(20, 20, [0, 0, 0, 255]).unwrap();
    for y in 0..20 {
        for x in 0..10 {
            clear_left.set_pixel(x, y, [255, 255, 255, 255]);
        }
    }
    let wide = picture("photo", 200);
    let outline = AlignmentStyle {
        shape: AlignmentShape::Match,
        emphasis: Emphasis::Outline,
    };
    let gradient =
        Palette::from_tokens([("fg", "linear-gradient(135deg, #1d2a6b, #5a1340)")]).unwrap();
    for kind in ["disc", "photo", "rings"] {
        let h = || Halftone::new(picture(kind, 200));
        let variants = [
            ("unsteered", Style::new().halftone(h().steer(false))),
            (
                "outline",
                Style::new()
                    .halftone(h())
                    .alignment(outline)
                    .timing(TimingStyle::Dotted),
            ),
            (
                "round",
                Style::new().halftone(h()).frame(Frame::Circle {
                    filler_density: 0.5,
                    ring: FrameRing::Solid,
                }),
            ),
            (
                "clarity",
                Style::new().halftone(h().clarity(Clarity::Uniform(0.5))),
            ),
            (
                "clarity map",
                Style::new().halftone(h().clarity(Clarity::Map(clear_left.clone()))),
            ),
            (
                "contain",
                Style::new().halftone(Halftone::new(wide.clone()).fit(Fit::Contain)),
            ),
            (
                "gradient",
                Style::new().halftone(h()).palette(gradient.clone()),
            ),
            (
                "two colors",
                Style::new().halftone(h().ink(Ink::Color { colors: 2 })),
            ),
        ];
        for (name, style) in variants {
            for version in [7, 14] {
                let qr = fitted(&style, version, Ecc::M);
                assert_verifies(&style, &qr, &format!("{kind} {name}"));
            }
        }
    }
}

#[test]
fn v20_pictures_stay_small() {
    // SPEC §7.7 rule 5: at most 150 KB for a v20 code in mono.
    for kind in PICTURES {
        let style = Style::new().halftone(Halftone::new(picture(kind, 200)));
        let qr = fitted(&style, 20, Ecc::M);
        assert_eq!(qr.version().number(), 20);
        let len = style.render(&qr).unwrap().len();
        assert!(len <= 150 * 1024, "{kind}: {} KB", len / 1024);
    }
}

#[test]
fn intros_end_on_the_static_picture() {
    for preset in Preset::ALL {
        for ink in [Ink::Mono, Ink::Color { colors: 4 }] {
            let plain = Style::new().halftone(Halftone::new(picture("photo", 120)).ink(ink));
            let animated = plain.clone().intro(Intro::new(preset));
            let qr = fitted(&plain, 7, Ecc::M);
            let a = rasterize(&plain.render(&qr).unwrap(), 8.0).unwrap();
            let b = rasterize(&animated.render(&qr).unwrap(), 8.0).unwrap();
            assert_eq!(a, b, "{preset:?} {ink:?}: static picture");
            let end = animated.plan(&qr).unwrap().total_ms;
            let c = rasterize(&animated.render_frame(&qr, end).unwrap(), 8.0).unwrap();
            assert_eq!(b, c, "{preset:?} {ink:?}: last frame");
            assert_verifies(&animated, &qr, &format!("{preset:?} {ink:?}"));
        }
    }
}

#[test]
fn intros_break_the_picture_into_tiles() {
    for ink in [Ink::Mono, Ink::Color { colors: 4 }] {
        let plain = Style::new().halftone(Halftone::new(picture("photo", 120)).ink(ink));
        let qr = fitted(&plain, 10, Ecc::M);
        for preset in [
            Preset::Settle,
            Preset::Ripple,
            Preset::Assemble,
            Preset::LockOn,
        ] {
            let animated = plain.clone().intro(Intro::new(preset));
            let plan = animated.plan(&qr).unwrap();
            let mut groups: Vec<u8> = plan
                .layers
                .iter()
                .filter(|l| l.role == LayerRole::Halftone)
                .filter_map(|l| l.group)
                .collect();
            groups.dedup();
            // Tiles of the picture land in every stagger group, in order.
            assert_eq!(groups, (0..12).collect::<Vec<u8>>(), "{preset:?} {ink:?}");
            // Live, each group's strokes share one animated group.
            let svg = animated.render(&qr).unwrap();
            let wrappers = svg.matches("\"><svg x=").count();
            assert_eq!(wrappers, 12, "{preset:?} {ink:?}");
            let report = animated.check(&qr);
            assert!(
                report.warnings.is_empty(),
                "{preset:?}: {:?}",
                report.warnings
            );
            let at = freeqrent_verify::scannable_after(&animated, &qr, TEXT, 50).unwrap();
            println!(
                "{preset:?} {ink:?} v10: scannable from {at} ms of {}",
                plan.total_ms
            );
            assert!(at <= plan.total_ms);
        }
        // Develop fades the picture whole.
        let develop = plain.clone().intro(Intro::new(Preset::Develop));
        let plan = develop.plan(&qr).unwrap();
        assert!(
            plan.layers
                .iter()
                .filter(|l| l.role == LayerRole::Halftone)
                .all(|l| l.group.is_none())
        );
    }
}

#[test]
fn options_are_checked() {
    let image = picture("disc", 40);
    let qr = fitted(&Style::new(), 7, Ecc::M);
    let narrow = Style::new().halftone(
        Halftone::new(image.clone())
            .ink(Ink::Color { colors: 4 })
            .bands(0.4, 0.75),
    );
    assert!(matches!(
        narrow.check(&qr).errors[..],
        [RenderError::ContrastTooLow {
            token: Token::Fg,
            ..
        }]
    ));
    let soft = Style::new().halftone(
        Halftone::new(image.clone())
            .ink(Ink::Color { colors: 4 })
            .bands(0.35, 0.9),
    );
    let report = soft.check(&qr);
    assert!(report.ok());
    assert!(matches!(
        report.warnings[..],
        [Warning::LowContrast {
            token: Token::Fg,
            ..
        }]
    ));
    let bad = Style::new().halftone(Halftone::new(image).clarity(Clarity::Uniform(1.5)));
    assert!(matches!(
        bad.check(&qr).errors[..],
        [RenderError::OptionOutOfRange {
            option: "Clarity::Uniform",
            ..
        }]
    ));
}
