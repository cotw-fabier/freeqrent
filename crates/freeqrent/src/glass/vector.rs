//! Vector glass (SPEC §7.9, [`GlassRender::Vector`](crate::GlassRender::Vector)):
//! the glass as SVG elements over the photo instead of a baked PNG.
//!
//! - Lenses are the Liquid tracer's outlines ([`geometry::draw_inset`]),
//!   moved in by half the gap the lens width leaves, so same-polarity
//!   neighbours melt into one blob as the smooth union melts them.
//! - Each lens is a veil: black (or the picture's dark tint) darkens, white
//!   (or its light tint) lightens. Renderers blend `fill-opacity` in sRGB,
//!   so a black veil is a multiply and a white one a screen; every opacity
//!   here is solved in sRGB for the lightness it must reach.
//! - A data module's veil brings its centre (the photo's mean over the
//!   central [`READ_DATA`] box) to its target. Strengths are stepped:
//!   [`VECTOR_LEVELS`] nested paths per polarity, level `k` holding every
//!   module that needs at least level `k`, so stacked veils add up to each
//!   module's strength and the outermost path is the whole union.
//! - Finder, alignment and timing glass and the halo are veils too, one
//!   strength per connected region, solved for the region's worst pixel,
//!   so the finder and plate lightness are met everywhere over the photo.
//! - The frost is the photo itself, referenced (never embedded twice) and
//!   blurred, clipped to the union of all glass; the rim is a thin white
//!   stroke along the lenses.
//!
//! No refraction, no feathered core, no OKLab shift: the photo's detail is
//! scaled in sRGB, not shifted.

use alloc::vec;
use alloc::vec::Vec;

use super::{CORNER, DOT_HALF, FINDER_CHROMA, from_oklab, oklab};
use crate::color::{Color, srgb_to_linear};
use crate::geometry::{self, Grid, Path, UNIT};
use crate::halftone::{self, Sampler, Span};
use crate::math;
use crate::matrix::{ModuleRole, QrMatrix};
use crate::scanbox::TimingStyle;
use crate::scene::{self, Canvas};
use crate::style::Style;

/// The stepped strengths of data veils, per polarity (SPEC §7.9).
pub(crate) const VECTOR_LEVELS: usize = 4;
/// Photo pixels per module the opacities are solved from.
const SAMPLE: i64 = 8;
/// The central box of a data module whose mean colour its veil brings to
/// the target (the baked default's core).
const READ_DATA: f64 = 0.6;
/// Function and halo strengths are rounded up to this step.
const FUNC_STEP: f64 = 0.05;
/// Lightness the data veils aim past their targets by: 8-bit compositing
/// and resampling move a pixel by about this much.
const MARGIN: f64 = 0.01;
/// How far finder glass aims under its lightness and the plate over its
/// own, measured (SPEC §7.9): at the nominal pair (margin 0.01 both) the
/// degraded render lost 4 of 14 codes at v6 and 11 of 1400 noisy renders;
/// at these, none of either, and rqrr read every code up to v11.
const FINDER_MARGIN: f64 = 0.08;
const PLATE_MARGIN: f64 = 0.06;
/// The tint colours' lightness: dark glass under its target, light over.
const TINT_DARK: f64 = 0.12;
const TINT_LIGHT: f64 = 0.97;
/// The rim: a white stroke this wide (path units) at this opacity.
pub(crate) const RIM_STROKE: i64 = 40;
const RIM_OPACITY: f32 = 0.3;

/// One veil: a filled path and its color, whose alpha is its opacity.
pub(crate) struct Veil {
    pub(crate) path: Path,
    pub(crate) color: Color,
    /// Dark glass (it darkens) or light.
    pub(crate) dark: bool,
}

/// Vector glass, in paint order.
pub(crate) struct Vector {
    /// The frost: the photo blurred by this much (modules), clipped to the
    /// path. `None` at `frost(0)`.
    pub(crate) frost: Option<(Path, f64)>,
    pub(crate) veils: Vec<Veil>,
    /// The rim strokes and their color.
    pub(crate) rims: Vec<(Path, Color)>,
}

