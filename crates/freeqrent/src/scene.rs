//! Layout (SPEC §4 step 1): sorts a matrix's dark modules into the layers
//! a style paints, each with its token, and works out which modules a style
//! leaves out. Geometry happens later, in the SVG writer; the checks only
//! need the tokens and the cleared modules.

use alloc::vec::Vec;

use crate::geometry::Grid;
use crate::math;
use crate::matrix::{Corner, ModuleRole, QrMatrix};
use crate::palette::Token;
use crate::rng::SplitMix64;
use crate::scanbox::{Emphasis, TimingStyle, Toward};
use crate::style::{Frame, FrameRing, Style};
#[cfg(feature = "halftone")]
use crate::version::Version;

pub(crate) const CORNERS: [Corner; 3] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft];

/// Where the code sits (SPEC §4 step 1). Lengths are in modules.
#[derive(Clone, Debug)]
pub(crate) struct Canvas {
    /// The side of the square viewBox.
    pub(crate) side: i64,
    /// The code's top-left module, on both axes.
    pub(crate) origin: i64,
    /// The circle of a round frame.
    pub(crate) round: Option<Round>,
}

/// The geometry of [`Frame::Circle`], centered on the canvas.
#[derive(Clone, Debug)]
pub(crate) struct Round {
    /// The background disc's radius.
    pub(crate) radius: f64,
    /// The ring's inner and outer radius, if drawn.
    pub(crate) ring: Option<(f64, f64)>,
    /// Whether the ring is dashed.
    pub(crate) dashed: bool,
    /// Filler cells lie inside this circle: the quiet-zone square's
    /// circumcircle, as `(2r)²` so the test is exact on integers.
    filler_r2: i64,
}

/// Room between the filler circle and the edge of the disc, with and
/// without a ring. The ring is half a module wide, one module out.
const ROUND_MARGIN: f64 = 1.0;
const RING_GAP: f64 = 1.0;
const RING_WIDTH: f64 = 0.5;

/// Lays out the canvas for `style`'s frame.
pub(crate) fn canvas(style: &Style, qr: &QrMatrix) -> Canvas {
    let n = qr.size() as i64;
    let q = style.quiet_zone as i64;
    let Frame::Circle {
        ring: ring_style, ..
    } = style.frame
    else {
        return Canvas {
            side: n + 2 * q,
            origin: q,
            round: None,
        };
    };
    // The quiet-zone square, inscribed: its circumradius is side / √2.
    let square = n + 2 * q;
    let filler_radius = square as f64 / core::f64::consts::SQRT_2;
    let ring = (ring_style != FrameRing::None).then(|| {
        let inner = filler_radius + RING_GAP;
        (inner, inner + RING_WIDTH)
    });
    let radius = ring.map_or(filler_radius, |(_, outer)| outer) + ROUND_MARGIN;
    // Same parity as the code, so the code sits on whole modules.
    let mut side = math::ceil(2.0 * radius) as i64;
    if (side - n) % 2 != 0 {
        side += 1;
    }
    Canvas {
        side,
        origin: (side - n) / 2,
        round: Some(Round {
            radius,
            ring,
            dashed: ring_style == FrameRing::Dashed,
            filler_r2: 2 * square * square,
        }),
    }
}

