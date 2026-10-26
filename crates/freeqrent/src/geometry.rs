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
