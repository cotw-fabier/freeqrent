//! Scan boxes (SPEC §7.2): finder, alignment and timing pattern styles and
//! their geometry. Every shape keeps the pattern's footprint and crosses
//! each axis through the center at the module boundaries, so the 1:1:3:1:1
//! (finder) and 1:1:1:1:1 (alignment) profiles scanners look for survive.

use crate::geometry::{Path, UNIT, units};
use crate::matrix::Corner;

/// The outer ring of a finder (7×7) or alignment pattern (5×5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RingShape {
    /// Square corners.
    Square,
    /// Rounded corners of this radius in modules: finders 0–2.5 (rounder
    /// finder rings, circles included, made ZXing miss about 1 in 200 clean
    /// renders from version 15 up), alignment patterns 0–2.5 (2.5 is a
    /// circle).
    Rounded(f32),
    /// A superellipse, between a square and a circle.
    Squircle,
    /// Three corners rounded, the one pointing at the code's center sharp.
    Leaf,
}

/// The center of a finder (3×3) or alignment pattern (1×1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EyeShape {
    /// A square.
    Square,
    /// Rounded corners of this radius in modules (finders: 0–1.5).
    Rounded(f32),
    /// A circle.
    Circle,
    /// A superellipse.
    Squircle,
    /// Three corners rounded, the one pointing at the code's center sharp.
    Leaf,
}

/// How the three finders are drawn, with optional per-corner overrides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinderStyle {
    /// The outer ring.
    pub outer: RingShape,
    /// The eye.
    pub eye: EyeShape,
    corners: [Option<(RingShape, EyeShape)>; 3],
}

impl FinderStyle {
    /// Classic square finders.
    pub const SQUARE: FinderStyle = FinderStyle::new(RingShape::Square, EyeShape::Square);

    /// The same ring and eye at all three corners.
    pub const fn new(outer: RingShape, eye: EyeShape) -> FinderStyle {
        FinderStyle {
            outer,
            eye,
            corners: [None; 3],
        }
    }

    /// Overrides one corner's shapes.
    pub const fn corner(mut self, corner: Corner, outer: RingShape, eye: EyeShape) -> FinderStyle {
        self.corners[corner_index(corner)] = Some((outer, eye));
        self
    }

    /// The shapes drawn at `corner`.
    pub const fn at(&self, corner: Corner) -> (RingShape, EyeShape) {
        match self.corners[corner_index(corner)] {
            Some(shapes) => shapes,
            None => (self.outer, self.eye),
        }
    }
}

impl Default for FinderStyle {
    fn default() -> FinderStyle {
        FinderStyle::SQUARE
    }
}

const fn corner_index(corner: Corner) -> usize {
    match corner {
        Corner::TopLeft => 0,
        Corner::TopRight => 1,
        Corner::BottomLeft => 2,
    }
}

/// The shape of alignment patterns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignmentShape {
    /// The top-left finder's shapes, scaled to 5×5 and 1×1.
    Match,
    /// Their own shapes. Radii are in modules (ring 0–2.5, eye 0–0.5).
    Own {
        /// The 5×5 ring.
        outer: RingShape,
        /// The center module.
        eye: EyeShape,
    },
}

/// How much alignment patterns stand out (SPEC §7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Emphasis {
    /// Painted like data (`fg`).
    None,
    /// Painted with the `alignment` token.
    Accent,
    /// Painted with the `alignment` token on a clean plate: the data
    /// modules in a one-module ring around each pattern are left out. They
    /// count against the error-correction budget.
    Outline,
}

/// How alignment patterns are drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentStyle {
    /// Their shape.
    pub shape: AlignmentShape,
    /// How they're painted.
    pub emphasis: Emphasis,
}

impl Default for AlignmentStyle {
    /// Matching the finders, in the `alignment` token (which falls back to
    /// `finder.outer`).
    fn default() -> AlignmentStyle {
        AlignmentStyle {
            shape: AlignmentShape::Match,
            emphasis: Emphasis::Accent,
        }
    }
}

/// How the timing patterns are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TimingStyle {
    /// In the data module shape.
    #[default]
    Plain,
    /// Circles of 0.8 modules.
    Dotted,
    /// Dashes along the line, 0.6 modules thick.
    Dashed,
}

