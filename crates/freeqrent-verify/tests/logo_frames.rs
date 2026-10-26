//! Logo sizes, frames, and the logo's own intros and idle loops (SPEC §7.3,
//! §8). Frames sit inside the plate the budget pays for, so the worst case
//! is the largest logo a code affords with the frame on it: it must scan.
//! The logo's motion never leaves the plate at rest or while it idles.

mod common;

use freeqrent::{
    Ecc, Idle, Intro, Logo, LogoFrame, LogoIntro, LogoShape, LogoSize, LogoTiming, ModuleShape,
    Preset, QrCode, QrMatrix, RenderError, Style, Version,
};
use freeqrent_verify::{Gray, Render, rasterize, verify_one, verify_style};

/// The worst case for a scanner: dark, with light stripes, no margin.
const DARK: &str = r##"<svg viewBox="0 0 10 10"><rect width="10" height="10" fill="#17161c"/><path d="M2 2h6v2h-6zM2 6h6v2h-6z" fill="#fefcf8"/></svg>"##;
const MARK: &str = r##"<svg viewBox="0 0 10 10"><rect width="10" height="10" rx="2" fill="#c2410c"/><path d="M3 2h4v2h-2v1h2v2h-2v1h-2z" fill="#fefcf8"/></svg>"##;
const TEXT: &str = "https://example.com/menu";
const SHAPES: [LogoShape; 3] = [
    LogoShape::Square,
    LogoShape::Rounded(1.5),
    LogoShape::Circle,
];

fn code(text: &str, version: u8, ecc: Ecc) -> QrMatrix {
    QrCode::builder()
        .ecc(ecc)
        .boost_ecc(false)
        .version(Version::new(version).unwrap())
        .encode_text(text)
        .unwrap()
}

fn fitted(style: &Style) -> QrMatrix {
    QrCode::builder().encode_text_for(TEXT, style).unwrap()
}

fn assert_verifies(style: &Style, qr: &QrMatrix, text: &str, label: &str) {
    let report = style.check(qr);
    assert!(report.ok(), "{label}: {report:?}");
    if let Err(e) = verify_style(style, qr, text) {
        panic!("{label} v{}: {e}", qr.version().number());
    }
}

#[test]
fn frames_scan_on_the_largest_logo() {
    for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
        for version in [4, 8, 15] {
            let text = format!("FRAMED {version}");
            let qr = code(&text, version, ecc);
            for shape in SHAPES {
                for frame in LogoFrame::ALL {
                    for (modules, backdrop) in [
                        (ModuleShape::SQUARE, true),
                        (ModuleShape::Dots { size: 0.85 }, false),
                    ] {
                        let logo = Logo::svg(DARK)
                            .unwrap()
                            .shape(shape)
                            .backdrop(backdrop)
                            .frame(frame)
                            .size(LogoSize::Largest)
                            .scale(0.05);
                        let style = Style::new().modules(modules).logo(logo);
                        let label = format!("{ecc:?} {shape:?} {frame:?} {modules:?}");
                        let report = style.check(&qr);
                        if !report.ok() {
                            // Version 4 at L can't afford a framed plate.
                            assert!(
                                matches!(report.errors[..], [RenderError::LogoTooLarge { .. }]),
                                "{label}: {report:?}"
                            );
                            continue;
                        }
                        assert!(report.logo_scale.unwrap() >= 0.05, "{label}");
                        assert_verifies(&style, &qr, &text, &label);
                    }
                }
            }
        }
    }
}

