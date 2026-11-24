//! Glass cut from the code's own geometry (SPEC §7.9, W20261124091813):
//! for any module shape, finder shape, alignment or timing style other than
//! plain squares, the baked glass's signed distance comes from the shapes a
//! plain render draws, rasterized ([`crate::raster`]) and turned into a
//! distance field by an exact Euclidean distance transform, instead of
//! [`super::Lenses`]'s one rounded square per module.
//!
//! Two fields, dark glass and light glass, each the nearer of its data
//! shapes (narrowed by the lens width) and its function shapes. Light
//! function glass, the plate, is every function, cleared and halo cell
//! with the dark function shapes cut out of it, so a rounded finder ring
//! sits on plate, not on bare photo.

use alloc::vec;
use alloc::vec::Vec;

use crate::geometry::{self, Grid, Path, UNIT};
use crate::math;
use crate::matrix::{ModuleRole, QrMatrix};
use crate::raster::Coverage;
use crate::scanbox::{AlignmentShape, EyeShape, RingShape, TimingStyle};
use crate::scene::{self, Canvas, Part};
use crate::style::{ModuleShape, Style};
use crate::svg;

use super::Lens;

/// Whether `style` draws glass from per-module rounded squares: square
/// modules with no gap, square finders and alignment, and plain or dotted
/// timing (the dots are drawn as discs either way). Everything else is cut
/// from the geometry.
pub(crate) fn analytic(style: &Style) -> bool {
    let square = |r: RingShape, e: EyeShape| r == RingShape::Square && e == EyeShape::Square;
    style.modules == ModuleShape::SQUARE
        && scene::CORNERS.iter().all(|&c| {
            let (r, e) = style.finders.at(c);
            square(r, e)
        })
        && match style.alignment.shape {
            AlignmentShape::Match => true,
            AlignmentShape::Own { outer, eye } => square(outer, eye),
        }
        && style.timing != TimingStyle::Dashed
}

/// The dark and light glass's signed distance per pixel of the box, in
/// modules (negative inside).
pub(super) struct Fields {
    side: usize,
    dark: Vec<f32>,
    light: Vec<f32>,
    /// The fields blurred by a pixel, for normals.
    dark_soft: Vec<f32>,
    light_soft: Vec<f32>,
}

impl Fields {
    fn of(&self, kind: u8) -> (&[f32], &[f32]) {
        if kind == 1 {
            (&self.dark, &self.dark_soft)
        } else {
            (&self.light, &self.light_soft)
        }
    }

    fn clamp(&self, x: i64, y: i64) -> usize {
        let top = self.side as i64 - 1;
        y.clamp(0, top) as usize * self.side + x.clamp(0, top) as usize
    }

    /// `kind`'s distance at pixel `(x, y)` (clamped to the box).
    pub(super) fn dist(&self, kind: u8, x: i64, y: i64) -> f64 {
        self.of(kind).0[self.clamp(x, y)] as f64
    }

    /// `kind`'s outward normal at pixel `(x, y)`, from the softened field.
    pub(super) fn normal(&self, kind: u8, x: i64, y: i64) -> [f64; 2] {
        let f = self.of(kind).1;
        let gx = f[self.clamp(x + 1, y)] as f64 - f[self.clamp(x - 1, y)] as f64;
        let gy = f[self.clamp(x, y + 1)] as f64 - f[self.clamp(x, y - 1)] as f64;
        let len = math::sqrt(gx * gx + gy * gy).max(1e-9);
        [gx / len, gy / len]
    }
}

