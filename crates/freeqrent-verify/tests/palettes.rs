//! Extracted palettes (SPEC §6.4): whatever the source, the palette clears
//! the contrast rules without warnings, and codes drawn with it scan.

mod common;

use common::{PAYLOADS, assert_scans_at};
use freeqrent::{Ecc, ModuleShape, Palette, PaletteOpts, QrCode, Rgba8Image, Style, Token};

/// Builds a `w × h` image from a function of the pixel position.
fn image(w: u32, h: u32, f: impl Fn(f64, f64) -> [u8; 4]) -> Rgba8Image {
    let mut img = Rgba8Image::filled(w, h, [0; 4]).unwrap();
    for y in 0..h {
        for x in 0..w {
            img.set_pixel(x, y, f(x as f64 / w as f64, y as f64 / h as f64));
        }
    }
    img
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 4] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    opaque([r, g, b].map(|t| (t + m) * 255.0))
}

fn opaque(rgb: [f64; 3]) -> [u8; 4] {
    let [r, g, b] = rgb.map(|v| v.round().clamp(0.0, 255.0) as u8);
    [r, g, b, 255]
}

/// Synthetic sources covering the cases the role assigner branches on.
fn corpus() -> Vec<(&'static str, Rgba8Image)> {
    vec![
        (
            "brand",
            image(120, 80, |x, y| {
                if (x - 0.3).hypot(y - 0.5) < 0.2 {
                    [232, 84, 28, 255]
                } else if x > 0.55 && ((y * 10.0) as u32).is_multiple_of(3) {
                    [24, 36, 92, 255]
                } else {
                    [250, 247, 240, 255]
                }
            }),
        ),
        (
            "transparent logo",
            image(64, 64, |x, y| {
                if (x - 0.5).hypot(y - 0.5) < 0.35 {
                    [0, 140, 120, 255]
                } else {
                    [0, 0, 0, 0]
                }
            }),
        ),
        (
            "dark theme",
            image(90, 90, |x, y| {
                if y < 0.15 {
                    [0, 200, 255, 255]
                } else if ((x * 20.0) as u32).is_multiple_of(4) {
                    [240, 240, 245, 255]
                } else {
                    [16, 18, 24, 255]
                }
            }),
        ),
        (
            "hue wheel",
            image(100, 100, |x, y| {
                hsv((y - 0.5).atan2(x - 0.5) / std::f64::consts::TAU, 0.9, 0.95)
            }),
        ),
        (
            "pastel",
            image(80, 40, |x, _| {
                if x < 0.5 {
                    [255, 230, 240, 255]
                } else {
                    [210, 235, 255, 255]
                }
            }),
        ),
        (
            "yellow",
            Rgba8Image::filled(30, 30, [255, 214, 0, 255]).unwrap(),
        ),
        (
            "mid gray",
            Rgba8Image::filled(30, 30, [128, 128, 128, 255]).unwrap(),
        ),
        (
            "photo",
            image(200, 150, |x, y| {
                let sky = [90.0 + 120.0 * y, 150.0 + 80.0 * y, 230.0];
                let ground = [70.0 + 60.0 * x, 110.0, 40.0];
                let t = if y > 0.6 + 0.1 * (x * 9.0).sin() {
                    ground
                } else {
                    sky
                };
                let n = (x * 7919.0 + y * 104_729.0).sin() * 12.0;
                opaque(t.map(|v| v + n))
            }),
        ),
    ]
}

#[test]
fn extracted_palettes_pass_without_warnings() {
    let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
    for (name, img) in corpus() {
        for opts in [
            PaletteOpts::new(),
            PaletteOpts::new().seed(9).clusters(3),
            PaletteOpts::new().clusters(12).min_contrast(0.75),
        ] {
            let (palette, report) = Palette::from_image(&img, opts).unwrap();
            println!("{name} {opts:?}: {:?}", report.choices);
            for token in [Token::Fg, Token::FinderEye] {
                let c = palette.contrast(token);
                assert!(
                    !c.inverted && c.delta_l >= opts.contrast(),
                    "{name} {token}: {c:?}"
                );
            }
            let report = Style::new().palette(palette).check(&qr);
            assert!(
                report.ok() && report.warnings.is_empty(),
                "{name}: {report:?}"
            );
        }
    }
}

#[test]
fn extracted_palettes_scan() {
    for (name, img) in corpus() {
        let (palette, _) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        for shape in [ModuleShape::SQUARE, ModuleShape::Dots { size: 0.85 }] {
            let style = Style::new().modules(shape).palette(palette.clone());
            assert_scans_at(&style, &format!("{name} {shape:?}"), &PAYLOADS[..3], Ecc::M);
        }
    }
}

#[test]
fn extraction_is_deterministic() {
    for (name, img) in corpus() {
        let a = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        let b = Palette::from_image(&img.clone(), PaletteOpts::new()).unwrap();
        assert_eq!(a, b, "{name}");
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let svg = |p: &Palette| Style::new().palette(p.clone()).render(&qr).unwrap();
        assert_eq!(svg(&a.0), svg(&b.0), "{name}");
    }
}
