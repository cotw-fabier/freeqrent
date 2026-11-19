//! Shared round-trip helpers.

// Each test binary uses its own subset.
#![allow(dead_code)]

use freeqrent::{Ecc, QrCode, Style, Version};

/// Payloads pinned to versions with 0, 1, 6 and 22 alignment patterns.
pub const PAYLOADS: [(&str, u8); 4] = [
    ("HELLO", 1),
    ("https://example.com/menu", 5),
    (
        "https://example.com/order/12345678901234567890?ref=freeqrent",
        10,
    ),
    (
        "A longer payload that needs a larger symbol to hold it; version 25 at ECC M.",
        25,
    ),
];

/// Renders `style` for every payload in `PAYLOADS` and checks it against
/// the style's own bar (`Bar::for_report`).
pub fn assert_scans(style: &Style, label: &str) {
    assert_scans_at(style, label, &PAYLOADS, Ecc::M);
}

/// [`assert_scans`] for chosen payloads and ECC level.
pub fn assert_scans_at(style: &Style, label: &str, payloads: &[(&str, u8)], ecc: Ecc) {
    for &(text, version) in payloads {
        let qr = QrCode::builder()
            .ecc(ecc)
            .boost_ecc(false)
            .version(Version::new(version).unwrap())
            .encode_text(text)
            .unwrap();
        let result = freeqrent_verify::verify_style(style, &qr, text);
        if let Err(e) = result {
            panic!("{label} v{version}: {e}");
        }
    }
}

