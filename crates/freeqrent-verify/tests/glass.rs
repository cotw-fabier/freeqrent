//! Glass ink (SPEC §7.9): the photo full frame, the code baked over it as
//! liquid glass. Every case is held to the bar its own report implies:
//! the full bar up to `GLASS_FULL_BAR_VERSION`, ZXing on every render
//! (the degraded one included) past it.

mod common;

use common::{PHOTOS, photo_with_file};
use freeqrent::{
    Border, Caption, Ecc, Frame, GLASS_FULL_BAR_VERSION, Glass, Halftone, Idle, Ink, Intro, Logo,
    LogoShape, Palette, Preset, QrCode, QrMatrix, RasterFormat, RenderError, Style, Trigger,
    Version, Warning,
};
use freeqrent_verify::{Bar, Gray, rasterize, scannable_after, verify_style};

const TEXT: &str = "https://example.com";
/// The pictures' side: about the baked resolution at v5–v10.
const SIDE: u32 = 1024;

fn halftone(kind: &str, g: Glass) -> Halftone {
    halftone_at(kind, g, SIDE)
}

fn halftone_at(kind: &str, g: Glass, side: u32) -> Halftone {
    let (image, file) = photo_with_file(kind, side);
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

/// Pixels that differ by more than a rounding step, and the largest
/// difference.
fn diff(a: &Gray, b: &Gray) -> (usize, u8) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let mut count = 0;
    let mut worst = 0;
    for (&p, &q) in a.pixels.iter().zip(&b.pixels) {
        let d = p.abs_diff(q);
        worst = worst.max(d);
        if d > 2 {
            count += 1;
        }
    }
    (count, worst)
}

/// The static-fallback rule (SPEC §8.2) on glass: the animated document
/// as resvg draws it, with no CSS animation, and the plan's last frame are
/// the static code, pixel for pixel, and it scans.
fn assert_same_picture(plain: &Style, animated: &Style, qr: &QrMatrix, label: &str) {
    let a = rasterize(&plain.render(qr).unwrap(), 6.0).unwrap();
    let b = rasterize(&animated.render(qr).unwrap(), 6.0).unwrap();
    assert_eq!(diff(&a, &b), (0, 0), "{label}: static picture");
    let end = animated.plan(qr).unwrap().total_ms;
    let c = rasterize(&animated.render_frame(qr, end).unwrap(), 6.0).unwrap();
    assert_eq!(diff(&b, &c), (0, 0), "{label}: last frame");
}

#[test]
fn intros_keep_the_static_state() {
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#c33"/></svg>"##)
            .unwrap()
            .scale(0.12);
    let plain = Style::new().halftone(halftone_at("landscape", Glass::new(), 512));
    let framed = plain
        .clone()
        .frame(Frame::RoundedSquare(2.0))
        .border(Border::Solid { width: 0.5 })
        .caption(Caption::new("Scan me"));
    let logoed = plain.clone().logo(logo);
    for preset in Preset::ALL {
        for (label, style) in [("plain", &plain), ("framed", &framed), ("logo", &logoed)] {
            let qr = fitted(style, 5);
            let animated = style.clone().intro(preset);
            let label = format!("{preset:?} {label}");
            assert_same_picture(style, &animated, &qr, &label);
            if let Err(e) = verify_style(&animated, &qr, TEXT) {
                panic!("{label}: {e}");
            }
        }
    }
    // A replay, a palette shift and the idles glass takes: still the code.
    let qr = fitted(&logoed, 5);
    for (label, style) in [
        (
            "replay",
            logoed.clone().intro(
                Intro::new(Preset::Develop)
                    .duration_ms(1400)
                    .trigger(Trigger::Every(5000)),
            ),
        ),
        (
            "palette shift",
            logoed
                .clone()
                .intro(Intro::palette_from(&Palette::new().dark_twin())),
        ),
        (
            "idles",
            logoed
                .clone()
                .idle(Idle::Breathe)
                .idle(Idle::LogoGlint)
                .idle(Idle::LogoSpin),
        ),
    ] {
        assert_same_picture(&logoed, &style, &qr, label);
        if let Err(e) = verify_style(&style, &qr, TEXT) {
            panic!("{label}: {e}");
        }
    }
}

#[test]
fn intros_scan_before_they_end() {
    // Measured, printed for the log: when a glass code is readable for good.
    for kind in ["face", "landscape"] {
        for version in [4u8, 10] {
            let style = Style::new().halftone(halftone_at(kind, Glass::new(), 512));
            let qr = code(&style, version, Ecc::M);
            for preset in Preset::ALL {
                let style = style.clone().intro(preset);
                let total = style.plan(&qr).unwrap().total_ms;
                let at = scannable_after(&style, &qr, TEXT, 50).unwrap();
                println!("glass {kind} {preset:?} v{version}: scannable from {at} ms of {total}");
                assert!(at <= total, "{kind} {preset:?} v{version}: {at} > {total}");
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
