//! Glass ink (SPEC §7.9): the photograph full frame, with the code laid on
//! it as liquid glass. The photo is embedded from its original bytes
//! ([`Halftone::source`](crate::Halftone::source)); this module bakes the
//! glass over it into one RGBA layer, embedded as a PNG
//! ([`crate::png::encode_rgba`]).
//!
//! Per pixel: every module that needs glass is a squircle (a rounded square
//! signed distance field); same-polarity squircles melt together with a
//! quadratic smooth minimum; inside, the photo is frosted (a Gaussian blur),
//! refracted through a quartic bevel by Snell's law, shifted in OKLab
//! lightness to the module's target (hue and chroma kept, the photo's own
//! detail riding on the shift), tinted toward the picture's dominant dark or
//! light colour, and given a rim light, an inner shade and a faint drop
//! shadow. The math follows liquid-rust (github.com/al1h3n/liquid-rust,
//! MIT/Apache-2.0, a wgpu shader), reimplemented here on the CPU.
//!
//! Every float goes through `+ − × ÷`, the crate's own `math` and the
//! Newton iterations below, so the layer is byte-identical on every
//! platform.

use alloc::vec;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use crate::check::{GLASS_RESOLUTION, GLASS_SAFE_TARGETS};
use crate::halftone::{self, Halftone, Sampler, Span};
use crate::image::Rgba8Image;
use crate::math;
use crate::matrix::{ModuleRole, QrMatrix};
use crate::palette::{Paint, Token};
use crate::scanbox::TimingStyle;
use crate::scene::{self, Canvas};
use crate::style::Style;

mod field;
mod vector;
pub(crate) use vector::{RIM_STROKE, vector};

/// The glass of [`Ink::Glass`](crate::Ink::Glass) (SPEC §7.9). Plain data;
/// the defaults are the measured ones, and [`Style::check`](crate::Style::check)
/// holds every option to its range.
///
/// ```
/// use freeqrent::Glass;
///
/// let glass = Glass::new().border(0.12).halo(3);
/// assert_eq!(glass.border_share(), 0.12);
/// assert_eq!(glass.target_lightness(), (0.25, 0.90));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Glass {
    pub(crate) border: f32,
    pub(crate) halo: u8,
    pub(crate) lens: f32,
    pub(crate) core: f32,
    pub(crate) targets: (f32, f32),
    pub(crate) union: f32,
    pub(crate) frost: f32,
    pub(crate) finders: (f32, f32),
    pub(crate) colors: GlassColors,
    pub(crate) refraction: bool,
    pub(crate) rim: bool,
    pub(crate) resolution: u8,
    pub(crate) render: GlassRender,
    pub(crate) texture: f32,
    pub(crate) bevel: f32,
    pub(crate) ior: f32,
    pub(crate) dome: f32,
    pub(crate) opacity: f32,
}

/// Where glass takes its colour from (SPEC §7.9, [`Glass::colors`]). The
/// lightness is always the targets'; this sets the hue and chroma.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GlassColors {
    /// The picture's own dominant dark and light colours (the default).
    Photo,
    /// The style's palette (W20261124091813): dark glass in each module's
    /// token (`fg`, `fg.2`–`fg.4`, the finder, alignment and timing
    /// tokens), light glass and the plate in `bg`'s hue.
    Palette,
    /// No tint: the photo's own hue under the lightness shift (baked),
    /// black and white veils (vector).
    Neutral,
}

/// How glass is drawn (SPEC §7.9): [`Glass::render`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GlassRender {
    /// Baked per pixel into one PNG over the photo: the full liquid-glass
    /// look (frost, OKLab shift, smooth union, refraction, rim light),
    /// identical in every renderer.
    Baked,
    /// Drawn as SVG elements over the photo: traced lens outlines filled
    /// with translucent black and white (or the picture's tints) at a few
    /// stepped strengths, a blurred reference to the photo for the frost,
    /// and rim strokes. No second raster; no refraction; stepped strength.
    Vector,
}

/// The default [`GlassRender`]: the one constant to change.
pub const GLASS_RENDER: GlassRender = GlassRender::Baked;

/// PNG bytes per pixel of the glass box: the most any test picture took
/// was 0.97 (SPEC §7.9), rounded up, for the size estimate behind
/// [`Warning::LargeOutput`](crate::Warning::LargeOutput).
const BYTES_PER_PIXEL: f64 = 1.0;

impl Glass {
    /// The measured defaults: border 0.08, halo 2, lens 0.9 with a 0.6
    /// core, targets 0.25/0.90, union 0.4, frost 0.12, finders 0.30 on a
    /// 0.92 plate, tint, refraction and rim on, 16 px per module.
    pub const fn new() -> Glass {
        Glass {
            border: 0.08,
            halo: 2,
            lens: 0.9,
            core: 0.6,
            targets: GLASS_SAFE_TARGETS,
            union: 0.4,
            frost: 0.12,
            finders: (0.30, 0.92),
            colors: GlassColors::Photo,
            refraction: true,
            rim: true,
            resolution: GLASS_RESOLUTION,
            render: GLASS_RENDER,
            texture: 0.0,
            bevel: 0.12,
            ior: ETA as f32,
            dome: 0.0,
            opacity: 1.0,
        }
    }

    /// The photo's margin around the quiet zone, as a share of the canvas
    /// width, 0–0.25 (default 0.08). It is rounded to whole modules.
    pub fn border(mut self, share: f32) -> Glass {
        self.border = share;
        self
    }

    /// The frosted light halo over the quiet zone, in modules from the
    /// code, 0 to the quiet zone (default 2). Below 1 ZXing lost most codes
    /// ([`RenderError::GlassHaloTooNarrow`](crate::RenderError::GlassHaloTooNarrow)).
    pub fn halo(mut self, modules: u8) -> Glass {
        self.halo = modules;
        self
    }

    /// A data module's lens width in modules, 0.5–1.0 (default 0.9); vector
    /// glass takes 0.7–1.0 ([`GLASS_VECTOR_MIN_LENS`](crate::GLASS_VECTOR_MIN_LENS)).
    pub fn lens(mut self, width: f32) -> Glass {
        self.lens = width;
        self
    }

    /// The lens's fully shifted core, 0.3 modules up to the lens width
    /// (default 0.6); the rest of the lens feathers out.
    pub fn core(mut self, width: f32) -> Glass {
        self.core = width;
        self
    }

    /// The lightness a dark module's glass brings it down to and a light
    /// one's up to, OKLab L 0–1 (default [`GLASS_SAFE_TARGETS`]). Modules
    /// already past their target get no lens.
    pub fn targets(mut self, dark: f32, light: f32) -> Glass {
        self.targets = (dark, light);
        self
    }

    /// How far neighboring lenses melt together, in modules, 0.2–0.8
    /// (default 0.4; the smooth minimum's `k`).
    pub fn union(mut self, k: f32) -> Glass {
        self.union = k;
        self
    }

    /// The frost: the blur of the photo seen through the glass, in modules,
    /// 0–0.3 (default 0.12).
    pub fn frost(mut self, sigma: f32) -> Glass {
        self.frost = sigma;
        self
    }

    /// The finder, alignment and timing glass: the dark glass's lightness
    /// and the frosted plate's, OKLab L 0–1 (default 0.30 on 0.92). Their
    /// difference is the code's contrast; see
    /// [`GLASS_FINDER_FLOOR`](crate::GLASS_FINDER_FLOOR) and
    /// [`GLASS_PLATE_FLOOR`](crate::GLASS_PLATE_FLOOR).
    pub fn finders(mut self, dark: f32, plate: f32) -> Glass {
        self.finders = (dark, plate);
        self
    }

    /// Tints the glass toward the picture's dominant dark and light colours
    /// (default on): `true` is [`GlassColors::Photo`], `false`
    /// [`GlassColors::Neutral`].
    pub fn tint(mut self, tint: bool) -> Glass {
        self.colors = if tint {
            GlassColors::Photo
        } else {
            GlassColors::Neutral
        };
        self
    }

    /// Where the glass takes its colour (default [`GlassColors::Photo`]);
    /// [`GlassColors::Palette`] uses the style's colors (W20261124091813).
    pub fn colors(mut self, colors: GlassColors) -> Glass {
        self.colors = colors;
        self
    }

    /// How much of the photo's own detail shows through the plate and the
    /// finder, alignment and timing glass, 0–1 (default 0: flat, as
    /// measured). Their lightness rides on the photo around the finder and
    /// plate targets, held past the floors (W20261124091813).
    pub fn texture(mut self, texture: f32) -> Glass {
        self.texture = texture;
        self
    }

    /// The depth of each lens's bevel, where refraction bends the photo, in
    /// modules, 0.05–0.45 (default 0.12). Baked only.
    pub fn bevel(mut self, depth: f32) -> Glass {
        self.bevel = depth;
        self
    }

    /// The refractive index, 1.0–2.0 (default 1.5): 1 bends nothing, higher
    /// bends more. Baked only.
    pub fn ior(mut self, ior: f32) -> Glass {
        self.ior = ior;
        self
    }

    /// A dome across each lens, 0–1 (default 0): the photo magnified
    /// toward the lens's center, as a drop of water does, on top of the
    /// bevel's bend at the edge. Baked only.
    pub fn dome(mut self, dome: f32) -> Glass {
        self.dome = dome;
        self
    }

    /// The glass's opacity over the photo, 0.1–1 (default 1). Below 1 the
    /// photo shows through unshifted, which takes contrast: below the
    /// measured threshold the code warns
    /// [`GlassLowReadability`](crate::Warning::Unchecked) (W20261124091813).
    pub fn opacity(mut self, opacity: f32) -> Glass {
        self.opacity = opacity;
        self
    }

    /// Bends the photo through the lens edges, by Snell's law at an index
    /// of 1.5 (default on).
    pub fn refraction(mut self, refraction: bool) -> Glass {
        self.refraction = refraction;
        self
    }

    /// A rim light, an inner shade and a faint drop shadow on each lens
    /// (default on).
    pub fn rim(mut self, rim: bool) -> Glass {
        self.rim = rim;
        self
    }

    /// Baked pixels per module, 8–32 (default [`GLASS_RESOLUTION`]). The
    /// layer's size grows with its square.
    pub fn resolution(mut self, px_per_module: u8) -> Glass {
        self.resolution = px_per_module;
        self
    }

