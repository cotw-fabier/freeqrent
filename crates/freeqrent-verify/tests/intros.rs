//! Intros (SPEC §8): the document without its animation is the finished
//! code. Renderers that skip CSS animation (resvg here) must show the same
//! picture as the style without an intro, it must scan, and the plan's last
//! frame must match it too.

mod common;

use common::{PAYLOADS, assert_scans};
use freeqrent::{
    Ecc, Frame, FrameRing, Intro, Logo, ModuleShape, Preset, QrCode, QrMatrix, StaggerOrder, Style,
    Version,
};
use freeqrent_verify::{Gray, rasterize, scannable_after};

fn code(text: &str, version: u8) -> QrMatrix {
    QrCode::builder()
        .ecc(Ecc::M)
        .boost_ecc(false)
        .version(Version::new(version).unwrap())
        .encode_text(text)
        .unwrap()
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

fn shapes() -> [ModuleShape; 5] {
    [
        ModuleShape::SQUARE,
        ModuleShape::Square { gap: 0.2 },
        ModuleShape::Rounded { radius: 0.5 },
        ModuleShape::Dots { size: 0.85 },
        ModuleShape::Liquid { smoothness: 1.0 },
    ]
}

/// Asserts the animated document's static picture is the plain one, pixel
/// for pixel: groups are made of whole pieces, so the outlines are the same.
fn assert_same_picture(plain: &Style, animated: &Style, qr: &QrMatrix, label: &str) {
    let a = rasterize(&plain.render(qr).unwrap(), 8.0).unwrap();
    let b = rasterize(&animated.render(qr).unwrap(), 8.0).unwrap();
    let (count, worst) = diff(&a, &b);
    assert_eq!((count, worst), (0, 0), "{label}: static picture");
    let end = animated.plan(qr).unwrap().total_ms;
    let c = rasterize(&animated.render_frame(qr, end).unwrap(), 8.0).unwrap();
    let (count, worst) = diff(&b, &c);
    assert_eq!((count, worst), (0, 0), "{label}: last frame");
}

#[test]
fn static_state_is_the_plain_code() {
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 4 4"><circle cx="2" cy="2" r="2" fill="#c33"/></svg>"##)
            .unwrap()
            .scale(0.18);
    for shape in shapes() {
        for frame in [
            Frame::Square,
            Frame::Circle {
                filler_density: 0.6,
                ring: FrameRing::Dashed,
            },
        ] {
            for order in [StaggerOrder::Random(0), StaggerOrder::Radial] {
                let plain = Style::new().modules(shape).frame(frame);
                let intro = Intro::new(Preset::Settle).stagger(order);
                let animated = plain.clone().intro(intro.clone());
                for &(text, version) in &PAYLOADS[1..] {
                    let qr = code(text, version);
                    assert_same_picture(
                        &plain,
                        &animated,
                        &qr,
                        &format!("{shape:?} {frame:?} {order:?} v{version}"),
                    );
                }
            }
        }
    }
    let qr = QrCode::builder()
        .ecc(Ecc::H)
        .version(Version::new(6).unwrap())
        .encode_text("https://example.com/logo")
        .unwrap();
    let plain = Style::new().logo(logo);
    for preset in Preset::ALL {
        let animated = plain.clone().intro(preset);
        assert_same_picture(&plain, &animated, &qr, &format!("logo {preset:?}"));
    }
}

#[test]
fn every_preset_keeps_the_static_state() {
    // The reveal presets add a mask whose natural state covers the canvas;
    // the group presets split layers. Either way nothing may change.
    for preset in Preset::ALL {
        for shape in [
            ModuleShape::SQUARE,
            ModuleShape::Liquid { smoothness: 1.0 },
            ModuleShape::Bars {
                axis: freeqrent::Axis::Vertical,
                radius: 0.5,
            },
        ] {
            for frame in [
                Frame::Square,
                Frame::Circle {
                    filler_density: 0.5,
                    ring: FrameRing::Solid,
                },
            ] {
                let plain = Style::new().modules(shape).frame(frame);
                let animated = plain.clone().intro(preset);
                for &(text, version) in &PAYLOADS[1..3] {
                    let qr = code(text, version);
                    assert_same_picture(
                        &plain,
                        &animated,
                        &qr,
                        &format!("{preset:?} {shape:?} {frame:?} v{version}"),
                    );
                }
            }
        }
    }
}

#[test]
fn animated_codes_scan() {
    for preset in Preset::ALL {
        for shape in shapes() {
            let style = Style::new().modules(shape).intro(preset);
            assert_scans(&style, &format!("{preset:?} {shape:?}"));
        }
    }
}

#[test]
fn intros_scan_before_they_end() {
    // Measured, printed for the log: when the code is readable for good.
    for preset in Preset::ALL {
        for &(text, version) in &PAYLOADS {
            let qr = code(text, version);
            let style = Style::new().intro(preset);
            let total = style.plan(&qr).unwrap().total_ms;
            let at = scannable_after(&style, &qr, text, 50).unwrap();
            println!("{preset:?} v{version}: scannable from {at} ms of {total}");
            assert!(at <= total, "{preset:?} v{version}: {at} > {total}");
        }
    }
}
