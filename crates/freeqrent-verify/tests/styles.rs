//! Round trips for every style option (SPEC §9): render, rasterize with
//! resvg, decode with ZXing and rqrr, against each style's own bar
//! (`Bar::for_report`).

mod common;

use common::assert_scans;
use freeqrent::{
    Axis, ColorSpread, Ecc, Frame, FrameRing, ModuleShape, Palette, QrCode, Style, Token, Warning,
};
use freeqrent_verify::rasterize;

fn shapes() -> Vec<ModuleShape> {
    vec![
        ModuleShape::SQUARE,
        ModuleShape::Square { gap: 0.15 },
        ModuleShape::Square { gap: 0.25 },
        ModuleShape::Rounded { radius: 0.25 },
        ModuleShape::Rounded { radius: 0.5 },
        ModuleShape::Dots { size: 1.0 },
        ModuleShape::Dots { size: 0.8 },
        ModuleShape::Dots { size: 0.6 },
        ModuleShape::Bars {
            axis: Axis::Vertical,
            radius: 0.5,
        },
        ModuleShape::Bars {
            axis: Axis::Horizontal,
            radius: 0.2,
        },
        ModuleShape::Liquid { smoothness: 0.0 },
        ModuleShape::Liquid { smoothness: 0.5 },
        ModuleShape::Liquid { smoothness: 1.0 },
    ]
}

#[test]
fn every_module_shape_scans() {
    for shape in shapes() {
        assert_scans(&Style::new().modules(shape), &format!("{shape:?}"));
    }
}

#[test]
fn quiet_zones_scan() {
    for qz in [2, 3, 4, 8] {
        assert_scans(&Style::new().quiet_zone(qz), &format!("quiet zone {qz}"));
    }
}

#[test]
fn palettes_scan() {
    let palettes = [
        vec![("bg", "#fff"), ("fg", "#000")],
        vec![("bg", "oklch(97% .02 90)"), ("fg", "oklch(30% .12 265)")],
        vec![("fg", "rgb(0 0 0 / .85)")],
        vec![("bg", "transparent")],
        vec![("fg", "linear-gradient(135deg, #1d2b64, #512d6d)")],
        vec![
            ("fg", "radial-gradient(#3a1c71, #1b1b2f)"),
            ("finder.eye", "#a52a22"),
        ],
        vec![
            ("finder.top-left.outer", "#0b6e4f"),
            ("finder.top-right.eye", "oklch(45% .15 30)"),
            ("timing", "#555"),
            ("alignment", "#2c3e50"),
        ],
        // Warning band: clean renders only.
        vec![("finder.eye", "#c0392b")],
        vec![("bg", "#fff"), ("fg", "oklch(58% 0 0)")],
    ];
    for tokens in palettes {
        let palette = Palette::from_tokens(tokens.iter().copied()).unwrap();
        for shape in [
            ModuleShape::SQUARE,
            ModuleShape::Dots { size: 0.85 },
            ModuleShape::Liquid { smoothness: 1.0 },
        ] {
            assert_scans(
                &Style::new().palette(palette.clone()).modules(shape),
                &format!("{tokens:?} {shape:?}"),
            );
        }
    }
}

#[test]
fn warning_band_is_reported() {
    let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
    let palette = Palette::from_tokens([("finder.eye", "#c0392b")]).unwrap();
    let report = Style::new().palette(palette).check(&qr);
    assert!(report.ok());
    assert!(
        report.warnings.iter().any(|w| matches!(
            w,
            Warning::LowContrast {
                token: Token::FinderEye,
                ..
            }
        )),
        "{report:?}"
    );
}

