//! A small path rasterizer (SPEC §7.9, W20261124091813): fills SVG path
//! data into anti-aliased coverage, so baked glass can be cut from the
//! code's own geometry (its module shape, finder rings and eyes) instead of
//! one hard-coded rounded square.
//!
//! It reads the path grammar the core writes (`geometry::Path`: `m l h v a
//! c q z`, relative) and its absolute and smooth forms (`M L H V A C Q S T
//! Z`), flattens curves into lines with a fixed tolerance, and accumulates
//! each line's exact signed area per pixel, as font rasterizers do. Every
//! float goes through basic operations and the crate's `math`, so the
//! coverage is the same bytes on every platform.

use alloc::vec;
use alloc::vec::Vec;

use crate::math;

/// The flattening tolerance, in pixels: the most a line strays from the
/// curve it stands for.
const TOLERANCE: f64 = 0.01;

/// Coverage of a `width` × `height` pixel grid, 0–1 per pixel.
#[derive(Clone, Debug)]
pub(crate) struct Coverage {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) alpha: Vec<f64>,
}

impl Coverage {
    pub(crate) fn new(width: usize, height: usize) -> Coverage {
        Coverage {
            width,
            height,
            alpha: vec![0.0; width * height],
        }
    }

    /// Coverage at pixel `(x, y)`, 0 outside the grid.
    #[cfg(test)]
    pub(crate) fn at(&self, x: i64, y: i64) -> f64 {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return 0.0;
        }
        self.alpha[y as usize * self.width + x as usize]
    }

    /// Adds `path`'s fill: the union, `a + b − a·b`, with what is already
    /// here. `path` is SVG path data in user units; a point `(x, y)` lands at
    /// pixel `((x − origin.0) · scale, (y − origin.1) · scale)`.
    pub(crate) fn fill(&mut self, path: &str, evenodd: bool, origin: (f64, f64), scale: f64) {
        let mut acc = Accumulator::new(self.width, self.height);
        for poly in flatten(path, TOLERANCE / scale) {
            for pair in poly.windows(2) {
                let p = |(x, y): (f64, f64)| ((x - origin.0) * scale, (y - origin.1) * scale);
                acc.line(p(pair[0]), p(pair[1]));
            }
        }
        let mut sum = 0.0;
        for (i, a) in self.alpha.iter_mut().enumerate() {
            sum += acc.cells[i];
            let w = if evenodd {
                // The winding number folded to 0–1: odd inside, even out.
                let m = sum.abs() % 2.0;
                if m > 1.0 { 2.0 - m } else { m }
            } else {
                sum.abs().min(1.0)
            };
            *a = *a + w - *a * w;
        }
    }
}

/// Signed area per pixel; a running sum along the rows gives the winding.
struct Accumulator {
    width: usize,
    height: usize,
    cells: Vec<f64>,
}

impl Accumulator {
    fn new(width: usize, height: usize) -> Accumulator {
        Accumulator {
            width,
            height,
            // Room for the cell right of the last column.
            cells: vec![0.0; width * height + 2],
        }
    }

    fn add(&mut self, row: usize, x: i64, v: f64) {
        let x = x.clamp(0, self.width as i64) as usize;
        self.cells[row * self.width + x] += v;
    }