#[test]
fn frames_cost_budget_and_widths_are_checked() {
    let qr = code(TEXT, 8, Ecc::Q);
    let used = |logo: Logo| Style::new().logo(logo).check(&qr).ecc_budget_used;
    let plain = Logo::svg(MARK).unwrap().scale(0.16);
    for frame in LogoFrame::ALL {
        assert!(
            used(plain.clone().frame(frame)) >= used(plain.clone()),
            "{frame:?}"
        );
    }
    let wide = Style::new().logo(plain.clone().frame(LogoFrame::Ring).frame_width(3.0));
    // Out of range (and, as wide as that, too large for this code too).
    assert!(wide.check(&qr).errors.iter().any(|e| matches!(
        e,
        RenderError::OptionOutOfRange {
            option: "Logo.frame_width",
            ..
        }
    )));
    let moon = Style::new().logo(plain.frame(LogoFrame::Orbit).moon_size(4.5));
    // Out of range (and, as wide as that, too large for this code too).
    assert!(moon.check(&qr).errors.iter().any(|e| matches!(
        e,
        RenderError::OptionOutOfRange {
            option: "Logo.moon_size",
            ..
        }
    )));
}

#[test]
fn widths_and_moons_scan_at_their_limits() {
    for frame in LogoFrame::ALL {
        for width in [0.1, 2.5] {
            for moon in [0.4, 4.0] {
                let logo = Logo::svg(DARK)
                    .unwrap()
                    .shape(LogoShape::Circle)
                    .frame(frame)
                    .frame_width(width)
                    .moon_size(moon)
                    .size(LogoSize::Largest);
                let style = Style::new().logo(logo);
                let qr = fitted(&style);
                assert_verifies(&style, &qr, TEXT, &format!("{frame:?} {width} {moon}"));
            }
        }
    }
}

/// Pixels that differ by more than a rounding step.
fn differs(a: &Gray, b: &Gray) -> usize {
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.pixels
        .iter()
        .zip(&b.pixels)
        .filter(|(p, q)| p.abs_diff(**q) > 2)
        .count()
}

#[test]
fn logo_intros_end_on_the_static_code() {
    for intro in LogoIntro::ALL {
        for timing in LogoTiming::ALL {
            for preset in [
                Preset::Settle,
                Preset::LockOn,
                Preset::Iris,
                Preset::Develop,
            ] {
                for (shape, frame) in [
                    (LogoShape::Circle, LogoFrame::Ring),
                    (LogoShape::Square, LogoFrame::Brackets),
                    (LogoShape::Rounded(1.5), LogoFrame::Beads),
                ] {
                    let logo = Logo::svg(MARK).unwrap().shape(shape).frame(frame);
                    let plain = Style::new().logo(logo);
                    let animated = plain
                        .clone()
                        .intro(Intro::new(preset).logo(intro).logo_timing(timing));
                    let label = format!("{intro:?} {timing:?} {preset:?} {frame:?}");
                    let qr = fitted(&plain);
                    let a = rasterize(&plain.render(&qr).unwrap(), 6.0).unwrap();
                    let b = rasterize(&animated.render(&qr).unwrap(), 6.0).unwrap();
                    assert_eq!(differs(&a, &b), 0, "{label}: static picture");
                    let plan = animated.plan(&qr).unwrap();
                    assert!(plan.total_ms <= 1200, "{label}: {}", plan.total_ms);
                    let end = animated.render_frame(&qr, plan.total_ms).unwrap();
                    let c = rasterize(&end, 6.0).unwrap();
                    assert_eq!(differs(&b, &c), 0, "{label}: last frame");
                    assert_verifies(&animated, &qr, TEXT, &label);
                }
            }
        }
    }
}