    /// How the glass is drawn (default [`GLASS_RENDER`]): baked into a PNG,
    /// or as SVG elements. Vector glass ignores `core`, `refraction` and
    /// `resolution` (SPEC §7.9).
    pub fn render(mut self, render: GlassRender) -> Glass {
        self.render = render;
        self
    }

    /// How the glass is drawn.
    pub fn rendering(&self) -> GlassRender {
        self.render
    }

    /// Whether the glass is drawn as SVG elements.
    pub(crate) fn is_vector(&self) -> bool {
        self.render == GlassRender::Vector
    }

    /// The border's share of the canvas width.
    pub fn border_share(&self) -> f32 {
        self.border
    }

    /// The halo, in modules.
    pub fn halo_modules(&self) -> u8 {
        self.halo
    }

    /// The lens width.
    pub fn lens_width(&self) -> f32 {
        self.lens
    }

    /// The lens's core width.
    pub fn core_width(&self) -> f32 {
        self.core
    }

    /// The dark and light lightness targets.
    pub fn target_lightness(&self) -> (f32, f32) {
        self.targets
    }

    /// The smooth union's reach.
    pub fn union_reach(&self) -> f32 {
        self.union
    }

    /// The frost's blur.
    pub fn frost_sigma(&self) -> f32 {
        self.frost
    }

    /// The finder glass's and the plate's lightness.
    pub fn finder_lightness(&self) -> (f32, f32) {
        self.finders
    }

    /// Whether the glass is tinted (any colours but
    /// [`GlassColors::Neutral`]).
    pub fn tints(&self) -> bool {
        self.colors != GlassColors::Neutral
    }

    /// Where the glass takes its colour.
    pub fn color_source(&self) -> GlassColors {
        self.colors
    }

    /// How much photo detail shows through the function glass.
    pub fn texture_share(&self) -> f32 {
        self.texture
    }

    /// The bevel's depth, in modules.
    pub fn bevel_depth(&self) -> f32 {
        self.bevel
    }

    /// The refractive index.
    pub fn refractive_index(&self) -> f32 {
        self.ior
    }

    /// The dome's strength.
    pub fn dome_strength(&self) -> f32 {
        self.dome
    }

    /// The glass's opacity.
    pub fn glass_opacity(&self) -> f32 {
        self.opacity
    }

    /// Whether the lens edges refract.
    pub fn refracts(&self) -> bool {
        self.refraction
    }

    /// Whether the lenses have a rim.
    pub fn has_rim(&self) -> bool {
        self.rim
    }

    /// Baked pixels per module.
    pub fn px_per_module(&self) -> u8 {
        self.resolution
    }

    /// The fields compared by bits, so equality is total and agrees with
    /// the hash.
    fn key(&self) -> ([u32; 14], [u8; 6]) {
        let b = |v: f32| v.to_bits();
        (
            [
                b(self.border),
                b(self.lens),
                b(self.core),
                b(self.targets.0),
                b(self.targets.1),
                b(self.union),
                b(self.frost),
                b(self.finders.0),
                b(self.finders.1),
                b(self.texture),
                b(self.bevel),
                b(self.ior),
                b(self.dome),
                b(self.opacity),
            ],
            [
                self.halo,
                self.colors as u8,
                self.refraction as u8,
                self.rim as u8,
                self.resolution,
                self.render as u8,
            ],
        )
    }
}

impl Default for Glass {
    fn default() -> Glass {
        Glass::new()
    }
}

impl PartialEq for Glass {
    fn eq(&self, other: &Glass) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Glass {}

impl Hash for Glass {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state);
    }
}

/// The photo's margin around the quiet zone, in whole modules, for a code
/// `n` modules wide with a `q`-module quiet zone: `border` of the canvas
/// width, rounded.
pub(crate) fn margin(border: f32, n: i64, q: i64) -> i64 {
    let b = if border.is_nan() {
        0.0
    } else {
        (border as f64).clamp(0.0, 0.25)
    };
    math::round(b * (n + 2 * q) as f64 / (1.0 - 2.0 * b)) as i64
}

/// Where a glass code of `n` modules lies in the photo's square, which
/// fills the card: the steering and the glass sample this span.
pub(crate) fn span(style: &Style, n: usize) -> Span {
    if style.halftone.as_ref().and_then(|h| h.glass()).is_none() {
        return Span::WHOLE;
    }
    let n = n as i64;
    let origin = style.quiet_zone as i64 + scene::square_margin(style, n);
    let card = n + 2 * origin;
    Span {
        start: origin as f64 / card as f64,
        len: n as f64 / card as f64,
    }
}

/// The glass's size in the document: the baked layer's PNG, estimated
/// (baking is too slow for every version fitting checks), or vector glass's
/// elements, drawn and counted.
pub(crate) fn estimated_bytes(style: &Style, g: &Glass, qr: &QrMatrix) -> usize {
    if g.is_vector() {
        let canvas = scene::canvas(style, qr);
        return vector(style, qr, &canvas).map_or(0, |v| v.bytes());
    }
    let (n, q) = (qr.size() as usize, style.quiet_zone as usize);
    let side = ((n + 2 * q) * g.resolution as usize) as f64;
    (side * side * BYTES_PER_PIXEL) as usize
}

/// Where the photo's `<image>` goes, in modules: x, y, width, height. The
/// picture's square (as `fit` and `focal` choose it) fills the card; the
/// rest is clipped.
pub(crate) fn photo_box(h: &Halftone, canvas: &Canvas) -> (f64, f64, f64, f64) {
    let (side, x0, y0) = halftone::square(&h.image, h.fit, h.focal);
    let k = canvas.card as f64 / side;
    (
        -x0 * k,
        -y0 * k,
        h.image.width() as f64 * k,
        h.image.height() as f64 * k,
    )
}

/// The baked glass layer: a PNG covering the code and its quiet zone.
pub(crate) struct Baked {
    pub(crate) png: Vec<u8>,
    /// The layer's top-left corner and side, in modules.
    pub(crate) at: i64,
    pub(crate) side: i64,
}

// The measured look (SPEC §7.9, W20261119133827-0, Look E).
/// The Gaussian that blends shift, feather and tint between lenses.
const BLEND_SIGMA: f64 = 0.35;
/// Tint strength for data lenses and for finder glass, and the chroma boost
/// finder glass gets.
const TINT_DATA: f64 = 0.35;
/// How far glass moves toward its token's hue with the palette's colours:
/// most of the way, so the colour reads, with a hint of the photo's own.
const PALETTE_TINT: f64 = 0.85;
const TINT_FINDER: f64 = 0.8;
const FINDER_CHROMA: f64 = 1.6;
/// The halo's target sits this far under the plate.
const HALO_DROP: f64 = 0.08;
/// Feathers of function and halo glass, in modules.
const FUNC_FEATHER: f64 = 0.06;
const HALO_FEATHER: f64 = 0.1;
/// Corner radius as a share of the half width.
const CORNER: f64 = 0.38;
/// Dotted timing's glass dots, half width.
const DOT_HALF: f64 = 0.4;
/// Central boxes the photo's lightness is read over: data (at least),
/// function, halo.
const READ_DATA_MIN: f64 = 0.5;
const READ_FUNC: f64 = 0.8;
const READ_HALO: f64 = 0.6;
/// The quartic bevel: its width and height, the refraction's scale, the
/// index, and the steepest slope.
const BEVEL_DEPTH: f64 = 0.12;
const BEVEL_HEIGHT: f64 = 0.08;
const REFRACT_SCALE: f64 = 0.15;
const ETA: f64 = 1.5;
const MAX_SLOPE: f64 = 12.0;
/// Normals are taken this far in from an edge.
const NORMAL_BAND: f64 = 0.2;
/// A dome's reach inside a lens edge (half the default lens), and how much
/// of the distance to there it pulls the photo at full strength.
const DOME_RADIUS: f64 = 0.45;
const DOME_SCALE: f64 = 0.8;
/// The rim light and the inner shade: falloff and strength.
const RIM_WIDTH: f64 = 0.03;
const RIM_LIGHT: f64 = 0.6;
const SHADE_WIDTH: f64 = 0.06;
const SHADE: f64 = 0.22;
/// The drop shadow: reach, opacity, softness and offset.
const SHADOW_REACH: f64 = 0.15;
const SHADOW_ALPHA: f64 = 0.10;
const SHADOW_SOFT: f64 = 0.03;
const SHADOW_OFFSET: (f64, f64) = (0.035, 0.05);
/// Finite-difference step for the normal.
const NORMAL_STEP: f64 = 0.01;

/// What a module's glass is.
#[derive(Clone, Copy, Default)]
struct Lens {
    /// 0 none, 1 dark glass, 2 light glass.
    kind: u8,
    /// The OKLab lightness shift.
    delta: f64,
    half: f64,
    radius: f64,
    feather: f64,
    tint: f64,
    /// 1 for function and halo glass, drawn flat at the finder or plate
    /// lightness.
    func: f64,
    /// The OKLab a, b the tint pulls toward, with the palette's colours.
    ab: [f64; 2],
    /// The photo's read lightness under the module (what its shift was
    /// worked out from), for function glass's texture.
    read: f64,
}

/// The cells of the box (the code and its quiet zone), by code module.
struct Lenses {
    cells: Vec<Lens>,
    /// The quiet zone, in modules.
    q: i64,
    /// The box's side, in modules.
    w: i64,
    union: f64,
}

impl Lenses {
    fn get(&self, i: i64, j: i64) -> Option<&Lens> {
        let (a, b) = (i + self.q, j + self.q);
        if a < 0 || b < 0 || a >= self.w || b >= self.w {
            return None;
        }
        Some(&self.cells[(b * self.w + a) as usize])
    }

    /// Whether any cell around `(ci, cj)` has glass.
    fn near(&self, ci: i64, cj: i64) -> bool {
        (-1..=1)
            .any(|dj| (-1..=1).any(|di| self.get(ci + di, cj + dj).is_some_and(|c| c.kind != 0)))
    }

