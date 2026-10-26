//! Image-embedded codes (SPEC §7.7): a picture mapped onto the code's
//! modules. This module samples it; [`steer`](crate::steer) chooses the
//! free data bits so modules follow it.

use alloc::vec;
use alloc::vec::Vec;

use crate::color::{Color, linear_to_srgb, srgb_to_linear, srgb_to_oklab};
use crate::image::Rgba8Image;
use crate::math;
use crate::matrix::ModuleRole;
use crate::steer::Want;

/// How a picture fills the code's square.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Scaled to cover the square, cropped around the focal point.
    #[default]
    Cover,
    /// Scaled to fit inside the square; the rest is background. The focal
    /// point places it in the leftover space.
    Contain,
}

/// A picture to build the code from (SPEC §7.7). Plain data: the image is
/// read when a code is encoded or drawn.
///
/// ```
/// use freeqrent::{Ecc, Halftone, QrCode, Rgba8Image, Style, Version};
///
/// let image = Rgba8Image::filled(32, 32, [20, 20, 30, 255]).unwrap();
/// let style = Style::new().halftone(Halftone::new(image));
/// let qr = QrCode::builder()
///     .ecc(Ecc::M)
///     .version(Version::new(7).unwrap())
///     .encode_text_for("https://example.com", &style)
///     .unwrap();
/// let steering = style.check(&qr).steering.unwrap();
/// assert!(steering.matched_modules > steering.code_modules * 3 / 4);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Halftone {
    pub(crate) image: Rgba8Image,
    pub(crate) fit: Fit,
    pub(crate) focal: (f32, f32),
    pub(crate) priority: Option<Rgba8Image>,
    pub(crate) steer: bool,
}

impl Halftone {
    /// The picture, covering the code, centered, with steering on.
    pub fn new(image: Rgba8Image) -> Halftone {
        Halftone {
            image,
            fit: Fit::Cover,
            focal: (0.5, 0.5),
            priority: None,
            steer: true,
        }
    }

    /// How the picture fills the square (default `Cover`).
    pub fn fit(mut self, fit: Fit) -> Halftone {
        self.fit = fit;
        self
    }

    /// The point to keep in view, as fractions of the picture's width and
    /// height (default the center; clamped to 0–1).
    pub fn focal(mut self, x: f32, y: f32) -> Halftone {
        let unit = |v: f32| if v.is_nan() { 0.5 } else { v.clamp(0.0, 1.0) };
        self.focal = (unit(x), unit(y));
        self
    }

    /// Which parts of the picture matter most, laid out like the picture:
    /// light areas are matched first, black ones not at all. By default
    /// strong lights and darks, edges and the center come first.
    pub fn priority(mut self, map: Rgba8Image) -> Halftone {
        self.priority = Some(map);
        self
    }

    /// Whether the encoder chooses the free data bits to match the picture
    /// (default on). Off keeps the standard pad bytes, for environments
    /// that insist on them (SPEC §7.7, `strict_padding`).
    pub fn steer(mut self, steer: bool) -> Halftone {
        self.steer = steer;
        self
    }

    /// The picture.
    pub fn image(&self) -> &Rgba8Image {
        &self.image
    }

    /// How it fills the square.
    pub fn fit_mode(&self) -> Fit {
        self.fit
    }

    /// The focal point.
    pub fn focal_point(&self) -> (f32, f32) {
        self.focal
    }

    /// Whether steering is on.
    pub fn steers(&self) -> bool {
        self.steer
    }
}

/// How steering went for a code, in [`ScanReport::steering`](crate::ScanReport::steering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Steering {
    /// Modules whose value the encoder could choose: the free data bits
    /// after the payload (0 with steering off).
    pub steerable_modules: usize,
    /// Data and error-correction modules.
    pub code_modules: usize,
    /// Of those, how many match the picture's light and dark.
    pub matched_modules: usize,
}

/// A cell's color: linear sRGB, premultiplied, and coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Cell {
    rgb: [f64; 3],
    alpha: f64,
}

impl Cell {
    /// The cell over `backdrop` (opaque), as sRGB.
    pub(crate) fn over(self, backdrop: Color) -> [f64; 3] {
        let bg = backdrop.to_srgb().map(srgb_to_linear);
        core::array::from_fn(|i| {
            linear_to_srgb((self.rgb[i] + (1.0 - self.alpha) * bg[i]).clamp(0.0, 1.0))
        })
    }
}