/// Cuts the glass of `cells` (the box's modules, as [`super::bake`] works
/// them out) from `style`'s geometry, at `res` pixels per module.
#[allow(clippy::too_many_arguments)]
pub(super) fn fields(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    cells: &[Lens],
    q: i64,
    w: i64,
    res: i64,
    lens: f64,
) -> Fields {
    let n = qr.size() as i64;
    let side = (w * res) as usize;
    let r = res as f64;
    let cell = |i: i64, j: i64| &cells[((j + q) * w + i + q) as usize];
    // Code modules by what their glass is.
    let (mut dark_data, mut light_data) = (Grid::new(n as usize), Grid::new(n as usize));
    let (mut dark_timing, mut dark_other) = (Grid::new(n as usize), Grid::new(n as usize));
    for j in 0..n {
        for i in 0..n {
            let c = cell(i, j);
            let (x, y) = (i as usize, j as usize);
            match (c.kind, c.func > 0.5) {
                (1, false) => dark_data.set(x, y),
                (2, false) => light_data.set(x, y),
                (1, true) => match qr.role(i as u16, j as u16) {
                    ModuleRole::Timing => dark_timing.set(x, y),
                    // Finders and alignment are drawn whole, below.
                    ModuleRole::Finder { .. } | ModuleRole::Alignment { .. } => {}
                    _ => dark_other.set(x, y),
                },
                _ => {}
            }
        }
    }

    // The box's top-left corner, in canvas modules: where pixel 0 is.
    let corner = (canvas.origin - q) as f64;
    let at = (corner, corner);
    let origin = canvas.origin * UNIT;
    let fill = |cov: &mut Coverage, path: &Path, evenodd: bool| {
        if !path.is_empty() {
            cov.fill(path.data(), evenodd, at, r);
        }
    };
    let shape = &style.modules;

    let mut data = Coverage::new(side, side);
    let mut path = Path::new();
    geometry::draw_modules(shape, &dark_data, origin, &mut path);
    fill(&mut data, &path, false);
    let dark_data_cov = data;

    let mut func = Coverage::new(side, side);
    let mut path = Path::new();
    geometry::draw_modules(shape, &dark_other, origin, &mut path);
    if style.timing == TimingStyle::Plain {
        geometry::draw_modules(shape, &dark_timing, origin, &mut path);
    } else {
        svg::draw_part(style, qr, canvas, &Part::Timing(dark_timing), &mut path);
    }
    fill(&mut func, &path, false);
    // Finder rings and alignment patterns are an outer and an inner outline.
    let mut rings = Path::new();
    svg::draw_part(
        style,
        qr,
        canvas,
        &Part::FinderOuter(scene::CORNERS.to_vec()),
        &mut rings,
    );
    let centers = scene::alignment_centers_drawn(style, qr);
    if !centers.is_empty() {
        svg::draw_part(style, qr, canvas, &Part::Alignment(centers), &mut rings);
    }
    fill(&mut func, &rings, true);
    let mut eyes = Path::new();
    svg::draw_part(
        style,
        qr,
        canvas,
        &Part::FinderEye(scene::CORNERS.to_vec()),
        &mut eyes,
    );
    fill(&mut func, &eyes, false);
    // Nothing under a clearing logo or on Outline's cleared ring. Shapes
    // may reach into their light neighbours otherwise (a rounded ring's
    // inner corners into the gap), as a plain render draws them.
    let (removed, cleared) = (scene::removed(style, qr), scene::cleared(style, qr));
    for j in 0..n {
        for i in 0..n {
            if removed.get(i, j) || cleared.get(i, j) {
                clear_cell(&mut func, i + q, j + q, res, side);
            }
        }
    }
    let dark_func_cov = func;

    let mut light = Coverage::new(side, side);
    let mut path = Path::new();
    geometry::draw_modules(shape, &light_data, origin, &mut path);
    fill(&mut light, &path, false);
    let light_data_cov = light;

    // The plate: every function, cleared and halo cell, less dark glass.
    let mut plate = Coverage::new(side, side);
    for j in -q..n + q {
        for i in -q..n + q {
            let c = cell(i, j);
            if c.func > 0.5 && c.kind != 0 {
                let (x0, y0) = (((i + q) * res) as usize, ((j + q) * res) as usize);
                for y in y0..y0 + res as usize {
                    for x in x0..x0 + res as usize {
                        let k = y * side + x;
                        let dark = dark_func_cov.alpha[k].max(dark_data_cov.alpha[k]);
                        plate.alpha[k] = 1.0 - dark;
                    }
                }
            }
        }
    }

    // A square lens is narrowed by the gap the lens width leaves; other
    // shapes keep the size they were given (a dot's diameter, a bar's
    // width), as a plain render draws them.
    let erode = match style.modules {
        ModuleShape::Square { .. } => ((1.0 - lens) / 2.0).max(0.0) as f32,
        _ => 0.0,
    };
    let dark = nearer(
        &signed(&dark_data_cov, r),
        &signed(&dark_func_cov, r),
        erode,
    );
    let light = nearer(&signed(&light_data_cov, r), &signed(&plate, r), erode);
    Fields {
        dark_soft: soften(&dark, side),
        light_soft: soften(&light, side),
        side,
        dark,
        light,
    }
}

fn clear_cell(cov: &mut Coverage, i: i64, j: i64, res: i64, side: usize) {
    let (x0, y0) = ((i * res) as usize, (j * res) as usize);
    for y in y0..(y0 + res as usize).min(side) {
        for x in x0..(x0 + res as usize).min(side) {
            cov.alpha[y * side + x] = 0.0;
        }
    }
}

/// Per pixel, the nearer of the data field moved out by `erode` and the
/// function field.
fn nearer(data: &[f32], func: &[f32], erode: f32) -> Vec<f32> {
    data.iter()
        .zip(func)
        .map(|(&d, &f)| (d + erode).min(f))
        .collect()
}

/// A 3 × 3 box blur, twice: enough to smooth the transform's steps out of
/// the normals.
fn soften(f: &[f32], side: usize) -> Vec<f32> {
    let mut a = f.to_vec();
    let mut b = vec![0.0f32; f.len()];
    for _ in 0..2 {
        for y in 0..side {
            for x in 0..side {
                let mut sum = 0.0;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let yy = (y as i64 + dy).clamp(0, side as i64 - 1) as usize;
                        let xx = (x as i64 + dx).clamp(0, side as i64 - 1) as usize;
                        sum += a[yy * side + xx];
                    }
                }
                b[y * side + x] = sum / 9.0;
            }
        }
        core::mem::swap(&mut a, &mut b);
    }
    a
}

