//! Round trips for logos (SPEC §7.3): the largest logo each code says it
//! can afford must scan, whatever the logo looks like.

mod common;

use common::assert_scans_at;
use freeqrent::{
    Ecc, Frame, FrameRing, Logo, LogoShape, ModuleShape, QrCode, QrMatrix, RasterFormat,
    RenderError, Style, Version,
};
use freeqrent_verify::verify_style;
use resvg::tiny_skia;

/// The worst case for a scanner: dark, with light stripes, no margin.
const DARK: &str = r##"<svg viewBox="0 0 10 10"><rect width="10" height="10" fill="#17161c"/><path d="M2 2h6v2h-6zM2 6h6v2h-6z" fill="#fefcf8"/></svg>"##;

fn code(text: &str, version: u8, ecc: Ecc) -> QrMatrix {
    QrCode::builder()
        .ecc(ecc)
        .boost_ecc(false)
        .version(Version::new(version).unwrap())
        .encode_text(text)
        .unwrap()
}

/// The `max_scale` a code reports for an oversized logo.
fn max_scale(logo: &Logo, qr: &QrMatrix) -> f32 {
    let report = Style::new().logo(logo.clone().scale(0.5)).check(qr);
    report
        .errors
        .iter()
        .find_map(|e| match e {
            RenderError::LogoTooLarge { max_scale } => Some(*max_scale),
            _ => None,
        })
        .expect("a half-width logo never fits")
}

#[test]
fn the_largest_affordable_logo_scans() {
    let shapes = [
        LogoShape::Square,
        LogoShape::Rounded(1.5),
        LogoShape::Circle,
    ];
    for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
        for version in [2, 5, 10, 25] {
            let text = format!("EXAMPLE {version}");
            let qr = code(&text, version, ecc);
            for shape in shapes {
                for backdrop in [true, false] {
                    let logo = Logo::svg(DARK).unwrap().shape(shape).backdrop(backdrop);
                    let max = max_scale(&logo, &qr);
                    if max == 0.0 {
                        continue;
                    }
                    let style = Style::new().logo(logo.scale(max));
                    let report = style.check(&qr);
                    assert!(report.ok(), "{report:?}");
                    if let Err(e) = verify_style(&style, &qr, &text) {
                        panic!("v{version} {ecc:?} {shape:?} backdrop {backdrop} at {max}: {e}");
                    }
                }
            }
        }
    }
}

#[test]
fn encoding_for_a_style_grows_the_code_until_the_logo_fits() {
    let text = "https://example.com/menu";
    let style = Style::new().logo(Logo::svg(DARK).unwrap().scale(0.3));
    let plain = QrCode::builder().encode_text(text).unwrap();
    assert!(matches!(
        style.check(&plain).errors[..],
        [RenderError::LogoTooLarge { .. }]
    ));
    let fitted = QrCode::builder().encode_text_for(text, &style).unwrap();
    assert!(fitted.version() > plain.version(), "{:?}", fitted.version());
    assert!(style.check(&fitted).ok());
    verify_style(&style, &fitted, text).unwrap();
    // Pinned to a version that can't afford it: the plain code comes back.
    let pinned = QrCode::builder()
        .version(plain.version())
        .encode_text_for(text, &style)
        .unwrap();
    assert_eq!(pinned.version(), plain.version());
}

#[test]
fn raster_logos_scan() {
    // A dark checkerboard, as PNG.
    let mut pixmap = tiny_skia::Pixmap::new(16, 16).unwrap();
    for (i, px) in pixmap.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (i % 16, i / 16);
        let v = if (x / 4 + y / 4) % 2 == 0 { 20 } else { 240 };
        *px = tiny_skia::PremultipliedColorU8::from_rgba(v, v, v, 255).unwrap();
    }
    let png = pixmap.encode_png().unwrap();
    let logo = Logo::raster(png, RasterFormat::Png)
        .unwrap()
        .shape(LogoShape::Circle);
    let text = "https://example.com/raster";
    let qr = code(text, 6, Ecc::Q);
    let style = Style::new().logo(logo.clone().scale(max_scale(&logo, &qr)));
    let svg = style.render(&qr).unwrap();
    assert!(svg.contains("<image ") && svg.contains("href=\"data:image/png;base64,iVBOR"));
    verify_style(&style, &qr, text).unwrap();
}

