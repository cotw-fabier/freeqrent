//! Layout (SPEC §4 step 1): sorts a matrix's dark modules into the layers
//! a style paints, each with its token, and works out which modules a style
//! leaves out. Geometry happens later, in the SVG writer; the checks only
//! need the tokens and the cleared modules.

use alloc::vec::Vec;

use crate::geometry::Grid;
use crate::matrix::{Corner, ModuleRole, QrMatrix};
use crate::palette::Token;
use crate::scanbox::{Emphasis, TimingStyle, Toward};
use crate::style::Style;

pub(crate) const CORNERS: [Corner; 3] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft];

/// What a layer draws.
#[derive(Clone, Debug)]
pub(crate) enum Part {
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

#[derive(Clone, Debug)]
pub(crate) struct Layer {
    pub(crate) token: Token,
    pub(crate) part: Part,
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
    let size = qr.size();
    let mut grid = Grid::new(size as usize);
    if style.alignment.emphasis != Emphasis::Outline {
        return grid;
    }
    for (cx, cy) in alignment_centers(qr) {
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx.abs() != 3 && dy.abs() != 3 {
                    continue;
                }
                let (x, y) = (cx as i32 + dx, cy as i32 + dy);
                if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
                    continue;
                }
                if !qr.role(x as u16, y as u16).is_function() {
                    grid.set(x as usize, y as usize);
                }
            }
        }
    }
    grid
}

/// The layers in paint order: data, timing, alignment, finder rings, eyes.
/// Empty layers are left out.
pub(crate) fn layers(style: &Style, qr: &QrMatrix) -> Vec<Layer> {
    let size = qr.size() as usize;
    let cleared = cleared(style, qr);
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

    let mut layers = Vec::new();
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
    let centers = alignment_centers(qr);
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