/// The signed distance to `cov`'s edge per pixel, in modules at `r` pixels
/// per module: negative inside. Pixels on the edge take it from their
/// coverage (`0.5 − c` pixels); the rest from the distance transform to the
/// nearest pixel of the other side, less half a pixel.
fn signed(cov: &Coverage, r: f64) -> Vec<f32> {
    let side = cov.width;
    let inside: Vec<bool> = cov.alpha.iter().map(|&c| c >= 0.5).collect();
    let to_out = edt(&inside, side, false);
    let to_in = edt(&inside, side, true);
    cov.alpha
        .iter()
        .enumerate()
        .map(|(k, &c)| {
            // Accumulated areas leave a whole pixel a hair under 1.
            let px = if c > EDGE && c < 1.0 - EDGE {
                0.5 - c
            } else if inside[k] {
                -(math::sqrt(to_out[k]) - 0.5)
            } else {
                math::sqrt(to_in[k]) - 0.5
            };
            (px / r) as f32
        })
        .collect()
}

/// Coverage this close to 0 or 1 counts as none or whole.
const EDGE: f64 = 1e-3;

/// A big number for "no such pixel": the squared distance past any box.
const FAR: f64 = 1e12;

/// The squared Euclidean distance from each pixel to the nearest pixel
/// whose `inside` equals `target` (Felzenszwalb and Huttenlocher's exact
/// transform: a lower envelope of parabolas, columns then rows).
fn edt(inside: &[bool], side: usize, target: bool) -> Vec<f64> {
    let mut grid: Vec<f64> = inside
        .iter()
        .map(|&v| if v == target { 0.0 } else { FAR })
        .collect();
    let mut f = vec![0.0; side];
    let mut d = vec![0.0; side];
    let mut v = vec![0usize; side];
    let mut z = vec![0.0; side + 1];
    for x in 0..side {
        for y in 0..side {
            f[y] = grid[y * side + x];
        }
        edt_1d(&f, &mut d, &mut v, &mut z);
        for y in 0..side {
            grid[y * side + x] = d[y];
        }
    }
    for y in 0..side {
        f.copy_from_slice(&grid[y * side..(y + 1) * side]);
        edt_1d(&f, &mut d, &mut v, &mut z);
        grid[y * side..(y + 1) * side].copy_from_slice(&d);
    }
    grid
}

fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    // Where parabola q meets parabola p.
    let meet = |q: usize, p: usize| {
        ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q - p) as f64)
    };
    let mut k = 0;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s = meet(q, v[k]);
        // z[0] is −∞, so this stops at k = 0.
        while s <= z[k] {
            k -= 1;
            s = meet(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f64 - p as f64;
        *out = dq * dq + f[p];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transform_is_exact() {
        // One target pixel in a 7 × 7 grid: distances are squared offsets.
        let mut inside = vec![false; 49];
        inside[3 * 7 + 2] = true;
        let d = edt(&inside, 7, true);
        for y in 0..7 {
            for x in 0..7 {
                let (dx, dy) = (x as f64 - 2.0, y as f64 - 3.0);
                assert_eq!(d[y * 7 + x], dx * dx + dy * dy);
            }
        }
        // Brute force over a scattered set.
        let inside: Vec<bool> = (0..400).map(|k| (k * 37 + k / 7) % 23 == 0).collect();
        let d = edt(&inside, 20, true);
        for (p, &got) in d.iter().enumerate() {
            let (px, py) = ((p % 20) as f64, (p / 20) as f64);
            let best = (0..400)
                .filter(|&t| inside[t])
                .map(|t| {
                    let (tx, ty) = ((t % 20) as f64, (t / 20) as f64);
                    (tx - px) * (tx - px) + (ty - py) * (ty - py)
                })
                .fold(f64::INFINITY, f64::min);
            assert_eq!(got, best, "pixel {p}");
        }
    }

    #[test]
    fn a_square_field_is_signed_and_in_modules() {
        // A 2-module square at 10 px per module in a 6-module box.
        let mut cov = Coverage::new(60, 60);
        cov.fill("M2 2H4V4H2Z", false, (0.0, 0.0), 10.0);
        let f = signed(&cov, 10.0);
        let at = |x: usize, y: usize| f[y * 60 + x];
        // The center is a module from the edge; far outside, positive.
        assert!((at(30, 30) + 0.95).abs() < 0.06, "{}", at(30, 30));
        assert!(at(5, 5) > 1.0);
        // Just inside and just outside the left edge.
        assert!(at(20, 30) < 0.0 && at(19, 30) > 0.0);
    }
}