/// The seed for decorative choices: `Style::seed`, or a hash (FNV-1a) of
/// the modules.
pub(crate) fn seed(style: &Style, qr: &QrMatrix) -> u64 {
    if let Some(seed) = style.seed {
        return seed;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (_, _, dark, _) in qr.modules() {
        hash ^= dark as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Filler cells beyond the quiet zone stay this far from each finder's
/// quiet zone, so a finder's surroundings stay clean.
const FINDER_HALO: i64 = 2;

/// The decorative modules of a round frame, in canvas coordinates: cells
/// fully inside the filler circle and outside the quiet-zone square, away
/// from the finders, each kept with probability `filler_density`.
pub(crate) fn filler(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Grid {
    let side = canvas.side;
    let mut grid = Grid::new(side as usize);
    let (Some(round), Frame::Circle { filler_density, .. }) = (&canvas.round, style.frame) else {
        return grid;
    };
    let (n, q, o) = (qr.size() as i64, style.quiet_zone as i64, canvas.origin);
    let finders = CORNERS.map(|c| {
        let (x, y) = finder_origin(c, qr.size());
        (o + x, o + y)
    });
    let mut rng = SplitMix64::new(seed(style, qr));
    for y in 0..side {
        for x in 0..side {
            let in_quiet_square =
                (o - q..o + n + q).contains(&x) && (o - q..o + n + q).contains(&y);
            // The farthest corner from the center, in half-modules.
            let far = |i: i64| (2 * i - side).abs().max((2 * i + 2 - side).abs());
            let inside = far(x).pow(2) + far(y).pow(2) <= round.filler_r2;
            let near_finder = finders.iter().any(|&(fx, fy)| {
                let gap = |i: i64, f: i64| (f - (i + 1)).max(i - (f + 7)).max(0);
                gap(x, fx).max(gap(y, fy)) < q + FINDER_HALO
            });
            if in_quiet_square || !inside || near_finder {
                continue;
            }
            // Drawn for every candidate, so density changes keep the
            // same cells in the same order.
            if rng.next_f64() < filler_density as f64 {
                grid.set(x as usize, y as usize);
            }
        }
    }
    grid
}

/// What a drawn layer is, in an [`AnimationPlan`](crate::AnimationPlan).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LayerRole {
    /// The background: a square, rounded square or disc.
    Background,
    /// The ring of a round frame.
    Ring,
    /// Decorative filler of a round frame.
    Filler,
    /// Data and error-correction modules.
    Data,
    /// Timing patterns.
    Timing,
    /// Alignment patterns.
    Alignment,
    /// Finder outer rings.
    FinderOuter,
    /// Finder eyes.
    FinderEye,
    /// The logo; its path is the plate.
    Logo,
    /// The shimmer band ([`Idle::Shimmer`](crate::Idle::Shimmer)): a
    /// white band fading in and out across its width at 18% opacity at
    /// most, parked left of the canvas.
    Shimmer,
}

/// What a layer draws.
#[derive(Clone, Debug)]
pub(crate) enum Part {
    /// The ring of a round frame.
    Ring,
    /// Decorative filler, in canvas coordinates.
    Filler(Grid),
    /// Modules in the style's module shape.
    Modules(Grid),
    /// Timing modules in a non-plain timing style.
    Timing(Grid),
    /// Finder outer rings at these corners.
    FinderOuter(Vec<Corner>),
    /// Finder eyes at these corners.
    FinderEye(Vec<Corner>),
    /// Alignment patterns (ring and center) at these center modules. They
    /// never use the module shape: scanners look for their 1:1:1:1:1
    /// profile.
    Alignment(Vec<(u16, u16)>),
}

impl Part {
    /// Whether the part's geometry needs the even-odd fill rule (rings
    /// drawn as an outer and an inner outline).
    pub(crate) fn evenodd(&self) -> bool {
        matches!(self, Part::FinderOuter(_) | Part::Alignment(_) | Part::Ring)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Layer {
    pub(crate) token: Token,
    pub(crate) part: Part,
}

impl Layer {
    pub(crate) fn role(&self) -> LayerRole {
        match self.part {
            Part::Ring => LayerRole::Ring,
            Part::Filler(_) => LayerRole::Filler,
            Part::Modules(_) if self.token == Token::Timing => LayerRole::Timing,
            Part::Modules(_) => LayerRole::Data,
            Part::Timing(_) => LayerRole::Timing,
            Part::FinderOuter(_) => LayerRole::FinderOuter,
            Part::FinderEye(_) => LayerRole::FinderEye,
            Part::Alignment(_) => LayerRole::Alignment,
        }
    }
}

/// The top-left module of a finder.
pub(crate) fn finder_origin(corner: Corner, size: u16) -> (i64, i64) {
    let far = size as i64 - 7;
    match corner {
        Corner::TopLeft => (0, 0),
        Corner::TopRight => (far, 0),
        Corner::BottomLeft => (0, far),
    }
}

/// Which way a finder's corner points at the code's center.
pub(crate) fn finder_toward(corner: Corner) -> Toward {
    match corner {
        Corner::TopLeft => (true, true),
        Corner::TopRight => (false, true),
        Corner::BottomLeft => (true, false),
    }
}

/// Which way an alignment pattern centered at `(x, y)` faces the code's
/// center. The pattern at the very center points down-right.
pub(crate) fn alignment_toward((x, y): (u16, u16), size: u16) -> Toward {
    let mid = (size - 1) / 2;
    (x <= mid, y <= mid)
}

/// Alignment pattern centers, in matrix order.
pub(crate) fn alignment_centers(qr: &QrMatrix) -> Vec<(u16, u16)> {
    qr.modules()
        .filter_map(|(x, y, _, role)| match role {
            ModuleRole::Alignment { center } if center == (x, y) => Some(center),
            _ => None,
        })
        .collect()
}

/// The modules the style leaves out: with `Emphasis::Outline`, the data,
/// ECC and remainder modules in the ring one module outside each alignment
/// pattern. Function modules are never cleared.
pub(crate) fn cleared(style: &Style, qr: &QrMatrix) -> Grid {
    let size = qr.size() as usize;
    let mut grid = Grid::new(size);
    for (x, y) in outline_ring(style, size, |x, y| qr.role(x as u16, y as u16)) {
        grid.set(x, y);
    }
    grid
}

/// [`cleared`] for a version whose codewords aren't chosen yet, from its
/// function-pattern roles (row-major): module indices.
#[cfg(feature = "halftone")]
pub(crate) fn cleared_in(style: &Style, version: Version, roles: &[ModuleRole]) -> Vec<usize> {
    let size = version.size() as usize;
    outline_ring(style, size, |x, y| roles[y * size + x])
        .map(|(x, y)| y * size + x)
        .collect()
}

/// The non-function modules one ring outside each alignment pattern, when
/// the style outlines them.
fn outline_ring(
    style: &Style,
    size: usize,
    role: impl Fn(usize, usize) -> ModuleRole,
) -> impl Iterator<Item = (usize, usize)> {
    let mut ring = Vec::new();
    if style.alignment.emphasis == Emphasis::Outline {
        for y in 0..size {
            for x in 0..size {
                let ModuleRole::Alignment { center } = role(x, y) else {
                    continue;
                };
                if center != (x as u16, y as u16) {
                    continue;
                }
                for dy in -3i32..=3 {
                    for dx in -3i32..=3 {
                        if dx.abs() != 3 && dy.abs() != 3 {
                            continue;
                        }
                        let (x, y) = (x as i32 + dx, y as i32 + dy);
                        if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
                            continue;
                        }
                        let (x, y) = (x as usize, y as usize);
                        if !role(x, y).is_function() {
                            ring.push((x, y));
                        }
                    }
                }
            }
        }
    }
    ring.into_iter()
}

/// Every module left out of the drawing: [`cleared`] plus, when the logo
/// clears modules, the ones under its plate (alignment patterns there are
/// dropped separately, see [`alignment_centers_drawn`]).
pub(crate) fn removed(style: &Style, qr: &QrMatrix) -> Grid {
    let mut grid = cleared(style, qr);
    if let Some(logo) = style.logo.as_ref().filter(|l| l.clear_modules) {
        let cover = crate::logo::cover(logo, qr, logo.scale);
        for (x, y, _, _) in qr.modules() {
            if cover.modules.get(x as i64, y as i64) {
                grid.set(x as usize, y as usize);
            }
        }
    }
    grid
}

/// Alignment centers that are drawn: all but those under a clearing logo.
pub(crate) fn alignment_centers_drawn(style: &Style, qr: &QrMatrix) -> Vec<(u16, u16)> {
    let mut centers = alignment_centers(qr);
    if let Some(logo) = style.logo.as_ref().filter(|l| l.clear_modules) {
        let cover = crate::logo::cover(logo, qr, logo.scale);
        centers.retain(|c| !cover.alignment.contains(c));
    }
    centers
}

/// The layers in paint order: frame ring, filler, data, timing,
/// alignment, finder rings, eyes. Empty layers are left out.
pub(crate) fn layers(style: &Style, qr: &QrMatrix) -> Vec<Layer> {
    let size = qr.size() as usize;
    let canvas = canvas(style, qr);
    let mut layers = Vec::new();
    if canvas.round.as_ref().is_some_and(|r| r.ring.is_some()) {
        layers.push(Layer {
            token: Token::Frame,
            part: Part::Ring,
        });
    }
    let filler = filler(style, qr, &canvas);
    if !filler.is_empty() {
        layers.push(Layer {
            token: Token::Filler,
            part: Part::Filler(filler),
        });
    }
    let cleared = removed(style, qr);
    let mut data = Grid::new(size);
    let mut timing = Grid::new(size);
    for (x, y, dark, role) in qr.modules() {
        let (x, y) = (x as usize, y as usize);
        if !dark || cleared.get(x as i64, y as i64) {
            continue;
        }
        match role {
            ModuleRole::Finder { .. } | ModuleRole::Separator | ModuleRole::Alignment { .. } => {}
            ModuleRole::Timing => timing.set(x, y),
            _ => data.set(x, y),
        }
    }

    if !data.is_empty() {
        layers.push(Layer {
            token: Token::Fg,
            part: Part::Modules(data),
        });
    }
    if !timing.is_empty() {
        layers.push(Layer {
            token: Token::Timing,
            part: match style.timing {
                TimingStyle::Plain => Part::Modules(timing),
                _ => Part::Timing(timing),
            },
        });
    }
    let centers = alignment_centers_drawn(style, qr);
    if !centers.is_empty() {
        layers.push(Layer {
            token: match style.alignment.emphasis {
                Emphasis::None => Token::Fg,
                Emphasis::Accent | Emphasis::Outline => Token::Alignment,
            },
            part: Part::Alignment(centers),
        });
    }

    // One finder layer for all three corners, unless a corner has its own
    // token: then each corner gets a layer, so it can be re-themed alone.
    let palette = &style.palette;
    for (shared, per_corner, part) in [
        (
            Token::FinderOuter,
            Token::CornerOuter as fn(Corner) -> Token,
            Part::FinderOuter as fn(Vec<Corner>) -> Part,
        ),
        (Token::FinderEye, Token::CornerEye, Part::FinderEye),
    ] {
        if CORNERS
            .iter()
            .any(|&c| palette.get(per_corner(c)).is_some())
        {
            for c in CORNERS {
                layers.push(Layer {
                    token: per_corner(c),
                    part: part(alloc::vec![c]),
                });
            }
        } else {
            layers.push(Layer {
                token: shared,
                part: part(CORNERS.to_vec()),
            });
        }
    }
    layers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, QrCode};

    fn round(density: f32, ring: FrameRing) -> Style {
        Style::new().frame(Frame::Circle {
            filler_density: density,
            ring,
        })
    }

    #[test]
    fn square_frames_keep_the_quiet_zone_layout() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let c = canvas(&Style::new().quiet_zone(3), &qr);
        assert_eq!((c.side, c.origin), (27, 3));
        assert!(c.round.is_none());
    }

    #[test]
    fn round_frames_circumscribe_the_quiet_zone() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap(); // 21 modules
        for ring in [FrameRing::None, FrameRing::Solid] {
            let c = canvas(&round(0.5, ring), &qr);
            let r = c.round.as_ref().unwrap();
            // 29 / √2 ≈ 20.5, plus the margin (and the ring).
            assert!(r.radius >= 29.0 / core::f64::consts::SQRT_2 + ROUND_MARGIN);
            assert!(c.side as f64 >= 2.0 * r.radius);
            assert_eq!((c.side - 21) % 2, 0);
            assert_eq!(c.origin * 2 + 21, c.side);
        }
    }

    #[test]
    fn filler_stays_outside_the_quiet_zone_and_inside_the_circle() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let style = round(1.0, FrameRing::None);
        let c = canvas(&style, &qr);
        let grid = filler(&style, &qr, &c);
        let (o, q, n) = (c.origin, 4, 21);
        let half = c.side as f64 / 2.0;
        let mut count = 0;
        for y in 0..c.side {
            for x in 0..c.side {
                if !grid.get(x, y) {
                    continue;
                }
                count += 1;
                let quiet = (o - q..o + n + q).contains(&x) && (o - q..o + n + q).contains(&y);
                assert!(!quiet, "({x}, {y})");
                for (cx, cy) in [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)] {
                    let (dx, dy) = (cx as f64 - half, cy as f64 - half);
                    assert!(dx * dx + dy * dy <= 29.0 * 29.0 / 2.0 + 1e-9);
                }
            }
        }
        assert!(count > 0);
    }

    #[test]
    fn filler_is_seeded() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let cells = |style: &Style| {
            let c = canvas(style, &qr);
            let g = filler(style, &qr, &c);
            (0..c.side)
                .flat_map(|y| (0..c.side).map(move |x| (x, y)))
                .filter(|&(x, y)| g.get(x, y))
                .collect::<Vec<_>>()
        };
        let style = round(0.5, FrameRing::None);
        assert_eq!(cells(&style), cells(&style));
        assert_ne!(cells(&style.clone().seed(1)), cells(&style.clone().seed(2)));
        assert!(cells(&round(0.0, FrameRing::None)).is_empty());
        let other = QrCode::encode("WORLD", Ecc::M).unwrap();
        let c = canvas(&style, &other);
        let g = filler(&style, &other, &c);
        let differs = (0..c.side)
            .flat_map(|y| (0..c.side).map(move |x| (x, y)))
            .any(|(x, y)| g.get(x, y) != cells(&style).contains(&(x, y)));
        assert!(differs, "the default seed follows the code");
    }
}