/// Samples `image` onto an `n` × `n` grid laid over the square as `fit`
/// and `focal` place it: box averages where cells cover several pixels,
/// bilinear where they're smaller than one.
pub(crate) fn sample(image: &Rgba8Image, fit: Fit, focal: (f32, f32), n: usize) -> Vec<Cell> {
    let (w, h) = (image.width() as f64, image.height() as f64);
    // The square in image pixels: its side and top-left corner.
    let (side, x0, y0) = match fit {
        Fit::Cover => {
            let s = w.min(h);
            let at = |len: f64, f: f32| (f as f64 * len - s / 2.0).clamp(0.0, len - s);
            (s, at(w, focal.0), at(h, focal.1))
        }
        Fit::Contain => {
            let s = w.max(h);
            (s, -(s - w) * focal.0 as f64, -(s - h) * focal.1 as f64)
        }
    };
    let linear: Vec<f64> = (0..=255u8)
        .map(|c| srgb_to_linear(c as f64 / 255.0))
        .collect();
    let data = image.data();
    let (iw, ih) = (image.width() as i64, image.height() as i64);
    // Premultiplied linear RGBA of a pixel; transparent outside.
    let px = |x: i64, y: i64| -> [f64; 4] {
        if x < 0 || y < 0 || x >= iw || y >= ih {
            return [0.0; 4];
        }
        let i = (y * iw + x) as usize * 4;
        let a = data[i + 3] as f64 / 255.0;
        [
            linear[data[i] as usize] * a,
            linear[data[i + 1] as usize] * a,
            linear[data[i + 2] as usize] * a,
            a,
        ]
    };
    let step = side / n as f64;
    let mut cells = Vec::with_capacity(n * n);
    for cy in 0..n {
        for cx in 0..n {
            let (left, top) = (x0 + cx as f64 * step, y0 + cy as f64 * step);
            let mut sum = [0.0f64; 4];
            if step < 1.0 {
                // Bilinear at the cell's center, between pixel centers.
                let (fx, fy) = (left + step / 2.0 - 0.5, top + step / 2.0 - 0.5);
                let (ix, iy) = (math::floor(fx), math::floor(fy));
                let (tx, ty) = (fx - ix, fy - iy);
                let clamp = |v: f64, len: i64| (v as i64).clamp(0, len - 1);
                for (dx, dy, k) in [
                    (0, 0, (1.0 - tx) * (1.0 - ty)),
                    (1, 0, tx * (1.0 - ty)),
                    (0, 1, (1.0 - tx) * ty),
                    (1, 1, tx * ty),
                ] {
                    // Inside the picture, edges extend; outside, it's empty.
                    let (x, y) = (ix as i64 + dx, iy as i64 + dy);
                    let inside = |v: f64, len: f64| v >= 0.0 && v < len;
                    let p = if inside(fx + 0.5, w) && inside(fy + 0.5, h) {
                        px(clamp(x as f64, iw), clamp(y as f64, ih))
                    } else {
                        px(x, y)
                    };
                    for (s, v) in sum.iter_mut().zip(p) {
                        *s += v * k;
                    }
                }
            } else {
                // Box average with fractional coverage at the edges.
                let (right, bottom) = (left + step, top + step);
                let mut area = 0.0;
                let mut y = math::floor(top);
                while y < bottom {
                    let ky = (y + 1.0).min(bottom) - y.max(top);
                    let mut x = math::floor(left);
                    while x < right {
                        let kx = (x + 1.0).min(right) - x.max(left);
                        let p = px(x as i64, y as i64);
                        for (s, v) in sum.iter_mut().zip(p) {
                            *s += v * kx * ky;
                        }
                        area += kx * ky;
                        x += 1.0;
                    }
                    y += 1.0;
                }
                for s in &mut sum {
                    *s /= area;
                }
            }
            cells.push(Cell {
                rgb: [sum[0], sum[1], sum[2]],
                alpha: sum[3].clamp(0.0, 1.0),
            });
        }
    }
    cells
}

/// A lightness stretch: the 2nd to 98th percentile mapped onto 0–1, when
/// they're far enough apart to be worth stretching.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Levels {
    low: f64,
    high: f64,
}

/// Percentile spread below which a picture is left as it is: stretching a
/// nearly flat image would only amplify its noise.
const MIN_SPREAD: f64 = 0.05;

impl Levels {
    pub(crate) fn of(lightness: &[f64]) -> Levels {
        let mut sorted = lightness.to_vec();
        sorted.sort_by(f64::total_cmp);
        let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize];
        let (low, high) = (at(0.02), at(0.98));
        if high - low < MIN_SPREAD {
            Levels {
                low: 0.0,
                high: 1.0,
            }
        } else {
            Levels { low, high }
        }
    }

    pub(crate) fn apply(self, l: f64) -> f64 {
        ((l - self.low) / (self.high - self.low)).clamp(0.0, 1.0)
    }
}

/// OKLab lightness of each cell over `backdrop`.
pub(crate) fn lightness(cells: &[Cell], backdrop: Color) -> Vec<f64> {
    cells
        .iter()
        .map(|c| srgb_to_oklab(c.over(backdrop))[0])
        .collect()
}

