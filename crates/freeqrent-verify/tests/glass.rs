//! Glass ink (SPEC §7.9): the photo full frame, the code over it as liquid
//! glass, baked into a PNG or drawn as SVG elements (vector). Every case is
//! held to the bar its own report implies: the full bar up to
//! `GLASS_FULL_BAR_VERSION` (baked) or `GLASS_VECTOR_FULL_BAR_VERSION`
//! (vector), ZXing on every render (the degraded one included) past it.

mod common;

use common::{PHOTOS, photo_with_file};
use freeqrent::{
    Border, Caption, Ecc, Effect, Frame, GLASS_FULL_BAR_VERSION, GLASS_VECTOR_FULL_BAR_LENS,
    GLASS_VECTOR_FULL_BAR_VERSION, GLASS_VECTOR_NARROW_FULL_BAR_VERSION, Glass, GlassRender,
    Halftone, Idle, Ink, Intro, Logo, LogoShape, Palette, Preset, QrCode, QrMatrix, RasterFormat,
    RenderError, Style, Trigger, Version, Warning,
};

const BAKED: GlassRender = GlassRender::Baked;
const VECTOR: GlassRender = GlassRender::Vector;

/// The default glass, drawn as `render` says.
fn glass(render: GlassRender) -> Glass {
    Glass::new().render(render)
}
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
fn assert_scans(style: &Style, qr: &QrMatrix, render: GlassRender, label: &str) {
    assert_scans_lens(style, qr, render, 0.9, label);
}

/// [`assert_scans`] for vector glass with lenses `lens` wide.
fn assert_scans_lens(style: &Style, qr: &QrMatrix, render: GlassRender, lens: f32, label: &str) {
    let report = style.check(qr);
    assert!(report.ok() && report.scannable, "{label}: {report:?}");
    let version = qr.version().number();
    let boundary = if render == VECTOR && lens < GLASS_VECTOR_FULL_BAR_LENS {
        GLASS_VECTOR_NARROW_FULL_BAR_VERSION
    } else if render == VECTOR {
        GLASS_VECTOR_FULL_BAR_VERSION
    } else {
        GLASS_FULL_BAR_VERSION
    };
    for w in &report.warnings {
        let expected = match w {
            Warning::GlassNeedsZxing { version: v } => *v == version && version > boundary,
            Warning::LargeOutput { .. } => true,
            _ => false,
        };
        assert!(expected, "{label} v{version}: {w:?}");
    }
    let bar = Bar::for_report(&report);
    assert!(bar.degraded, "{label}: glass keeps the degraded render");
    assert_eq!(bar.rqrr, version <= boundary, "{label}");
    if let Err(e) = verify_style(style, qr, TEXT) {
        panic!("{label} v{version}: {e}");
    }
}

/// Every version from 3 to 14 at M and Q, at the defaults.
fn sweep(kind: &str, render: GlassRender) {
    let style = Style::new().halftone(halftone(kind, glass(render)));
    for version in 3..=14 {
        for ecc in [Ecc::M, Ecc::Q] {
            let qr = code(&style, version, ecc);
            assert_scans(&style, &qr, render, &format!("{kind} {render:?} {ecc:?}"));
        }
    }
}

#[test]
fn disc() {
    sweep(PHOTOS[0], BAKED);
}

#[test]
fn gradient() {
    sweep(PHOTOS[1], BAKED);
}

#[test]
fn rings() {
    sweep(PHOTOS[2], BAKED);
}

#[test]
fn photo() {
    sweep(PHOTOS[3], BAKED);
}

#[test]
fn face() {
    sweep(PHOTOS[4], BAKED);
}

#[test]
fn landscape() {
    sweep(PHOTOS[5], BAKED);
}

#[test]
fn dusk() {
    sweep(PHOTOS[6], BAKED);
}

#[test]
fn vector_disc() {
    sweep(PHOTOS[0], VECTOR);
}

