//! Geometry (SPEC §4 step 2): modules to SVG path data. Coordinates are
//! integers in thousandths of a module, and commands are written relative,
//! so path data is exact and every number has at most three decimals.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::num::write_scaled;
use crate::style::{Axis, ModuleShape};

/// One module, in path units.
pub(crate) const UNIT: i64 = 1000;

/// Converts a fraction of a module to path units.
pub(crate) fn units(fraction: f32) -> i64 {
    crate::math::round(fraction as f64 * UNIT as f64) as i64
}

/// Path data written as it's built.
#[derive(Clone, Debug, Default)]
pub(crate) struct Path {
    d: String,
    at: (i64, i64),
    start: (i64, i64),
    /// After a number: whether it had a decimal point. `None` after a
    /// command letter.
    last_number_dot: Option<bool>,
}

impl Path {
    pub(crate) fn new() -> Path {
        Path::default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.d.is_empty()
    }

    pub(crate) fn data(&self) -> &str {
        &self.d
    }

    fn cmd(&mut self, c: char) {
        self.d.push(c);
        self.last_number_dot = None;
    }

    /// Writes a number, adding a separator only where the grammar needs one.
    fn num(&mut self, v: i64) {
        let fraction = v % UNIT != 0;
        let starts_with_dot = v.unsigned_abs() < UNIT as u64 && v != 0;
        if let Some(prev_dot) = self.last_number_dot {
            let self_delimiting = v < 0 || (starts_with_dot && prev_dot);
            if !self_delimiting {
                self.d.push(' ');
            }
        }
        write_scaled(&mut self.d, v, 3).expect("writing to a String");
        self.last_number_dot = Some(fraction);
    }

    pub(crate) fn move_to(&mut self, x: i64, y: i64) {
        self.cmd('m');
        self.num(x - self.at.0);
        self.num(y - self.at.1);
        self.at = (x, y);
        self.start = (x, y);
    }

    pub(crate) fn line_to(&mut self, x: i64, y: i64) {
        let (dx, dy) = (x - self.at.0, y - self.at.1);
        match (dx, dy) {
            (0, 0) => return,
            (_, 0) => {
                self.cmd('h');
                self.num(dx);
            }
            (0, _) => {
                self.cmd('v');
                self.num(dy);
            }
            _ => {
                self.cmd('l');
                self.num(dx);
                self.num(dy);
            }
        }
        self.at = (x, y);
    }

    /// A circular arc of radius `r` to `(x, y)`.
    pub(crate) fn arc_to(&mut self, r: i64, large: bool, sweep: bool, x: i64, y: i64) {
        self.cmd('a');
        self.num(r);
        self.num(r);
        self.num(0);
        self.num(UNIT * large as i64);
        self.num(UNIT * sweep as i64);
        self.num(x - self.at.0);
        self.num(y - self.at.1);
        self.at = (x, y);
    }

    /// A cubic Bézier through control points `c1`, `c2` to `end`.
    pub(crate) fn cubic_to(&mut self, c1: (i64, i64), c2: (i64, i64), end: (i64, i64)) {
        self.cmd('c');
        for (x, y) in [c1, c2, end] {
            self.num(x - self.at.0);
            self.num(y - self.at.1);
        }
        self.at = end;
    }

    /// A quadratic Bézier through control point `c` to `end`.
    pub(crate) fn quad_to(&mut self, c: (i64, i64), end: (i64, i64)) {
        self.cmd('q');
        for (x, y) in [c, end] {
            self.num(x - self.at.0);
            self.num(y - self.at.1);
        }
        self.at = end;
    }

    pub(crate) fn close(&mut self) {
        self.cmd('z');
        self.at = self.start;
    }

    /// An axis-aligned rectangle with each corner's radius given separately
    /// (top-left, top-right, bottom-right, bottom-left), drawn clockwise.
    pub(crate) fn rounded_rect(&mut self, x: i64, y: i64, w: i64, h: i64, radii: [i64; 4]) {
        let [tl, tr, br, bl] = radii;
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        if tr > 0 {
            self.arc_to(tr, false, true, x + w, y + tr);
        }
        self.line_to(x + w, y + h - br);
        if br > 0 {
            self.arc_to(br, false, true, x + w - br, y + h);
        }
        self.line_to(x + bl, y + h);
        if bl > 0 {
            self.arc_to(bl, false, true, x, y + h - bl);
        }
        // With a square top-left corner, closing draws the last edge.
        if tl > 0 {
            self.line_to(x, y + tl);
            self.arc_to(tl, false, true, x + tl, y);
        }
        self.close();
    }