/// Error diffusion (Floyd–Steinberg, serpentine) of an `n` × `n`
/// lightness grid: true where the result is dark.
pub(crate) fn dither(lightness: &[f64], n: usize) -> Vec<bool> {
    let mut v = lightness.to_vec();
    let mut dark = vec![false; n * n];
    for y in 0..n {
        let reverse = y % 2 == 1;
        for i in 0..n {
            let x = if reverse { n - 1 - i } else { i };
            let at = y * n + x;
            let d = v[at] < 0.5;
            dark[at] = d;
            let err = v[at] - if d { 0.0 } else { 1.0 };
            let ahead = |dx: i64| -> Option<usize> {
                let x = x as i64 + if reverse { -dx } else { dx };
                (x >= 0 && x < n as i64).then_some(x as usize)
            };
            if let Some(x1) = ahead(1) {
                v[y * n + x1] += err * 7.0 / 16.0;
            }
            if y + 1 < n {
                for (dx, k) in [(-1, 3.0), (0, 5.0), (1, 1.0)] {
                    if let Some(x1) = ahead(dx) {
                        v[(y + 1) * n + x1] += err * k / 16.0;
                    }
                }
            }
        }
    }
    dark
}

/// The picture's lightness at module resolution, stretched.
pub(crate) fn module_lightness(halftone: &Halftone, n: usize, backdrop: Color) -> Vec<f64> {
    let cells = sample(&halftone.image, halftone.fit, halftone.focal, n);
    let l = lightness(&cells, backdrop);
    let levels = Levels::of(&l);
    l.into_iter().map(|v| levels.apply(v)).collect()
}

/// The picture's light and dark at module resolution.
pub(crate) fn module_targets(halftone: &Halftone, n: usize, backdrop: Color) -> Vec<bool> {
    squeezed(&module_lightness(halftone, n, backdrop), n)
}

fn squeezed(l: &[f64], n: usize) -> Vec<bool> {
    let span = TARGET_SPAN;
    let l: Vec<f64> = l.iter().map(|v| span + v * (1.0 - 2.0 * span)).collect();
    dither(&l, n)
}

/// Top priority, for modules a style needs light whatever the picture.
pub(crate) const MUST: u32 = 1 << 24;
/// Module targets are dithered from the picture's lightness squeezed into
/// `[TARGET_SPAN, 1 − TARGET_SPAN]`, so every neighborhood keeps modules
/// of both colors. Measured (W20261026081347): unsqueezed, steered codes
/// with large flat areas failed the blurred, noisy ZXing render (29% of
/// 132 cases, v5–v27 × L/M/Q/H); ZXing's local thresholds average 5 × 5
/// blocks, so inside a solid area the threshold sits at the area's own
/// level and the noise flips half its pixels. From a span of 0.15 every
/// case passed.
pub(crate) const TARGET_SPAN: f64 = 0.2;
/// The most an image-derived priority can be.
const MAX_PRIORITY: f64 = 65_535.0;

