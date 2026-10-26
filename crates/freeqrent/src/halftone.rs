//! Image-embedded codes (SPEC §7.7): a picture mapped onto the code's
//! modules. This module samples it, aims the free-bit solver (`steer`) at
//! it, and draws it into 3 × 3 sub-cells whose centers carry the code.

use alloc::vec;
use alloc::vec::Vec;

use crate::color::{Color, linear_to_oklab, srgb_to_linear};
use crate::geometry::{FIFTHS, Grid, Path, UNIT};
use crate::image::Rgba8Image;
use crate::math;
use crate::matrix::{ModuleRole, QrMatrix};
use crate::steer::Want;

/// How a picture fills the code's square.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Fit {
    /// Scaled to cover the square, cropped around the focal point.
    #[default]
    Cover,
    /// Scaled to fit inside the square; the rest is background. The focal
    /// point places it in the leftover space.
    Contain,
}

/// How halftone sub-cells are painted (SPEC §7.7 rule 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Ink {
    /// Dark sub-cells in `fg`; light ones show `bg`. The default.
    #[default]
    Mono,
    /// Every sub-cell keeps the picture's color, its lightness clamped
    /// into the dark or light band ([`Halftone::bands`]), quantized to
    /// `colors` colors per band (2–16) so the code stays a few paths.
    Color {
        /// Colors per band.
        colors: u8,
    },
}

/// Where the picture gives way to plain modules (SPEC §7.7 rule 4):
/// 0 draws the picture as it is, 1 draws pure modules. Sub-cells around a
/// module's center are pulled toward its value by this much.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Clarity {
    /// The same everywhere.
    Uniform(f32),
    /// Laid out like the picture: light areas clear, dark ones pictorial.
    Map(Rgba8Image),
}

impl Default for Clarity {
    fn default() -> Clarity {
        Clarity::Uniform(DEFAULT_CLARITY)
    }
}

/// The default [`Clarity`].
pub const DEFAULT_CLARITY: f32 = 0.0;

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
    pub(crate) ink: Ink,
    pub(crate) bands: (f32, f32),
    pub(crate) clarity: Clarity,
}

/// The default [`Halftone::bands`].
pub const DEFAULT_BANDS: (f32, f32) = (0.28, 0.94);

impl Halftone {
    /// The picture, covering the code, centered, with steering on.
    pub fn new(image: Rgba8Image) -> Halftone {
        Halftone {
            image,
            fit: Fit::Cover,
            focal: (0.5, 0.5),
            priority: None,
            steer: true,
            ink: Ink::Mono,
            bands: DEFAULT_BANDS,
            clarity: Clarity::default(),
        }
    }

    /// How sub-cells are painted (default [`Ink::Mono`]).
    pub fn ink(mut self, ink: Ink) -> Halftone {
        self.ink = ink;
        self
    }

    /// For [`Ink::Color`]: the lightest a dark sub-cell may be and the
    /// darkest a light one may be, in OKLab lightness (default
    /// [`DEFAULT_BANDS`]). The gap is checked like any contrast.
    pub fn bands(mut self, dark: f32, light: f32) -> Halftone {
        self.bands = (dark, light);
        self
    }

    /// Where the picture gives way to plain modules (default
    /// [`DEFAULT_CLARITY`] everywhere).
    pub fn clarity(mut self, clarity: Clarity) -> Halftone {
        self.clarity = clarity;
        self
    }

    /// How sub-cells are painted.
    pub fn ink_mode(&self) -> Ink {
        self.ink
    }

    /// The dark and light band limits.
    pub fn band_limits(&self) -> (f32, f32) {
        self.bands
    }

