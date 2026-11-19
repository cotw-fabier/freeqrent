//! Glass ink (SPEC §7.9): the photo full frame, the code baked over it as
//! liquid glass. Every case is held to the bar its own report implies:
//! the full bar up to `GLASS_FULL_BAR_VERSION`, ZXing on every render
//! (the degraded one included) past it.

mod common;

use common::{PHOTOS, photo_with_file};
use freeqrent::{
    Border, Caption, Ecc, Frame, GLASS_FULL_BAR_VERSION, Glass, Halftone, Ink, Logo, LogoShape,
    Palette, QrCode, QrMatrix, RasterFormat, RenderError, Style, Version, Warning,
};
use freeqrent_verify::{Bar, verify_style};

const TEXT: &str = "https://example.com";
/// The pictures' side: about the baked resolution at v5–v10.
const SIDE: u32 = 1024;

fn halftone(kind: &str, g: Glass) -> Halftone {
    let (image, file) = photo_with_file(kind, SIDE);
    Halftone::new(image)
        .source(file, RasterFormat::Png)
        .unwrap()
        .ink(Ink::Glass(g))
}

fn code(style: &Style, version: u8, ecc: Ecc) -> QrMatrix {
    QrCode::builder()
        .ecc(ecc)
        .boost_ecc(false)
        .version(Version::new(version).unwrap())
        .encode_text_for(TEXT, style)
        .unwrap()
}

/// The smallest code from `version` up whose budget affords the style.
fn fitted(style: &Style, version: u8) -> QrMatrix {
    QrCode::builder()
        .ecc(Ecc::Q)
        .min_version(Version::new(version).unwrap())
        .encode_text_for(TEXT, style)
        .unwrap()
}

/// Checks `style` on `qr`: it renders, scannable, with only the warnings
/// its version and size call for, and it scans at its own bar.
fn assert_scans(style: &Style, qr: &QrMatrix, label: &str) {
    let report = style.check(qr);
    assert!(report.ok() && report.scannable, "{label}: {report:?}");
    let version = qr.version().number();
    for w in &report.warnings {
        let expected = match w {
            Warning::GlassNeedsZxing { version: v } => {
                *v == version && version > GLASS_FULL_BAR_VERSION
            }
            Warning::LargeOutput { .. } => true,
            _ => false,
        };
        assert!(expected, "{label} v{version}: {w:?}");
    }
    let bar = Bar::for_report(&report);
    assert!(bar.degraded, "{label}: glass keeps the degraded render");
    assert_eq!(bar.rqrr, version <= GLASS_FULL_BAR_VERSION, "{label}");
    if let Err(e) = verify_style(style, qr, TEXT) {
        panic!("{label} v{version}: {e}");
    }
}

/// Every version from 3 to 14 at M and Q, at the defaults.
fn sweep(kind: &str) {
    let style = Style::new().halftone(halftone(kind, Glass::new()));
    for version in 3..=14 {
        for ecc in [Ecc::M, Ecc::Q] {
            let qr = code(&style, version, ecc);
            assert_scans(&style, &qr, &format!("{kind} {ecc:?}"));
        }
    }
}

#[test]
fn disc() {
    sweep(PHOTOS[0]);
}

#[test]
fn gradient() {
    sweep(PHOTOS[1]);
}

#[test]
fn rings() {
    sweep(PHOTOS[2]);
}

#[test]
fn photo() {
    sweep(PHOTOS[3]);
}

#[test]
fn face() {
    sweep(PHOTOS[4]);
}

#[test]
fn landscape() {
    sweep(PHOTOS[5]);
}

#[test]
fn dusk() {
    sweep(PHOTOS[6]);
}

#[test]
fn borders_scan() {
    for kind in ["disc", "face", "landscape"] {
        for border in [0.0, 0.08, 0.15] {
            let style = Style::new().halftone(halftone(kind, Glass::new().border(border)));
            for version in [4u8, 10] {
                let qr = code(&style, version, Ecc::M);
                assert_scans(&style, &qr, &format!("{kind} border {border}"));
            }
        }
    }
}

#[test]
fn dark_palettes_scan() {
    // Glass paints no `fg`: on a dark twin its modules are still dark and
    // light glass, and only the background under the photo changes.
    let dark = Palette::new().dark_twin();
    for kind in ["face", "landscape", "photo"] {
        let style = Style::new()
            .halftone(halftone(kind, Glass::new()))
            .palette(dark.clone());
        for version in [4u8, 10] {
            let qr = code(&style, version, Ecc::M);
            assert_scans(&style, &qr, &format!("{kind} dark"));
        }
    }
}

#[test]
fn logos_scan() {
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#c33"/></svg>"##)
            .unwrap();
    for kind in ["disc", "face", "dusk"] {
        for (version, shape) in [(5u8, LogoShape::Square), (6, LogoShape::Circle)] {
            let style = Style::new()
                .halftone(halftone(kind, Glass::new()))
                .logo(logo.clone().scale(0.12).shape(shape));
            let qr = fitted(&style, version);
            assert_scans(&style, &qr, &format!("{kind} logo {shape:?}"));
        }
        // Art mode's outlined alignment patterns let a logo cover them.
        let art = Style::art(halftone(kind, Glass::new())).logo(logo.clone().scale(0.12));
        let qr = fitted(&art, 10);
        assert_scans(&art, &qr, &format!("{kind} art logo"));
    }
}

#[test]
fn frames_and_art_scan() {
    let h = || halftone("landscape", Glass::new());
    let styles = [
        (
            "rounded",
            Style::new().halftone(h()).frame(Frame::RoundedSquare(3.0)),
        ),
        ("frameless", Style::new().halftone(h()).frame(Frame::None)),
        (
            "border and shadow",
            Style::new()
                .halftone(h())
                .border(Border::Double { width: 1.5 })
                .shadow(1.0),
        ),
        (
            "caption",
            Style::new()
                .halftone(h())
                .frame(Frame::RoundedSquare(2.0))
                .caption(Caption::new("Scan me")),
        ),
        (
            "narrow quiet zone",
            Style::new().halftone(h()).quiet_zone(2),
        ),
        ("art", Style::art(h())),
    ];
    for (label, style) in styles {
        for version in [4u8, 9] {
            let qr = code(&style, version, Ecc::M);
            let report = style.check(&qr);
            assert!(report.ok() && report.scannable, "{label}: {report:?}");
            if let Err(e) = verify_style(&style, &qr, TEXT) {
                panic!("{label} v{version}: {e}");
            }
        }
    }
}

#[test]
fn measured_limits_are_refused_and_allowed_on_request() {
    let cases = [
        (Glass::new().halo(0), "halo"),
        (Glass::new().finders(0.6, 0.92), "finders"),
        (Glass::new().targets(0.40, 0.75), "targets"),
    ];
    for (g, label) in cases {
        let style = Style::new().halftone(halftone("face", g));
        let qr = code(&style, 5, Ecc::M);
        let refused = style.check(&qr);
        assert!(
            refused.errors.iter().any(|e| matches!(
                e,
                RenderError::GlassHaloTooNarrow { .. }
                    | RenderError::GlassFinderContrast { .. }
                    | RenderError::GlassTargetsTooWeak { .. }
            )),
            "{label}: {refused:?}"
        );
        assert!(style.render(&qr).is_err());
        let allowed = style.allow_unscannable(true);
        let report = allowed.check(&qr);
        assert!(report.ok() && !report.scannable, "{label}: {report:?}");
        assert!(allowed.render(&qr).is_ok(), "{label}");
    }
}
