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
use crate::style::{ColorSpread, Frame, FrameRing, ModuleShape, Style};
#[cfg(feature = "halftone")]
use crate::version::Version;

pub(crate) const CORNERS: [Corner; 3] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft];

/// Where the code sits (SPEC §4 step 1). Lengths are in modules.
#[derive(Clone, Debug)]
pub(crate) struct Canvas {
    /// The side of the square viewBox.
    pub(crate) side: i64,
    /// The side of the card: the frame's square, from the canvas's
    /// top-left corner. A shadow lies beyond it, so it equals `side`
    /// without one.
    pub(crate) card: i64,
    /// The code's top-left module, on both axes.
    pub(crate) origin: i64,
    /// The border's width in modules, 0 without one. It lies at the
    /// card's edge, inside the card.
    pub(crate) border: f64,
    /// The shadow's offset in modules, 0 without one.
    pub(crate) shadow: f64,
    /// The caption's band under the card, in whole modules, 0 without
    /// one. A square or rounded card grows to take it in.
    pub(crate) band: i64,
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

/// A caption's band, in multiples of its letters' size: room for the
/// letters and the space under them.
pub(crate) const CAPTION_BAND: f64 = 1.75;

impl Canvas {
    /// The viewBox's height: its side, and the caption's band below.
    pub(crate) fn height(&self) -> i64 {
        self.side + self.band
    }

    /// Where a caption `size` modules tall centers its line: in the light
    /// space between the quiet zone and the border (or the card's edge),
    /// a little high, as optical centering wants. Under a round card, in
    /// the band below it.
    pub(crate) fn caption_center(&self, size: f64) -> (f64, f64) {
        let margin = if self.round.is_some() {
            0.0
        } else {
            math::ceil(self.border)
        };
        let y = self.card as f64 - margin + self.band as f64 / 2.0 - size * 0.1;
        (self.card as f64 / 2.0, y)
    }