/// A pattern's size: the ring's half-width, the eye's half-width, and how
/// `Match` scales finder radii into it.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    ring_half: i64,
    eye_half: i64,
    ring_scale: f32,
    eye_scale: f32,
}

pub(crate) const FINDER: Frame = Frame {
    ring_half: 3500,
    eye_half: 1500,
    ring_scale: 1.0,
    eye_scale: 1.0,
};

pub(crate) const ALIGNMENT: Frame = Frame {
    ring_half: 2500,
    eye_half: 500,
    ring_scale: 5.0 / 7.0,
    eye_scale: 1.0 / 3.0,
};

/// Which corner of a pattern points at the code's center: `(right, down)`.
pub(crate) type Toward = (bool, bool);

/// Superellipse control-point factor: a cubic from (1, 0) to (0, 1) with
/// controls (1, k) and (k, 1) passes through 2^-¼ ≈ 0.841 at 45°, the n = 4
/// superellipse's point there, when k = 0.909.
const SQUIRCLE_K: i64 = 909;

/// Draws a ring centered at `(cx, cy)`. `scaled` applies `Frame`'s radius
/// scale (for `Match`).
pub(crate) fn draw_ring(
    path: &mut Path,
    shape: RingShape,
    frame: Frame,
    scaled: bool,
    (cx, cy): (i64, i64),
    toward: Toward,
) {
    let outer = frame.ring_half;
    let inner = outer - UNIT;
    let scale = if scaled { frame.ring_scale } else { 1.0 };
    match shape {
        RingShape::Square => {
            contour(path, Contour::Rect([0; 4]), cx, cy, outer);
            contour(path, Contour::Rect([0; 4]), cx, cy, inner);
        }
        RingShape::Rounded(r) => {
            let r = units(r * scale).clamp(0, outer);
            contour(path, Contour::Rect([r; 4]), cx, cy, outer);
            contour(path, Contour::Rect([(r - UNIT).max(0); 4]), cx, cy, inner);
        }
        RingShape::Squircle => {
            contour(path, Contour::Squircle, cx, cy, outer);
            contour(path, Contour::Squircle, cx, cy, inner);
        }
        RingShape::Leaf => {
            let r = outer * 6 / 7;
            contour(path, Contour::Rect(leaf(r, toward)), cx, cy, outer);
            contour(path, Contour::Rect(leaf(r - UNIT, toward)), cx, cy, inner);
        }
    }
}

/// Draws an eye centered at `(cx, cy)`.
pub(crate) fn draw_eye(
    path: &mut Path,
    shape: EyeShape,
    frame: Frame,
    scaled: bool,
    (cx, cy): (i64, i64),
    toward: Toward,
) {
    let half = frame.eye_half;
    let scale = if scaled { frame.eye_scale } else { 1.0 };
    match shape {
        EyeShape::Square => contour(path, Contour::Rect([0; 4]), cx, cy, half),
        EyeShape::Rounded(r) => {
            let r = units(r * scale).clamp(0, half);
            contour(path, Contour::Rect([r; 4]), cx, cy, half);
        }
        EyeShape::Circle => path.circle(cx, cy, half),
        EyeShape::Squircle => contour(path, Contour::Squircle, cx, cy, half),
        EyeShape::Leaf => contour(
            path,
            Contour::Rect(leaf(half * 2 / 3, toward)),
            cx,
            cy,
            half,
        ),
    }
}

/// Radii (top-left, top-right, bottom-right, bottom-left) with the corner
/// facing `toward` left sharp.
fn leaf(r: i64, (right, down): Toward) -> [i64; 4] {
    let sharp = match (right, down) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    };
    let mut radii = [r.max(0); 4];
    radii[sharp] = 0;
    radii
}

enum Contour {
    Rect([i64; 4]),
    Squircle,
}