    /// Accumulates the line from `p0` to `p1` (pixels). Points left of the
    /// grid count at its left edge and right of it past its right edge, so
    /// the winding stays right inside.
    fn line(&mut self, p0: (f64, f64), p1: (f64, f64)) {
        if p0.1 == p1.1 {
            return;
        }
        let (dir, p0, p1) = if p0.1 < p1.1 {
            (1.0, p0, p1)
        } else {
            (-1.0, p1, p0)
        };
        let w = self.width as f64;
        let clampx = |x: f64| x.clamp(0.0, w);
        let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1);
        let top = p0.1.max(0.0);
        let bottom = p1.1.min(self.height as f64);
        if top >= bottom {
            return;
        }
        let mut x = p0.0 + (top - p0.1) * dxdy;
        let first = math::floor(top) as usize;
        let last = (math::ceil(bottom) as usize).min(self.height);
        for row in first..last {
            let y0 = (row as f64).max(top);
            let y1 = ((row + 1) as f64).min(bottom);
            let dy = y1 - y0;
            if dy <= 0.0 {
                continue;
            }
            let xnext = x + dxdy * dy;
            let d = dy * dir;
            let (a, b) = if x < xnext {
                (clampx(x), clampx(xnext))
            } else {
                (clampx(xnext), clampx(x))
            };
            let af = math::floor(a);
            let ai = af as i64;
            let bc = math::ceil(b);
            let bi = bc as i64;
            if bi <= ai + 1 {
                // Within one pixel: split by the mean x.
                let mid = 0.5 * (a + b) - af;
                self.add(row, ai, d * (1.0 - mid));
                self.add(row, ai + 1, d * mid);
            } else {
                let s = 1.0 / (b - a);
                let a_frac = a - af;
                let first_area = 0.5 * s * (1.0 - a_frac) * (1.0 - a_frac);
                let b_frac = b - bc + 1.0;
                let last_area = 0.5 * s * b_frac * b_frac;
                self.add(row, ai, d * first_area);
                if bi == ai + 2 {
                    self.add(row, ai + 1, d * (1.0 - first_area - last_area));
                } else {
                    let second = s * (1.5 - a_frac);
                    self.add(row, ai + 1, d * (second - first_area));
                    for xi in ai + 2..bi - 1 {
                        self.add(row, xi, d * s);
                    }
                    let before_last = second + (bi - ai - 3) as f64 * s;
                    self.add(row, bi - 1, d * (1.0 - before_last - last_area));
                }
                self.add(row, bi, d * last_area);
            }
            x = xnext;
        }
    }
}