    pub(crate) fn rect(&mut self, x: i64, y: i64, w: i64, h: i64) {
        self.rounded_rect(x, y, w, h, [0; 4]);
    }

    pub(crate) fn circle(&mut self, cx: i64, cy: i64, r: i64) {
        self.move_to(cx - r, cy);
        self.arc_to(r, false, true, cx + r, cy);
        self.arc_to(r, false, true, cx - r, cy);
        self.close();
    }
}

/// Which modules belong to one layer: a square grid where anything out of
/// range is empty.
#[derive(Clone, Debug)]
pub(crate) struct Grid {
    size: usize,
    cells: Vec<bool>,
}

impl Grid {
    pub(crate) fn new(size: usize) -> Grid {
        Grid {
            size,
            cells: vec![false; size * size],
        }
    }

    pub(crate) fn set(&mut self, x: usize, y: usize) {
        self.cells[y * self.size + x] = true;
    }

    pub(crate) fn get(&self, x: i64, y: i64) -> bool {
        let n = self.size as i64;
        (0..n).contains(&x) && (0..n).contains(&y) && self.cells[(y * n + x) as usize]
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.cells.contains(&true)
    }

    fn size(&self) -> i64 {
        self.size as i64
    }
}

/// Draws every module of `grid` in `shape`, with the grid's origin at
/// `origin` path units.
pub(crate) fn draw_modules(shape: &ModuleShape, grid: &Grid, origin: i64, path: &mut Path) {
    let n = grid.size();
    let at = |i: i64| origin + i * UNIT;
    match *shape {
        ModuleShape::Square { gap } if units(gap) == 0 => {
            for_runs(grid, Axis::Horizontal, |y, x0, x1| {
                path.rect(at(x0), at(y), (x1 - x0 + 1) * UNIT, UNIT);
            });
        }
        ModuleShape::Square { gap } => {
            let g = units(gap);
            let side = UNIT - g;
            for y in 0..n {
                for x in (0..n).filter(|&x| grid.get(x, y)) {
                    path.rect(at(x) + g / 2, at(y) + g / 2, side, side);
                }
            }
        }
        ModuleShape::Rounded { radius } => {
            let r = units(radius);
            // A run's corner is rounded when nothing touches it on either
            // side, so only the run's ends and their vertical neighbors matter.
            for_runs(grid, Axis::Horizontal, |y, x0, x1| {
                let free = |x: i64, dy: i64| if grid.get(x, y + dy) { 0 } else { r };
                path.rounded_rect(
                    at(x0),
                    at(y),
                    (x1 - x0 + 1) * UNIT,
                    UNIT,
                    [free(x0, -1), free(x1, -1), free(x1, 1), free(x0, 1)],
                );
            });
        }
        ModuleShape::Dots { size } => {
            let r = units(size) / 2;
            for y in 0..n {
                for x in (0..n).filter(|&x| grid.get(x, y)) {
                    path.circle(at(x) + UNIT / 2, at(y) + UNIT / 2, r);
                }
            }
        }
        ModuleShape::Liquid { smoothness } => {
            draw_liquid(grid, origin, units(smoothness) / 2, path);
        }
        ModuleShape::Bars { axis, radius } => {
            let r = units(radius);
            for_runs(grid, axis, |line, a, b| {
                let len = (b - a + 1) * UNIT;
                match axis {
                    Axis::Horizontal => path.rounded_rect(at(a), at(line), len, UNIT, [r; 4]),
                    Axis::Vertical => path.rounded_rect(at(line), at(a), UNIT, len, [r; 4]),
                }
            });
        }
    }
}