#[test]
fn vector_gradient() {
    sweep(PHOTOS[1], VECTOR);
}

#[test]
fn vector_rings() {
    sweep(PHOTOS[2], VECTOR);
}

#[test]
fn vector_photo() {
    sweep(PHOTOS[3], VECTOR);
}

#[test]
fn vector_face() {
    sweep(PHOTOS[4], VECTOR);
}

#[test]
fn vector_landscape() {
    sweep(PHOTOS[5], VECTOR);
}

#[test]
fn vector_dusk() {
    sweep(PHOTOS[6], VECTOR);
}

#[test]
fn borders_scan() {
    for render in [BAKED, VECTOR] {
        for kind in ["disc", "face", "landscape"] {
            for border in [0.0, 0.08, 0.15] {
                let style = Style::new().halftone(halftone(kind, glass(render).border(border)));
                for version in [4u8, 10] {
                    let qr = code(&style, version, Ecc::M);
                    assert_scans(&style, &qr, render, &format!("{kind} border {border}"));
                }
            }
        }
    }
}

#[test]
fn dark_palettes_scan() {
    // Glass paints no `fg`: on a dark twin its modules are still dark and
    // light glass, and only the background under the photo changes.
    let dark = Palette::new().dark_twin();
    for render in [BAKED, VECTOR] {
        for kind in ["face", "landscape", "photo"] {
            let style = Style::new()
                .halftone(halftone(kind, glass(render)))
                .palette(dark.clone());
            for version in [4u8, 10] {
                let qr = code(&style, version, Ecc::M);
                assert_scans(&style, &qr, render, &format!("{kind} dark"));
            }
        }
    }
}

#[test]
fn logos_scan() {
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#c33"/></svg>"##)
            .unwrap();
    for render in [BAKED, VECTOR] {
        for kind in ["disc", "face", "dusk"] {
            for (version, shape) in [(5u8, LogoShape::Square), (6, LogoShape::Circle)] {
                let style = Style::new()
                    .halftone(halftone(kind, glass(render)))
                    .logo(logo.clone().scale(0.12).shape(shape));
                let qr = fitted(&style, version);
                assert_scans(&style, &qr, render, &format!("{kind} logo {shape:?}"));
            }
            // Art mode's outlined alignment patterns let a logo cover them.
            let art = Style::art(halftone(kind, glass(render))).logo(logo.clone().scale(0.12));
            let qr = fitted(&art, 10);
            assert_scans(&art, &qr, render, &format!("{kind} art logo"));
        }
    }
}

#[test]
fn frames_and_art_scan() {
    for render in [BAKED, VECTOR] {
        let h = || halftone("landscape", glass(render));
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
                    panic!("{label} {render:?} v{version}: {e}");
                }
            }
        }
    }
}

/// Vector glass's switches: no frost (no filter at all, for renderers
/// without them), no rim, no tint, and the extremes of lens and union
/// still scan at their own bar.
#[test]
fn vector_options_scan() {
    for kind in ["face", "landscape", "rings"] {
        for (label, g) in [
            ("no frost", glass(VECTOR).frost(0.0)),
            ("frost 0.3", glass(VECTOR).frost(0.3)),
            ("no rim, no tint", glass(VECTOR).rim(false).tint(false)),
            ("lens 0.7", glass(VECTOR).lens(0.7).core(0.6)),
            ("lens 1, union 0.8", glass(VECTOR).lens(1.0).union(0.8)),
            ("union 0.2", glass(VECTOR).union(0.2)),
            ("halo 1", glass(VECTOR).halo(1)),
        ] {
            let style = Style::new().halftone(halftone(kind, g));
            for version in [3u8, 8] {
                let qr = code(&style, version, Ecc::M);
                assert_scans_lens(
                    &style,
                    &qr,
                    VECTOR,
                    g.lens_width(),
                    &format!("{kind} {label}"),
                );
            }
        }
    }
}