impl Vector {
    /// About what the elements take in the document: their path data and
    /// each element's markup.
    pub(crate) fn bytes(&self) -> usize {
        let frost = self.frost.as_ref().map_or(0, |(p, _)| p.data().len() + 400);
        let veils: usize = self.veils.iter().map(|v| v.path.data().len() + 50).sum();
        let rims: usize = self.rims.iter().map(|(p, _)| p.data().len() + 80).sum();
        frost + veils + rims
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    None,
    /// A data module's lens, with the opacity its centre needs.
    Data {
        dark: bool,
        need: f64,
    },
    /// Finder, alignment, timing, cleared or halo glass.
    Func {
        dark: bool,
        dot: bool,
    },
}

/// The glass of `style` for `qr` as SVG elements, if it has vector glass.
pub(crate) fn vector(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Option<Vector> {
    let h = style.halftone.as_ref()?;
    let g = h.glass()?;
    if !g.is_vector() {
        return None;
    }
    let n = qr.size() as i64;
    let q = style.quiet_zone as i64;
    let w = n + 2 * q;
    let s = SAMPLE;
    let side = (w * s) as usize;

    // The photo over the box, linear sRGB over bg.
    let backdrop = halftone::linear(style.palette.background());
    let sampler = Sampler::new(
        &h.image,
        h.fit,
        h.focal,
        Span::WHOLE,
        (canvas.card * s) as usize,
    );
    let first = (canvas.origin - q) * s;
    let mut photo = Vec::with_capacity(side * side);
    for y in 0..side as i64 {
        for x in 0..side as i64 {
            photo.push(sampler.cell(first + x, first + y).over_linear(backdrop));
        }
    }

    // The picture's dominant dark and light colours under the code (as the
    // baked glass takes them), and the veils' colours.
    let (dark_ab, light_ab) = {
        let from = (q * s) as usize;
        let to = from + (n * s) as usize;
        let mut labs = Vec::new();
        for y in (from..to).step_by(2) {
            for x in (from..to).step_by(2) {
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
    let (dark_t, light_t) = (g.targets.0 as f64, g.targets.1 as f64);
    let (finder_t, plate_t) = (g.finders.0 as f64, g.finders.1 as f64);
    let tint = |l: f64, ab: [f64; 2], k: f64| -> [u8; 3] {
        from_oklab([l, ab[0] * k, ab[1] * k])
            .map(|c| math::round(super::encode(c) * 255.0).clamp(0.0, 255.0) as u8)
    };
    // Untinted, dark glass is black and light glass white.
    let (data_dark, data_light, func_dark) = if g.tint {
        (
            tint(TINT_DARK.min(dark_t - 0.08).max(0.0), dark_ab, 1.0),
            tint(TINT_LIGHT.max(light_t + 0.04).min(1.0), light_ab, 1.0),
            tint(
                TINT_DARK.min(finder_t - FINDER_MARGIN - 0.04).max(0.0),
                dark_ab,
                FINDER_CHROMA,
            ),
        )
    } else {
        ([0; 3], [255; 3], [0; 3])
    };
    let plate = [255u8; 3];

    // Each cell's glass.
    let cleared = scene::cleared(style, qr);
    let removed = scene::removed(style, qr);
    let dotted = style.timing == TimingStyle::Dotted;
    let mut kinds = vec![Kind::None; (w * w) as usize];
    for j in -q..n + q {
        for i in -q..n + q {
            let at = ((j + q) * w + i + q) as usize;
            let inside = i >= 0 && j >= 0 && i < n && j < n;
            if !inside {
                let dist = (-i).max(i - n + 1).max(-j).max(j - n + 1);
                if dist <= g.halo as i64 {
                    kinds[at] = Kind::Func {
                        dark: false,
                        dot: false,
                    };
                }
                continue;
            }
            if cleared.get(i, j) {
                kinds[at] = Kind::Func {
                    dark: false,
                    dot: false,
                };
                continue;
            }
            if removed.get(i, j) {
                continue;
            }
            let (x, y) = (i as u16, j as u16);
            let dark = qr.is_dark(x, y);
            let role = qr.role(x, y);
            kinds[at] = if role.is_function() {
                Kind::Func {
                    dark,
                    dot: dark && dotted && role == ModuleRole::Timing,
                }
            } else {
                let mean = mean(&photo, side, (i + q, j + q), s, READ_DATA);
                let (tint, target) = if dark {
                    (data_dark, dark_t - MARGIN)
                } else {
                    (data_light, light_t + MARGIN)
                };
                let need = needed(encode3(mean), unit(tint), target, dark);
                if need > 0.0 {
                    Kind::Data { dark, need }
                } else {
                    Kind::None
                }
            };
        }
    }

    let origin = (canvas.origin - q) * UNIT;
    let gap = geometry::units((1.0 - g.lens.clamp(0.5, 1.0)) / 2.0);
    let lens_corner = math::round(CORNER * (UNIT - 2 * gap) as f64 / 2.0) as i64;
    let func_corner = math::round(CORNER * UNIT as f64 / 2.0) as i64;
    let concave = geometry::units(g.union.clamp(0.2, 0.8) / 2.0);
    let grid_of = |keep: &dyn Fn(Kind) -> bool| {
        let mut grid = Grid::new(w as usize);
        for (k, &kind) in kinds.iter().enumerate() {
            if keep(kind) {
                grid.set(k % w as usize, k / w as usize);
            }
        }
        grid
    };

    // The frost, under the union of all glass.
    let frost = (g.frost > 0.0).then(|| {
        let mut path = Path::new();
        let grid = grid_of(&|k| k != Kind::None);
        geometry::draw_inset(&grid, origin, gap, (lens_corner, concave), &mut path);
        (path, g.frost as f64)
    });

    let mut veils = Vec::new();
    // Function and halo glass: one strength per connected region, for its
    // worst pixel.
    for dark in [false, true] {
        let (tint, target) = if dark {
            (func_dark, finder_t - FINDER_MARGIN)
        } else {
            (plate, plate_t + PLATE_MARGIN)
        };
        let mine = |k: Kind| matches!(k, Kind::Func { dark: d, .. } if d == dark);
        let mut region = vec![usize::MAX; kinds.len()];
        let mut strengths: Vec<f64> = Vec::new();
        for start in 0..kinds.len() {
            if !mine(kinds[start]) || region[start] != usize::MAX {
                continue;
            }
            let id = strengths.len();
            let mut need: f64 = 0.0;
            let mut stack = vec![start];
            region[start] = id;
            while let Some(c) = stack.pop() {
                let (ci, cj) = ((c as i64) % w, (c as i64) / w);
                let worst = worst(&photo, side, (ci, cj), s, dark);
                need = need.max(needed(encode3(worst), unit(tint), target, dark));
                for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (ni, nj) = (ci + di, cj + dj);
                    if ni < 0 || nj < 0 || ni >= w || nj >= w {
                        continue;
                    }
                    let nk = (nj * w + ni) as usize;
                    if mine(kinds[nk]) && region[nk] == usize::MAX {
                        region[nk] = id;
                        stack.push(nk);
                    }
                }
            }
            strengths.push(step_up(need));
        }
        let mut levels: Vec<f64> = strengths.clone();
        crate::sort::sort_by(&mut levels, |a, b| a.total_cmp(b));
        levels.dedup();
        for &a in levels.iter().filter(|&&a| a > 0.0) {
            let mut path = Path::new();
            let mut grid = Grid::new(w as usize);
            for (k, &kind) in kinds.iter().enumerate() {
                if region[k] == usize::MAX || strengths[region[k]] != a {
                    continue;
                }
                let (ci, cj) = ((k as i64) % w, (k as i64) / w);
                match kind {
                    Kind::Func { dot: true, .. } => {
                        let c = |v: i64| origin + v * UNIT + UNIT / 2;
                        path.circle(c(ci), c(cj), geometry::units(DOT_HALF as f32));
                    }
                    _ => grid.set(ci as usize, cj as usize),
                }
            }
            geometry::draw_inset(&grid, origin, 0, (func_corner, concave), &mut path);
            veils.push(Veil {
                path,
                color: color(tint, a),
                dark,
            });
        }
    }

    // Data lenses: stepped strengths, nested.
    let mut rims = Vec::new();
    for dark in [false, true] {
        let tint = if dark { data_dark } else { data_light };
        let needs = |k: Kind| match k {
            Kind::Data { dark: d, need } if d == dark => Some(need),
            _ => None,
        };
        // The levels: the needs split into equal shares, each level the
        // largest need of its share, so every module gets at least what
        // it needs and the steps follow the picture.
        let mut sorted: Vec<f64> = kinds.iter().filter_map(|&k| needs(k)).collect();
        if sorted.is_empty() {
            continue;
        }
        crate::sort::sort_by(&mut sorted, |a, b| a.total_cmp(b));
        let mut tops: Vec<f64> = (1..=VECTOR_LEVELS)
            .map(|k| sorted[(k * sorted.len()).div_ceil(VECTOR_LEVELS) - 1])
            .collect();
        tops.dedup();
        let level = |need: f64| {
            tops.iter()
                .position(|&t| need <= t)
                .unwrap_or(tops.len() - 1)
                + 1
        };
        // What the levels below let through, as written.
        let mut through = 1.0;
        for (k, &total) in (1..).zip(&tops) {
            let grid = grid_of(&|kind| needs(kind).is_some_and(|need| level(need) >= k));
            let a = thousandths_up(1.0 - (1.0 - total) / through);
            through *= 1.0 - a;
            let mut path = Path::new();
            geometry::draw_inset(&grid, origin, gap, (lens_corner, concave), &mut path);
            if k == 1 && g.rim {
                rims.push((
                    path.clone(),
                    Color::srgb(255, 255, 255).with_alpha(RIM_OPACITY),
                ));
            }
            veils.push(Veil {
                path,
                color: color(tint, a),
                dark,
            });
        }
    }

    Some(Vector { frost, veils, rims })
}

/// The opacity a veil of `tint` (sRGB, 0–1) needs over `photo` (sRGB) to
/// bring its OKLab lightness to `target` (down for dark glass, up for
/// light), blended in sRGB as renderers do: 0 if it's already there, 1 if
/// the tint itself can't get there.
fn needed(photo: [f64; 3], tint: [f64; 3], target: f64, dark: bool) -> f64 {
    let past = |a: f64| {
        let l = lightness(core::array::from_fn(|k| photo[k] * (1.0 - a) + tint[k] * a));
        if dark { l <= target } else { l >= target }
    };
    if past(0.0) {
        return 0.0;
    }
    if !past(1.0) {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if past(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// OKLab L of an sRGB colour.
fn lightness(srgb: [f64; 3]) -> f64 {
    oklab(srgb.map(srgb_to_linear))[0]
}

fn encode3(linear: [f64; 3]) -> [f64; 3] {
    linear.map(super::encode)
}

fn unit(c: [u8; 3]) -> [f64; 3] {
    c.map(|v| v as f64 / 255.0)
}

fn color(c: [u8; 3], alpha: f64) -> Color {
    Color::srgb(c[0], c[1], c[2]).with_alpha(alpha as f32)
}

/// Up to the next thousandth (what an attribute carries), at least one.
fn thousandths_up(a: f64) -> f64 {
    (math::ceil(a * 1000.0 - 1e-9) / 1000.0).clamp(0.001, 1.0)
}

/// Up to the next [`FUNC_STEP`]; 0 stays 0.
fn step_up(a: f64) -> f64 {
    if a <= 0.0 {
        return 0.0;
    }
    (math::ceil(a / FUNC_STEP - 1e-9) * FUNC_STEP).min(1.0)
}

/// The mean linear colour over the central `size` box of cell `(ci, cj)`.
fn mean(photo: &[[f64; 3]], side: usize, (ci, cj): (i64, i64), s: i64, size: f64) -> [f64; 3] {
    let r = s as f64;
    let half = size / 2.0 * r;
    let range = |c: i64| {
        let center = (c as f64 + 0.5) * r;
        let lo = math::ceil(center - half - 0.5).max(0.0) as usize;
        let hi = (math::floor(center + half - 0.5) as usize).min(side - 1);
        lo..=hi
    };
    let (mut sum, mut count) = ([0.0; 3], 0.0);
    for y in range(cj) {
        for x in range(ci) {
            let p = photo[y * side + x];
            for k in 0..3 {
                sum[k] += p[k];
            }
            count += 1.0;
        }
    }
    sum.map(|v| v / f64::max(count, 1.0))
}

/// The cell's lightest pixel (for dark glass) or darkest (for light).
fn worst(photo: &[[f64; 3]], side: usize, (ci, cj): (i64, i64), s: i64, dark: bool) -> [f64; 3] {
    let mut best = ([0.0; 3], if dark { f64::MIN } else { f64::MAX });
    for y in (cj * s) as usize..((cj + 1) * s) as usize {
        for x in (ci * s) as usize..((ci + 1) * s) as usize {
            let p = photo[y * side + x];
            let l = oklab(p)[0];
            if (dark && l > best.1) || (!dark && l < best.1) {
                best = (p, l);
            }
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn veils_are_solved_in_srgb() {
        let close = |a: f64, b: f64| (a - b).abs() < 2e-3;
        // A black veil over white to L 0.25: a multiply down to sRGB 0.131.
        let a = needed([1.0; 3], [0.0; 3], 0.25, true);
        assert!(close(a, 0.8686), "{a}");
        assert!(close(lightness([1.0 - a; 3]), 0.25));
        // A white veil over black to L 0.90: a screen up to sRGB 0.870.
        let a = needed([0.0; 3], [1.0; 3], 0.90, false);
        assert!(close(a, 0.8698), "{a}");
        // Already past: none; out of the tint's reach: all of it.
        assert_eq!(needed([0.1; 3], [0.0; 3], 0.25, true), 0.0);
        assert_eq!(needed([1.0; 3], [0.5; 3], 0.25, true), 1.0);
        assert_eq!(step_up(0.0), 0.0);
        assert!((step_up(0.51) - 0.55).abs() < 1e-12);
        assert_eq!(thousandths_up(0.0), 0.001);
        assert!((thousandths_up(0.2341) - 0.235).abs() < 1e-12);
    }
}
