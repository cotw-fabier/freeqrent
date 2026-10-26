//! Layout (SPEC §4 step 1): sorts a matrix's dark modules into the layers
//! a style paints, each with its token. Geometry happens later, in the SVG
//! writer; the checks only need to know which tokens end up on the page.

use alloc::vec::Vec;

use crate::geometry::Grid;
use crate::matrix::{Corner, ModuleRole, QrMatrix};
use crate::palette::Token;
use crate::style::Style;

pub(crate) const CORNERS: [Corner; 3] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft];

/// What a layer draws.
#[derive(Clone, Debug)]
pub(crate) enum Part {
    /// Modules in the style's module shape.
    Modules(Grid),
    /// Finder outer rings at these corners.
    FinderOuter(Vec<Corner>),
    /// Finder eyes at these corners.
    FinderEye(Vec<Corner>),
    /// Alignment patterns (ring and center) at these center modules. They
    /// stay solid whatever the module shape: scanners look for their
    /// 1:1:1:1:1 profile.
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

/// The layers in paint order: data, timing, alignment, finder rings, eyes.
/// Empty layers are left out.
pub(crate) fn layers(style: &Style, qr: &QrMatrix) -> Vec<Layer> {
    let size = qr.size() as usize;
    let mut data = Grid::new(size);
    let mut timing = Grid::new(size);
    let mut centers = Vec::new();
    for (x, y, dark, role) in qr.modules() {
        if let ModuleRole::Alignment { center } = role
            && center == (x, y)
        {
            centers.push(center);
        }
        if !dark {
            continue;
        }
        let (x, y) = (x as usize, y as usize);
        match role {
            ModuleRole::Finder { .. } | ModuleRole::Separator | ModuleRole::Alignment { .. } => {}
            ModuleRole::Timing => timing.set(x, y),
            _ => data.set(x, y),
        }
    }

    let mut layers = Vec::new();
    for (token, grid) in [(Token::Fg, data), (Token::Timing, timing)] {
        if !grid.is_empty() {
            layers.push(Layer {
                token,
                part: Part::Modules(grid),
            });
        }
    }
    if !centers.is_empty() {
        layers.push(Layer {
            token: Token::Alignment,
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