#[test]
fn cut_loose_frames_end_on_the_static_code() {
    for frame_intro in LogoIntro::ALL {
        for timing in LogoTiming::ALL {
            for (logo_intro, preset) in [
                (LogoIntro::Shrink, Preset::Settle),
                (LogoIntro::Ink, Preset::Iris),
                (LogoIntro::None, Preset::Develop),
            ] {
                for (shape, frame) in [
                    (LogoShape::Circle, LogoFrame::Ring),
                    (LogoShape::Square, LogoFrame::Brackets),
                ] {
                    let logo = Logo::svg(MARK).unwrap().shape(shape).frame(frame);
                    let plain = Style::new().logo(logo);
                    let animated = plain.clone().intro(
                        Intro::new(preset)
                            .logo(logo_intro)
                            .frame(frame_intro)
                            .frame_timing(timing),
                    );
                    let label =
                        format!("{logo_intro:?} + {frame_intro:?} {timing:?} {preset:?} {frame:?}");
                    let qr = fitted(&plain);
                    let a = rasterize(&plain.render(&qr).unwrap(), 6.0).unwrap();
                    let b = rasterize(&animated.render(&qr).unwrap(), 6.0).unwrap();
                    assert_eq!(differs(&a, &b), 0, "{label}: static picture");
                    let plan = animated.plan(&qr).unwrap();
                    assert!(plan.total_ms <= 1200, "{label}: {}", plan.total_ms);
                    let end = animated.render_frame(&qr, plan.total_ms).unwrap();
                    let c = rasterize(&end, 6.0).unwrap();
                    assert_eq!(differs(&b, &c), 0, "{label}: last frame");
                    assert_verifies(&animated, &qr, TEXT, &label);
                }
            }
        }
    }
}

#[test]
fn cut_loose_frames_scan_before_they_end() {
    // ZXing at 6 px per module, every 50 ms (SPEC §8.1).
    for (logo_intro, frame_intro, timing) in [
        (LogoIntro::Shrink, LogoIntro::Draw, LogoTiming::After),
        (LogoIntro::Shrink, LogoIntro::Spin, LogoTiming::After),
        (LogoIntro::Ink, LogoIntro::Spin, LogoTiming::With),
        (LogoIntro::Stamp, LogoIntro::Lock, LogoTiming::Before),
        (LogoIntro::Spin, LogoIntro::Shrink, LogoTiming::After),
        (LogoIntro::Reveal, LogoIntro::None, LogoTiming::After),
    ] {
        for version in [5, 10] {
            let logo = Logo::svg(MARK)
                .unwrap()
                .shape(LogoShape::Circle)
                .frame(LogoFrame::Ring)
                .scale(0.14);
            let style = Style::new().logo(logo).intro(
                Intro::new(Preset::Settle)
                    .logo(logo_intro)
                    .frame(frame_intro)
                    .frame_timing(timing),
            );
            let qr = code(TEXT, version, Ecc::Q);
            let total = style.plan(&qr).unwrap().total_ms;
            let at = freeqrent_verify::scannable_after(&style, &qr, TEXT, 50).unwrap();
            let label = format!("{logo_intro:?} + {frame_intro:?} {timing:?} v{version}");
            println!("{label}: scannable from {at} ms of {total}");
            assert!(at <= total, "{label}: {at} > {total}");
        }
    }
}

#[test]
fn idles_scan_all_the_way_round() {
    let render = Render {
        px_per_module: 6,
        blur: false,
        noise: false,
    };
    for (shape, frame) in [
        (LogoShape::Circle, LogoFrame::Brackets),
        (LogoShape::Circle, LogoFrame::Stamp),
        (LogoShape::Square, LogoFrame::Orbit),
    ] {
        let logo = Logo::svg(DARK).unwrap().shape(shape).frame(frame);
        let style = Style::new()
            .logo(logo.size(LogoSize::Largest))
            .intro(Intro::new(Preset::Settle))
            .idle(Idle::LogoTurn)
            .idle(Idle::LogoGlint)
            .idle(Idle::LogoSpin)
            .idle(Idle::LogoFlip);
        let qr = fitted(&style);
        let label = format!("{shape:?} {frame:?}");
        assert_verifies(&style, &qr, TEXT, &label);
        // Every eighth of a turn, the glint at every stage of its pass,
        // and the spin and the flip all the way through.
        let start = style.plan(&qr).unwrap().total_ms;
        let times = (0..8)
            .map(|k| start + k * 3000)
            .chain((0..8).map(|k| start + k * 175))
            .chain((0..13).map(|k| start + k * 100))
            .chain((0..11).map(|k| start + 4000 + k * 100));
        for t in times {
            let frame = style.render_frame(&qr, t).unwrap();
            if let Err(e) = verify_one(&frame, TEXT, render, true) {
                panic!("{label} at {t} ms: {e}");
            }
        }
    }
}

