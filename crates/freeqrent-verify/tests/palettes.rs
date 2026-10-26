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

/// Hand-written site styles in the shapes real sites use.
fn sites() -> Vec<(&'static str, bool, &'static str)> {
    vec![
        (
            "classic sheet",
            false,
            "body { background: #fdfcf9; color: #2b2b2b; font: 16px/1.5 serif }
             a { color: #b5442c } a:hover { color: #8e3320 }
             header { background: #fdfcf9; border-bottom: 1px solid #e6e1d8 }
             .button { background: #b5442c; color: #fff }",
        ),
        (
            "design tokens",
            false,
            ":root { --color-bg: oklch(98% .01 95); --color-text: oklch(28% .02 260);
                     --color-primary: oklch(62% .19 255); --radius: 8px;
                     --color-gray-100: #f4f4f5; --color-gray-900: #18181b }
             body { background: var(--color-bg); color: var(--color-text) }
             .btn-primary { background: var(--color-primary); color: white }
             @media (prefers-color-scheme: dark) {
               :root { --color-bg: #0b0b0f; --color-text: #e4e4e7 } }",
        ),
        (
            "dark first",
            false,
            "html { background: #0e1116; color: #e6edf3 } a { color: #4493f8 }
             .card { background: #161b22; border: 1px solid #30363d }",
        ),
        (
            "brand page",
            true,
            r##"<!doctype html><html><head><meta name="theme-color" content="#0f7b6c">
             <style>:root{--brand:#0f7b6c;--paper:#fbfbf8;--ink:#1f2328}
             body{background:var(--paper);color:var(--ink)} .cta{background:var(--brand)}</style>
             </head><body><h1 style="color:var(--ink)">Hello</h1>
             <a class="cta" style="background-color:#0f7b6c">Order</a></body></html>"##,
        ),
        (
            "colorless",
            true,
            "<html><body><p>No styles at all, but a red <span style=\"color:red\">word</span>.</p></body></html>",
        ),
    ]
}

#[test]
fn site_palettes_pass_and_scan() {
    let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
    for (name, html, text) in sites() {
        let (palette, report) = if html {
            Palette::from_html(text)
        } else {
            Palette::from_css(text)
        }
        .unwrap();
        println!("{name}: {:?}", report.choices);
        let check = Style::new().palette(palette.clone()).check(&qr);
        assert!(check.ok() && check.warnings.is_empty(), "{name}: {check:?}");
        let style = Style::new()
            .modules(ModuleShape::Rounded { radius: 0.4 })
            .palette(palette);
        assert_scans_at(&style, name, &PAYLOADS[..3], Ecc::M);
    }
}

#[test]
fn site_roles_follow_the_markup() {
    let (_, report) = Palette::from_css(sites()[0].2).unwrap();
    let color = |t: Token| {
        report
            .choices
            .iter()
            .find(|c| c.token == t)
            .map(|c| c.color.to_string())
    };
    assert_eq!(color(Token::Bg).as_deref(), Some("#fdfcf9"));
    assert_eq!(color(Token::Fg).as_deref(), Some("#2b2b2b"));
    // The link red keeps its hue as the eye.
    assert!(color(Token::FinderEye).is_some());
    // Named accents survive darkening.
    for (name, html, text) in [&sites()[1], &sites()[3]] {
        let (_, report) = if *html {
            Palette::from_html(text)
        } else {
            Palette::from_css(text)
        }
        .unwrap();
        assert!(
            report.choices.iter().any(|c| c.token == Token::FinderEye),
            "{name}: {report:?}"
        );
    }
    // A dark-first site still gets a light background: its light text
    // color, the only light swatch.
    let (palette, _) = Palette::from_css(sites()[2].2).unwrap();
    assert!(palette.background().to_oklab()[0] > 0.85);
}