    /// The clarity.
    pub fn clarity_map(&self) -> &Clarity {
        &self.clarity
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
#[non_exhaustive]
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
    /// The cell over `backdrop` (opaque, linear sRGB), in OKLab.
    pub(crate) fn over(self, backdrop: [f64; 3]) -> [f64; 3] {
        linear_to_oklab(core::array::from_fn(|i| {
            (self.rgb[i] + (1.0 - self.alpha) * backdrop[i]).clamp(0.0, 1.0)
        }))
    }
}

/// A backdrop color as linear sRGB, for [`Cell::over`].
pub(crate) fn linear(color: Color) -> [f64; 3] {
    color.to_srgb().map(srgb_to_linear)
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
    let backdrop = linear(backdrop);
    cells.iter().map(|c| c.over(backdrop)[0]).collect()
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

/// The picture's lightness at module resolution, stretched. With
/// `inverted` ink (lighter than the background, SPEC §7.7) it is turned
/// over, so ink goes where the picture is light and the picture still
/// shows the right way round.
pub(crate) fn module_lightness(
    halftone: &Halftone,
    n: usize,
    backdrop: Color,
    inverted: bool,
) -> Vec<f64> {
    let cells = sample(&halftone.image, halftone.fit, halftone.focal, n);
    let l = lightness(&cells, backdrop);
    let levels = Levels::of(&l);
    l.into_iter()
        .map(|v| flip(levels.apply(v), inverted))
        .collect()
}

/// `l`, turned over when the ink is lighter than the background.
fn flip(l: f64, inverted: bool) -> f64 {
    if inverted { 1.0 - l } else { l }
}

/// The picture's ink and paper at module resolution (`true` for ink).
pub(crate) fn module_targets(
    halftone: &Halftone,
    n: usize,
    backdrop: Color,
    inverted: bool,
) -> Vec<bool> {
    squeezed(&module_lightness(halftone, n, backdrop, inverted), n)
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
    inverted: bool,
) -> Vec<Want> {
    let l = module_lightness(halftone, n, backdrop, inverted);
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

// ----------------------------------------------------------------- drawing

/// The three sub-cells across a module, as (start, length) in fifths: a
/// thin edge, a center 0.6 of a module wide, a thin edge. Measured
/// (W20261026092854), ZXing and rqrr on clean renders: an even 3 × 3 grid
/// (a center a third of a module wide) failed 118 of 150 codes at
/// 4 px/module, where the center covers 1.3 px; a half-module center
/// failed none there, but rqrr, which samples one pixel through a
/// perspective fitted to a single alignment pattern, still missed 2 of
/// 216 renders from 4 to 12 px/module (18 with a logo); at 0.6, none.
const SPANS: [(usize, usize); 3] = [(0, 1), (1, 3), (4, 1)];
/// Sub-cell lightness is squeezed into `[CELL_SPAN, 1 − CELL_SPAN]` before
/// dithering, for the reason module targets are (`TARGET_SPAN`): a flat
/// area of the picture drawn flat makes ZXing's local threshold sink into
/// the noise. Measured (W20261026092854): unsqueezed, every code failed
/// the blurred, noisy render; with the 0.6 center, 0.15 left the color
/// disc failing at ECC L (the blur swallows thin light edges) and 0.2
/// left nothing.
pub(crate) const CELL_SPAN: f64 = 0.2;

/// The drawn sub-cells of a code: 3 × 3 per module.
#[derive(Clone)]
pub(crate) struct Subcells {
    /// Sub-cells per side.
    pub(crate) side: usize,
    /// Row-major; `None` where nothing is drawn (function modules and
    /// cleared ones).
    pub(crate) dark: Vec<Option<bool>>,
    /// In color mode: the palette (each color with its band, dark or
    /// not), and each sub-cell's index into it.
    pub(crate) colors: Option<Quantized>,
}

/// A color palette (each color with its band, dark or not) and each
/// sub-cell's index into it.
pub(crate) type Quantized = (Vec<(Color, bool)>, Vec<u8>);

/// Dithers the picture into 3 × 3 sub-cells per module (SPEC §7.7 B). The
/// center of each data, ECC or remainder module takes the module's value;
/// the eight around it follow the picture, pulled toward the module by the
/// clarity. Error diffusion (Floyd–Steinberg, serpentine, weighted by
/// sub-cell area) runs over the whole grid, centers included, so the
/// picture's tone survives them.
pub(crate) fn subcells(
    halftone: &Halftone,
    qr: &QrMatrix,
    removed: &Grid,
    backdrop: Color,
    inverted: bool,
) -> Subcells {
    let n = qr.size() as usize;
    let side = n * 3;
    let fine = n * FIFTHS;
    let levels = Levels::of(&lightness(
        &sample(&halftone.image, halftone.fit, halftone.focal, n),
        backdrop,
    ));
    // Sampled in fifths of a module, then gathered into sub-cells.
    let gather = |values: &[[f64; 3]]| -> Vec<[f64; 3]> {
        let mut out = vec![[0.0; 3]; side * side];
        for (cy, (y0, h)) in (0..side).map(|c| (c, SPANS[c % 3])) {
            for (cx, (x0, w)) in (0..side).map(|c| (c, SPANS[c % 3])) {
                let (bx, by) = ((cx / 3) * FIFTHS + x0, (cy / 3) * FIFTHS + y0);
                let mut sum = [0.0; 3];
                for y in by..by + h {
                    for x in bx..bx + w {
                        for (s, v) in sum.iter_mut().zip(values[y * fine + x]) {
                            *s += v;
                        }
                    }
                }
                out[cy * side + cx] = sum.map(|v| v / (w * h) as f64);
            }
        }
        out
    };
    let cells = sample(&halftone.image, halftone.fit, halftone.focal, fine);
    let under = linear(backdrop);
    let lab = gather(&cells.iter().map(|c| c.over(under)).collect::<Vec<_>>());
    let clarity: Vec<f64> = match &halftone.clarity {
        Clarity::Uniform(c) => vec![(*c as f64).clamp(0.0, 1.0); side * side],
        Clarity::Map(map) => {
            let l = lightness(
                &sample(map, halftone.fit, halftone.focal, fine),
                Color::srgb(0, 0, 0),
            );
            gather(&l.iter().map(|&v| [v, 0.0, 0.0]).collect::<Vec<_>>())
                .iter()
                .map(|v| v[0].clamp(0.0, 1.0))
                .collect()
        }
    };
    let area = |i: usize| (SPANS[(i % side) % 3].1 * SPANS[(i / side) % 3].1) as f64;
    let mut v: Vec<f64> = (0..side * side)
        .map(|i| {
            let (mx, my) = ((i % side) / 3, (i / side) / 3);
            let target = if qr.is_dark(mx as u16, my as u16) {
                0.0
            } else {
                1.0
            };
            let l = CELL_SPAN + flip(levels.apply(lab[i][0]), inverted) * (1.0 - 2.0 * CELL_SPAN);
            (1.0 - clarity[i]) * l + clarity[i] * target
        })
        .collect();
    let mut dark = vec![None; side * side];
    for y in 0..side {
        let reverse = y % 2 == 1;
        for k in 0..side {
            let x = if reverse { side - 1 - k } else { k };
            let i = y * side + x;
            let (mx, my) = (x / 3, y / 3);
            if qr.role(mx as u16, my as u16).is_function() || removed.get(mx as i64, my as i64) {
                continue;
            }
            let d = if x % 3 == 1 && y % 3 == 1 {
                qr.is_dark(mx as u16, my as u16)
            } else {
                v[i] < 0.5
            };
            dark[i] = Some(d);
            // The error as ink, shared out by area.
            let err = (v[i] - if d { 0.0 } else { 1.0 }) * area(i);
            let ahead = |dx: i64| -> Option<usize> {
                let x = x as i64 + if reverse { -dx } else { dx };
                (x >= 0 && x < side as i64).then_some(x as usize)
            };
            if let Some(x1) = ahead(1) {
                let j = y * side + x1;
                v[j] += err * 7.0 / 16.0 / area(j);
            }
            if y + 1 < side {
                for (dx, k) in [(-1, 3.0), (0, 5.0), (1, 1.0)] {
                    if let Some(x1) = ahead(dx) {
                        let j = (y + 1) * side + x1;
                        v[j] += err * k / 16.0 / area(j);
                    }
                }
            }
        }
    }
    let colors = match halftone.ink {
        Ink::Mono => None,
        Ink::Color { colors } => Some(quantize(
            &lab,
            &dark,
            halftone.bands,
            colors.clamp(2, 16) as usize,
            inverted,
        )),
    };
    Subcells { side, dark, colors }
}

/// Clamps each sub-cell's color into its band (dark at most `bands.0`,
/// light at least `bands.1`; ink takes the light band when `inverted`),
/// then reduces each band to `k` colors with k-means in OKLab, started
/// from evenly spaced lightness quantiles so the result is deterministic.
/// The palette's flag marks ink.
fn quantize(
    lab: &[[f64; 3]],
    dark: &[Option<bool>],
    bands: (f32, f32),
    k: usize,
    inverted: bool,
) -> Quantized {
    let clamp = |c: [f64; 3], ink: bool| {
        let l = if ink != inverted {
            c[0].min(bands.0 as f64)
        } else {
            c[0].max(bands.1 as f64)
        };
        [l, c[1], c[2]]
    };
    let mut palette = Vec::new();
    let mut index = vec![0u8; lab.len()];
    for band in [true, false] {
        let members: Vec<usize> = (0..lab.len()).filter(|&i| dark[i] == Some(band)).collect();
        if members.is_empty() {
            continue;
        }
        let points: Vec<[f64; 3]> = members.iter().map(|&i| clamp(lab[i], band)).collect();
        let mut by_l: Vec<usize> = (0..points.len()).collect();
        by_l.sort_by(|&a, &b| points[a][0].total_cmp(&points[b][0]).then(a.cmp(&b)));
        let k = k.min(points.len());
        let mut centers: Vec<[f64; 3]> = (0..k)
            .map(|j| points[by_l[(2 * j + 1) * points.len() / (2 * k)]])
            .collect();
        let mut owner = vec![0usize; points.len()];
        for _ in 0..QUANTIZE_ROUNDS {
            for (p, o) in points.iter().zip(owner.iter_mut()) {
                *o = (0..k)
                    .min_by(|&a, &b| dist2(p, &centers[a]).total_cmp(&dist2(p, &centers[b])))
                    .expect("k ≥ 1");
            }
            let mut sums = vec![([0.0f64; 3], 0usize); k];
            for (p, &o) in points.iter().zip(&owner) {
                for (s, v) in sums[o].0.iter_mut().zip(p) {
                    *s += v;
                }
                sums[o].1 += 1;
            }
            for (c, (sum, count)) in centers.iter_mut().zip(sums) {
                if count > 0 {
                    *c = sum.map(|v| v / count as f64);
                }
            }
        }
        let base = palette.len();
        palette.extend(
            centers
                .iter()
                .map(|c| (Color::from_oklab(c[0], c[1], c[2]).opaque_srgb(), band)),
        );
        for (&i, &o) in members.iter().zip(&owner) {
            index[i] = (base + o) as u8;
        }
    }
    (palette, index)
}

/// Lloyd rounds in [`quantize`].
const QUANTIZE_ROUNDS: usize = 8;

fn dist2(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// The paths of a picture: per paint (the dark sub-cells in mono, each
/// color in color mode, dark band first), one path for the thin edge rows
/// and one for the center rows. Each is a run of horizontal lines along
/// the rows' middles, to be stroked at the rows' height with butt caps:
/// half the bytes of outlined rectangles. Coordinates are fifths of a
/// module from the code's corner. `None` paints with `fg`.
pub(crate) fn paths(cells: &Subcells) -> Vec<Stroke> {
    let side = cells.side;
    let at = |c: usize| ((c / 3) * FIFTHS + SPANS[c % 3].0) as i64 * UNIT;
    let mut out = Vec::new();
    let mut emit = |paint: Option<Color>, dark: bool, member: &dyn Fn(usize) -> bool| {
        for height in [SPANS[0].1, SPANS[1].1] {
            let mut path = Path::new();
            for y in (0..side).filter(|y| SPANS[y % 3].1 == height) {
                let mid = at(y) + height as i64 * UNIT / 2;
                let mut x = 0;
                while x < side {
                    if !member(y * side + x) {
                        x += 1;
                        continue;
                    }
                    let start = x;
                    while x + 1 < side && member(y * side + x + 1) {
                        x += 1;
                    }
                    path.move_to(at(start), mid);
                    path.line_to(at(x) + SPANS[x % 3].1 as i64 * UNIT, mid);
                    x += 1;
                }
            }
            if !path.is_empty() {
                out.push(Stroke {
                    paint,
                    dark,
                    width: height as u8,
                    path,
                });
            }
        }
    };
    match &cells.colors {
        None => emit(None, true, &|i| cells.dark[i] == Some(true)),
        Some((palette, index)) => {
            for (c, &(color, dark)) in palette.iter().enumerate() {
                emit(Some(color), dark, &|i| {
                    cells.dark[i].is_some() && index[i] as usize == c
                });
            }
        }
    }
    out
}

/// One stroked path of a picture (see [`paths`]).
pub(crate) struct Stroke {
    /// A fixed color, or `None` for the `fg` token.
    pub(crate) paint: Option<Color>,
    /// Whether the paint is in the dark band.
    pub(crate) dark: bool,
    /// The stroke width, in fifths of a module.
    pub(crate) width: u8,
    pub(crate) path: Path,
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
        let t = module_targets(&black, n, Color::srgb(255, 255, 255), false);
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
        let targets = module_targets(&picture, n, style.palette.background(), false);
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

    fn code_and_cells(ink: Ink) -> (QrMatrix, Style, Subcells) {
        let style = Style::new()
            .halftone(Halftone::new(stripes(90)).ink(ink))
            .alignment(crate::AlignmentStyle {
                shape: crate::AlignmentShape::Match,
                emphasis: crate::Emphasis::Outline,
            });
        let qr = QrCode::builder()
            .ecc(Ecc::M)
            .version(Version::new(7).unwrap())
            .encode_text_for("https://example.com", &style)
            .unwrap();
        let removed = crate::scene::removed(&style, &qr);
        let h = style.halftone.as_ref().unwrap();
        let cells = subcells(h, &qr, &removed, style.palette.background(), false);
        (qr, style, cells)
    }

    #[test]
    fn centers_carry_the_modules() {
        let (qr, style, cells) = code_and_cells(Ink::Mono);
        let removed = crate::scene::removed(&style, &qr);
        assert!(!removed.is_empty());
        let side = cells.side;
        for (x, y, dark, role) in qr.modules() {
            let (x, y) = (x as usize, y as usize);
            let center = cells.dark[(y * 3 + 1) * side + x * 3 + 1];
            if role.is_function() || removed.get(x as i64, y as i64) {
                for i in 0..9 {
                    assert_eq!(cells.dark[(y * 3 + i / 3) * side + x * 3 + i % 3], None);
                }
            } else {
                assert_eq!(center, Some(dark), "({x}, {y})");
            }
        }
    }

    /// Replays a stroke's `m`/`h` commands into the sub-cells it covers.
    fn covered(stroke: &Stroke, side: usize) -> Vec<bool> {
        let d = stroke.path.data();
        let mut out = vec![false; side * side];
        let (mut x, mut y) = (0.0f64, 0.0f64);
        let mut cmd = ' ';
        let mut nums: Vec<f64> = Vec::new();
        let mut flush = |cmd: char, nums: &mut Vec<f64>, out: &mut Vec<bool>| {
            match cmd {
                'm' => {
                    x += nums[0];
                    y += nums[1];
                }
                'h' => {
                    // Fifths to sub-cells: edges 1 wide, centers 3.
                    let row = (y - stroke.width as f64 / 2.0).round() as usize;
                    let cy =
                        row / FIFTHS * 3 + SPANS.iter().position(|s| s.0 == row % FIFTHS).unwrap();
                    let (from, to) = (x.round() as usize, (x + nums[0]).round() as usize);
                    for f in from..to {
                        if let Some(k) = SPANS.iter().position(|s| s.0 == f % FIFTHS) {
                            out[cy * side + f / FIFTHS * 3 + k] = true;
                        }
                    }
                    x += nums[0];
                }
                _ => {}
            }
            nums.clear();
        };
        let mut num = alloc::string::String::new();
        for c in d.chars().chain(core::iter::once(' ')) {
            let starts = c == '-' || (c == '.' && num.contains('.'));
            if c.is_ascii_digit() || (c == '.' && !starts) {
                num.push(c);
                continue;
            }
            if !num.is_empty() {
                nums.push(num.parse().unwrap());
                num.clear();
            }
            if c.is_ascii_alphabetic() {
                if cmd != ' ' {
                    flush(cmd, &mut nums, &mut out);
                }
                cmd = c;
            } else if starts {
                num.push(c);
            }
        }
        flush(cmd, &mut nums, &mut out);
        out
    }

    #[test]
    fn strokes_cover_exactly_their_cells() {
        for ink in [Ink::Mono, Ink::Color { colors: 3 }] {
            let (_, _, cells) = code_and_cells(ink);
            let side = cells.side;
            let mut drawn = vec![None; side * side];
            for stroke in paths(&cells) {
                for (i, hit) in covered(&stroke, side).into_iter().enumerate() {
                    if hit {
                        assert_eq!(drawn[i], None, "{ink:?}: cell {i} drawn twice");
                        drawn[i] = Some(stroke.paint);
                    }
                }
            }
            for i in 0..side * side {
                let want = match (&cells.colors, cells.dark[i]) {
                    (_, None) => None,
                    (None, Some(d)) => d.then_some(None),
                    (Some((palette, index)), Some(_)) => Some(Some(palette[index[i] as usize].0)),
                };
                assert_eq!(drawn[i], want, "{ink:?}: cell {i}");
            }
        }
    }

    #[test]
    fn colors_stay_in_their_bands() {
        let (_, _, cells) = code_and_cells(Ink::Color { colors: 3 });
        let (palette, _) = cells.colors.unwrap();
        assert!(palette.len() <= 6);
        let (dark, light) = DEFAULT_BANDS;
        for (color, is_dark) in palette {
            let l = color.to_oklab()[0];
            if is_dark {
                assert!(l <= dark + 0.01, "{color} {l}");
            } else {
                assert!(l >= light - 0.01, "{color} {l}");
            }
        }
    }

    #[cfg(feature = "animate")]
    #[test]
    fn plans_describe_the_strokes() {
        let style = Style::new().halftone(Halftone::new(stripes(90)).ink(Ink::Color { colors: 2 }));
        let qr = QrCode::builder()
            .ecc(Ecc::M)
            .version(Version::new(7).unwrap())
            .encode_text_for("https://example.com", &style)
            .unwrap();
        let plan = style.plan(&qr).unwrap();
        let layers: Vec<_> = plan
            .layers
            .iter()
            .filter(|l| l.role == crate::LayerRole::Halftone)
            .collect();
        // Two bands of two colors, each with edge and center rows.
        assert_eq!(layers.len(), 8);
        for l in &layers {
            assert!(l.color.is_some());
            assert_eq!(l.cells, Some((4.0, 4.0)));
            assert!(matches!(l.stroke, Some(w) if w == 1.0 || w == 3.0));
        }
        let json = plan.to_json();
        assert!(json.contains(r##""role":"halftone","token":"fg","color":"#"##));
        assert!(json.contains(r#""cells":[4,4],"stroke":3"#));
    }
}