/// Directions on the module lattice, clockwise on screen (y grows down).
const RIGHT: usize = 0;
const DOWN: usize = 1;
const LEFT: usize = 2;
const UP: usize = 3;
const STEP: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Liquid (SPEC §7.1): traces the boundary of each dark region and rounds
/// every corner of it with radius `r` path units.
///
/// The boundary is the set of cell edges with dark on one side and light
/// on the other, directed so dark is on the right: outer contours run
/// clockwise and holes counter-clockwise, so the nonzero fill rule is
/// enough. At a vertex where two dark cells touch only diagonally the walk
/// turns right, which keeps them in separate blobs.
///
/// Each corner leaves the incoming edge `r` before the vertex and joins
/// the outgoing edge `r` after it, with a quadratic Bézier whose control
/// point is the vertex. On a convex corner that rounds the blob; on a
/// concave one the curve runs through the light cell, filling the inner
/// corner. Edges are at least one module long and `r` at most half of
/// one, so neighboring corners never overlap.
fn draw_liquid(grid: &Grid, origin: i64, r: i64, path: &mut Path) {
    let n = grid.size();
    let side = (n + 1) as usize;
    let dark = |x: i64, y: i64| grid.get(x, y);
    // Outgoing boundary edges at each lattice vertex, one bit per direction.
    let mut out = vec![0u8; side * side];
    for y in 0..=n {
        for x in 0..=n {
            let mut bits = 0;
            if dark(x, y) && !dark(x, y - 1) {
                bits |= 1 << RIGHT;
            }
            if dark(x - 1, y) && !dark(x, y) {
                bits |= 1 << DOWN;
            }
            if dark(x - 1, y - 1) && !dark(x - 1, y) {
                bits |= 1 << LEFT;
            }
            if dark(x, y - 1) && !dark(x - 1, y - 1) {
                bits |= 1 << UP;
            }
            out[y as usize * side + x as usize] = bits;
        }
    }
    let index = |(x, y): (i64, i64)| y as usize * side + x as usize;
    let mut dirs: Vec<usize> = Vec::new();
    let mut corners: Vec<((i64, i64), usize, usize)> = Vec::new();
    for start in 0..out.len() {
        while out[start] != 0 {
            let first_v = ((start % side) as i64, (start / side) as i64);
            let first = out[start].trailing_zeros() as usize;
            let (mut v, mut dir) = (first_v, first);
            dirs.clear();
            // Each edge has exactly one successor (at a diagonal touch the
            // two right turns pair the four edges), so the walk is a cycle
            // back to the first edge.
            loop {
                out[index(v)] &= !(1 << dir);
                dirs.push(dir);
                v = (v.0 + STEP[dir].0, v.1 + STEP[dir].1);
                let here = out[index(v)];
                let next = [(dir + 1) % 4, dir, (dir + 3) % 4]
                    .into_iter()
                    .find(|&d| here & (1 << d) != 0);
                match next {
                    Some(next) => dir = next,
                    // The only edge left is the first one, already taken.
                    None => break,
                }
            }
            // Corners: the vertex after step i, where the direction changes.
            corners.clear();
            let mut p = first_v;
            for (i, &d) in dirs.iter().enumerate() {
                p = (p.0 + STEP[d].0, p.1 + STEP[d].1);
                let next = dirs[(i + 1) % dirs.len()];
                if next != d {
                    corners.push((p, d, next));
                }
            }
            // Start at the loop's top-left vertex, where the walk began.
            corners.rotate_right(1);
            draw_corners(&corners, origin, r, path);
        }
    }
}

/// Draws one closed contour through `corners` (vertex, incoming and
/// outgoing direction), cutting each corner with a quadratic of reach `r`.
fn draw_corners(corners: &[((i64, i64), usize, usize)], origin: i64, r: i64, path: &mut Path) {
    let point = |(x, y): (i64, i64)| (origin + x * UNIT, origin + y * UNIT);
    let offset = |(x, y): (i64, i64), d: usize, by: i64| (x + STEP[d].0 * by, y + STEP[d].1 * by);
    let Some(&(p0, _, out0)) = corners.first() else {
        return;
    };
    let start = offset(point(p0), out0, r);
    path.move_to(start.0, start.1);
    for (i, (p, d_in, d_out)) in corners
        .iter()
        .copied()
        .enumerate()
        .skip(1)
        .chain([(0, corners[0])])
    {
        // With square corners, closing the path draws the last edge.
        if i == 0 && r == 0 {
            break;
        }
        let p = point(p);
        let before = offset(p, d_in, -r);
        path.line_to(before.0, before.1);
        if r > 0 {
            path.quad_to(p, offset(p, d_out, r));
        }
    }
    path.close();
}