/// What steering should aim for: each data module's target from the
/// picture, weighted by strong lights and darks, edges and closeness to
/// the center, times the priority map. Function modules are left alone.
pub(crate) fn wants(
    halftone: &Halftone,
    roles: &[ModuleRole],
    n: usize,
    backdrop: Color,
) -> Vec<Want> {
    let l = module_lightness(halftone, n, backdrop);
    let dark = squeezed(&l, n);
    let map = halftone.priority.as_ref().map(|p| {
        let cells = sample(p, halftone.fit, halftone.focal, n);
        lightness(&cells, Color::srgb(0, 0, 0))
    });
    let at = |x: i64, y: i64| {
        l[(y.clamp(0, n as i64 - 1) * n as i64 + x.clamp(0, n as i64 - 1)) as usize]
    };
    let mid = (n as f64 - 1.0) / 2.0;
    let mut out = vec![Want::default(); n * n];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            if roles[i].is_function() {
                continue;
            }
            let (xi, yi) = (x as i64, y as i64);
            // Sobel, normalized so a full step across the stencil gives 1.
            let gx = at(xi + 1, yi - 1) + 2.0 * at(xi + 1, yi) + at(xi + 1, yi + 1)
                - at(xi - 1, yi - 1)
                - 2.0 * at(xi - 1, yi)
                - at(xi - 1, yi + 1);
            let gy = at(xi - 1, yi + 1) + 2.0 * at(xi, yi + 1) + at(xi + 1, yi + 1)
                - at(xi - 1, yi - 1)
                - 2.0 * at(xi, yi - 1)
                - at(xi + 1, yi - 1);
            let edge = (math::sqrt(gx * gx + gy * gy) / 4.0).min(1.0);
            let decisive = (l[i] - 0.5).abs() * 2.0;
            let (dx, dy) = ((x as f64 - mid) / mid, (y as f64 - mid) / mid);
            let center = 1.0 - (math::sqrt(dx * dx + dy * dy) / core::f64::consts::SQRT_2).min(1.0);
            let mut p = 0.45 * decisive + 0.35 * edge + 0.2 * center;
            if let Some(map) = &map {
                p *= map[i].clamp(0.0, 1.0);
                if p <= 0.0 {
                    continue;
                }
            }
            out[i] = Want {
                dark: dark[i],
                priority: 1 + math::round(p * (MAX_PRIORITY - 1.0)) as u32,
            };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, QrCode, Style, Version};

    fn stripes(side: u32) -> Rgba8Image {
        let mut img = Rgba8Image::filled(side, side, [250, 250, 250, 255]).unwrap();
        for y in 0..side {
            for x in 0..side {
                if (x / 4 + y / 7) % 2 == 0 {
                    img.set_pixel(x, y, [20, 20, 20, 255]);
                }
            }
        }
        img
    }

    #[test]
    fn sampling_covers_and_contains() {
        // A 4×2 image, left half black, right half white.
        let mut img = Rgba8Image::filled(4, 2, [255, 255, 255, 255]).unwrap();
        for y in 0..2 {
            for x in 0..2 {
                img.set_pixel(x, y, [0, 0, 0, 255]);
            }
        }
        let white = Color::srgb(255, 255, 255);
        let l = |fit, focal| lightness(&sample(&img, fit, focal, 2), white);
        // Cover crops a 2×2 square: the left half at focal 0, the right at 1.
        assert!(l(Fit::Cover, (0.0, 0.5)).iter().all(|&v| v < 0.01));
        assert!(l(Fit::Cover, (1.0, 0.5)).iter().all(|&v| v > 0.99));
        // Contain puts the whole image in the top or bottom half.
        let top = l(Fit::Contain, (0.5, 0.0));
        assert!(top[0] < 0.01 && top[1] > 0.99 && top[2] > 0.99 && top[3] > 0.99);
        // Upsampling interpolates instead of averaging nothing.
        let fine = lightness(&sample(&img, Fit::Cover, (0.5, 0.5), 8), white);
        assert!(fine.iter().all(|v| v.is_finite()));
        assert!(fine[0] < 0.01 && fine[7] > 0.99);
    }

    #[test]
    fn levels_stretch_only_real_ranges() {
        let flat = Levels::of(&[0.5; 100]);
        assert_eq!(flat.apply(0.5), 0.5);
        let ramp: Vec<f64> = (0..100).map(|i| 0.3 + 0.4 * i as f64 / 99.0).collect();
        let levels = Levels::of(&ramp);
        assert!(levels.apply(0.3) < 0.01 && levels.apply(0.7) > 0.99);
    }

    #[test]
    fn targets_keep_both_colors_everywhere() {
        // A solid black picture still asks for some light modules in every
        // 5 × 5 window, so no area of the code is flat.
        let n = 45;
        let black = Halftone::new(Rgba8Image::filled(10, 10, [0, 0, 0, 255]).unwrap());
        let t = module_targets(&black, n, Color::srgb(255, 255, 255));
        for y in 0..n - 4 {
            for x in 0..n - 4 {
                let light = (0..25).filter(|i| !t[(y + i / 5) * n + x + i % 5]).count();
                assert!((2..=10).contains(&light), "({x}, {y}): {light}");
            }
        }
    }

    #[test]
    fn priority_maps_steer_where_asked() {
        // White on the left half only: the left follows the picture far
        // better than the right.
        let mut map = Rgba8Image::filled(40, 40, [0, 0, 0, 255]).unwrap();
        for y in 0..40 {
            for x in 0..20 {
                map.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        let picture = Halftone::new(stripes(120));
        let style = Style::new().halftone(picture.clone().priority(map));
        let qr = QrCode::builder()
            .ecc(Ecc::M)
            .version(Version::new(14).unwrap())
            .encode_text_for("https://example.com", &style)
            .unwrap();
        let n = qr.size() as usize;
        let targets = module_targets(&picture, n, style.palette.background());
        let half = |left: bool| {
            let (mut hit, mut all) = (0, 0);
            for (x, y, dark, role) in qr.modules() {
                let x = x as usize;
                if role.is_function() || (x < n / 2) != left || x == n / 2 {
                    continue;
                }
                all += 1;
                hit += usize::from(targets[y as usize * n + x] == dark);
            }
            hit as f64 / all as f64
        };
        assert!(
            half(true) > half(false) + 0.15,
            "{} {}",
            half(true),
            half(false)
        );
        // Black means "don't care": the right half is left to chance.
        assert!((half(false) - 0.5).abs() < 0.08, "{}", half(false));
    }
}