    /// The signed distance to `kind`'s glass at `(u, v)` (code modules):
    /// the smooth union of the squircles around it.
    fn dist(&self, u: f64, v: f64, kind: u8) -> f64 {
        let (ci, cj) = (math::floor(u) as i64, math::floor(v) as i64);
        let mut d = f64::INFINITY;
        for dj in -1..=1 {
            for di in -1..=1 {
                let Some(c) = self.get(ci + di, cj + dj) else {
                    continue;
                };
                if c.kind != kind {
                    continue;
                }
                let (dx, dy) = (u - (ci + di) as f64 - 0.5, v - (cj + dj) as f64 - 0.5);
                let dd = rounded_box(dx, dy, c.half, c.radius);
                d = if d == f64::INFINITY {
                    dd
                } else {
                    smin(d, dd, self.union)
                };
            }
        }
        d
    }

    /// [`Lenses::dist`] at a pixel center, with the shift, feather, tint
    /// and flatness blended by `wx`/`wy` (the Gaussian at the pixel, per
    /// neighbor column and row).
    fn field(
        &self,
        (u, v): (f64, f64),
        (ci, cj): (i64, i64),
        kind: u8,
        wx: &[f64; 3],
        wy: &[f64; 3],
    ) -> Option<(f64, [f64; 7])> {
        let mut d = f64::INFINITY;
        let mut sum = [0.0; 7];
        let mut weight = 0.0;
        for dj in -1..=1i64 {
            for di in -1..=1i64 {
                let Some(c) = self.get(ci + di, cj + dj) else {
                    continue;
                };
                if c.kind != kind {
                    continue;
                }
                let (dx, dy) = (u - (ci + di) as f64 - 0.5, v - (cj + dj) as f64 - 0.5);
                let dd = rounded_box(dx, dy, c.half, c.radius);
                d = if d == f64::INFINITY {
                    dd
                } else {
                    smin(d, dd, self.union)
                };
                let k = wx[(di + 1) as usize] * wy[(dj + 1) as usize];
                sum[0] += k * c.delta;
                sum[1] += k * c.feather;
                sum[2] += k * c.tint;
                sum[3] += k * c.func;
                sum[4] += k * c.ab[0];
                sum[5] += k * c.ab[1];
                sum[6] += k * c.read;
                weight += k;
            }
        }
        (weight > 0.0).then(|| (d, sum.map(|s| s / weight)))
    }
}

/// Photo pixels per module the readability estimate samples.
const READ_SAMPLE: i64 = 4;
/// A data module counts as clearly readable when it lands at least this
/// far from the dark/light midpoint (OKLab L), for [`Readability::share`].
const READ_CLEAR: f64 = 0.10;

/// What the composited glass leaves of the code (SPEC §7.9,
/// W20261124091813), estimated per module without baking: each data
/// module's centre is its glass's target over the photo's own lightness at
/// the glass's opacity (or the photo bare where no glass is needed),
/// blended in sRGB as renderers do, and its margin is how far that lands
/// from the midpoint of the safe targets ([`GLASS_SAFE_TARGETS`]), on its
/// own side. The finder contrast is the worst corner's mean light finder
/// glass minus its mean dark glass, composited the same way.
pub(crate) struct Readability {
    /// The share of data modules at least [`READ_CLEAR`] from the
    /// midpoint, 0–1: for people.
    pub(crate) share: f32,
    /// The weakest tenth's margin (the 10th percentile), OKLab L: what the
    /// check holds to [`GLASS_READABILITY`](crate::GLASS_READABILITY).
    pub(crate) margin: f32,
    /// The composited finder contrast, OKLab L.
    pub(crate) finder: f32,
}

pub(crate) fn readability(style: &Style, qr: &QrMatrix) -> Option<Readability> {
    let h = style.halftone.as_ref()?;
    let g = h.glass()?;
    let canvas = scene::canvas(style, qr);
    let n = qr.size() as i64;
    let s = READ_SAMPLE;
    let side = (n * s) as usize;
    let backdrop = halftone::linear(style.palette.background());
    let sampler = Sampler::new(
        &h.image,
        h.fit,
        h.focal,
        Span::WHOLE,
        (canvas.card * s) as usize,
    );
    let first = canvas.origin * s;
    let mut photo = Vec::with_capacity(side * side);
    for y in 0..side as i64 {
        for x in 0..side as i64 {
            photo.push(sampler.cell(first + x, first + y).over_linear(backdrop));
        }
    }
    let read = |i: i64, j: i64| -> f64 {
        let (mut sum, mut count) = ([0.0; 3], 0.0);
        // The central half of the module.
        for y in j * s + s / 4..j * s + s - s / 4 {
            for x in i * s + s / 4..i * s + s - s / 4 {
                let p = photo[y as usize * side + x as usize];
                for k in 0..3 {
                    sum[k] += p[k];
                }
                count += 1.0;
            }
        }
        oklab(sum.map(|v| v / f64::max(count, 1.0)))[0]
    };
    let opacity = (g.opacity as f64).clamp(0.0, 1.0);
    // Lightness `glass` over `under` at the opacity, blended in sRGB.
    let over = |glass: f64, under: f64| {
        let enc = |l: f64| encode((l * l * l).clamp(0.0, 1.0));
        let v = opacity * enc(glass) + (1.0 - opacity) * enc(under);
        cbrt(crate::color::srgb_to_linear(v))
    };
    let (dark_t, light_t) = (g.targets.0 as f64, g.targets.1 as f64);
    let (safe_dark, safe_light) = (GLASS_SAFE_TARGETS.0 as f64, GLASS_SAFE_TARGETS.1 as f64);
    let (finder, plate) = (g.finders.0 as f64, g.finders.1 as f64);
    let removed = scene::removed(style, qr);
    let mid = (safe_dark + safe_light) / 2.0;
    let mut margins: Vec<f64> = Vec::new();
    // Per corner: light and dark finder glass sums and counts.
    let mut corners = [([0.0f64; 2], [0usize; 2]); 3];
    for (x, y, dark, role) in qr.modules() {
        let (i, j) = (x as i64, y as i64);
        if removed.get(i, j) {
            continue;
        }
        let l = read(i, j);
        match role {
            ModuleRole::Data | ModuleRole::Ecc | ModuleRole::Remainder => {
                let at = if dark {
                    if l > dark_t { over(dark_t, l) } else { l }
                } else if l < light_t {
                    over(light_t, l)
                } else {
                    l
                };
                margins.push(if dark { mid - at } else { at - mid });
            }
            ModuleRole::Finder { corner, .. } => {
                let c = scene::CORNERS
                    .iter()
                    .position(|&k| k == corner)
                    .unwrap_or(0);
                let (glass, slot) = if dark { (finder, 1) } else { (plate, 0) };
                corners[c].0[slot] += over(glass, l);
                corners[c].1[slot] += 1;
            }
            _ => {}
        }
    }
    let finder_contrast = corners
        .iter()
        .map(|(sum, count)| {
            sum[0] / f64::max(count[0] as f64, 1.0) - sum[1] / f64::max(count[1] as f64, 1.0)
        })
        .fold(f64::INFINITY, f64::min);
    let clear = margins.iter().filter(|&&m| m >= READ_CLEAR).count();
    let share = if margins.is_empty() {
        1.0
    } else {
        clear as f32 / margins.len() as f32
    };
    crate::sort::sort_by(&mut margins, |a, b| a.total_cmp(b));
    let margin = margins.get(margins.len() / 10).copied().unwrap_or(mid);
    Some(Readability {
        share,
        margin: margin as f32,
        finder: finder_contrast as f32,
    })
}

