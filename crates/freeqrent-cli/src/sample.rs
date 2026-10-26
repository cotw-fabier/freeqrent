//! The picture `--halftone sample` and `--art sample` use: a sunset over
//! layered hills, drawn here so the CLI needs no image file.

use freeqrent::Rgba8Image;

/// Side of the sample picture, in pixels.
const SIDE: u32 = 160;

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// A sky from dusk blue to orange, a low sun, and three ridges darkening
/// toward the front.
pub fn picture() -> Rgba8Image {
    const TOP: [f64; 3] = [38.0, 48.0, 104.0];
    const GLOW: [f64; 3] = [244.0, 150.0, 92.0];
    const SUN: [f64; 3] = [255.0, 226.0, 150.0];
    const RIDGES: [([f64; 3], f64, f64, f64); 3] = [
        ([150.0, 88.0, 112.0], 0.62, 0.05, 2.1),
        ([84.0, 52.0, 92.0], 0.72, 0.07, 3.3),
        ([30.0, 24.0, 48.0], 0.84, 0.06, 5.2),
    ];
    let mut image = Rgba8Image::filled(SIDE, SIDE, [0, 0, 0, 255]).expect("a fixed size");
    let s = SIDE as f64;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let (u, v) = (x as f64 / s, y as f64 / s);
            let mut c = mix(TOP, GLOW, (v / 0.7).powf(1.6));
            let (dx, dy) = (u - 0.62, v - 0.56);
            let r = (dx * dx + dy * dy).sqrt();
            if r < 0.12 {
                c = SUN;
            } else if r < 0.2 {
                c = mix(SUN, c, (r - 0.12) / 0.08);
            }
            for (ridge, base, height, waves) in RIDGES {
                let top = base - height * (0.5 + 0.5 * (u * waves * 3.0 + base * 9.0).sin());
                if v > top {
                    c = ridge;
                }
            }
            let [r, g, b] = c.map(|v| v.round() as u8);
            image.set_pixel(x, y, [r, g, b, 255]);
        }
    }
    image
}