/// Effects filter the glass (W20261123231733): sketch on both renders,
/// glow and emboss on vector glass, each at its strongest, still scan at
/// their own bar. (Over the baked layer glow lost the bar and emboss lost
/// noise seeds, so those two are refused there; filters are slow to
/// rasterize, so this keeps to two versions.)
#[test]
fn effects_scan() {
    let cases = [
        (BAKED, "sketch 0.15", Effect::Sketch { roughness: 0.15 }),
        (VECTOR, "sketch 0.15", Effect::Sketch { roughness: 0.15 }),
        (
            VECTOR,
            "glow 0.5 / 0.25",
            Effect::Glow {
                radius: 0.5,
                strength: 0.25,
            },
        ),
        (VECTOR, "emboss 0.12", Effect::Emboss { depth: 0.12 }),
    ];
    for kind in ["face", "rings", "dusk", "photo"] {
        for (render, label, effect) in cases {
            let style = Style::new()
                .halftone(halftone(kind, glass(render)))
                .effect(effect);
            for version in [3u8, 8] {
                let qr = code(&style, version, Ecc::M);
                assert_scans(&style, &qr, render, &format!("{kind} {render:?} {label}"));
            }
        }
    }
    // Refused, not ignored, over the baked layer.
    for (effect, option) in [
        (
            Effect::Glow {
                radius: 0.3,
                strength: 0.1,
            },
            "Effect::Glow on baked glass",
        ),
        (
            Effect::Emboss { depth: 0.1 },
            "Effect::Emboss on baked glass",
        ),
    ] {
        let style = Style::new()
            .halftone(halftone("face", glass(BAKED)))
            .effect(effect);
        let report = style.check(&code(&style, 3, Ecc::M));
        assert!(
            report
                .errors
                .contains(&RenderError::GlassUnsupported { option }),
            "{option}: {report:?}"
        );
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
    for render in [BAKED, VECTOR] {
        static_state(render);
    }
}

fn static_state(render: GlassRender) {
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#c33"/></svg>"##)
            .unwrap()
            .scale(0.12);
    let plain = Style::new().halftone(halftone_at("landscape", glass(render), 512));
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
            let label = format!("{render:?} {preset:?} {label}");
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
            panic!("{render:?} {label}: {e}");
        }
    }
}

#[test]
fn intros_scan_before_they_end() {
    // Measured, printed for the log: when a glass code is readable for good.
    // Baked: face and landscape at v4 and v10; vector, sampled: v4 and v10
    // on the face only.
    let cases = [(BAKED, "face"), (BAKED, "landscape"), (VECTOR, "face")];
    for (render, kind) in cases {
        for version in [4u8, 10] {
            let style = Style::new().halftone(halftone_at(kind, glass(render), 512));
            let qr = code(&style, version, Ecc::M);
            for preset in Preset::ALL {
                let style = style.clone().intro(preset);
                let total = style.plan(&qr).unwrap().total_ms;
                let at = scannable_after(&style, &qr, TEXT, 50).unwrap();
                println!(
                    "glass {render:?} {kind} {preset:?} v{version}: scannable from {at} ms of {total}"
                );
                assert!(
                    at <= total,
                    "{render:?} {kind} {preset:?} v{version}: {at} > {total}"
                );
            }
        }
    }
}

#[test]
fn measured_limits_are_refused_and_allowed_on_request() {
    for render in [BAKED, VECTOR] {
        let cases = [
            (glass(render).halo(0), "halo"),
            (glass(render).finders(0.6, 0.92), "finders"),
            (glass(render).targets(0.40, 0.75), "targets"),
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
                "{render:?} {label}: {refused:?}"
            );
            assert!(style.render(&qr).is_err());
            let allowed = style.allow_unscannable(true);
            let report = allowed.check(&qr);
            assert!(report.ok() && !report.scannable, "{label}: {report:?}");
            assert!(allowed.render(&qr).is_ok(), "{label}");
        }
    }
}