/// Bakes `style`'s glass for `qr`, if it has glass.
pub(crate) fn bake(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Option<Baked> {
    let h = style.halftone.as_ref()?;
    let g = h.glass().filter(|g| !g.is_vector())?;
    let n = qr.size() as i64;
    let q = style.quiet_zone as i64;
    let res = g.resolution.clamp(8, 32) as i64;
    let r = res as f64;
    let w = n + 2 * q;
    let wpx = (w * res) as usize;
    let mpx = res as usize;
    let side = wpx + 2 * mpx;

    // The photo over the box and a module around it, linear sRGB over bg.
    let backdrop = halftone::linear(style.palette.background());
    let sampler = Sampler::new(
        &h.image,
        h.fit,
        h.focal,
        Span::WHOLE,
        (canvas.card * res) as usize,
    );
    let first = (canvas.origin - q - 1) * res;
    let mut photo = Vec::with_capacity(side * side);
    for y in 0..side as i64 {
        for x in 0..side as i64 {
            photo.push(sampler.cell(first + x, first + y).over_linear(backdrop));
        }
    }

    // The photo's OKLab lightness over a module's central box, unstretched.
    let center = |k: i64| (k + q + 1) as f64 * r + 0.5 * r;
    let lightness = |i: i64, j: i64, size: f64| -> f64 {
        let half = size / 2.0 * r;
        let range = |c: f64| {
            let lo = math::ceil(c - half - 0.5).max(0.0) as usize;
            let hi = (math::floor(c + half - 0.5) as usize).min(side - 1);
            lo..=hi
        };
        let (mut sum, mut count) = ([0.0; 3], 0.0);
        for y in range(center(j)) {
            for x in range(center(i)) {
                let p = photo[y * side + x];
                for k in 0..3 {
                    sum[k] += p[k];
                }
                count += 1.0;
            }
        }
        oklab(sum.map(|s| s / f64::max(count, 1.0)))[0]
    };

    // The picture's dominant dark and light colours under the code: the
    // mean OKLab a, b of its darkest and lightest quarters.
    let (dark_ab, light_ab) = {
        let from = mpx + (q * res) as usize;
        let to = from + (n * res) as usize;
        let mut labs = Vec::new();
        for y in (from..to).step_by(3) {
            for x in (from..to).step_by(3) {
                labs.push(oklab(photo[y * side + x]));
            }
        }
        crate::sort::sort_by(&mut labs, |a, b| a[0].total_cmp(&b[0]));
        let quarter = labs.len() / 4;
        let mean_ab = |s: &[[f64; 3]]| {
            let (a, b) = s
                .iter()
                .fold((0.0, 0.0), |acc, c| (acc.0 + c[1], acc.1 + c[2]));
            let k = f64::max(s.len() as f64, 1.0);
            [a / k, b / k]
        };
        (
            mean_ab(&labs[..quarter]),
            mean_ab(&labs[labs.len() - quarter..]),
        )
    };

    // Each module's glass.
    let (dark_t, light_t) = (g.targets.0 as f64, g.targets.1 as f64);
    let (finder, plate) = (g.finders.0 as f64, g.finders.1 as f64);
    let (lens, core) = (g.lens as f64, g.core as f64);
    let feather = ((lens - core) / 2.0).max(0.0);
    let cleared = scene::cleared(style, qr);
    let removed = scene::removed(style, qr);
    let dotted = style.timing == TimingStyle::Dotted;
    let plain = |half: f64| (half, CORNER * half);
    let mut cells = vec![Lens::default(); (w * w) as usize];
    for j in -q..n + q {
        for i in -q..n + q {
            let at = ((j + q) * w + i + q) as usize;
            let inside = i >= 0 && j >= 0 && i < n && j < n;
            if !inside {
                let dist = (-i).max(i - n + 1).max(-j).max(j - n + 1);
                if dist <= g.halo as i64 {
                    let l = lightness(i, j, READ_HALO);
                    let (half, radius) = plain(0.5);
                    cells[at] = Lens {
                        kind: 2,
                        delta: (plate - HALO_DROP - l).max(0.0),
                        half,
                        radius,
                        feather: HALO_FEATHER,
                        tint: 0.0,
                        func: 1.0,
                        ab: [0.0; 2],
                        read: l,
                    };
                }
                continue;
            }
            let (x, y) = (i as u16, j as u16);
            if cleared.get(i, j) {
                // Outlined around an alignment pattern: a clean plate.
                let l = lightness(i, j, READ_FUNC);
                let (half, radius) = plain(0.5);
                cells[at] = Lens {
                    kind: 2,
                    delta: (plate - l).max(0.0),
                    half,
                    radius,
                    feather: FUNC_FEATHER,
                    tint: 0.0,
                    func: 1.0,
                    ab: [0.0; 2],
                    read: l,
                };
                continue;
            }
            if removed.get(i, j) {
                // Under the logo's plate.
                continue;
            }
            let dark = qr.is_dark(x, y);
            let role = qr.role(x, y);
            cells[at] = if role.is_function() {
                let l = lightness(i, j, READ_FUNC);
                if dark {
                    let (half, radius) = if dotted && role == ModuleRole::Timing {
                        (DOT_HALF, DOT_HALF)
                    } else {
                        plain(0.5)
                    };
                    Lens {
                        kind: 1,
                        delta: (finder - l).min(0.0),
                        half,
                        radius,
                        feather: FUNC_FEATHER,
                        tint: TINT_FINDER,
                        func: 1.0,
                        ab: [0.0; 2],
                        read: l,
                    }
                } else {
                    let (half, radius) = plain(0.5);
                    Lens {
                        kind: 2,
                        delta: (plate - l).max(0.0),
                        half,
                        radius,
                        feather: FUNC_FEATHER,
                        tint: 0.0,
                        func: 1.0,
                        ab: [0.0; 2],
                        read: l,
                    }
                }
            } else {
                let l = lightness(i, j, core.max(READ_DATA_MIN));
                let (half, radius) = plain(lens / 2.0);
                let lens = |kind, delta| Lens {
                    kind,
                    delta,
                    half,
                    radius,
                    feather,
                    tint: TINT_DATA,
                    func: 0.0,
                    ab: [0.0; 2],
                    read: l,
                };
                if dark && l > dark_t {
                    lens(1, dark_t - l)
                } else if !dark && l < light_t {
                    lens(2, light_t - l)
                } else {
                    Lens::default()
                }
            };
        }
    }
    // With the palette's colours, each lens pulls toward its module's own
    // token, light glass and the plate toward bg's hue (W20261124091813).
    if g.colors == GlassColors::Palette {
        let tokens = module_tokens(style, qr);
        let bg = paint_ab(&style.palette.resolve(Token::Bg));
        for j in -q..n + q {
            for i in -q..n + q {
                let c = &mut cells[((j + q) * w + i + q) as usize];
                let inside = i >= 0 && j >= 0 && i < n && j < n;
                match c.kind {
                    0 => continue,
                    1 if inside => {
                        let token = tokens[(j * n + i) as usize];
                        c.ab = paint_ab(&style.palette.resolve(token));
                    }
                    _ => c.ab = bg,
                }
                c.tint = PALETTE_TINT;
            }
        }
    }
    let lenses = Lenses {
        cells,
        q,
        w,
        union: (g.union as f64).max(1e-3),
    };
    // Any shape but plain squares: glass cut from the code's own geometry.
    let raster = (!field::analytic(style))
        .then(|| field::fields(style, qr, canvas, &lenses.cells, q, w, res, lens));
    let shadow_px = (
        math::round(SHADOW_OFFSET.0 * r) as i64,
        math::round(SHADOW_OFFSET.1 * r) as i64,
    );

    // The frost, and the Gaussian blend's weights per pixel offset.
    let frost = blur(&photo, side, g.frost as f64 * r);
    let gauss: Vec<[f64; 3]> = (0..res)
        .map(|k| {
            let f = (k as f64 + 0.5) / r;
            core::array::from_fn(|d| {
                let dx = f - 0.5 - (d as f64 - 1.0);
                math::exp(-dx * dx / (2.0 * BLEND_SIGMA * BLEND_SIGMA))
            })
        })
        .collect();
    let sample = |sx: f64, sy: f64| -> [f64; 3] {
        let top = side as f64 - 1.001;
        let x = (sx + mpx as f64 - 0.5).clamp(0.0, top);
        let y = (sy + mpx as f64 - 0.5).clamp(0.0, top);
        let (xi, yi) = (x as usize, y as usize);
        let (tx, ty) = (x - xi as f64, y - yi as f64);
        let a = frost[yi * side + xi];
        let b = frost[yi * side + xi + 1];
        let c = frost[(yi + 1) * side + xi];
        let d = frost[(yi + 1) * side + xi + 1];
        core::array::from_fn(|k| {
            (a[k] * (1.0 - tx) + b[k] * tx) * (1.0 - ty) + (c[k] * (1.0 - tx) + d[k] * tx) * ty
        })
    };
    let aa = 1.0 / r;
    let texture = g.texture as f64;
    let opacity = g.opacity as f64;
    // The default bevel is exactly the measured one (0.12 as f32, scaled).
    let bevel = BEVEL_DEPTH * (g.bevel as f64 / 0.12f32 as f64);
    let eta = g.ior as f64;
    let dome = g.dome as f64;
    let normal_band = if dome > 0.0 {
        DOME_RADIUS + 0.05
    } else if bevel <= BEVEL_DEPTH {
        NORMAL_BAND
    } else {
        bevel + 0.08
    };
    let light_dir = [-core::f64::consts::FRAC_1_SQRT_2; 2];
    let mut rgba = vec![0u8; wpx * wpx * 4];
    for y in 0..wpx {
        let cj = (y / mpx) as i64 - q;
        let wy = &gauss[y % mpx];
        let v = cj as f64 + (((y % mpx) as f64) + 0.5) / r;
        for x in 0..wpx {
            let ci = (x / mpx) as i64 - q;
            if !lenses.near(ci, cj) {
                continue;
            }
            let wx = &gauss[x % mpx];
            let u = ci as f64 + (((x % mpx) as f64) + 0.5) / r;
            let (px, py) = (x as i64, y as i64);
            // The raster field's distance, where there is one, with the
            // cells' blended shift, feather, tint and flatness.
            let cut = |f: Option<(f64, [f64; 7])>, kind: u8| match (&raster, f) {
                (Some(fields), Some((_, p))) => Some((fields.dist(kind, px, py), p)),
                (_, f) => f,
            };
            let dark = cut(lenses.field((u, v), (ci, cj), 1, wx, wy), 1);
            let light = cut(lenses.field((u, v), (ci, cj), 2, wx, wy), 2);
            let (kind, d, [delta, feather, tint, func, ab_a, ab_b, read]) = match (dark, light) {
                (Some(a), Some(b)) if a.0 <= b.0 => (1u8, a.0, a.1),
                (Some(a), None) => (1, a.0, a.1),
                (_, Some(b)) => (2, b.0, b.1),
                (None, None) => continue,
            };
            let mut out = [0.0; 3];
            let mut alpha = 0.0;
            if d < 0.0 {
                alpha = if feather <= 0.0 {
                    (-d / aa + 0.5).clamp(0.0, 1.0)
                } else {
                    smoothstep(0.0, feather, -d)
                };
                // The outward normal, from the field's gradient.
                let normal = if (g.refraction || g.rim) && -d < normal_band {
                    if let Some(fields) = &raster {
                        fields.normal(kind, px, py)
                    } else {
                        let gx = lenses.dist(u + NORMAL_STEP, v, kind) - d;
                        let gy = lenses.dist(u, v + NORMAL_STEP, kind) - d;
                        let len = sqrt(gx * gx + gy * gy).max(1e-9);
                        [gx / len, gy / len]
                    }
                } else {
                    [0.0, 0.0]
                };
                let (mut sx, mut sy) = (x as f64 + 0.5, y as f64 + 0.5);
                if g.refraction {
                    let off = (refraction(-d, bevel, eta) + dome_pull(-d, dome)) * r;
                    sx -= normal[0] * off;
                    sy -= normal[1] * off;
                }
                let mut lab = oklab(sample(sx, sy));
                let photo_l = lab[0];
                lab[0] = (lab[0] + delta).clamp(0.0, 1.0);
                if g.colors == GlassColors::Palette && tint > 0.0 {
                    lab[1] += (ab_a - lab[1]) * tint;
                    lab[2] += (ab_b - lab[2]) * tint;
                } else if g.tints() && tint > 0.0 {
                    let target = if kind == 1 {
                        let boost = if tint > 0.5 { FINDER_CHROMA } else { 1.0 };
                        [dark_ab[0] * boost, dark_ab[1] * boost]
                    } else {
                        light_ab
                    };
                    lab[1] += (target[0] - lab[1]) * tint;
                    lab[2] += (target[1] - lab[2]) * tint;
                }
                if func > 0.5 {
                    // Function glass is flat at its lightness, with as much
                    // of the photo's detail around it as `texture` asks.
                    let flat = if kind == 1 { finder } else { plate };
                    lab[0] = (flat + texture * (photo_l - read)).clamp(0.0, 1.0);
                }
                let mut c = from_oklab(lab).map(encode);
                if g.rim {
                    let facing = normal[0] * light_dir[0] + normal[1] * light_dir[1];
                    let spec = math::exp(d / RIM_WIDTH) * facing.max(0.0) * RIM_LIGHT;
                    let shade = math::exp(d / SHADE_WIDTH) * (-facing).max(0.0) * SHADE;
                    c = c.map(|v| (v + spec * (1.0 - v)) * (1.0 - shade));
                }
                out = c;
            } else if g.rim && d < SHADOW_REACH {
                let ds = match &raster {
                    Some(fields) => fields.dist(kind, px - shadow_px.0, py - shadow_px.1),
                    None => lenses.dist(u - SHADOW_OFFSET.0, v - SHADOW_OFFSET.1, kind),
                };
                if ds < 0.0 {
                    alpha = SHADOW_ALPHA * smoothstep(0.0, SHADOW_SOFT, -ds);
                }
            }
            // Below full opacity the photo shows through, unshifted.
            let a8 = math::round(alpha * opacity * 255.0).clamp(0.0, 255.0);
            if a8 > 0.0 {
                let i = (y * wpx + x) * 4;
                for k in 0..3 {
                    rgba[i + k] = math::round(out[k] * 255.0).clamp(0.0, 255.0) as u8;
                }
                rgba[i + 3] = a8 as u8;
            }
        }
    }
    let image = Rgba8Image::new(wpx as u32, wpx as u32, rgba).ok()?;
    Some(Baked {
        png: crate::png::encode_rgba(&image),
        at: canvas.origin - q,
        side: w,
    })
}

/// A paint's OKLab a, b: a colour's own, a gradient's stops' mean.
pub(crate) fn paint_ab(paint: &Paint) -> [f64; 2] {
    let ab = |c: &crate::Color| {
        let [_, a, b] = c.to_oklab();
        [a as f64, b as f64]
    };
    match paint {
        Paint::Solid(c) => ab(c),
        Paint::Gradient(g) => {
            let stops = g.stops();
            let sum = stops.iter().fold([0.0; 2], |acc, s| {
                let v = ab(&s.color);
                [acc[0] + v[0], acc[1] + v[1]]
            });
            let k = f64::max(stops.len() as f64, 1.0);
            [sum[0] / k, sum[1] / k]
        }
    }
}

/// Each code module's token as a plain render paints it: the data colors
/// as spread (SPEC §6.1), timing, alignment, and each finder's ring and eye
/// (a corner's own token, else the shared one, else `fg`, by
/// [`Palette::resolve`](crate::Palette::resolve)'s fallbacks).
fn module_tokens(style: &Style, qr: &QrMatrix) -> Vec<Token> {
    let n = qr.size() as usize;
    let mut out = vec![Token::Fg; n * n];
    let mut plain = style.clone();
    plain.halftone = None;
    for layer in scene::layers(&plain, qr) {
        if let scene::Part::Modules(grid) | scene::Part::Timing(grid) = &layer.part {
            for y in 0..n {
                for x in 0..n {
                    if grid.get(x as i64, y as i64) {
                        out[y * n + x] = layer.token;
                    }
                }
            }
        }
    }
    for (x, y, _, role) in qr.modules() {
        let at = y as usize * n + x as usize;
        out[at] = match role {
            ModuleRole::Finder {
                corner,
                ring: crate::matrix::FinderRing::Eye,
            } => Token::CornerEye(corner),
            ModuleRole::Finder { corner, .. } => Token::CornerOuter(corner),
            ModuleRole::Alignment { .. } => match style.alignment.emphasis {
                crate::scanbox::Emphasis::None => Token::Fg,
                _ => Token::Alignment,
            },
            _ => out[at],
        };
    }
    out
}

/// How far the photo is pulled in at `depth` modules inside a lens edge:
/// the quartic bevel `h(t) = (1 − (1 − t)⁴)^¼`, its slope's angle θ, and
/// Snell's law at [`ETA`]: `tan(θ − asin(sin θ / η))`, without
/// trigonometry.
fn refraction(depth: f64, bevel: f64, eta: f64) -> f64 {
    let t = (depth / bevel).clamp(0.0, 1.0);
    if t >= 1.0 {
        return 0.0;
    }
    let s = 1.0 - t;
    let one = 1.0 - s * s * s * s;
    // h'(t) = one^(−3/4) · (1 − t)³.
    let root = sqrt(one.max(1e-6));
    let dh = s * s * s / (root * sqrt(root));
    // The bevel keeps its profile at any depth; a deeper one is thicker
    // glass, so it moves the photo further.
    let slope = (BEVEL_HEIGHT * dh / BEVEL_DEPTH).min(MAX_SLOPE);
    let sin_t = slope / sqrt(1.0 + slope * slope);
    let sin_r = sin_t / eta;
    let tan_r = sin_r / sqrt(1.0 - sin_r * sin_r);
    REFRACT_SCALE * (bevel / BEVEL_DEPTH) * (slope - tan_r) / (1.0 + slope * tan_r)
}

/// How far a dome of strength `dome` pulls the photo toward a lens's
/// middle at `depth` modules inside its edge: a magnifier's pull, largest
/// at the edge and none past [`DOME_RADIUS`] (W20261124091813).
fn dome_pull(depth: f64, dome: f64) -> f64 {
    if dome <= 0.0 {
        return 0.0;
    }
    dome * DOME_SCALE * (DOME_RADIUS - depth).max(0.0)
}

/// IQ's quadratic smooth minimum.
fn smin(a: f64, b: f64, k: f64) -> f64 {
    let h = (k - (a - b).abs()).max(0.0);
    a.min(b) - 0.25 * h * h / k
}

/// The signed distance to a square of half width `half` with corners of
/// `radius`, centered on the origin.
fn rounded_box(px: f64, py: f64, half: f64, radius: f64) -> f64 {
    let (qx, qy) = (px.abs() - half + radius, py.abs() - half + radius);
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    let outside = if ox > 0.0 && oy > 0.0 {
        sqrt(ox * ox + oy * oy)
    } else {
        ox + oy
    };
    outside + qx.max(qy).min(0.0) - radius
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A Gaussian blur of a `side` × `side` image, `sigma` pixels, edges
/// clamped.
fn blur(src: &[[f64; 3]], side: usize, sigma: f64) -> Vec<[f64; 3]> {
    if sigma.is_nan() || sigma <= 0.0 {
        return src.to_vec();
    }
    let r = math::ceil(sigma * 3.0) as i64;
    let mut kernel: Vec<f64> = (-r..=r)
        .map(|i| math::exp(-((i * i) as f64) / (2.0 * sigma * sigma)))
        .collect();
    let total: f64 = kernel.iter().sum();
    for k in &mut kernel {
        *k /= total;
    }
    let s = side as i64;
    let pass = |src: &[[f64; 3]], horizontal: bool| -> Vec<[f64; 3]> {
        let mut out = vec![[0.0; 3]; src.len()];
        for y in 0..s {
            for x in 0..s {
                let mut acc = [0.0; 3];
                for (j, k) in kernel.iter().enumerate() {
                    let d = j as i64 - r;
                    let (xx, yy) = if horizontal {
                        ((x + d).clamp(0, s - 1), y)
                    } else {
                        (x, (y + d).clamp(0, s - 1))
                    };
                    let p = src[(yy * s + xx) as usize];
                    for c in 0..3 {
                        acc[c] += p[c] * k;
                    }
                }
                out[(y * s + x) as usize] = acc;
            }
        }
        out
    };
    pass(&pass(src, true), false)
}

/// √x for x ≥ 0: Newton's iteration from an exponent-halving guess, five
/// steps (the guess is within 6%, so the fifth step is exact to the last
/// bit or two). Basic operations only, so every platform agrees.
fn sqrt(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 {
        return 0.0;
    }
    let mut r = f64::from_bits((x.to_bits() >> 1) + 0x1ff8_0000_0000_0000);
    for _ in 0..5 {
        r = 0.5 * (r + x / r);
    }
    r
}

/// ∛x for x ≥ 0: Halley's iteration from Kahan's exponent-thirding guess
/// (within 4%), three steps.
fn cbrt(x: f64) -> f64 {
    if x.is_nan() || x <= 1e-300 {
        return 0.0;
    }
    let mut r = f64::from_bits(x.to_bits() / 3 + 0x2a9f_7893_0000_0000);
    for _ in 0..3 {
        let r3 = r * r * r;
        r *= (r3 + 2.0 * x) / (2.0 * r3 + x);
    }
    r
}

/// Linear sRGB to gamma-encoded, `1.055·c^(5/12) − 0.055` as
/// `√(√c) · ∛(√c)`.
fn encode(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        return 12.92 * c;
    }
    let s = sqrt(c);
    1.055 * sqrt(s) * cbrt(s) - 0.055
}

/// Linear sRGB to OKLab (Björn Ottosson's matrices, as in `color`).
fn oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
    let m = cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
    let s = cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// OKLab to linear sRGB, clamped per channel.
fn from_oklab([l, a, b]: [f64; 3]) -> [f64; 3] {
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
    .map(|v| v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newton_roots_match_the_crate_math() {
        for i in 0..4000 {
            let x = i as f64 * 0.00025 + 1e-9;
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-14 * b.abs().max(1e-300);
            assert!(close(sqrt(x), math::sqrt(x)), "sqrt {x}");
            assert!(close(cbrt(x), math::cbrt(x)), "cbrt {x}");
            let c = crate::color::linear_to_srgb(x);
            assert!((encode(x) - c).abs() < 1e-13, "encode {x}");
        }
        assert_eq!((sqrt(0.0), cbrt(0.0)), (0.0, 0.0));
    }

    #[test]
    fn refraction_pulls_only_inside_the_bevel() {
        assert_eq!(refraction(BEVEL_DEPTH, BEVEL_DEPTH, ETA), 0.0);
        assert_eq!(refraction(0.5, BEVEL_DEPTH, ETA), 0.0);
        let edge = refraction(0.001, BEVEL_DEPTH, ETA);
        let mid = refraction(BEVEL_DEPTH / 2.0, BEVEL_DEPTH, ETA);
        assert!(edge > mid && mid > 0.0, "{edge} {mid}");
        // At most about 0.14 modules, as the spike measured.
        assert!(edge < 0.15, "{edge}");
    }

    #[test]
    fn margins_round_to_whole_modules() {
        // v5 with a 4-module quiet zone: 45 / 0.84 · 0.08 ≈ 4.3.
        assert_eq!(margin(0.08, 37, 4), 4);
        assert_eq!(margin(0.0, 37, 4), 0);
        assert_eq!(margin(f32::NAN, 37, 4), 0);
        assert_eq!(margin(0.25, 21, 4), 15);
        assert_eq!(margin(9.0, 21, 4), 15);
    }

    use crate::{
        Border, Checks, Ecc, Frame, Halftone, ImageError, Ink, Logo, ModuleShape, QrCode,
        RasterFormat, RenderError, Rgba8Image, Style, TimingStyle, Version, Warning,
    };
    use crate::{GLASS_TARGET_MARGIN, LARGE_OUTPUT_BYTES};
    use alloc::string::String;

    /// A small picture with light and dark parts, and its PNG file.
    fn picture() -> Halftone {
        let mut image = Rgba8Image::filled(24, 24, [235, 225, 210, 255]).unwrap();
        for y in 0..24 {
            for x in 0..24 {
                if (x as i32 - 12).pow(2) + (y as i32 - 12).pow(2) < 60 {
                    image.set_pixel(x, y, [40, 50, 90, 255]);
                }
            }
        }
        let file = crate::png::encode_rgba(&image);
        Halftone::new(image)
            .source(file, RasterFormat::Png)
            .unwrap()
    }

    fn baked() -> Glass {
        Glass::new().render(GlassRender::Baked)
    }

    fn vector_glass() -> Glass {
        Glass::new().render(GlassRender::Vector)
    }

    fn glass(g: Glass) -> Style {
        Style::new().halftone(picture().ink(Ink::Glass(g)))
    }

    fn code(style: &Style, version: u8) -> crate::QrMatrix {
        QrCode::builder()
            .ecc(Ecc::M)
            .boost_ecc(false)
            .version(Version::new(version).unwrap())
            .encode_text_for("HELLO", style)
            .unwrap()
    }

    #[test]
    fn defaults_render_clean_at_small_versions() {
        let style = glass(Glass::new());
        let qr = code(&style, crate::GLASS_FULL_BAR_VERSION);
        let report = style.check(&qr);
        assert!(report.ok() && report.scannable, "{report:?}");
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert!((report.min_contrast - 0.62).abs() < 1e-3);
        assert!(report.steering.is_some());
    }

    #[test]
    fn sources_are_signature_checked() {
        let h = Halftone::new(Rgba8Image::filled(2, 2, [0, 0, 0, 255]).unwrap());
        assert_eq!(
            h.clone().source(&b"GIF89a"[..], RasterFormat::Png),
            Err(ImageError::NotImage(RasterFormat::Png))
        );
        let h = h.source(&b"GIF89a.."[..], RasterFormat::Gif).unwrap();
        assert_eq!(h.source_file().unwrap().1, RasterFormat::Gif);
        // Glass without a file is refused; other inks don't need one.
        let bare = Halftone::new(Rgba8Image::filled(8, 8, [90, 90, 90, 255]).unwrap());
        let style = Style::new().halftone(bare.clone().ink(Ink::Glass(Glass::new())));
        let report = style.check(&code(&style, 2));
        assert!(report.errors.contains(&RenderError::GlassNeedsSource));
        let mono = Style::new().halftone(bare);
        assert!(mono.check(&code(&mono, 2)).ok());
    }

    #[test]
    fn options_are_range_checked() {
        let qr = code(&glass(Glass::new()), 2);
        for (g, option) in [
            (Glass::new().border(0.3), "Glass.border"),
            (Glass::new().halo(5), "Glass.halo"),
            (Glass::new().lens(1.2), "Glass.lens"),
            (Glass::new().lens(0.7).core(0.8), "Glass.core"),
            (Glass::new().union(0.1), "Glass.union"),
            (Glass::new().frost(f32::NAN), "Glass.frost"),
            (Glass::new().resolution(40), "Glass.resolution"),
            (Glass::new().targets(-0.1, 0.9), "Glass.targets dark"),
            (Glass::new().finders(0.3, 1.2), "Glass.finders plate"),
        ] {
            let report = glass(g).check(&qr);
            assert!(
                report.errors.iter().any(
                    |e| matches!(e, RenderError::OptionOutOfRange { option: o, .. } if *o == option)
                ),
                "{option}: {report:?}"
            );
        }
        // The halo may fill a wider quiet zone.
        assert!(glass(Glass::new().halo(5)).quiet_zone(5).check(&qr).ok());
    }

    #[test]
    fn measured_limits_are_refused_unless_allowed() {
        let qr = code(&glass(Glass::new()), 2);
        let cases = [
            (
                Glass::new().halo(0),
                RenderError::GlassHaloTooNarrow { halo: 0 },
                Warning::GlassHaloTooNarrow { halo: 0 },
            ),
            (
                Glass::new().finders(0.5, 0.92),
                RenderError::GlassFinderContrast {
                    delta_l: 0.92 - 0.5,
                    plate: 0.92,
                },
                Warning::GlassFinderContrast {
                    delta_l: 0.92 - 0.5,
                    plate: 0.92,
                },
            ),
            (
                Glass::new().targets(0.35, 0.80),
                RenderError::GlassTargetsTooWeak {
                    dark: 0.35,
                    light: 0.80,
                },
                Warning::GlassTargetsTooWeak {
                    dark: 0.35,
                    light: 0.80,
                },
            ),
        ];
        for (g, error, warning) in cases {
            let strict = glass(g).checks(Checks::Strict);
            let refused = strict.check(&qr);
            assert_eq!(refused.errors, core::slice::from_ref(&error), "{refused:?}");
            assert!(refused.scannable);
            assert!(strict.render(&qr).is_err());
            // A readability error: rendered with its warning by default,
            // and with checks off.
            for allowed in [glass(g), glass(g).checks(Checks::Off)] {
                let report = allowed.check(&qr);
                assert!(report.ok(), "{report:?}");
                assert!(!report.scannable);
                assert!(report.warnings.contains(&warning), "{report:?}");
                assert!(allowed.render(&qr).is_ok());
            }
            // The messages say what's wrong.
            let (e, w) = (alloc::format!("{error}"), alloc::format!("{warning}"));
            assert!(
                e.contains("glass") && w.contains("rendered anyway"),
                "{e} / {w}"
            );
        }
        // Inside the margin: no error.
        let near = Glass::new().targets(0.25 + GLASS_TARGET_MARGIN, 0.90 - GLASS_TARGET_MARGIN);
        assert!(glass(near).check(&qr).ok());
        // Between the floor and the warning: a low-contrast warning only.
        let soft = glass(Glass::new().finders(0.35, 0.95)).check(&qr);
        // The spike's plate, too dark: refused.
        let dim = glass(Glass::new().finders(0.20, 0.88))
            .checks(Checks::Strict)
            .check(&qr);
        assert!(
            matches!(dim.errors[..], [RenderError::GlassFinderContrast { .. }]),
            "{dim:?}"
        );
        assert!(soft.ok() && soft.scannable, "{soft:?}");
        assert!(matches!(
            soft.warnings[..],
            [Warning::LowContrast {
                token: crate::Token::FinderOuter,
                ..
            }]
        ));
    }

    #[test]
    fn check_levels_change_nothing_without_errors() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        for style in [
            Style::new(),
            Style::new().modules(ModuleShape::Dots { size: 0.7 }),
            Style::new().halftone(picture()),
        ] {
            for level in [Checks::Strict, Checks::Lenient, Checks::Off] {
                let leveled = style.clone().checks(level);
                assert_eq!(style.check(&qr), leveled.check(&qr));
                assert!(leveled.check(&qr).scannable);
                assert_eq!(style.render(&qr), leveled.render(&qr));
            }
        }
        // The 1.2 switch maps onto the levels.
        #[allow(deprecated)]
        {
            let old = Style::new().allow_unscannable(false);
            assert_eq!(old.checks_level(), Checks::Strict);
            assert!(!old.unscannable_allowed());
            let old = old.allow_unscannable(true);
            assert_eq!(old.checks_level(), Checks::Lenient);
            assert!(old.unscannable_allowed());
        }
        // A hard error is refused by default and rendered with checks off,
        // the conflicting option left out.
        let circle = glass(Glass::new()).frame(Frame::Circle {
            filler_density: 0.5,
            ring: crate::FrameRing::None,
        });
        let unsupported = RenderError::GlassUnsupported {
            option: "Frame::Circle",
        };
        assert_eq!(
            circle.check(&qr).errors,
            core::slice::from_ref(&unsupported)
        );
        let off = circle.clone().checks(Checks::Off);
        let report = off.check(&qr);
        assert!(report.ok() && !report.scannable, "{report:?}");
        assert!(report.warnings.contains(&Warning::Unchecked(unsupported)));
        assert_eq!(
            off.render(&qr).unwrap(),
            glass(Glass::new()).checks(Checks::Off).render(&qr).unwrap()
        );
    }

    #[test]
    fn larger_codes_warn_about_decoders_and_size() {
        let style = glass(baked());
        let big = code(&style, 14);
        let report = style.check(&big);
        assert!(report.ok(), "{report:?}");
        assert!(
            report
                .warnings
                .contains(&Warning::GlassNeedsZxing { version: 14 })
        );
        assert!(
            report.warnings.iter().any(
                |w| matches!(w, Warning::LargeOutput { bytes } if *bytes > LARGE_OUTPUT_BYTES)
            )
        );
        let small = style.check(&code(&style, crate::GLASS_FULL_BAR_VERSION));
        assert!(small.warnings.is_empty(), "{small:?}");
        // Below the default resolution the full bar wasn't measured.
        let coarse = glass(baked().resolution(8));
        let report = coarse.check(&code(&coarse, 2));
        assert_eq!(report.warnings, [Warning::GlassNeedsZxing { version: 2 }]);
        // Vector glass has its own boundary, no resolution, and paths
        // rather than a PNG to count.
        let vector = glass(vector_glass().resolution(8));
        let edge = crate::GLASS_VECTOR_FULL_BAR_VERSION;
        assert!(vector.check(&code(&vector, edge)).warnings.is_empty());
        let report = vector.check(&code(&vector, 14));
        assert_eq!(report.warnings, [Warning::GlassNeedsZxing { version: 14 }]);
        // Narrower vector lenses: a lower boundary, and none under 0.7.
        let narrow = glass(vector_glass().lens(0.8));
        let v = crate::GLASS_VECTOR_NARROW_FULL_BAR_VERSION;
        assert!(narrow.check(&code(&narrow, v)).warnings.is_empty());
        assert_eq!(
            narrow.check(&code(&narrow, v + 1)).warnings,
            [Warning::GlassNeedsZxing { version: v + 1 }]
        );
        let tiny = glass(vector_glass().lens(0.6).core(0.5));
        let narrow_lens = |e: &RenderError| {
            matches!(
                e,
                RenderError::OptionOutOfRange {
                    option: "Glass.lens",
                    ..
                }
            )
        };
        let strict = tiny.clone().checks(Checks::Strict);
        assert!(strict.check(&code(&tiny, 3)).errors.iter().any(narrow_lens));
        // A measured range: drawn as given by default, with a warning.
        let report = tiny.check(&code(&tiny, 3));
        assert!(report.ok() && !report.scannable, "{report:?}");
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::Unchecked(e) if narrow_lens(e)))
        );
        assert!(
            glass(baked().lens(0.6).core(0.5))
                .check(&code(&tiny, 3))
                .ok()
        );
        let report = vector.check(&code(&vector, 30));
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::LargeOutput { .. })),
            "{report:?}"
        );
    }

    #[test]
    fn what_glass_cant_draw_is_refused() {
        // Vector glass refuses and supports the same things as baked.
        refuses_and_supports(baked());
        refuses_and_supports(vector_glass());
    }

    fn refuses_and_supports(base: Glass) {
        let qr = code(&glass(base), 2);
        let g = || glass(base);
        let mut cases: Vec<(Style, &str)> = alloc::vec![
            (
                g().frame(Frame::Circle {
                    filler_density: 0.5,
                    ring: crate::FrameRing::None,
                }),
                "Frame::Circle",
            ),
            (
                g().texture(crate::Texture::Grain { amount: 0.05 }),
                "Style.textures",
            ),
            (
                Style::new().halftone(
                    picture()
                        .ink(Ink::Glass(base))
                        .clarity(crate::Clarity::Uniform(0.5)),
                ),
                "Halftone.clarity",
            ),
        ];
        // Glow and emboss hold over vector glass only (W20261123231733).
        let glow = crate::Effect::Glow {
            radius: 0.2,
            strength: 0.1,
        };
        let emboss = crate::Effect::Emboss { depth: 0.1 };
        if !base.is_vector() {
            cases.push((g().effect(glow), "Effect::Glow on baked glass"));
            cases.push((g().effect(emboss), "Effect::Emboss on baked glass"));
        }
        for (style, option) in cases {
            let report = style.check(&qr);
            assert!(
                report
                    .errors
                    .contains(&RenderError::GlassUnsupported { option }),
                "{option}: {report:?}"
            );
        }
        #[cfg(feature = "animate")]
        for (idle, option) in [
            (crate::Idle::FinderPulse, "Idle::FinderPulse"),
            (crate::Idle::Shimmer, "Idle::Shimmer"),
        ] {
            let report = g().idle(idle).check(&qr);
            assert!(
                report
                    .errors
                    .contains(&RenderError::GlassUnsupported { option }),
                "{option}: {report:?}"
            );
        }
        // What it supports.
        for style in [
            g().frame(Frame::RoundedSquare(2.0)),
            g().frame(Frame::None),
            g().border(Border::Solid { width: 0.5 }).shadow(1.0),
            g().caption(crate::Caption::new("Scan me")),
            g().palette(crate::Palette::new().dark_twin()),
            g().timing(TimingStyle::Dotted),
            // Cut from the code's own geometry (W20261124091813).
            g().modules(ModuleShape::Dots { size: 0.9 }),
            g().modules(ModuleShape::Liquid { smoothness: 0.8 }),
            g().modules(ModuleShape::Rounded { radius: 0.4 }),
            g().modules(ModuleShape::Bars {
                axis: crate::Axis::Vertical,
                radius: 0.3,
            }),
            g().finders(crate::FinderStyle::new(
                crate::RingShape::Rounded(2.5),
                crate::EyeShape::Circle,
            )),
            g().finders(crate::FinderStyle::new(
                crate::RingShape::Leaf,
                crate::EyeShape::Squircle,
            )),
            g().timing(TimingStyle::Dashed),
            g().palette(crate::Palette::from_tokens([("fg.2", "#700")]).unwrap()),
            Style::art(picture().ink(Ink::Glass(base))),
            g().effect(crate::Effect::Sketch { roughness: 0.1 }),
        ]
        .into_iter()
        .chain(
            base.is_vector()
                .then(|| [g().effect(glow), g().effect(emboss)])
                .into_iter()
                .flatten(),
        ) {
            let qr = code(&style, 2);
            let report = style.check(&qr);
            assert!(report.ok() && report.scannable, "{report:?}");
            assert!(style.render(&qr).is_ok());
        }
    }

    /// An effect filters the glass, never the photo under it (SPEC §7.9,
    /// W20261123231733): the photo's `<image>` comes before the effect's
    /// group, and the glass is inside it.
    #[test]
    fn effects_filter_the_glass_not_the_photo() {
        for (g, effect) in [
            (
                baked().resolution(8),
                crate::Effect::Sketch { roughness: 0.1 },
            ),
            (
                vector_glass(),
                crate::Effect::Glow {
                    radius: 0.3,
                    strength: 0.25,
                },
            ),
        ] {
            let style = glass(g).effect(effect);
            let qr = code(&style, 2);
            let svg = style.render(&qr).unwrap();
            assert!(svg.contains("-fx\""), "{g:?}: no effect filter");
            let (before, group) = svg.split_once("<g filter=\"url(#").unwrap();
            let group = group.split_once("</g>").map_or(group, |(g, _)| g);
            // The photo is the only image before the group.
            assert_eq!(
                before.matches("<image").count(),
                1,
                "{g:?}: the photo is filtered"
            );
            if g.is_vector() {
                // Vector glass: veils and rims as paths, no image of its own.
                assert!(
                    group.contains("<path") && !group.contains("<image"),
                    "{g:?}"
                );
            } else {
                // Baked glass: its one PNG.
                assert_eq!(
                    group.matches("<image").count(),
                    1,
                    "{g:?}: the glass isn't filtered"
                );
            }
        }
    }

    #[cfg(feature = "animate")]
    #[test]
    fn intros_move_the_photo_and_glass_whole() {
        use crate::plan::{Property, Target};
        use crate::{Intro, LayerRole, Preset};
        let still = glass(baked().resolution(8));
        let qr = code(&still, 2);
        let static_svg = still.render(&qr).unwrap();
        let payloads = images(&static_svg);
        for preset in Preset::ALL {
            let style = still.clone().intro(Intro::new(preset));
            let report = style.check(&qr);
            assert!(report.ok(), "{preset:?}: {report:?}");
            let svg = style.render(&qr).unwrap();
            // The same two images, the photo first.
            assert_eq!(images(&svg), payloads, "{preset:?}");
            let plan = style.plan(&qr).unwrap();
            let roles: alloc::vec::Vec<LayerRole> = plan.layers.iter().map(|l| l.role).collect();
            let photo = roles.iter().position(|&r| r == LayerRole::Photo).unwrap();
            assert_eq!(roles[photo + 1], LayerRole::Glass, "{preset:?}");
            // The plan carries the glass's bytes, the same as the document's.
            let layer = plan.layers[photo + 1].image.as_ref().unwrap();
            let mut b64 = String::new();
            crate::logo::base64(&mut b64, layer.png.as_ref().unwrap()).unwrap();
            assert_eq!(b64, payloads[1]);
            assert!(plan.layers[photo].image.as_ref().unwrap().png.is_none());
            let moves = |i: usize, p: Property| {
                plan.tracks
                    .iter()
                    .any(|t| t.target == Target::Layer(i) && t.keys.iter().any(|k| k.property == p))
            };
            if preset.is_reveal() {
                // One mask over everything from the photo up.
                assert_eq!(plan.reveal.unwrap().layers.0, photo, "{preset:?}");
                assert!(!moves(photo, Property::Opacity) && !moves(photo + 1, Property::Opacity));
            } else {
                assert!(moves(photo, Property::Opacity), "{preset:?}");
                assert!(moves(photo + 1, Property::Opacity), "{preset:?}");
                // The photo only fades.
                assert!(!moves(photo, Property::Scale) && !moves(photo, Property::TranslateX));
            }
            // From the end on, a frame is the static document's drawing.
            let end = style.render_frame(&qr, plan.total_ms).unwrap();
            assert_eq!(images(&end), payloads, "{preset:?}");
            assert!(!end.contains(" opacity=\"") && !end.contains("mask=\""));
            // Halfway, the glass is still coming in.
            let mid = style.render_frame(&qr, plan.total_ms / 2).unwrap();
            assert!(
                mid.contains(" opacity=\"") || mid.contains("mask=\""),
                "{preset:?}"
            );
        }
        // The plan's JSON has the glass as base64 PNG.
        let json = still.plan(&qr).unwrap().to_json();
        assert!(json.contains("\"role\":\"glass\"") && json.contains("\"png\":\"iVBORw0KGgo"));
        assert!(json.contains("\"role\":\"photo\""));
        // Palette shifts, theme transitions and the other idles are fine.
        let logo = Logo::svg(r##"<svg viewBox="0 0 1 1"><rect width="1" height="1"/></svg>"##)
            .unwrap()
            .scale(0.1);
        for style in [
            still
                .clone()
                .intro(Intro::palette_from(&crate::Palette::new().dark_twin())),
            still.clone().theme_transition(300),
            still.clone().idle(crate::Idle::Breathe),
            still
                .clone()
                .logo(logo.clone())
                .idle(crate::Idle::LogoGlint)
                .idle(crate::Idle::LogoSpin),
        ] {
            let qr = code(&style, 5);
            let report = style.check(&qr);
            assert!(report.ok(), "{report:?}");
            assert!(style.render(&qr).is_ok() && style.plan(&qr).is_ok());
        }
    }

    /// The `href` payloads of a document, in order.
    fn images(svg: &str) -> alloc::vec::Vec<&str> {
        svg.split("<image")
            .skip(1)
            .map(|s| {
                s.split("base64,")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn documents_are_deterministic_and_layered() {
        let logo = Logo::svg(r##"<svg viewBox="0 0 1 1"><rect width="1" height="1"/></svg>"##)
            .unwrap()
            .scale(0.1);
        let style = glass(baked()).logo(logo);
        let qr = code(&style, 5);
        let svg = style.render(&qr).unwrap();
        assert_eq!(svg, style.render(&qr).unwrap());
        // bg, the photo (its own file, untouched), the glass, the logo.
        let bg = svg.find("<rect").unwrap();
        let photo = svg.find("<image clip-path").unwrap();
        let layer = svg[photo + 10..].find("<image").unwrap() + photo + 10;
        let logo = svg.find("<svg x=").unwrap();
        assert!(
            bg < photo && photo < layer && layer < logo,
            "{bg} {photo} {layer} {logo}"
        );
        let payloads = images(&svg);
        assert_eq!(payloads.len(), 2);
        let mut file = String::new();
        crate::logo::base64(&mut file, picture().source_file().unwrap().0).unwrap();
        assert_eq!(payloads[0], file);
        // The glass covers the code and its quiet zone: 37 + 8 modules
        // inside a 4-module margin (0.08 of 45 / 0.84, rounded).
        let n = qr.size() as i64;
        let m = margin(0.08, n, 4);
        assert_eq!(m, 4);
        assert!(svg.contains(&alloc::format!(
            "viewBox=\"0 0 {s} {s}\"",
            s = n + 8 + 2 * m
        )));
        assert!(svg.contains(&alloc::format!(
            "<image x=\"{m}\" y=\"{m}\" width=\"{w}\" height=\"{w}\"",
            w = n + 8
        )));
        // A different picture, the same everything else: different glass.
        let other = Style::new()
            .halftone(
                Halftone::new(Rgba8Image::filled(24, 24, [200, 60, 60, 255]).unwrap())
                    .source(
                        picture().source_file().unwrap().0.to_vec(),
                        RasterFormat::Png,
                    )
                    .unwrap()
                    .ink(Ink::Glass(baked())),
            )
            .render(&qr)
            .unwrap();
        assert_ne!(images(&other)[1], payloads[1]);
    }

    #[test]
    fn the_layer_is_a_png_of_the_box() {
        let style = glass(baked().resolution(8));
        let qr = code(&style, 2);
        let canvas = scene::canvas(&style, &qr);
        let baked = bake(&style, &qr, &canvas).unwrap();
        assert_eq!(baked.side, 25 + 8);
        assert_eq!(baked.at, canvas.origin - 4);
        assert!(baked.png.starts_with(b"\x89PNG\r\n\x1a\n"));
        // IHDR: 33 modules × 8 px, RGBA (transparent between lenses).
        assert_eq!(&baked.png[16..24], &[0, 0, 1, 8, 0, 0, 1, 8]);
        assert_eq!(baked.png[25], 6);
        // Plain styles bake nothing.
        assert!(bake(&Style::new(), &qr, &canvas).is_none());
    }

    #[test]
    fn steering_samples_the_code_area() {
        let style = glass(Glass::new());
        let s = span(&style, 25);
        // 25 modules + 2 × (4 + 3): the code starts 7 modules in.
        assert_eq!(margin(0.08, 25, 4), 3);
        assert!((s.start - 7.0 / 39.0).abs() < 1e-12 && (s.len - 25.0 / 39.0).abs() < 1e-12);
        assert_eq!(span(&Style::new(), 25), Span::WHOLE);
    }

    #[test]
    fn equality_is_total() {
        let nan = Glass::new().frost(f32::NAN);
        assert_eq!(nan, nan);
        assert_ne!(Glass::new(), Glass::new().tint(false));
        assert_eq!(Glass::default(), Glass::new());
        assert_ne!(baked(), vector_glass());
        assert_eq!(Glass::new().rendering(), crate::GLASS_RENDER);
    }

    /// The document with every base64 payload cut out.
    fn without_payloads(svg: &str) -> String {
        let mut out = String::new();
        let mut rest = svg;
        while let Some(at) = rest.find("base64,") {
            out.push_str(&rest[..at + 7]);
            rest = &rest[at + 7..];
            rest = &rest[rest.find('"').unwrap()..];
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn vector_glass_is_paths_over_one_image() {
        let style = glass(vector_glass());
        let qr = code(&style, 4);
        let report = style.check(&qr);
        assert!(
            report.ok() && report.scannable && report.warnings.is_empty(),
            "{report:?}"
        );
        let svg = style.render(&qr).unwrap();
        assert_eq!(svg, style.render(&qr).unwrap());
        // One raster, the photo's own file; nothing baked.
        assert_eq!(svg.matches("<image").count(), 1);
        assert_eq!(svg.matches("base64,").count(), 1);
        let mut file = String::new();
        crate::logo::base64(&mut file, picture().source_file().unwrap().0).unwrap();
        assert_eq!(images(&svg), [file.as_str()]);
        assert!(bake(&style, &qr, &scene::canvas(&style, &qr)).is_none());
        // The frost refers to the photo, blurred, clipped to the glass.
        let prefix = svg.split("<clipPath id=\"").nth(1).unwrap();
        let prefix = &prefix[..prefix.find("-photo").unwrap()];
        assert!(svg.contains(&alloc::format!("<image id=\"{prefix}-pic\"")));
        assert!(svg.contains(&alloc::format!(
            "<use xlink:href=\"#{prefix}-pic\" filter=\"url(#{prefix}-blur)\"/>"
        )));
        assert!(svg.contains("<feGaussianBlur stdDeviation=\".12\"/>"));
        // Veils: black-ish and white-ish translucent paths, and rims.
        let shell = without_payloads(&svg);
        assert!(shell.matches(" fill-opacity=\"").count() >= 4, "{shell}");
        assert!(shell.contains("stroke=\"#fff\" stroke-opacity=\".3\" stroke-width=\".04\""));
        // The photo comes first, the glass over it, in one stretch.
        let photo = svg.find("<image").unwrap();
        assert!(photo < svg.find("<feGaussianBlur").unwrap());
    }

    #[test]
    fn vector_switches_drop_their_elements() {
        let qr = code(&glass(vector_glass()), 3);
        let plain = without_payloads(
            &glass(vector_glass().frost(0.0).rim(false).tint(false))
                .render(&qr)
                .unwrap(),
        );
        // frost(0) leaves no filter (and nothing refers to the photo).
        assert!(
            !plain.contains("<filter") && !plain.contains("<use"),
            "{plain}"
        );
        assert!(!plain.contains("stroke="));
        // Untinted veils are black and white only.
        for fill in plain.split(" fill=\"").skip(1) {
            let color = &fill[..fill.find('"').unwrap()];
            assert!(
                ["#000", "#fff", "#fefcf8"].contains(&color) || color.starts_with("url("),
                "{color}"
            );
        }
        // Refraction, core and resolution don't change vector glass.
        let base = glass(vector_glass()).render(&qr).unwrap();
        for g in [
            vector_glass().refraction(false),
            vector_glass().core(0.4),
            vector_glass().resolution(24),
        ] {
            assert_eq!(glass(g).render(&qr).unwrap(), base);
        }
    }

    #[test]
    fn vector_size_is_counted_not_guessed() {
        let style = glass(vector_glass());
        for v in [2u8, 7] {
            let qr = code(&style, v);
            let svg = style.render(&qr).unwrap();
            let photo = images(&svg)[0].len();
            let estimate = estimated_bytes(
                &style,
                style.halftone.as_ref().unwrap().glass().unwrap(),
                &qr,
            );
            // Everything but the photo and the document's frame.
            let glass_bytes = svg.len() - photo;
            assert!(
                estimate <= glass_bytes && glass_bytes < estimate + 3000,
                "v{v}: {estimate} vs {glass_bytes}"
            );
        }
    }

    #[cfg(feature = "animate")]
    #[test]
    fn vector_glass_moves_whole_and_plans_as_paths() {
        use crate::plan::{Property, Target};
        use crate::{Intro, LayerRole, Preset};
        let still = glass(vector_glass());
        let qr = code(&still, 3);
        let static_svg = still.render(&qr).unwrap();
        let plan = still.plan(&qr).unwrap();
        let glass_layers: alloc::vec::Vec<_> = plan
            .layers
            .iter()
            .filter(|l| l.role == LayerRole::Glass)
            .collect();
        assert!(glass_layers.len() >= 4);
        // The frost: the photo's box, blurred, no bytes.
        let frost = glass_layers[0].image.as_ref().unwrap();
        assert_eq!((frost.png.as_ref(), frost.blur), (None, Some(0.12)));
        // Veils and rims: plain paths with a color, no image.
        for l in &glass_layers[1..] {
            assert!(l.image.is_none() && l.color.is_some() && !l.path.is_empty());
        }
        assert!(glass_layers.iter().any(|l| l.outline.is_some()));
        let json = plan.to_json();
        assert!(json.contains("\"blur\":0.12") && !json.contains("\"png\""));
        for preset in Preset::ALL {
            let style = still.clone().intro(Intro::new(preset));
            assert!(style.check(&qr).ok(), "{preset:?}");
            let plan = style.plan(&qr).unwrap();
            let glass: alloc::vec::Vec<usize> = (0..plan.layers.len())
                .filter(|&i| plan.layers[i].role == LayerRole::Glass)
                .collect();
            if !preset.is_reveal() {
                // Every glass layer plays the same keys: the glass moves whole.
                let keys = |i: usize| {
                    plan.tracks
                        .iter()
                        .filter(|t| t.target == Target::Layer(i))
                        .map(|t| (t.delay_ms, t.duration_ms, t.keys.clone()))
                        .collect::<alloc::vec::Vec<_>>()
                };
                let first = keys(glass[0]);
                assert!(!first.is_empty(), "{preset:?}");
                assert!(glass.iter().all(|&i| keys(i) == first), "{preset:?}");
                assert!(
                    plan.tracks
                        .iter()
                        .any(|t| t.target == Target::Layer(glass[0])
                            && t.keys.iter().any(|k| k.property == Property::Opacity))
                );
            }
            // From the end on, a frame is the static document's drawing.
            let end = style.render_frame(&qr, plan.total_ms).unwrap();
            assert_eq!(images(&end), images(&static_svg), "{preset:?}");
            assert!(
                !end.contains(" opacity=\"") && !end.contains("mask=\""),
                "{preset:?}"
            );
            let mid = style.render_frame(&qr, plan.total_ms / 2).unwrap();
            assert!(
                mid.contains(" opacity=\"") || mid.contains("mask=\""),
                "{preset:?}"
            );
        }
    }
}