/// Synthetic pictures for halftone tests, `side` pixels square.
pub fn picture(kind: &str, side: u32) -> freeqrent::Rgba8Image {
    let mut img = freeqrent::Rgba8Image::filled(side, side, [0, 0, 0, 255]).unwrap();
    let s = side as f64;
    for y in 0..side {
        for x in 0..side {
            let (u, v) = (x as f64 / s, y as f64 / s);
            let (dx, dy) = (u - 0.5, v - 0.5);
            let r = (dx * dx + dy * dy).sqrt();
            let px: [u8; 4] = match kind {
                // A dark disc on a light page.
                "disc" => {
                    if r < 0.3 {
                        [30, 40, 90, 255]
                    } else {
                        [245, 240, 230, 255]
                    }
                }
                // Left to right, black to white.
                "gradient" => {
                    let g = (u * 255.0) as u8;
                    [g, g, g, 255]
                }
                // Concentric rings, smooth.
                "rings" => {
                    let g = (((r * 40.0).sin() * 0.5 + 0.5) * 255.0) as u8;
                    [g, (g as f64 * 0.8) as u8, 255 - g / 2, 255]
                }
                // Soft blobs, like an out-of-focus photo, in color.
                "photo" => {
                    let a = (u * 7.0).sin() * (v * 5.0 + 1.0).cos();
                    let b = ((u + v) * 4.0).sin();
                    let l = ((a * 0.35 + b * 0.25 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
                    [l, (l as f64 * 0.6 + 60.0 * u) as u8, (200.0 * v) as u8, 255]
                }
                // A logo shape on transparency: a dark ring.
                _ => {
                    if (0.25..0.4).contains(&r) {
                        [20, 20, 20, 255]
                    } else {
                        [0, 0, 0, 0]
                    }
                }
            };
            img.set_pixel(x, y, px);
        }
    }
    img
}

/// The seven pictures glass is measured on (SPEC §7.9): four from
/// [`picture`] and three photograph-like ones with fine texture.
pub const PHOTOS: [&str; 7] = [
    "disc",
    "gradient",
    "rings",
    "photo",
    "face",
    "landscape",
    "dusk",
];

/// A picture from [`PHOTOS`], `side` pixels square.
pub fn photo(kind: &str, side: u32) -> freeqrent::Rgba8Image {
    if ["disc", "gradient", "rings", "photo"].contains(&kind) {
        return picture(kind, side);
    }
    let mut img = freeqrent::Rgba8Image::filled(side, side, [0, 0, 0, 255]).unwrap();
    let s = side as f64;
    for y in 0..side {
        for x in 0..side {
            let (u, v) = ((x as f64 + 0.5) / s, (y as f64 + 0.5) / s);
            let c = texture(kind, u, v, synth(kind, u, v));
            let c = c.map(|v| v.round().clamp(0.0, 255.0) as u8);
            img.set_pixel(x, y, [c[0], c[1], c[2], 255]);
        }
    }
    img
}

/// A picture's pixels with its PNG file, as a decoder would hand them over
/// (the file goes to `Halftone::source`).
pub fn photo_with_file(kind: &str, side: u32) -> (freeqrent::Rgba8Image, Vec<u8>) {
    let image = photo(kind, side);
    let file = freeqrent::png::encode_rgba(&image);
    (image, file)
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    core::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// Photograph-like synthetic pictures (sRGB 0–255), deterministic.
fn synth(kind: &str, u: f64, v: f64) -> [f64; 3] {
    match kind {
        // A face: mid-tone skin on a darker ground, hair, eyes, mouth,
        // shoulders in a mid-tone shirt.
        "face" => {
            let bg = mix([35.0, 32.0, 48.0], [85.0, 75.0, 95.0], v);
            let vign = 1.0 - 0.35 * ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt();
            let mut c = bg.map(|x| x * vign);
            let sh = ((u - 0.5) / 0.42).powi(2) + ((v - 1.05) / 0.3).powi(2);
            c = mix(c, [95.0, 115.0, 145.0], 1.0 - smooth(0.9, 1.0, sh));
            let neck = ((u - 0.5) / 0.08).powi(2) + ((v - 0.78) / 0.12).powi(2);
            c = mix(c, [180.0, 135.0, 110.0], 1.0 - smooth(0.85, 1.0, neck));
            let hair = ((u - 0.5) / 0.26).powi(2) + ((v - 0.40) / 0.32).powi(2);
            c = mix(c, [55.0, 38.0, 28.0], 1.0 - smooth(0.9, 1.0, hair));
            let face = ((u - 0.5) / 0.2).powi(2) + ((v - 0.5) / 0.27).powi(2);
            let shade = 1.1 - 0.35 * (u - 0.3).max(0.0) - 0.15 * (v - 0.3).max(0.0);
            let skin = [205.0 * shade, 160.0 * shade, 130.0 * shade];
            c = mix(c, skin, 1.0 - smooth(0.88, 1.0, face));
            for ex in [0.43, 0.57] {
                let eye = ((u - ex) / 0.035).powi(2) + ((v - 0.45) / 0.018).powi(2);
                c = mix(c, [40.0, 30.0, 30.0], 1.0 - smooth(0.7, 1.0, eye));
                let brow = ((u - ex) / 0.045).powi(2) + ((v - 0.41) / 0.009).powi(2);
                c = mix(c, [70.0, 50.0, 40.0], 1.0 - smooth(0.7, 1.0, brow));
            }
            let mouth = ((u - 0.5) / 0.07).powi(2) + ((v - 0.63) / 0.018).powi(2);
            c = mix(c, [150.0, 75.0, 75.0], 1.0 - smooth(0.7, 1.0, mouth));
            let nose = ((u - 0.52) / 0.02).powi(2) + ((v - 0.55) / 0.04).powi(2);
            mix(
                c,
                [165.0, 120.0, 100.0],
                0.6 * (1.0 - smooth(0.5, 1.0, nose)),
            )
        }
        // A landscape: sky, sun, far and near hills.
        "landscape" => {
            let mut c = mix(
                [110.0, 160.0, 225.0],
                [240.0, 220.0, 200.0],
                smooth(0.0, 0.55, v),
            );
            let sun = ((u - 0.72).powi(2) + (v - 0.28).powi(2)).sqrt();
            c = mix(
                c,
                [255.0, 245.0, 210.0],
                (1.0 - smooth(0.06, 0.14, sun)) * 0.9,
            );
            let far = 0.48 + 0.05 * (u * 4.0 + 1.0).sin() + 0.02 * (u * 11.0).sin();
            c = mix(
                c,
                [110.0, 125.0, 150.0],
                smooth(far - 0.005, far + 0.005, v),
            );
            let near = 0.6 + 0.07 * (u * 6.0).sin() + 0.03 * (u * 13.0 + 2.0).sin();
            let ground = mix(
                [80.0, 120.0, 60.0],
                [25.0, 55.0, 30.0],
                smooth(near, 1.0, v),
            );
            mix(c, ground, smooth(near - 0.005, near + 0.005, v))
        }
        // Low-contrast mid-tones: soft purple and orange blobs.
        _ => {
            let a = (u * 5.0 + 0.5).sin() * (v * 4.0).cos() * 0.5 + 0.5;
            let b = ((u - v) * 3.0).sin() * 0.5 + 0.5;
            let c = mix([120.0, 95.0, 140.0], [200.0, 140.0, 100.0], a);
            mix(c, [150.0, 150.0, 160.0], b * 0.4)
        }
    }
}

/// Hashed lattice value noise in [-1, 1], smooth, `f` cells across.
fn vnoise(u: f64, v: f64, f: f64, seed: u64) -> f64 {
    let (x, y) = (u * f, v * f);
    let (xi, yi) = (x.floor() as i64, y.floor() as i64);
    let (tx, ty) = (x - xi as f64, y - yi as f64);
    let h = |i: i64, j: i64| {
        let mut z = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
        z ^= z >> 31;
        z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z ^= z >> 29;
        (z % 20001) as f64 / 10000.0 - 1.0
    };
    let sm = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (sm(tx), sm(ty));
    let a = h(xi, yi) + (h(xi + 1, yi) - h(xi, yi)) * sx;
    let b = h(xi, yi + 1) + (h(xi + 1, yi + 1) - h(xi, yi + 1)) * sx;
    a + (b - a) * sy
}

fn fbm(u: f64, v: f64, f: f64, seed: u64) -> f64 {
    vnoise(u, v, f, seed) * 0.6
        + vnoise(u, v, f * 2.1, seed + 1) * 0.3
        + vnoise(u, v, f * 4.3, seed + 2) * 0.1
}

/// Fine photographic detail: hair strands, skin and fabric grain, grass,
/// cloud wisps.
fn texture(kind: &str, u: f64, v: f64, c: [f64; 3]) -> [f64; 3] {
    let add = |c: [f64; 3], d: f64| c.map(|x| x + d);
    let grain = fbm(u, v, 420.0, 7) * 5.0;
    match kind {
        "face" => {
            let hair = ((u - 0.5) / 0.26).powi(2) + ((v - 0.40) / 0.32).powi(2);
            let face = ((u - 0.5) / 0.2).powi(2) + ((v - 0.5) / 0.27).powi(2);
            let mut c = add(c, grain);
            if hair < 1.0 && face > 0.9 {
                let s = ((u * 900.0 + fbm(u, v, 30.0, 3) * 40.0).sin() * 0.5 + 0.5).powi(3);
                c = add(c, s * 28.0 - 8.0);
            }
            if face < 1.0 {
                c = add(c, fbm(u, v, 260.0, 11) * 7.0);
            }
            if v > 0.78 {
                let weave = (u * 700.0).sin() * (v * 700.0).sin();
                c = add(c, weave * 9.0);
            }
            c
        }
        "landscape" => {
            let near = 0.6 + 0.07 * (u * 6.0).sin() + 0.03 * (u * 13.0 + 2.0).sin();
            let far = 0.48 + 0.05 * (u * 4.0 + 1.0).sin() + 0.02 * (u * 11.0).sin();
            if v > near {
                add(c, fbm(u * 3.0, v * 0.6, 200.0, 5) * 26.0)
            } else if v > far {
                add(c, fbm(u, v, 140.0, 9) * 14.0)
            } else {
                let wisps = (fbm(u * 0.4, v * 2.0, 18.0, 13) - 0.1).max(0.0) * 60.0;
                add(add(c, wisps), grain * 0.5)
            }
        }
        _ => add(c, grain + fbm(u, v, 90.0, 17) * 6.0),
    }
}