#[test]
fn inverted_codes_scan_when_allowed() {
    // rqrr reads light-on-dark codes once the image is inverted, like
    // scanners that support them do.
    let palette = Palette::from_tokens([("bg", "#101418"), ("fg", "#f4f1ea")]).unwrap();
    let style = Style::new().palette(palette).allow_inverted(true);
    let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
    let svg = style.render(&qr).unwrap();
    let mut image = rasterize(&svg, 6.0).unwrap();
    image.pixels.iter_mut().for_each(|p| *p = 255 - *p);
    assert_eq!(
        freeqrent_verify::decode(&image, freeqrent_verify::Decoder::Rqrr).unwrap(),
        ["https://example.com"]
    );
}

#[test]
fn resvg_skips_supports_blocks_and_keeps_attributes() {
    // The paint form relies on this (see freeqrent's svg.rs): resvg paints
    // black for `var()` or `oklch()` in an inline style, but skips an
    // `@supports` block and keeps the hex `fill` attribute.
    let blue = (255 * 114 + 500) / 1000;
    let luma = |svg: &str| {
        let image = rasterize(svg, 4.0).unwrap();
        image.pixels[image.width * 4 + 4] as u32
    };
    let inline = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2 2"><rect width="2" height="2" fill="#00f" style="fill:var(--x,#f00)"/></svg>"##;
    let supports = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 2 2"><style>@supports (color:var(--a)){.f{fill:var(--x,#f00)}}</style><rect class="f" width="2" height="2" fill="#00f"/></svg>"##;
    assert_eq!(
        luma(inline),
        0,
        "resvg learned var(); the paint form could be simpler"
    );
    assert_eq!(luma(supports), blue);
}

#[test]
fn data_colors_scan() {
    // Three hues near the warning band (ΔL ≈ 0.62 from the default bg),
    // one module or one region at a time, and as their dark twin.
    let palette = Palette::from_tokens([
        ("fg", "oklch(36% .16 27)"),
        ("fg.2", "oklch(36% .18 265)"),
        ("fg.3", "oklch(36% .12 145)"),
        ("fg.4", "#17161c"),
    ])
    .unwrap();
    let shapes = [
        ModuleShape::SQUARE,
        ModuleShape::Dots { size: 0.85 },
        ModuleShape::Liquid { smoothness: 1.0 },
        ModuleShape::Bars {
            axis: Axis::Vertical,
            radius: 0.5,
        },
    ];
    for spread in [ColorSpread::Modules, ColorSpread::Regions] {
        for shape in shapes {
            let style = Style::new()
                .palette(palette.clone())
                .modules(shape)
                .color_spread(spread);
            assert_scans(&style, &format!("{spread:?} {shape:?}"));
            let twin = style.palette(palette.dark_twin()).allow_inverted(true);
            assert_scans(&twin, &format!("{spread:?} {shape:?} dark"));
        }
    }
}

#[test]
fn gradient_backgrounds_scan() {
    // Backgrounds whose darkest point sits at the warning band from fg,
    // linear and radial, on square and round frames, light and inverted.
    let backgrounds = [
        "linear-gradient(135deg, #ffffff, oklch(90% .05 300))",
        "radial-gradient(oklch(96% .03 85), oklch(88% .06 60))",
    ];
    let frames = [
        Frame::Square,
        Frame::Circle {
            filler_density: 0.5,
            ring: FrameRing::Solid,
        },
    ];
    for bg in backgrounds {
        let palette = Palette::from_tokens([("bg", bg), ("fg", "#1d2433")]).unwrap();
        assert!(palette.contrast(Token::Fg).delta_l >= 0.6, "{bg}");
        for frame in frames {
            for shape in [ModuleShape::SQUARE, ModuleShape::Dots { size: 0.85 }] {
                let style = Style::new()
                    .palette(palette.clone())
                    .modules(shape)
                    .frame(frame);
                assert_scans(&style, &format!("{bg} {frame:?} {shape:?}"));
                let twin = style.palette(palette.dark_twin()).allow_inverted(true);
                assert_scans(&twin, &format!("{bg} {frame:?} {shape:?} dark"));
            }
        }
    }
}