fn contour(path: &mut Path, shape: Contour, cx: i64, cy: i64, half: i64) {
    match shape {
        Contour::Rect(radii) => path.rounded_rect(cx - half, cy - half, 2 * half, 2 * half, radii),
        Contour::Squircle => {
            let k = half * SQUIRCLE_K / 1000;
            path.move_to(cx + half, cy);
            path.cubic_to((cx + half, cy + k), (cx + k, cy + half), (cx, cy + half));
            path.cubic_to((cx - k, cy + half), (cx - half, cy + k), (cx - half, cy));
            path.cubic_to((cx - half, cy - k), (cx - k, cy - half), (cx, cy - half));
            path.cubic_to((cx + k, cy - half), (cx + half, cy - k), (cx + half, cy));
            path.close();
        }
    }
}

/// Draws one dark timing module at `(x, y)` path units (its top-left).
/// `horizontal` is the direction of the timing line.
pub(crate) fn draw_timing_module(
    path: &mut Path,
    style: TimingStyle,
    x: i64,
    y: i64,
    horizontal: bool,
) {
    match style {
        TimingStyle::Plain => unreachable!("plain timing uses the module shape"),
        TimingStyle::Dotted => path.circle(x + UNIT / 2, y + UNIT / 2, UNIT * 2 / 5),
        TimingStyle::Dashed => {
            let inset = UNIT / 5;
            if horizontal {
                path.rect(x, y + inset, UNIT, UNIT - 2 * inset);
            } else {
                path.rect(x + inset, y, UNIT - 2 * inset, UNIT);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    fn ring(shape: RingShape) -> String {
        let mut p = Path::new();
        draw_ring(&mut p, shape, FINDER, false, (3500, 3500), (true, true));
        String::from(p.data())
    }

    fn eye(shape: EyeShape) -> String {
        let mut p = Path::new();
        draw_eye(&mut p, shape, FINDER, false, (3500, 3500), (true, true));
        String::from(p.data())
    }

    #[test]
    fn square_ring_and_eye() {
        assert_eq!(ring(RingShape::Square), "m0 0h7v7h-7zm1 1h5v5h-5z");
        assert_eq!(eye(EyeShape::Square), "m2 2h3v3h-3z");
    }

    #[test]
    fn rounded_ring_stays_concentric() {
        assert_eq!(
            ring(RingShape::Rounded(1.5)),
            "m1.5 0h4a1.5 1.5 0 0 1 1.5 1.5v4a1.5 1.5 0 0 1-1.5 1.5h-4a1.5 1.5 0 0 1-1.5-1.5v-4a1.5 1.5 0 0 1 1.5-1.5z\
             m0 1h4a.5.5 0 0 1 .5.5v4a.5.5 0 0 1-.5.5h-4a.5.5 0 0 1-.5-.5v-4a.5.5 0 0 1 .5-.5z"
        );
    }

    #[test]
    fn leaf_keeps_the_inward_corner_sharp() {
        // Top-left finder: the center is down and to the right.
        assert_eq!(leaf(3000, (true, true)), [3000, 3000, 0, 3000]);
        assert_eq!(leaf(3000, (false, true)), [3000, 3000, 3000, 0]);
        assert_eq!(leaf(3000, (true, false)), [3000, 0, 3000, 3000]);
    }

    #[test]
    fn shapes_cross_the_axes_at_module_boundaries() {
        // Each contour touches (cx ± half, cy) and (cx, cy ± half).
        assert_eq!(eye(EyeShape::Circle), "m2 3.5a1.5 1.5 0 1 1 0 .001z");
        assert!(eye(EyeShape::Squircle).starts_with("m5 3.5c0 1.363"));
    }

    #[test]
    fn match_scales_radii() {
        let mut p = Path::new();
        draw_ring(
            &mut p,
            RingShape::Rounded(3.5),
            ALIGNMENT,
            true,
            (2500, 2500),
            (true, true),
        );
        assert!(p.data().starts_with("m2.5 0a2.5 2.5 0 0 1"), "{}", p.data());
    }

    #[test]
    fn per_corner_overrides() {
        let style = FinderStyle::new(RingShape::Squircle, EyeShape::Circle).corner(
            Corner::TopRight,
            RingShape::Square,
            EyeShape::Leaf,
        );
        assert_eq!(
            style.at(Corner::TopLeft),
            (RingShape::Squircle, EyeShape::Circle)
        );
        assert_eq!(
            style.at(Corner::TopRight),
            (RingShape::Square, EyeShape::Leaf)
        );
    }
}