    /// The card's height: square, or taller by the caption's band when
    /// the frame grows to hold it.
    pub(crate) fn card_height(&self) -> i64 {
        if self.round.is_some() {
            self.card
        } else {
            self.card + self.band
        }
    }
}

/// Lays out the canvas for `style`'s frame. A border takes whole modules
/// of margin beyond the quiet zone (any spare is light, between the two),
/// so the code stays on whole modules; a shadow widens the canvas past
/// the card.
pub(crate) fn canvas(style: &Style, qr: &QrMatrix) -> Canvas {
    let n = qr.size() as i64;
    let q = style.quiet_zone as i64;
    let border = style.border.map_or(0.0, |b| b.width().max(0.0) as f64);
    let shadow = style.shadow.map_or(0.0, |s| s.max(0.0) as f64);
    let reach = math::ceil(shadow) as i64;
    let band = style.caption.as_ref().map_or(0, |c| {
        math::ceil(c.size.max(0.0) as f64 * CAPTION_BAND) as i64
    });
    let Frame::Circle {
        ring: ring_style, ..
    } = style.frame
    else {
        let margin = math::ceil(border) as i64;
        let card = n + 2 * (q + margin);
        return Canvas {
            side: card + reach,
            card,
            origin: q + margin,
            border,
            shadow,
            band,
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
    // The border lies at the disc's edge, beyond the margin.
    let radius = ring.map_or(filler_radius, |(_, outer)| outer) + ROUND_MARGIN + border;
    // Same parity as the code, so the code sits on whole modules.
    let mut card = math::ceil(2.0 * radius) as i64;
    if (card - n) % 2 != 0 {
        card += 1;
    }
    Canvas {
        side: card + reach,
        card,
        origin: (card - n) / 2,
        border,
        shadow,
        band,
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
    let (side, card) = (canvas.side, canvas.card);
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
            let far = |i: i64| (2 * i - card).abs().max((2 * i + 2 - card).abs());
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

/// Whether neighboring modules of `shape` touch, so a region must stay in
/// one element (and, spread by region, in one color).
pub(crate) fn touches(shape: &ModuleShape) -> bool {
    match *shape {
        ModuleShape::Square { gap } => crate::geometry::units(gap) == 0,
        ModuleShape::Dots { .. } => false,
        ModuleShape::Rounded { .. } | ModuleShape::Liquid { .. } | ModuleShape::Bars { .. } => true,
    }
}

/// The 4-connected regions of `grid` (or its single cells), row-major.
pub(crate) fn regions(grid: &Grid, size: usize, connected: bool) -> Vec<Vec<(u16, u16)>> {
    let mut seen = alloc::vec![false; size * size];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..size * size {
        let (sx, sy) = ((start % size) as i64, (start / size) as i64);
        if seen[start] || !grid.get(sx, sy) {
            continue;
        }
        seen[start] = true;
        if !connected {
            out.push(alloc::vec![(sx as u16, sy as u16)]);
            continue;
        }
        let mut cells = Vec::new();
        stack.push((sx, sy));
        while let Some((x, y)) = stack.pop() {
            cells.push((x as u16, y as u16));
            for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if grid.get(nx, ny) {
                    let i = ny as usize * size + nx as usize;
                    if !seen[i] {
                        seen[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }
        }
        crate::sort::sort_by_key(&mut cells, |&(x, y)| (y, x));
        out.push(cells);
    }
    out
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
    /// A picture drawn into sub-cells (SPEC §7.7 B): one layer per
    /// paint. Its path is in sub-cells of a third of a module, from the
    /// code's top-left corner ([`PlanLayer::cells`](crate::plan::PlanLayer::cells)).
    Halftone,
    /// The frame on the logo's plate ([`LogoFrame`](crate::LogoFrame)),
    /// drawn over the logo. Ring and Brackets are stroked
    /// ([`PlanLayer::outline`](crate::plan::PlanLayer::outline)).
    LogoFrame,
    /// The logo's silhouette in `fg` for
    /// [`LogoIntro::Ink`](crate::LogoIntro::Ink): the plate path, masked by
    /// the logo's alpha. It rests invisible
    /// ([`PlanLayer::opacity`](crate::plan::PlanLayer::opacity) 0).
    LogoInk,
    /// The highlight band of [`Idle::LogoGlint`](crate::Idle::LogoGlint): a
    /// white slanted band, 45% opacity at most, clipped to the plate and
    /// parked left of it.
    LogoGlint,
    /// The logo's content on its own, for the idles that move it
    /// ([`Idle::LogoSpin`](crate::Idle::LogoSpin),
    /// [`Idle::LogoFlip`](crate::Idle::LogoFlip)). It is drawn inside the
    /// [`LayerRole::Logo`] layer, clipped to the window its path gives: it
    /// takes that layer's motion as well as its own, and the window moves
    /// with the plate, not with the content.
    LogoArt,
    /// The caption ([`Style::caption`](crate::Style::caption)): its path is
    /// empty; plans carry the text in `AnimationPlan::caption`.
    Caption,
    /// Paper grain over the background ([`Texture::Grain`](crate::Texture::Grain)):
    /// its path is the card, filtered into seeded speckles.
    Grain,
    /// Scanlines over the whole card ([`Texture::Scanlines`](crate::Texture::Scanlines)),
    /// painted last: its path is the card, filled with lines in `bg`.
    Scanlines,
    /// A frame's hard shadow ([`Style::shadow`](crate::Style::shadow)),
    /// painted first.
    Shadow,
    /// The border at the frame's edge ([`Style::border`](crate::Style::border)):
    /// one or two rings, drawn with the even-odd rule.
    Border,
}

impl LayerRole {
    /// The frame's own layers (shadow, background, grain, border, and the
    /// scanlines over everything), which hold still while the code
    /// animates.
    #[cfg_attr(not(feature = "animate"), allow(dead_code))]
    pub(crate) fn is_chrome(self) -> bool {
        matches!(
            self,
            LayerRole::Shadow
                | LayerRole::Background
                | LayerRole::Grain
                | LayerRole::Border
                | LayerRole::Caption
                | LayerRole::Scanlines
        )
    }

    /// The logo and the layers that belong to it.
    #[cfg_attr(not(feature = "animate"), allow(dead_code))]
    pub(crate) fn is_logo(self) -> bool {
        matches!(
            self,
            LayerRole::Logo
                | LayerRole::LogoArt
                | LayerRole::LogoFrame
                | LayerRole::LogoInk
                | LayerRole::LogoGlint
        )
    }
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
    /// The data, ECC and remainder modules drawn into the style's picture.
    /// Its sub-cells are worked out when it's drawn.
    #[cfg_attr(not(feature = "halftone"), allow(dead_code))]
    Halftone,
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
    /// The share of its paint the layer keeps, 0–1: below 1 for uneven
    /// ink (`Texture::Ink`), as `fill-opacity`.
    pub(crate) fade: f32,
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
            Part::Halftone => LayerRole::Halftone,
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
        let cover = crate::logo::cover(logo, qr, crate::check::logo_scale(style, qr));
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
        let cover = crate::logo::cover(logo, qr, crate::check::logo_scale(style, qr));
        centers.retain(|c| !cover.alignment.contains(c));
    }
    centers
}

/// The data colors in use: `fg`, then each of `fg.2` to `fg.4` that is
/// set.
pub(crate) fn data_tokens(style: &Style) -> Vec<Token> {
    let mut tokens = alloc::vec![Token::Fg];
    tokens.extend(
        [Token::Fg2, Token::Fg3, Token::Fg4]
            .into_iter()
            .filter(|&t| style.palette.get(t).is_some()),
    );
    tokens
}

/// Splits the data modules across the data colors (SPEC §6.1), one module
/// or one touching region at a time, in row-major order from a generator
/// of its own (so filler picks stay as they were). Empty colors are left
/// out.
fn spread(style: &Style, qr: &QrMatrix, data: Grid) -> Vec<(Token, Grid)> {
    let tokens = data_tokens(style);
    if tokens.len() == 1 {
        return if data.is_empty() {
            Vec::new()
        } else {
            alloc::vec![(Token::Fg, data)]
        };
    }
    let size = qr.size() as usize;
    let mut grids: Vec<Grid> = tokens.iter().map(|_| Grid::new(size)).collect();
    let connected = style.spread == ColorSpread::Regions && touches(&style.modules);
    let mut rng = SplitMix64::new(seed(style, qr) ^ SPREAD_SALT);
    for region in regions(&data, size, connected) {
        let pick = (rng.next_f64() * tokens.len() as f64) as usize;
        let grid = &mut grids[pick.min(tokens.len() - 1)];
        for (x, y) in region {
            grid.set(x as usize, y as usize);
        }
    }
    tokens
        .into_iter()
        .zip(grids)
        .filter(|(_, g)| !g.is_empty())
        .collect()
}

/// Uneven ink (`Texture::Ink`): a color's modules (or touching regions,
/// spread by region) split across three seeded levels, keeping all of
/// their paint, `fade / 2` less, or `fade` less. Without it, the grid as
/// it is.
fn ink(style: &Style, rng: &mut SplitMix64, size: usize, grid: Grid) -> Vec<(f32, Grid)> {
    let Some(fade) = style.ink_fade() else {
        return alloc::vec![(1.0, grid)];
    };
    let levels = [1.0, 1.0 - fade / 2.0, 1.0 - fade];
    let mut grids: Vec<Grid> = levels.iter().map(|_| Grid::new(size)).collect();
    // Like the colors, by touching region when the spread is.
    let connected = style.spread == ColorSpread::Regions && touches(&style.modules);
    for region in regions(&grid, size, connected) {
        let pick = (rng.next_f64() * levels.len() as f64) as usize;
        for (x, y) in region {
            grids[pick.min(levels.len() - 1)].set(x as usize, y as usize);
        }
    }
    levels
        .into_iter()
        .zip(grids)
        .filter(|(_, g)| !g.is_empty())
        .collect()
}

/// Keeps the ink's picks apart from the others.
const INK_SALT: u64 = 0x0000_1a4c_f00d;

/// Keeps the data-color picks apart from the filler's.
const SPREAD_SALT: u64 = 0x005e_ed0f_c010;

/// The layers in paint order: frame ring, filler, data, timing,
/// alignment, finder rings, eyes. Empty layers are left out.
pub(crate) fn layers(style: &Style, qr: &QrMatrix) -> Vec<Layer> {
    let size = qr.size() as usize;
    let canvas = canvas(style, qr);
    let mut layers = Vec::new();
    if canvas.round.as_ref().is_some_and(|r| r.ring.is_some()) {
        layers.push(Layer {
            fade: 1.0,
            token: Token::Frame,
            part: Part::Ring,
        });
    }
    let filler = filler(style, qr, &canvas);
    if !filler.is_empty() {
        layers.push(Layer {
            fade: 1.0,
            token: Token::Filler,
            part: Part::Filler(filler),
        });
    }
    let cleared = removed(style, qr);
    #[cfg(feature = "halftone")]
    let pictured = style.halftone.is_some();
    #[cfg(not(feature = "halftone"))]
    let pictured = false;
    if pictured {
        layers.push(Layer {
            fade: 1.0,
            token: Token::Fg,
            part: Part::Halftone,
        });
    }
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
            // Format and version modules stay whole (SPEC §7.7 rule 2).
            ModuleRole::Data | ModuleRole::Ecc | ModuleRole::Remainder if pictured => {}
            _ => data.set(x, y),
        }
    }

    let mut rng = SplitMix64::new(seed(style, qr) ^ INK_SALT);
    for (token, grid) in spread(style, qr, data) {
        for (fade, grid) in ink(style, &mut rng, qr.size() as usize, grid) {
            layers.push(Layer {
                fade,
                token,
                part: Part::Modules(grid),
            });
        }
    }
    if !timing.is_empty() {
        layers.push(Layer {
            fade: 1.0,
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
            fade: 1.0,
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
                    fade: 1.0,
                    token: per_corner(c),
                    part: part(alloc::vec![c]),
                });
            }
        } else {
            layers.push(Layer {
                fade: 1.0,
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
    fn data_colors_split_the_data_modules() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        let palette =
            crate::Palette::from_tokens([("fg.2", "#7a0000"), ("fg.4", "#1e3a8a")]).unwrap();
        let data = |l: &Layer| match &l.part {
            Part::Modules(g) if l.role() == LayerRole::Data => Some(g.clone()),
            _ => None,
        };
        let plain: Vec<Grid> = layers(&Style::new(), &qr).iter().filter_map(data).collect();
        for spread in [ColorSpread::Modules, ColorSpread::Regions] {
            let style = Style::new()
                .palette(palette.clone())
                .modules(ModuleShape::Liquid { smoothness: 1.0 })
                .color_spread(spread);
            let split: Vec<(Token, Grid)> = layers(&style, &qr)
                .iter()
                .filter_map(|l| data(l).map(|g| (l.token, g)))
                .collect();
            let tokens: Vec<Token> = split.iter().map(|(t, _)| *t).collect();
            assert_eq!(tokens, [Token::Fg, Token::Fg2, Token::Fg4]);
            // Every data module lands in exactly one color.
            let n = qr.size() as i64;
            for y in 0..n {
                for x in 0..n {
                    let count = split.iter().filter(|(_, g)| g.get(x, y)).count();
                    assert_eq!(count, usize::from(plain[0].get(x, y)), "({x}, {y})");
                }
            }
            if spread == ColorSpread::Regions {
                // Touching modules share a color.
                for (_, g) in &split {
                    for y in 0..n {
                        for x in 0..n {
                            if g.get(x, y) && plain[0].get(x + 1, y) {
                                assert!(g.get(x + 1, y));
                            }
                        }
                    }
                }
            }
            assert_eq!(layers(&style, &qr).len(), layers(&style, &qr).len());
        }
        // Spreading leaves the round frame's filler where it was.
        let round = round(0.5, FrameRing::None);
        let c = canvas(&round, &qr);
        let before = filler(&round, &qr, &c);
        let after = filler(&round.clone().palette(palette), &qr, &c);
        for y in 0..c.side {
            for x in 0..c.side {
                assert_eq!(before.get(x, y), after.get(x, y));
            }
        }
    }

    #[test]
    fn ink_fades_every_data_module_once() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        let style = Style::new().texture(crate::Texture::Ink { fade: 0.2 });
        let data: Vec<Layer> = layers(&style, &qr)
            .into_iter()
            .filter(|l| l.role() == LayerRole::Data)
            .collect();
        let fades: Vec<f32> = data.iter().map(|l| l.fade).collect();
        assert_eq!(fades, [1.0, 0.9, 0.8]);
        let plain = layers(&Style::new(), &qr);
        let Part::Modules(all) = &plain
            .iter()
            .find(|l| l.role() == LayerRole::Data)
            .unwrap()
            .part
        else {
            unreachable!()
        };
        let n = qr.size() as i64;
        for y in 0..n {
            for x in 0..n {
                let count = data
                    .iter()
                    .filter(|l| matches!(&l.part, Part::Modules(g) if g.get(x, y)))
                    .count();
                assert_eq!(count, usize::from(all.get(x, y)));
            }
        }
        // One texture of each kind: the second replaces the first.
        let twice = style.texture(crate::Texture::Ink { fade: 0.1 });
        assert_eq!(twice.textures(), [crate::Texture::Ink { fade: 0.1 }]);
    }

    #[test]
    fn captions_add_a_band_below_the_card() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap(); // 21 modules
        let style = Style::new().caption(crate::Caption::new("Scan me"));
        let c = canvas(&style, &qr);
        // Size 2 needs 3.5 modules: four, under a 29-module card.
        assert_eq!(
            (c.side, c.card, c.band, c.height(), c.card_height()),
            (29, 29, 4, 33, 33)
        );
        let (x, y) = c.caption_center(2.0);
        assert!((x - 14.5).abs() < 1e-9 && y > 29.0 && y < 33.0);
        // A round card keeps its shape; the caption hangs below it.
        let round = canvas(
            &round(0.5, FrameRing::None).caption(crate::Caption::new("x")),
            &qr,
        );
        assert_eq!(round.card_height(), round.card);
        assert_eq!(round.height(), round.side + 4);
    }

    #[test]
    fn borders_take_whole_modules_and_shadows_widen_the_canvas() {
        use crate::Border;
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap(); // 21 modules
        let style = Style::new()
            .border(Border::Solid { width: 0.5 })
            .shadow(1.5);
        let c = canvas(&style, &qr);
        // One module of margin for half a module of border.
        assert_eq!((c.card, c.origin, c.side), (31, 5, 33));
        let bordered = canvas(
            &round(0.5, FrameRing::None).border(Border::Solid { width: 1.0 }),
            &qr,
        );
        let plain = canvas(&round(0.5, FrameRing::None), &qr);
        assert!(
            bordered.round.as_ref().unwrap().radius >= plain.round.as_ref().unwrap().radius + 1.0
        );
        assert_eq!(bordered.side, bordered.card);
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