#[test]
fn only_round_frames_turn() {
    let qr = code(TEXT, 8, Ecc::Q);
    let turn = |logo: Option<Logo>| {
        let style = Style::new().idle(Idle::LogoTurn);
        match logo {
            Some(logo) => style.logo(logo.scale(0.16)),
            None => style,
        }
        .check(&qr)
        .errors
    };
    let mark = || Logo::svg(MARK).unwrap();
    assert_eq!(turn(None), [RenderError::TurnNeedsRoundFrame]);
    assert_eq!(turn(Some(mark())), [RenderError::TurnNeedsRoundFrame]);
    assert_eq!(
        turn(Some(mark().frame(LogoFrame::Ring))),
        [RenderError::TurnNeedsRoundFrame]
    );
    assert!(turn(Some(mark().frame(LogoFrame::Orbit))).is_empty());
    assert!(turn(Some(mark().frame(LogoFrame::Ring).shape(LogoShape::Circle))).is_empty());
}

#[test]
fn logo_intros_scan_before_they_end() {
    // ZXing at 6 px per module, every 50 ms (SPEC §8.1).
    for intro in LogoIntro::ALL {
        for timing in LogoTiming::ALL {
            for version in [5, 10] {
                let logo = Logo::svg(MARK)
                    .unwrap()
                    .shape(LogoShape::Circle)
                    .frame(LogoFrame::Ring)
                    .scale(0.14);
                let style = Style::new()
                    .logo(logo)
                    .intro(Intro::new(Preset::Settle).logo(intro).logo_timing(timing));
                let qr = code(TEXT, version, Ecc::Q);
                let total = style.plan(&qr).unwrap().total_ms;
                let at = freeqrent_verify::scannable_after(&style, &qr, TEXT, 50).unwrap();
                println!("{intro:?} {timing:?} v{version}: scannable from {at} ms of {total}");
                assert!(
                    at <= total,
                    "{intro:?} {timing:?} v{version}: {at} > {total}"
                );
            }
        }
    }
}

#[test]
fn faint_frames_scan_at_their_floor() {
    // The frame isn't read, so it only needs FRAME_FLOOR: at exactly that,
    // gray and orange, on the largest logo each code affords, light and
    // dark twin, it scans at the full bar (W20261026160831).
    use freeqrent::{Color, FRAME_FLOOR, Palette, Token};
    for hue in [(0.0, 0.0), (0.12, 50.0)] {
        let mut palette = Palette::new();
        let mut l = 1.0;
        while palette.contrast(Token::LogoFrame).delta_l < FRAME_FLOOR {
            l -= 0.002;
            let color = Color::oklch(l, hue.0, hue.1);
            palette.set(Token::LogoFrame, color).unwrap();
        }
        for dark in [false, true] {
            let palette = if dark {
                palette.dark_twin()
            } else {
                palette.clone()
            };
            for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
                for version in [4, 8, 15] {
                    let text = format!("FRAMED {version}");
                    let qr = code(&text, version, ecc);
                    for shape in [LogoShape::Square, LogoShape::Circle] {
                        for frame in LogoFrame::ALL {
                            let logo = Logo::svg(DARK)
                                .unwrap()
                                .shape(shape)
                                .frame(frame)
                                .size(LogoSize::Largest)
                                .scale(0.05);
                            let style = Style::new()
                                .logo(logo)
                                .palette(palette.clone())
                                .allow_inverted(dark);
                            let report = style.check(&qr);
                            if report
                                .errors
                                .iter()
                                .any(|e| matches!(e, RenderError::LogoTooLarge { .. }))
                            {
                                continue;
                            }
                            let label = format!("{hue:?} dark={dark} {ecc:?} {shape:?} {frame:?}");
                            assert!(report.ok(), "{label}: {report:?}");
                            if let Err(e) = verify_style(&style, &qr, &text) {
                                panic!("{label} v{version}: {e}");
                            }
                        }
                    }
                }
            }
        }
    }
}