/// Calls `f(line, start, end)` for every maximal run of set cells along
/// `axis` (inclusive ends).
fn for_runs(grid: &Grid, axis: Axis, mut f: impl FnMut(i64, i64, i64)) {
    let n = grid.size();
    let get = |line: i64, i: i64| match axis {
        Axis::Horizontal => grid.get(i, line),
        Axis::Vertical => grid.get(line, i),
    };
    for line in 0..n {
        let mut i = 0;
        while i < n {
            if !get(line, i) {
                i += 1;
                continue;
            }
            let start = i;
            while i + 1 < n && get(line, i + 1) {
                i += 1;
            }
            f(line, start, i);
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> Grid {
        let mut g = Grid::new(rows.len());
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                if c == '#' {
                    g.set(x, y);
                }
            }
        }
        g
    }

    fn draw(shape: ModuleShape, rows: &[&str]) -> String {
        let mut p = Path::new();
        draw_modules(&shape, &grid(rows), 0, &mut p);
        String::from(p.data())
    }

    #[test]
    fn separators_only_where_needed() {
        let mut p = Path::new();
        p.move_to(1500, 2000);
        p.line_to(1500, 2500);
        p.line_to(1000, 3000);
        p.line_to(1250, 3250);
        p.close();
        assert_eq!(p.data(), "m1.5 2v.5l-.5.5l.25.25z");
        let mut p = Path::new();
        p.move_to(500, 500);
        p.move_to(1000, 1000);
        assert_eq!(p.data(), "m.5.5m.5.5");
        let mut p = Path::new();
        p.move_to(1000, 2000);
        p.move_to(3000, 3500);
        assert_eq!(p.data(), "m1 2m2 1.5");
    }

    #[test]
    fn squares_merge_runs() {
        let d = draw(ModuleShape::SQUARE, &["##.", ".##", "..."]);
        assert_eq!(d, "m0 0h2v1h-2zm1 1h2v1h-2z");
    }

    #[test]
    fn gapped_squares_are_separate() {
        let d = draw(ModuleShape::Square { gap: 0.2 }, &["##", ".."]);
        assert_eq!(d, "m.1.1h.8v.8h-.8zm1 0h.8v.8h-.8z");
    }

    #[test]
    fn rounded_corners_only_where_free() {
        // An L: the top module's bottom-left corner touches the one below.
        let d = draw(ModuleShape::Rounded { radius: 0.5 }, &["#.", "##"]);
        assert_eq!(
            d,
            "m.5 0a.5.5 0 0 1 .5.5v.5h-1v-.5a.5.5 0 0 1 .5-.5zm-.5 1h1.5a.5.5 0 0 1 .5.5a.5.5 0 0 1-.5.5h-1a.5.5 0 0 1-.5-.5z"
        );
    }

    #[test]
    fn dots_are_circles() {
        let d = draw(ModuleShape::Dots { size: 0.8 }, &["#"]);
        assert_eq!(d, "m.1.5a.4.4 0 0 1 .8 0a.4.4 0 0 1-.8 0z");
    }

    fn liquid(smoothness: f32, rows: &[&str]) -> String {
        draw(ModuleShape::Liquid { smoothness }, rows)
    }

    #[test]
    fn liquid_traces_region_outlines() {
        // Square corners: the outline of each region, runs merged.
        assert_eq!(liquid(0.0, &["#"]), "m0 0h1v1h-1z");
        assert_eq!(liquid(0.0, &["##", "#."]), "m0 0h2v1h-1v1h-1z");
        // Rounded: each corner is a quadratic with the vertex as control.
        assert_eq!(
            liquid(1.0, &["#"]),
            "m.5 0q.5 0 .5.5q0 .5-.5.5q-.5 0-.5-.5q0-.5.5-.5z"
        );
    }

    #[test]
    fn liquid_fills_inner_corners() {
        // The concave corner at (1, 1) bends into the light cell.
        let d = liquid(0.5, &["##", "#."]);
        assert_eq!(d.matches('q').count(), 6);
        assert!(d.contains("h-.5q-.25 0-.25.25v.5"), "{d}");
    }

    #[test]
    fn liquid_keeps_diagonal_neighbors_apart() {
        let d = liquid(0.0, &["#.", ".#"]);
        assert_eq!(d, "m0 0h1v1h-1zm1 1h1v1h-1z");
    }

    #[test]
    fn liquid_holes_wind_the_other_way() {
        let d = liquid(0.0, &["###", "#.#", "###"]);
        // Outer contour clockwise, the hole counter-clockwise.
        assert_eq!(d, "m0 0h3v3h-3zm1 1v1h1v-1z");
    }

    #[test]
    fn bars_follow_their_axis() {
        let v = draw(
            ModuleShape::Bars {
                axis: Axis::Vertical,
                radius: 0.0,
            },
            &["#.", "##"],
        );
        assert_eq!(v, "m0 0h1v2h-1zm1 1h1v1h-1z");
        let h = draw(
            ModuleShape::Bars {
                axis: Axis::Horizontal,
                radius: 0.0,
            },
            &["#.", "##"],
        );
        assert_eq!(h, "m0 0h1v1h-1zm0 1h2v1h-2z");
    }
}
