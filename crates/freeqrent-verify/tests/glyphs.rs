//! Round trips for custom shapes (SPEC §7.1, §7.2): SVG and PNG glyphs for
//! the modules, finder rings and eyes, tinted and in their own colors, on
//! square and round frames, still and animated.

mod common;

use common::{PAYLOADS, assert_scans};
use freeqrent::{
    Corner, Ecc, Frame, FrameRing, Glyph, Intro, Preset, QrCode, QrMatrix, RasterFormat, Style,
    Version, Warning,
};
use freeqrent_verify::{Bar, rasterize, verify_with};

/// Module shapes that fill the cell's center, in a 100-unit box.
const HEART: &str = r#"<svg viewBox="0 0 100 100"><path d="M50 92 8 50C-8 30 8 4 30 6c10 1 16 7 20 14 4-7 10-13 20-14 22-2 38 24 22 44z"/></svg>"#;
const STAR: &str = r#"<svg viewBox="0 0 100 100"><path d="M50 2 63 36 99 37 71 59 81 95 50 74 19 95 29 59 1 37 37 36z"/></svg>"#;
const PLUS: &str =
    r#"<svg viewBox="0 0 100 100"><path d="M32 0h36v32h32v36H68v32H32V68H0V32h32z"/></svg>"#;
const DOT: &str = r#"<svg viewBox="0 0 100 100"><circle cx="50" cy="50" r="35"/></svg>"#;
/// Two colors and a gradient, for keep-colors.
const BADGE: &str = r##"<svg viewBox="0 0 10 10"><defs><linearGradient id="g"><stop offset="0" stop-color="#1b2a4a"/><stop offset="1" stop-color="#5a1430"/></linearGradient></defs><rect width="10" height="10" rx="2" fill="url(#g)"/><circle cx="5" cy="5" r="2" fill="#0b0b0b"/></svg>"##;

/// A rounded square ring a module thick, in the finder's 7×7 box: outer
/// radius 2, inner radius 1, concentric like `RingShape::Rounded` (a square
/// hole under a round outside thins the band at the corners, and ZXing
/// lost v25 codes that way).
const RING: &str = r#"<svg viewBox="0 0 7 7"><path fill-rule="evenodd" d="M2 0h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2V2a2 2 0 0 1 2-2zM2 1a1 1 0 0 0-1 1v3a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1V2a1 1 0 0 0-1-1z"/></svg>"#;
/// A circle eye and a heart eye, in the 3×3 box.
const EYE: &str = r#"<svg viewBox="0 0 3 3"><circle cx="1.5" cy="1.5" r="1.5"/></svg>"#;

/// A disc on transparency, `side` pixels square, `r` of the side.
fn png_disc(side: u32, r: f64) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    let c = side as f64 / 2.0;
    for y in 0..side {
        for x in 0..side {
            let d = ((x as f64 + 0.5 - c).powi(2) + (y as f64 + 0.5 - c).powi(2)).sqrt();
            let a = ((r * side as f64 - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            pixels.extend_from_slice(&[200, 30, 60, a]);
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, side, side);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
    out
}

fn svg(text: &str) -> Glyph {
    Glyph::svg(text).unwrap()
}

fn code(text: &str, version: u8) -> QrMatrix {
    QrCode::builder()
        .ecc(Ecc::M)
        .boost_ecc(false)
        .version(Version::new(version).unwrap())
        .encode_text(text)
        .unwrap()
}

#[test]
fn module_glyphs_scan() {
    let disc = Glyph::raster(png_disc(48, 0.46), RasterFormat::Png).unwrap();
    let glyphs = [
        ("heart", svg(HEART)),
        ("star", svg(STAR)),
        ("plus", svg(PLUS)),
        ("dot 0.7", svg(DOT)),
        ("png disc", disc),
        ("badge in its colors", svg(BADGE).keep_colors(true)),
    ];
    for (name, glyph) in glyphs {
        for frame in [
            Frame::Square,
            Frame::Circle {
                filler_density: 0.6,
                ring: FrameRing::Solid,
            },
        ] {
            let style = Style::new().module_glyph(glyph.clone()).frame(frame);
            let report = style.check(&code("HELLO", 1));
            assert!(report.warnings.contains(&Warning::CustomModules));
            assert_scans(&style, &format!("{name} {frame:?}"));
        }
    }
}

#[test]
fn finder_glyphs_scan() {
    let disc = Glyph::raster(png_disc(48, 0.5), RasterFormat::Png).unwrap();
    let cases = [
        ("ring", Some(svg(RING)), None),
        ("eye", None, Some(svg(EYE))),
        ("heart eye", None, Some(svg(HEART))),
        ("png eye", None, Some(disc.clone())),
        ("ring + eye", Some(svg(RING)), Some(svg(EYE))),
    ];
    for (name, ring, eye) in cases {
        let style = Style::new().finder_glyphs(ring, eye);
        assert!(
            style
                .check(&code("HELLO", 1))
                .warnings
                .contains(&Warning::CustomFinder)
        );
        assert_scans(&style, name);
    }
    // One corner on its own, with the module glyph too.
    let style = Style::new().module_glyph(svg(HEART)).corner_glyphs(
        Corner::TopRight,
        Some(svg(RING)),
        Some(disc),
    );
    assert_scans(&style, "top-right corner");
}

/// Glyphs that leave the module's center empty, or break a finder's
/// profile, don't scan; these are why the report warns and the docs say
/// to fill the center (SPEC §7.1, §7.2).
#[test]
fn hollow_glyphs_are_caught_by_verify() {
    let hollow = r#"<svg viewBox="0 0 10 10"><path fill-rule="evenodd" d="M0 0h10v10H0zM2 2v6h6V2z"/></svg>"#;
    let style = Style::new().module_glyph(svg(hollow));
    let qr = code(PAYLOADS[2].0, PAYLOADS[2].1);
    let bar = Bar::for_report(&style.check(&qr));
    assert!(verify_with(&style.render(&qr).unwrap(), PAYLOADS[2].0, bar).is_err());
    // A ring with no band through the middle of its sides.
    let corners =
        r#"<svg viewBox="0 0 7 7"><path d="M0 0h2v2H0zM5 0h2v2H5zM0 5h2v2H0zM5 5h2v2H5z"/></svg>"#;
    let style = Style::new().finder_glyphs(Some(svg(corners)), None);
    let bar = Bar::for_report(&style.check(&qr));
    assert!(verify_with(&style.render(&qr).unwrap(), PAYLOADS[2].0, bar).is_err());
}

#[test]
fn glyph_intros_keep_the_static_state() {
    let plain = Style::new()
        .module_glyph(svg(STAR))
        .finder_glyphs(Some(svg(RING)), Some(svg(EYE).keep_colors(true)));
    for preset in Preset::ALL {
        let animated = plain.clone().intro(Intro::new(preset));
        for &(text, version) in &PAYLOADS[1..3] {
            let qr = code(text, version);
            let a = rasterize(&plain.render(&qr).unwrap(), 8.0).unwrap();
            let b = rasterize(&animated.render(&qr).unwrap(), 8.0).unwrap();
            assert_eq!(a.pixels, b.pixels, "{preset:?} v{version}: static picture");
            let end = animated.plan(&qr).unwrap().total_ms;
            let c = rasterize(&animated.render_frame(&qr, end).unwrap(), 8.0).unwrap();
            assert_eq!(b.pixels, c.pixels, "{preset:?} v{version}: last frame");
        }
        assert_scans(&animated, &format!("{preset:?}"));
    }
}