#[test]
fn logos_combine_with_shapes_and_frames() {
    let logo = Logo::svg(DARK)
        .unwrap()
        .scale(0.15)
        .shape(LogoShape::Circle);
    let styles = [
        Style::new().modules(ModuleShape::Liquid { smoothness: 1.0 }),
        Style::new().modules(ModuleShape::Dots { size: 0.85 }),
        Style::new().frame(Frame::Circle {
            filler_density: 0.6,
            ring: FrameRing::Solid,
        }),
        // Modules kept under the plate.
        Style::new(),
    ];
    for (i, style) in styles.into_iter().enumerate() {
        let logo = logo.clone().clear_modules(i != 3);
        // Version 1 can't hold even this logo, so the shared payloads
        // start at 5.
        let label = format!("style {i}");
        assert_scans_at(&style.logo(logo), &label, &common::PAYLOADS[1..], Ecc::Q);
    }
}

#[test]
fn bytes_and_segments_fitted_for_a_logo_scan() {
    // `encode_bytes_for` and `encode_segments_for` grow the code for the
    // logo like `encode_text_for`; a numeric and an alphanumeric payload,
    // at every ECC level, with the worst-case logo.
    let style = Style::new().logo(Logo::svg(DARK).unwrap().scale(0.2));
    for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
        let b = QrCode::builder().ecc(ecc);
        let digits = "31415926535897932384626433832795";
        let numeric = [freeqrent::Segment::numeric(digits).unwrap()];
        let qr = b.encode_segments_for(&numeric, &style).unwrap();
        verify_style(&style, &qr, digits).unwrap_or_else(|e| panic!("numeric {ecc:?}: {e}"));
        let text = "HTTPS://EXAMPLE.COM/A1";
        let qr = b.encode_bytes_for(text.as_bytes(), &style).unwrap();
        verify_style(&style, &qr, text).unwrap_or_else(|e| panic!("bytes {ecc:?}: {e}"));
    }
}

#[test]
fn the_largest_logos_at_h_scan() {
    // At ECC H logos get DAMAGE_BUDGET_H: at 60%, the largest logos on
    // v16–20 went unfound by ZXing under blur and noise (W20261026171519).
    use freeqrent::{DAMAGE_BUDGET_H, LogoFrame, LogoSize, Palette};
    for version in 16..=20 {
        let text = format!("LOGO 1234 {version}");
        let qr = code(&text, version, Ecc::H);
        for frame in [None, Some(LogoFrame::Ring)] {
            for shape in [LogoShape::Square, LogoShape::Circle] {
                for modules in [ModuleShape::SQUARE, ModuleShape::Dots { size: 0.85 }] {
                    for dark in [false, true] {
                        let mut logo = Logo::svg(DARK)
                            .unwrap()
                            .shape(shape)
                            .size(LogoSize::Largest)
                            .scale(0.05);
                        if let Some(frame) = frame {
                            logo = logo.frame(frame);
                        }
                        let palette = if dark {
                            Palette::new().dark_twin()
                        } else {
                            Palette::new()
                        };
                        let style = Style::new()
                            .modules(modules)
                            .logo(logo)
                            .palette(palette)
                            .allow_inverted(dark);
                        let label =
                            format!("v{version} {frame:?} {shape:?} {modules:?} dark={dark}");
                        let report = style.check(&qr);
                        assert!(report.ok(), "{label}: {report:?}");
                        assert!(report.ecc_budget_used <= DAMAGE_BUDGET_H, "{label}");
                        verify_style(&style, &qr, &text).unwrap_or_else(|e| panic!("{label}: {e}"));
                    }
                }
            }
        }
    }
}