/// Reads SVG path data into closed polylines, curves flattened to within
/// `tol` user units.
pub(crate) fn flatten(path: &str, tol: f64) -> Vec<Vec<(f64, f64)>> {
    let mut polys: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    let mut lexer = Lexer {
        s: path.as_bytes(),
        i: 0,
    };
    let (mut at, mut start) = ((0.0, 0.0), (0.0, 0.0));
    // The last control point, for S and T.
    let mut last_cubic: Option<(f64, f64)> = None;
    let mut last_quad: Option<(f64, f64)> = None;
    let mut cmd = b'M';
    let close = |cur: &mut Vec<(f64, f64)>, polys: &mut Vec<Vec<(f64, f64)>>, start| {
        if cur.len() > 1 {
            cur.push(start);
            polys.push(core::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    loop {
        lexer.skip();
        let Some(&c) = lexer.s.get(lexer.i) else {
            break;
        };
        if c.is_ascii_alphabetic() {
            cmd = c;
            lexer.i += 1;
            if cmd == b'z' || cmd == b'Z' {
                close(&mut cur, &mut polys, start);
                at = start;
                last_cubic = None;
                last_quad = None;
                continue;
            }
        } else if cmd == b'z' || cmd == b'Z' {
            // Numbers after a close: not valid path data.
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { at } else { (0.0, 0.0) };
        let point = |lexer: &mut Lexer| -> Option<(f64, f64)> {
            Some((base.0 + lexer.number()?, base.1 + lexer.number()?))
        };
        let (mut cubic, mut quad) = (None, None);
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let Some(p) = point(&mut lexer) else { break };
                close(&mut cur, &mut polys, start);
                at = p;
                start = p;
                cur.push(p);
                // Further pairs are lines.
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                let Some(p) = point(&mut lexer) else { break };
                line_to(&mut cur, &mut at, p);
            }
            b'H' => {
                let Some(x) = lexer.number() else { break };
                let p = (if rel { at.0 + x } else { x }, at.1);
                line_to(&mut cur, &mut at, p);
            }
            b'V' => {
                let Some(y) = lexer.number() else { break };
                let p = (at.0, if rel { at.1 + y } else { y });
                line_to(&mut cur, &mut at, p);
            }
            b'C' | b'S' => {
                let c1 = if cmd.eq_ignore_ascii_case(&b'C') {
                    let Some(c1) = point(&mut lexer) else { break };
                    c1
                } else {
                    last_cubic.map_or(at, |c| (2.0 * at.0 - c.0, 2.0 * at.1 - c.1))
                };
                let (Some(c2), Some(end)) = (point(&mut lexer), point(&mut lexer)) else {
                    break;
                };
                begin(&mut cur, at);
                cubic_points(&mut cur, at, c1, c2, end, tol);
                at = end;
                cubic = Some(c2);
            }
            b'Q' | b'T' => {
                let c = if cmd.eq_ignore_ascii_case(&b'Q') {
                    let Some(c) = point(&mut lexer) else { break };
                    c
                } else {
                    last_quad.map_or(at, |c| (2.0 * at.0 - c.0, 2.0 * at.1 - c.1))
                };
                let Some(end) = point(&mut lexer) else { break };
                // A quadratic as the cubic it equals.
                let c1 = (
                    at.0 + 2.0 / 3.0 * (c.0 - at.0),
                    at.1 + 2.0 / 3.0 * (c.1 - at.1),
                );
                let c2 = (
                    end.0 + 2.0 / 3.0 * (c.0 - end.0),
                    end.1 + 2.0 / 3.0 * (c.1 - end.1),
                );
                begin(&mut cur, at);
                cubic_points(&mut cur, at, c1, c2, end, tol);
                at = end;
                quad = Some(c);
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rot)) =
                    (lexer.number(), lexer.number(), lexer.number())
                else {
                    break;
                };
                let (Some(large), Some(sweep)) = (lexer.flag(), lexer.flag()) else {
                    break;
                };
                let Some(end) = point(&mut lexer) else { break };
                begin(&mut cur, at);
                arc_points(&mut cur, at, (rx, ry), rot, large, sweep, end, tol);
                at = end;
            }
            _ => break,
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    close(&mut cur, &mut polys, start);
    polys
}

fn begin(cur: &mut Vec<(f64, f64)>, at: (f64, f64)) {
    if cur.is_empty() {
        cur.push(at);
    }
}

fn line_to(cur: &mut Vec<(f64, f64)>, at: &mut (f64, f64), p: (f64, f64)) {
    begin(cur, *at);
    cur.push(p);
    *at = p;
}

/// A cubic Bézier as `n` lines, `n` from the control polygon's size so the
/// deviation stays under `tol`.
fn cubic_points(
    cur: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    tol: f64,
) {
    // The second differences bound how far the curve leaves its chords.
    let dd = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        let (x, y) = (a.0 - 2.0 * b.0 + c.0, a.1 - 2.0 * b.1 + c.1);
        math::sqrt(x * x + y * y)
    };
    let m = dd(p0, p1, p2).max(dd(p1, p2, p3));
    let n = (math::ceil(math::sqrt(0.75 * m / tol)) as usize).clamp(1, 256);
    for k in 1..=n {
        let t = k as f64 / n as f64;
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        cur.push((
            a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
        ));
    }
}

/// An elliptical arc (SVG's endpoint form, converted to its center form as
/// the SVG specification's appendix does) as lines.
#[allow(clippy::too_many_arguments)]
fn arc_points(
    cur: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    (rx, ry): (f64, f64),
    rotation: f64,
    large: bool,
    sweep: bool,
    p1: (f64, f64),
    tol: f64,
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || p0 == p1 {
        cur.push(p1);
        return;
    }
    let (sin, cos) = math::sin_cos(math::radians(rotation));
    let (hx, hy) = ((p0.0 - p1.0) / 2.0, (p0.1 - p1.1) / 2.0);
    let (x1, y1) = (cos * hx + sin * hy, -sin * hx + cos * hy);
    // Radii too small to reach are scaled up until they do.
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = math::sqrt(lambda);
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = math::sqrt((num / den).max(0.0));
    if large == sweep {
        k = -k;
    }
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let (cx, cy) = (
        cos * cx1 - sin * cy1 + (p0.0 + p1.0) / 2.0,
        sin * cx1 + cos * cy1 + (p0.1 + p1.1) / 2.0,
    );
    let angle = |ux: f64, uy: f64| math::atan2(uy, ux);
    let theta = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - theta;
    let tau = 2.0 * core::f64::consts::PI;
    if sweep && delta < 0.0 {
        delta += tau;
    } else if !sweep && delta > 0.0 {
        delta -= tau;
    }
    // Segments short enough that the sagitta stays under `tol`.
    let r = rx.max(ry);
    let step = if tol < r {
        2.0 * math::sqrt(2.0 * tol / r)
    } else {
        delta.abs()
    };
    let n = (math::ceil(delta.abs() / step.max(1e-6)) as usize).clamp(1, 1024);
    for i in 1..n {
        let t = theta + delta * i as f64 / n as f64;
        let (s, c) = math::sin_cos(t);
        let (ex, ey) = (rx * c, ry * s);
        cur.push((cos * ex - sin * ey + cx, sin * ex + cos * ey + cy));
    }
    cur.push(p1);
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip(&mut self) {
        while let Some(&c) = self.s.get(self.i) {
            if c == b' ' || c == b',' || c == b'\n' || c == b'\t' || c == b'\r' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    /// One SVG number: an optional sign, digits with at most one point, an
    /// optional exponent. A second point or a sign ends it (`.5.5` is two).
    fn number(&mut self) -> Option<f64> {
        self.skip();
        let s = self.s;
        let begin = self.i;
        let mut i = begin;
        if matches!(s.get(i), Some(b'-' | b'+')) {
            i += 1;
        }
        let (mut digits, mut dot) = (false, false);
        while let Some(&c) = s.get(i) {
            if c.is_ascii_digit() {
                digits = true;
            } else if c == b'.' && !dot {
                dot = true;
            } else {
                break;
            }
            i += 1;
        }
        if !digits {
            return None;
        }
        if matches!(s.get(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(s.get(j), Some(b'-' | b'+')) {
                j += 1;
            }
            if s.get(j).is_some_and(u8::is_ascii_digit) {
                while s.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                i = j;
            }
        }
        self.i = i;
        parse(&s[begin..i])
    }

    /// An arc flag: `0` or `1`, which may run into the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// Parses a number the lexer delimited, exactly enough for path data
/// (digits accumulated as integers, then scaled once).
fn parse(t: &[u8]) -> Option<f64> {
    let (neg, t) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let (mut mantissa, mut scale, mut exp, mut seen_dot) = (0.0f64, 0i32, 0i32, false);
    let mut i = 0;
    while i < t.len() {
        match t[i] {
            c @ b'0'..=b'9' => {
                mantissa = mantissa * 10.0 + (c - b'0') as f64;
                if seen_dot {
                    scale -= 1;
                }
            }
            b'.' => seen_dot = true,
            b'e' | b'E' => {
                let rest = core::str::from_utf8(&t[i + 1..]).ok()?;
                exp = rest.parse().ok()?;
                break;
            }
            _ => return None,
        }
        i += 1;
    }
    let p = scale + exp;
    let mut v = mantissa;
    if p > 0 {
        for _ in 0..p {
            v *= 10.0;
        }
    } else {
        // Divide by an exact power of ten, not multiply by 0.1s.
        let mut d = 1.0;
        for _ in 0..(-p) {
            d *= 10.0;
        }
        v /= d;
    }
    Some(if neg { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(c: &Coverage) -> f64 {
        c.alpha.iter().sum()
    }

    #[test]
    fn numbers_split_as_svg_does() {
        let mut l = Lexer {
            s: b"1.5.5-2 .25e1,3",
            i: 0,
        };
        let got: Vec<f64> = core::iter::from_fn(|| l.number()).collect();
        assert_eq!(got, [1.5, 0.5, -2.0, 2.5, 3.0]);
        let mut f = Lexer { s: b"01 1", i: 0 };
        assert_eq!(
            (f.flag(), f.flag(), f.number()),
            (Some(false), Some(true), Some(1.0))
        );
    }

    #[test]
    fn a_square_covers_its_pixels_exactly() {
        let mut c = Coverage::new(8, 8);
        c.fill("m2 2h3v3h-3z", false, (0.0, 0.0), 1.0);
        assert_eq!(area(&c), 9.0);
        assert_eq!(
            (c.at(2, 2), c.at(4, 4), c.at(5, 5), c.at(1, 2)),
            (1.0, 1.0, 0.0, 0.0)
        );
        // Half a pixel off the grid: halves along the edges.
        let mut c = Coverage::new(8, 8);
        c.fill("M2.5 2H5.5V5H2.5Z", false, (0.0, 0.0), 1.0);
        assert!((area(&c) - 9.0).abs() < 1e-9);
        assert_eq!((c.at(2, 3), c.at(5, 3), c.at(3, 3)), (0.5, 0.5, 1.0));
    }

    #[test]
    fn circles_have_their_area() {
        let mut c = Coverage::new(40, 40);
        // Two half-circle arcs of radius 15 around (20, 20).
        c.fill(
            "M5 20a15 15 0 1 1 30 0a15 15 0 1 1-30 0z",
            false,
            (0.0, 0.0),
            1.0,
        );
        let want = core::f64::consts::PI * 225.0;
        // Chords cut inside by at most the tolerance along the perimeter.
        let bound = 2.0 * core::f64::consts::PI * 15.0 * TOLERANCE;
        assert!((area(&c) - want).abs() < bound, "{} vs {want}", area(&c));
        assert_eq!(c.at(20, 20), 1.0);
        assert_eq!(c.at(1, 1), 0.0);
    }

    #[test]
    fn rings_follow_the_fill_rule() {
        // Two squares wound the same way: even-odd cuts a hole, nonzero not.
        let ring = "M1 1H9V9H1ZM3 3H7V7H3Z";
        let mut eo = Coverage::new(10, 10);
        eo.fill(ring, true, (0.0, 0.0), 1.0);
        assert_eq!((area(&eo), eo.at(5, 5), eo.at(2, 5)), (48.0, 0.0, 1.0));
        let mut nz = Coverage::new(10, 10);
        nz.fill(ring, false, (0.0, 0.0), 1.0);
        assert_eq!((area(&nz), nz.at(5, 5)), (64.0, 1.0));
    }

    #[test]
    fn curves_and_scale() {
        // A quadratic and a cubic stay inside their hull; scale and origin
        // move the shape.
        let mut c = Coverage::new(20, 20);
        c.fill(
            "M1 1Q5 1 5 5T9 9L1 9ZM10 10C12 10 14 12 14 14H10Z",
            false,
            (0.0, 0.0),
            1.0,
        );
        assert!(area(&c) > 10.0 && area(&c) < 100.0);
        let mut s = Coverage::new(20, 20);
        s.fill("m1 1h1v1h-1z", false, (0.5, 0.5), 10.0);
        assert!((area(&s) - 100.0).abs() < 1e-9);
        assert_eq!((s.at(5, 5), s.at(14, 14), s.at(15, 15)), (1.0, 1.0, 0.0));
    }

    #[test]
    fn shapes_off_the_grid_are_clipped() {
        let mut c = Coverage::new(4, 4);
        c.fill("M-2 -2H2V2H-2Z", false, (0.0, 0.0), 1.0);
        assert_eq!(area(&c), 4.0);
        let mut c = Coverage::new(4, 4);
        c.fill("M2 2H9V9H2Z", false, (0.0, 0.0), 1.0);
        assert_eq!(area(&c), 4.0);
    }

    #[test]
    fn unions_add_up_without_overflowing() {
        let mut c = Coverage::new(4, 4);
        c.fill("M0 0H2V2H0Z", false, (0.0, 0.0), 1.0);
        c.fill("M1 1H3V3H1Z", false, (0.0, 0.0), 1.0);
        assert_eq!((c.at(1, 1), area(&c)), (1.0, 7.0));
    }
}
