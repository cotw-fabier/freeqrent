//! Animation (SPEC §8): intros as data first. A style's intro becomes an
//! [`AnimationPlan`] of layers and tracks; the SVG writer turns the tracks
//! into CSS keyframes, and `render_frame` evaluates them at a fixed time.
//!
//! The document without its animation is the finished code (§8.2). Every
//! keyframe list leaves out its end, so each track ends on the element's
//! own value, and all of it sits in the `@supports` block that renderers
//! without CSS animation skip.
//!
//! Layers are split into stagger groups of whole pieces: a piece is a
//! 4-connected region of one layer (or a single module for shapes whose
//! modules never touch). Every module shape depends only on 4-neighbors in
//! its own layer, so a group of whole pieces has exactly the outline those
//! pieces have in the static drawing, and no two touching edges end up in
//! different elements, which would show anti-aliasing seams.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::color::Color;
use crate::geometry::{Grid, Path};
use crate::math;
use crate::matrix::{Corner, QrMatrix};
use crate::num::write_scaled;
use crate::palette::{Palette, Token};
use crate::rng::SplitMix64;
use crate::scene::{self, Canvas, LayerRole, Part};
use crate::style::{ModuleShape, Style};
use crate::svg::{self, Element, Pose};

/// A cubic Bézier timing curve from (0, 0) to (1, 1), as in CSS
/// `cubic-bezier(x1, y1, x2, y2)`. `x1` and `x2` are 0–1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    /// First control point, x.
    pub x1: f32,
    /// First control point, y.
    pub y1: f32,
    /// Second control point, x.
    pub x2: f32,
    /// Second control point, y.
    pub y2: f32,
}

impl CubicBezier {
    /// No easing.
    pub const LINEAR: CubicBezier = CubicBezier::new(0.0, 0.0, 1.0, 1.0);

    /// A curve through the given control points.
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> CubicBezier {
        CubicBezier { x1, y1, x2, y2 }
    }

    /// The eased progress at time fraction `x` (0–1).
    pub fn at(self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let (x1, y1, x2, y2) = (
            self.x1 as f64,
            self.y1 as f64,
            self.x2 as f64,
            self.y2 as f64,
        );
        let bez = |a: f64, b: f64, t: f64| {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        };
        // x(t) is monotonic for x1, x2 in 0–1, so bisection always finds t.
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..48 {
            let mid = (lo + hi) / 2.0;
            if bez(x1, x2, mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        bez(y1, y2, (lo + hi) / 2.0)
    }
}

/// Timing curves (SPEC §8.1): exponential ease-outs only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    /// `cubic-bezier(0.25, 1, 0.5, 1)`, refined. The default.
    Quart,
    /// `cubic-bezier(0.22, 1, 0.36, 1)`, snappier.
    Quint,
    /// `cubic-bezier(0.16, 1, 0.3, 1)`, confident.
    Expo,
    /// Any curve; `x1` and `x2` must be 0–1.
    Custom(CubicBezier),
}

impl Ease {
    /// The curve.
    pub fn curve(self) -> CubicBezier {
        match self {
            Ease::Quart => CubicBezier::new(0.25, 1.0, 0.5, 1.0),
            Ease::Quint => CubicBezier::new(0.22, 1.0, 0.36, 1.0),
            Ease::Expo => CubicBezier::new(0.16, 1.0, 0.3, 1.0),
            Ease::Custom(c) => c,
        }
    }
}

/// The order stagger groups play in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaggerOrder {
    /// From the center outward.
    Radial,
    /// Top to bottom.
    Linear,
    /// From the top-left corner to the bottom-right.
    Diagonal,
    /// Shuffled by the given seed.
    Random(u64),
    /// Clockwise from twelve o'clock around the center.
    Polar,
}

/// What starts an intro.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Trigger {
    /// The document loading. The default.
    Load,
    /// The pointer over the code; the intro replays on each hover. Inline
    /// SVG only (an `<img>` gets no pointer events).
    Hover,
    /// An ancestor (or the inline `<svg>` itself) gaining this class, for
    /// scroll-into-view without script inside the SVG. Until the class is
    /// added the code waits in its intro state, so a page that never adds
    /// it shows nothing; reduced motion still shows the code.
    Class(String),
}

/// A preset intro (SPEC §8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Groups fade in from 92% scale toward their own centers, in a
    /// shuffled stagger. The default.
    Settle,
}

/// A code's entrance: a preset and its timing. Build one with
/// [`Intro::new`] or from a [`Preset`].
#[derive(Clone, Debug, PartialEq)]
pub struct Intro {
    pub(crate) preset: Preset,
    pub(crate) duration_ms: u32,
    pub(crate) delay_ms: u32,
    pub(crate) ease: Ease,
    pub(crate) stagger: Option<StaggerOrder>,
    pub(crate) groups: u8,
    pub(crate) trigger: Trigger,
    pub(crate) logo_reveal: bool,
}

impl From<Preset> for Intro {
    fn from(preset: Preset) -> Intro {
        Intro::new(preset)
    }
}

impl Intro {
    /// The preset with its defaults: 1200 ms in all, 12 groups, ease-out
    /// quart, on load, with the logo revealed last.
    pub fn new(preset: Preset) -> Intro {
        Intro {
            preset,
            duration_ms: 1200,
            delay_ms: 0,
            ease: Ease::Quart,
            stagger: None,
            groups: 12,
            trigger: Trigger::Load,
            logo_reveal: true,
        }
    }

    /// The whole intro's length; the stagger is compressed to fit. Over
    /// 2000 ms raises a warning, since the code should scan soon after it
    /// appears. 100–10000.
    pub fn duration_ms(mut self, ms: u32) -> Intro {
        self.duration_ms = ms;
        self
    }

    /// Time before the intro starts, 0–10000 ms.
    pub fn delay_ms(mut self, ms: u32) -> Intro {
        self.delay_ms = ms;
        self
    }

    /// The timing curve.
    pub fn ease(mut self, ease: Ease) -> Intro {
        self.ease = ease;
        self
    }

    /// The stagger order, instead of the preset's own.
    pub fn stagger(mut self, order: StaggerOrder) -> Intro {
        self.stagger = Some(order);
        self
    }

    /// How many stagger groups, 2–16 (default 12). Each group is one
    /// element per layer.
    pub fn groups(mut self, groups: u8) -> Intro {
        self.groups = groups;
        self
    }

    /// What starts the intro.
    pub fn trigger(mut self, trigger: Trigger) -> Intro {
        self.trigger = trigger;
        self
    }

    /// Whether the logo scales in after the modules (default on).
    pub fn logo_reveal(mut self, reveal: bool) -> Intro {
        self.logo_reveal = reveal;
        self
    }

    /// The preset.
    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// The whole length, in milliseconds.
    pub fn total_ms(&self) -> u32 {
        self.duration_ms
    }

    /// The number of stagger groups.
    pub fn group_count(&self) -> u8 {
        self.groups
    }

    fn order(&self) -> StaggerOrder {
        self.stagger.unwrap_or(match self.preset {
            Preset::Settle => StaggerOrder::Random(0),
        })
    }
}

/// What a keyframe sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Property {
    /// Opacity, 0–1.
    Opacity,
    /// Uniform scale about the layer's origin.
    Scale,
    /// Horizontal offset, in modules.
    TranslateX,
    /// Vertical offset, in modules.
    TranslateY,
    /// The fill color.
    Fill,
}

impl Property {
    fn is_transform(self) -> bool {
        matches!(
            self,
            Property::Scale | Property::TranslateX | Property::TranslateY
        )
    }

    fn name(self) -> &'static str {
        match self {
            Property::Opacity => "opacity",
            Property::Scale => "scale",
            Property::TranslateX => "translateX",
            Property::TranslateY => "translateY",
            Property::Fill => "fill",
        }
    }
}

/// A keyframe value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// A number.
    Number(f32),
    /// A color.
    Color(Color),
}

/// One property's value at one point of a track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    /// Where in the track, 0–1. Offsets 0 and 1 left out of a track take
    /// the layer's natural value.
    pub offset: f32,
    /// The property.
    pub property: Property,
    /// Its value.
    pub value: Value,
}

/// One animation of one layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// The layer it moves, an index into [`AnimationPlan::layers`].
    pub layer: usize,
    /// Start, from the trigger.
    pub delay_ms: u32,
    /// Length of one iteration.
    pub duration_ms: u32,
    /// Applied between each pair of neighboring keyframes, as CSS does.
    pub ease: CubicBezier,
    /// Iterations, or `None` for an endless loop. A finite track shows
    /// its first keyframe while it waits (CSS `backwards`); an endless one
    /// doesn't touch the layer until it starts.
    pub iterations: Option<u32>,
    /// The keyframes. Every track ends on the layer's natural value.
    pub keys: Vec<Keyframe>,
}

/// What a player shows when motion is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Fallback {
    /// The finished code (always, SPEC §8.2).
    FinalState,
}

/// One drawn layer of a plan.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanLayer {
    /// What it is.
    pub role: LayerRole,
    /// Its paint, looked up in [`AnimationPlan::palette`].
    pub token: Token,
    /// SVG path data in viewBox units (modules). For the logo, its plate;
    /// the logo's content is drawn from the style.
    pub path: String,
    /// Whether the path uses the even-odd fill rule.
    pub evenodd: bool,
    /// The stagger group, for split layers.
    pub group: Option<u8>,
    /// The finder corner, for a finder layer.
    pub corner: Option<Corner>,
    /// Scale origin, in viewBox units.
    pub origin: (f32, f32),
}

/// A style's animation as data (SPEC §8.5): the static layers and the
/// tracks that move them. The SVG writer serializes it to CSS; other
/// players (the Flutter widget in Phase 2) read the same plan.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AnimationPlan {
    /// Width and height of the viewBox, in modules.
    pub view_box: (f32, f32),
    /// The paints the layers' tokens resolve through.
    pub palette: Palette,
    /// The layers, in paint order.
    pub layers: Vec<PlanLayer>,
    /// The tracks.
    pub tracks: Vec<Track>,
    /// When the last finite track ends, from the trigger.
    pub total_ms: u32,
    /// What starts the tracks.
    pub trigger: Trigger,
    /// What to show with motion off.
    pub reduced_motion: Fallback,
}

/// The animation part of a document: its tracks, by element index.
#[derive(Clone, Debug)]
pub(crate) struct Motion {
    pub(crate) tracks: Vec<Track>,
    pub(crate) origins: Vec<(f64, f64)>,
    pub(crate) total_ms: u32,
    pub(crate) trigger: Trigger,
}

// ------------------------------------------------------------- grouping

/// A unit of a layer that is never split: a region, a module, a finder,
/// an alignment pattern or the ring.
#[derive(Clone, Debug)]
enum Unit {
    /// Cells of a grid layer.
    Cells(Vec<(u16, u16)>),
    Corner(Corner),
    Center((u16, u16)),
    Ring,
}

#[derive(Clone, Debug)]
struct Piece {
    layer: usize,
    unit: Unit,
    /// Bounding box in canvas modules: (x0, y0, x1, y1).
    bounds: (f64, f64, f64, f64),
    group: u8,
}

/// Whether neighboring modules of `shape` touch, so a region must stay in
/// one element.
fn touches(shape: &ModuleShape) -> bool {
    match *shape {
        ModuleShape::Square { gap } => crate::geometry::units(gap) == 0,
        ModuleShape::Dots { .. } => false,
        ModuleShape::Rounded { .. } | ModuleShape::Liquid { .. } | ModuleShape::Bars { .. } => true,
    }
}

/// The 4-connected regions of `grid` (or its single cells), row-major.
fn regions(grid: &Grid, size: usize, connected: bool) -> Vec<Vec<(u16, u16)>> {
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
        cells.sort_unstable_by_key(|&(x, y)| (y, x));
        out.push(cells);
    }
    out
}

/// Every piece of the style's layers, grouped by `intro`'s order.
fn pieces(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    intro: &Intro,
    layers: &[scene::Layer],
) -> Vec<Piece> {
    let o = canvas.origin as f64;
    let n = qr.size();
    let mut out = Vec::new();
    for (li, layer) in layers.iter().enumerate() {
        let mut push = |unit: Unit, bounds| {
            out.push(Piece {
                layer: li,
                unit,
                bounds,
                group: 0,
            })
        };
        let cells = |grid: &Grid, size: usize, offset: f64, connected: bool| {
            regions(grid, size, connected)
                .into_iter()
                .map(move |cells| {
                    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                    for &(x, y) in &cells {
                        let (x, y) = (x as f64 + offset, y as f64 + offset);
                        b = (b.0.min(x), b.1.min(y), b.2.max(x + 1.0), b.3.max(y + 1.0));
                    }
                    (cells, b)
                })
        };
        match &layer.part {
            Part::Modules(grid) => {
                for (c, b) in cells(grid, n as usize, o, touches(&style.modules)) {
                    push(Unit::Cells(c), b);
                }
            }
            Part::Filler(grid) => {
                for (c, b) in cells(grid, canvas.side as usize, 0.0, touches(&style.modules)) {
                    push(Unit::Cells(c), b);
                }
            }
            Part::Timing(grid) => {
                for (c, b) in cells(grid, n as usize, o, false) {
                    push(Unit::Cells(c), b);
                }
            }
            Part::FinderOuter(corners) | Part::FinderEye(corners) => {
                for &c in corners {
                    let (x, y) = scene::finder_origin(c, n);
                    let (x, y) = (o + x as f64, o + y as f64);
                    push(Unit::Corner(c), (x, y, x + 7.0, y + 7.0));
                }
            }
            Part::Alignment(centers) => {
                for &(cx, cy) in centers {
                    let (x, y) = (o + cx as f64 - 2.0, o + cy as f64 - 2.0);
                    push(Unit::Center((cx, cy)), (x, y, x + 5.0, y + 5.0));
                }
            }
            Part::Ring => {
                let s = canvas.side as f64;
                push(Unit::Ring, (0.0, 0.0, s, s));
            }
        }
    }

    // Stagger keys from each piece's center, binned into equal ranges.
    let mid = canvas.side as f64 / 2.0;
    let seed = scene::seed(style, qr);
    let order = intro.order();
    let key = |p: &Piece| {
        let (x0, y0, x1, y1) = p.bounds;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        match order {
            StaggerOrder::Radial if matches!(p.unit, Unit::Ring) => mid * core::f64::consts::SQRT_2,
            StaggerOrder::Radial => math::sqrt((cx - mid) * (cx - mid) + (cy - mid) * (cy - mid)),
            StaggerOrder::Linear => cy,
            StaggerOrder::Diagonal => cx + cy,
            StaggerOrder::Polar => {
                // Clockwise from twelve o'clock, 0 to 2π.
                let a = math::atan2(cx - mid, mid - cy);
                if a < 0.0 {
                    a + 2.0 * core::f64::consts::PI
                } else {
                    a
                }
            }
            StaggerOrder::Random(s) => {
                // From the center, so a finder's ring and eye share a group.
                let h = ((cx * 2.0) as u64) << 32 | (cy * 2.0) as u64;
                SplitMix64::new(seed ^ s ^ h.wrapping_mul(0x9e37_79b9_7f4a_7c15)).next_f64()
            }
        }
    };
    let keys: Vec<f64> = out.iter().map(key).collect();
    let (lo, hi) = match order {
        StaggerOrder::Random(_) => (0.0, 1.0),
        _ => keys
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &k| (lo.min(k), hi.max(k))),
    };
    let k = intro.groups.clamp(1, 16) as f64;
    for (p, key) in out.iter_mut().zip(keys) {
        let t = if hi > lo { (key - lo) / (hi - lo) } else { 0.0 };
        p.group = (math::floor(t * k) as i64).clamp(0, k as i64 - 1) as u8;
    }
    out
}

/// How many elements `style`'s intro animates, without drawing them.
pub(crate) fn element_count(style: &Style, qr: &QrMatrix) -> usize {
    let Some(intro) = &style.intro else {
        return 0;
    };
    let canvas = scene::canvas(style, qr);
    let layers = scene::layers(style, qr);
    let mut seen: Vec<(usize, u8, Option<Corner>)> = pieces(style, qr, &canvas, intro, &layers)
        .iter()
        .map(|p| (p.layer, p.group, corner_of(&p.unit)))
        .collect();
    seen.sort_unstable_by_key(|&(l, g, c)| (l, g, c.map(|c| c as u8)));
    seen.dedup();
    seen.len() + style.logo.is_some() as usize
}

fn corner_of(unit: &Unit) -> Option<Corner> {
    match unit {
        Unit::Corner(c) => Some(*c),
        _ => None,
    }
}

/// The split elements of an animated style: background, each layer's
/// groups in order (finders one corner each), then the logo.
fn split(style: &Style, qr: &QrMatrix, canvas: &Canvas, intro: &Intro) -> Vec<Element> {
    let layers = scene::layers(style, qr);
    let pieces = pieces(style, qr, canvas, intro, &layers);
    let mut elements = Vec::new();
    let mid = canvas.side as f64 / 2.0;
    if svg::draws_background(style) {
        let mut bg = Element::new(
            LayerRole::Background,
            Token::Bg,
            false,
            svg::background_path(style, canvas),
        );
        bg.origin = (mid, mid);
        elements.push(bg);
    }
    let n = qr.size() as usize;
    for (li, layer) in layers.iter().enumerate() {
        let mine: Vec<&Piece> = pieces.iter().filter(|p| p.layer == li).collect();
        // Finders stay one element per corner, so each scales about its
        // own center.
        let mut keys: Vec<(u8, Option<Corner>)> =
            mine.iter().map(|p| (p.group, corner_of(&p.unit))).collect();
        keys.sort_unstable_by_key(|&(g, c)| (g, c.map(|c| c as u8)));
        keys.dedup();
        for (group, corner) in keys {
            let chosen: Vec<&&Piece> = mine
                .iter()
                .filter(|p| p.group == group && corner_of(&p.unit) == corner)
                .collect();
            let part = match &layer.part {
                Part::Modules(_) | Part::Filler(_) | Part::Timing(_) => {
                    let size = if matches!(layer.part, Part::Filler(_)) {
                        canvas.side as usize
                    } else {
                        n
                    };
                    let mut grid = Grid::new(size);
                    for p in &chosen {
                        if let Unit::Cells(cells) = &p.unit {
                            for &(x, y) in cells {
                                grid.set(x as usize, y as usize);
                            }
                        }
                    }
                    match &layer.part {
                        Part::Modules(_) => Part::Modules(grid),
                        Part::Filler(_) => Part::Filler(grid),
                        _ => Part::Timing(grid),
                    }
                }
                Part::FinderOuter(_) => Part::FinderOuter(corner.into_iter().collect()),
                Part::FinderEye(_) => Part::FinderEye(corner.into_iter().collect()),
                Part::Alignment(_) => Part::Alignment(
                    chosen
                        .iter()
                        .filter_map(|p| match p.unit {
                            Unit::Center(c) => Some(c),
                            _ => None,
                        })
                        .collect(),
                ),
                Part::Ring => Part::Ring,
            };
            let mut path = Path::new();
            svg::draw_part(style, qr, canvas, &part, &mut path);
            if path.is_empty() {
                continue;
            }
            let b = chosen
                .iter()
                .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| {
                    (
                        b.0.min(p.bounds.0),
                        b.1.min(p.bounds.1),
                        b.2.max(p.bounds.2),
                        b.3.max(p.bounds.3),
                    )
                });
            let mut e = Element::new(layer.role(), layer.token, layer.part.evenodd(), path);
            e.group = Some(group);
            e.corner = corner;
            e.origin = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
            elements.push(e);
        }
    }
    if let Some((plate, center)) = svg::logo_plate(style, qr, canvas) {
        let mut logo = Element::new(LayerRole::Logo, Token::LogoBackdrop, false, plate);
        logo.origin = (center, center);
        elements.push(logo);
    }
    elements
}

// --------------------------------------------------------------- timing

/// Longest time one element animates.
const MAX_ELEMENT_MS: f64 = 800.0;
/// The logo's reveal, at most.
const LOGO_MS: f64 = 400.0;

fn number(property: Property, offset: f32, value: f32) -> Keyframe {
    Keyframe {
        offset,
        property,
        value: Value::Number(value),
    }
}

/// The tracks of `intro` for `elements`.
fn tracks(style: &Style, intro: &Intro, elements: &[Element]) -> Vec<Track> {
    let total = intro.duration_ms as f64;
    let reveal = intro.logo_reveal && style.logo.is_some();
    let logo_ms = if reveal {
        LOGO_MS.min(total * 0.4)
    } else {
        0.0
    };
    // The modules finish halfway through the logo's reveal.
    let window = total - logo_ms / 2.0;
    let k = intro.groups.clamp(1, 16) as f64;
    let each = (window / 2.0).min(MAX_ELEMENT_MS);
    let step = if k > 1.0 {
        (window - each) / (k - 1.0)
    } else {
        0.0
    };
    let ease = intro.ease.curve();
    let delay = intro.delay_ms as f64;
    let ms = |v: f64| math::round(v) as u32;

    let mut tracks = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        match e.role {
            LayerRole::Background => {}
            LayerRole::Logo if reveal => tracks.push(Track {
                layer: i,
                delay_ms: ms(delay + total - logo_ms),
                duration_ms: ms(logo_ms),
                ease,
                iterations: Some(1),
                keys: alloc::vec![
                    number(Property::Opacity, 0.0, 0.0),
                    number(Property::Scale, 0.0, 0.94),
                ],
            }),
            LayerRole::Logo => {}
            _ => {
                let g = e.group.unwrap_or(0) as f64;
                let keys = match intro.preset {
                    Preset::Settle => alloc::vec![
                        number(Property::Opacity, 0.0, 0.0),
                        number(Property::Scale, 0.0, 0.92),
                    ],
                };
                tracks.push(Track {
                    layer: i,
                    delay_ms: ms(delay + g * step),
                    duration_ms: ms(each),
                    ease,
                    iterations: Some(1),
                    keys,
                });
            }
        }
    }
    tracks
}

/// The animated layout of `style`, or `None` when it doesn't animate. With
/// `at`, each element gets its pose at that time instead of a class.
pub(crate) fn build(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    at: Option<u32>,
) -> Option<(Vec<Element>, Motion)> {
    let intro = style.intro.as_ref()?;
    let mut elements = split(style, qr, canvas, intro);
    let tracks = tracks(style, intro, &elements);
    let total_ms = tracks
        .iter()
        .filter_map(|t| Some(t.delay_ms + t.duration_ms * t.iterations?))
        .max()
        .unwrap_or(0);
    let motion = Motion {
        origins: elements.iter().map(|e| e.origin).collect(),
        tracks,
        total_ms,
        trigger: intro.trigger.clone(),
    };
    for (i, e) in elements.iter_mut().enumerate() {
        let mine = motion.tracks.iter().filter(|t| t.layer == i);
        match at {
            Some(t) => {
                let pose = pose(mine, t);
                e.pose = (pose != Pose::NATURAL).then_some(pose);
            }
            None => {
                if mine.count() > 0 {
                    e.anim = Some(i);
                }
            }
        }
    }
    Some((elements, motion))
}

// ----------------------------------------------------------- evaluation

/// The value of `property` on `track` at `t` ms, if the track touches it.
fn sample(track: &Track, property: Property, natural: f64, t: u32) -> Option<f64> {
    let dur = track.duration_ms.max(1) as f64;
    let local = t as f64 - track.delay_ms as f64;
    let p = match track.iterations {
        Some(n) => {
            if local >= dur * n as f64 {
                return None;
            }
            if local < 0.0 {
                0.0
            } else {
                (local % dur) / dur
            }
        }
        None => {
            if local < 0.0 {
                return None;
            }
            (local % dur) / dur
        }
    };
    let mut keys: Vec<(f64, f64)> = track
        .keys
        .iter()
        .filter(|k| k.property == property)
        .filter_map(|k| match k.value {
            Value::Number(v) => Some((k.offset as f64, v as f64)),
            Value::Color(_) => None,
        })
        .collect();
    if keys.is_empty() {
        return None;
    }
    if keys[0].0 > 0.0 {
        keys.insert(0, (0.0, natural));
    }
    if keys[keys.len() - 1].0 < 1.0 {
        keys.push((1.0, natural));
    }
    let i = keys
        .windows(2)
        .position(|w| p <= w[1].0)
        .unwrap_or(keys.len() - 2);
    let ((o0, v0), (o1, v1)) = (keys[i], keys[i + 1]);
    let local = if o1 > o0 { (p - o0) / (o1 - o0) } else { 1.0 };
    let e = track.ease.at(local);
    Some(v0 + (v1 - v0) * e)
}

/// An element's pose at `t` from its tracks; later tracks win, as in a
/// CSS animation list.
fn pose<'a>(tracks: impl Iterator<Item = &'a Track>, t: u32) -> Pose {
    let mut pose = Pose::NATURAL;
    for track in tracks {
        if let Some(v) = sample(track, Property::Opacity, 1.0, t) {
            pose.opacity = v;
        }
        if let Some(v) = sample(track, Property::Scale, 1.0, t) {
            pose.scale = v;
        }
        if let Some(v) = sample(track, Property::TranslateX, 0.0, t) {
            pose.translate.0 = v;
        }
        if let Some(v) = sample(track, Property::TranslateY, 0.0, t) {
            pose.translate.1 = v;
        }
    }
    pose
}

// ------------------------------------------------------------------ CSS

fn write_number<W: Write + ?Sized>(w: &mut W, v: f64) -> fmt::Result {
    write_scaled(w, math::round(v * 1000.0) as i64, 3)
}

/// A CSS identifier with everything outside `[A-Za-z0-9_-]` escaped (and
/// a leading digit), so a class name can't leave its selector.
fn write_ident<W: Write + ?Sized>(w: &mut W, name: &str) -> fmt::Result {
    for (i, c) in name.chars().enumerate() {
        let plain =
            c.is_ascii_alphabetic() || c == '_' || c == '-' || (c.is_ascii_digit() && i > 0);
        if plain {
            w.write_char(c)?;
        } else {
            write!(w, "\\{:x} ", c as u32)?;
        }
    }
    Ok(())
}

fn write_curve<W: Write + ?Sized>(w: &mut W, c: CubicBezier) -> fmt::Result {
    w.write_str("cubic-bezier(")?;
    for (i, v) in [c.x1, c.y1, c.x2, c.y2].into_iter().enumerate() {
        if i > 0 {
            w.write_char(',')?;
        }
        write_number(w, v as f64)?;
    }
    w.write_char(')')
}

/// One keyframe list as CSS rules, grouped by offset.
fn write_keyframes<W: Write + ?Sized>(w: &mut W, keys: &[Keyframe]) -> fmt::Result {
    let mut offsets: Vec<f32> = keys.iter().map(|k| k.offset).collect();
    offsets.sort_unstable_by(|a, b| a.total_cmp(b));
    offsets.dedup();
    for offset in offsets {
        write_number(w, offset as f64 * 100.0)?;
        w.write_str("%{")?;
        let at: Vec<&Keyframe> = keys.iter().filter(|k| k.offset == offset).collect();
        let num = |p: Property, natural: f64| {
            at.iter()
                .find(|k| k.property == p)
                .and_then(|k| match k.value {
                    Value::Number(v) => Some(v as f64),
                    Value::Color(_) => None,
                })
                .unwrap_or(natural)
        };
        let mut first = true;
        let mut sep = |w: &mut W| -> fmt::Result {
            if !first {
                w.write_char(';')?;
            }
            first = false;
            Ok(())
        };
        for k in &at {
            match (k.property, k.value) {
                (Property::Opacity, Value::Number(v)) => {
                    sep(w)?;
                    w.write_str("opacity:")?;
                    write_number(w, v as f64)?;
                }
                (Property::Fill, Value::Color(c)) => {
                    sep(w)?;
                    w.write_str("fill:")?;
                    c.opaque_srgb().write_opaque(w)?;
                }
                _ => {}
            }
        }
        if at.iter().any(|k| k.property.is_transform()) {
            sep(w)?;
            w.write_str("transform:")?;
            let (tx, ty, s) = (
                num(Property::TranslateX, 0.0),
                num(Property::TranslateY, 0.0),
                num(Property::Scale, 1.0),
            );
            let mut parts = 0;
            if tx != 0.0 || ty != 0.0 {
                w.write_str("translate(")?;
                write_number(w, tx)?;
                w.write_str("px,")?;
                write_number(w, ty)?;
                w.write_str("px)")?;
                parts += 1;
            }
            if s != 1.0 || parts == 0 {
                if parts > 0 {
                    w.write_char(' ')?;
                }
                w.write_str("scale(")?;
                write_number(w, s)?;
                w.write_char(')')?;
            }
        }
        w.write_char('}')?;
    }
    Ok(())
}

/// What a shared animation rule says about one track: keyframes name,
/// duration, curve (as bits) and iterations.
type Signature = (usize, u32, [u32; 4], Option<u32>);

impl Motion {
    /// The keyframes, one rule per animated element (`{prefix}-e{n}`), the
    /// trigger, and the reduced-motion override (SPEC §8.1).
    pub(crate) fn write_css<W: Write + ?Sized>(&self, w: &mut W, prefix: &str) -> fmt::Result {
        // Identical keyframe lists are written once.
        let mut lists: Vec<&[Keyframe]> = Vec::new();
        let name_of: Vec<usize> = self
            .tracks
            .iter()
            .map(
                |t| match lists.iter().position(|l| *l == t.keys.as_slice()) {
                    Some(i) => i,
                    None => {
                        lists.push(&t.keys);
                        lists.len() - 1
                    }
                },
            )
            .collect();
        for (i, keys) in lists.iter().enumerate() {
            write!(w, "@keyframes {prefix}-k{i}{{")?;
            write_keyframes(w, keys)?;
            w.write_char('}')?;
        }
        let mut layers: Vec<usize> = self.tracks.iter().map(|t| t.layer).collect();
        layers.dedup();
        // Elements whose tracks differ only in delay share one rule; each
        // element then sets its delays and origin.
        let signature = |layer: usize| -> Vec<Signature> {
            self.tracks
                .iter()
                .zip(&name_of)
                .filter(|(t, _)| t.layer == layer)
                .map(|(t, &name)| {
                    let e = t.ease;
                    (
                        name,
                        t.duration_ms,
                        [e.x1, e.y1, e.x2, e.y2].map(f32::to_bits),
                        t.iterations,
                    )
                })
                .collect()
        };
        let mut shared: Vec<(Vec<Signature>, Vec<usize>)> = Vec::new();
        for &layer in &layers {
            let sig = signature(layer);
            match shared.iter_mut().find(|(s, _)| *s == sig) {
                Some((_, members)) => members.push(layer),
                None => shared.push((sig, alloc::vec![layer])),
            }
        }
        for (sig, members) in &shared {
            for (i, &layer) in members.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                if let Trigger::Hover = self.trigger {
                    write!(w, ".{prefix}:hover ")?;
                }
                write!(w, ".{prefix}-e{layer}")?;
            }
            w.write_str("{animation:")?;
            for (j, &(name, duration, ease, iterations)) in sig.iter().enumerate() {
                if j > 0 {
                    w.write_char(',')?;
                }
                write!(w, "{prefix}-k{name} {duration}ms ")?;
                let [x1, y1, x2, y2] = ease.map(f32::from_bits);
                write_curve(w, CubicBezier::new(x1, y1, x2, y2))?;
                match iterations {
                    Some(1) => w.write_str(" backwards")?,
                    Some(n) => write!(w, " {n} backwards")?,
                    None => w.write_str(" infinite")?,
                }
            }
            if let Trigger::Class(_) = self.trigger {
                w.write_str(";animation-play-state:paused")?;
            }
            w.write_str(";transform-box:view-box}")?;
        }
        for &layer in &layers {
            write!(w, ".{prefix}-e{layer}{{animation-delay:")?;
            let mine = self.tracks.iter().filter(|t| t.layer == layer);
            for (j, t) in mine.enumerate() {
                if j > 0 {
                    w.write_char(',')?;
                }
                write!(w, "{}ms", t.delay_ms)?;
            }
            let (ox, oy) = self.origins[layer];
            w.write_str(";transform-origin:")?;
            write_number(w, ox)?;
            w.write_str("px ")?;
            write_number(w, oy)?;
            w.write_str("px}")?;
        }
        if let Trigger::Class(class) = &self.trigger {
            for (i, &layer) in layers.iter().enumerate() {
                w.write_str(if i == 0 { "." } else { ",." })?;
                write_ident(w, class)?;
                write!(w, " .{prefix}-e{layer}")?;
            }
            w.write_str("{animation-play-state:running}")?;
        }
        write!(
            w,
            "@media (prefers-reduced-motion:reduce){{.{prefix} *{{animation:none!important;transition:none!important}}}}"
        )
    }
}

// ----------------------------------------------------------------- plan

/// The plan of `style` for `qr` (SPEC §8.5). A style without an intro
/// gives its static layers and no tracks.
pub(crate) fn plan(style: &Style, qr: &QrMatrix) -> AnimationPlan {
    let canvas = scene::canvas(style, qr);
    let (elements, motion) = match build(style, qr, &canvas, None) {
        Some((e, m)) => (e, Some(m)),
        None => (svg::static_elements(style, qr, &canvas), None),
    };
    let side = canvas.side as f32;
    let layers = elements
        .into_iter()
        .map(|e| PlanLayer {
            role: e.role,
            token: e.token,
            path: String::from(e.path.data()),
            evenodd: e.evenodd,
            group: e.group,
            corner: e.corner,
            origin: (e.origin.0 as f32, e.origin.1 as f32),
        })
        .collect();
    let (tracks, total_ms, trigger) = match motion {
        Some(m) => (m.tracks, m.total_ms, m.trigger),
        None => (Vec::new(), 0, Trigger::Load),
    };
    AnimationPlan {
        view_box: (side, side),
        palette: style.palette.clone(),
        layers,
        tracks,
        total_ms,
        trigger,
        reduced_motion: Fallback::FinalState,
    }
}

// ----------------------------------------------------------------- JSON

/// A JSON string literal.
fn json_str<W: Write + ?Sized>(w: &mut W, s: &str) -> fmt::Result {
    w.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => w.write_char(c)?,
        }
    }
    w.write_char('"')
}

fn json_num<W: Write + ?Sized>(w: &mut W, v: f32) -> fmt::Result {
    if v.is_finite() {
        write_number(w, v as f64)
    } else {
        w.write_char('0')
    }
}

fn role_name(role: LayerRole) -> &'static str {
    match role {
        LayerRole::Background => "background",
        LayerRole::Ring => "ring",
        LayerRole::Filler => "filler",
        LayerRole::Data => "data",
        LayerRole::Timing => "timing",
        LayerRole::Alignment => "alignment",
        LayerRole::FinderOuter => "finder-outer",
        LayerRole::FinderEye => "finder-eye",
        LayerRole::Logo => "logo",
    }
}

fn corner_name(corner: Corner) -> &'static str {
    match corner {
        Corner::TopLeft => "top-left",
        Corner::TopRight => "top-right",
        Corner::BottomLeft => "bottom-left",
    }
}

impl AnimationPlan {
    /// The plan as compact JSON, with the same bytes for the same plan.
    /// Numbers have at most three decimals; paints are CSS strings keyed
    /// by token name; path data is SVG path syntax.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out).expect("writing to a String");
        out
    }

    fn write_json<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        w.write_str("{\"viewBox\":[")?;
        json_num(w, self.view_box.0)?;
        w.write_char(',')?;
        json_num(w, self.view_box.1)?;
        w.write_str("],\"palette\":{")?;
        for (i, (token, paint)) in self.palette.tokens().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            json_str(w, token.name())?;
            w.write_char(':')?;
            let mut css = String::new();
            write!(css, "{paint}")?;
            json_str(w, &css)?;
        }
        w.write_str("},\"layers\":[")?;
        for (i, l) in self.layers.iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            w.write_str("{\"role\":")?;
            json_str(w, role_name(l.role))?;
            w.write_str(",\"token\":")?;
            json_str(w, l.token.name())?;
            if l.evenodd {
                w.write_str(",\"evenodd\":true")?;
            }
            if let Some(g) = l.group {
                write!(w, ",\"group\":{g}")?;
            }
            if let Some(c) = l.corner {
                w.write_str(",\"corner\":")?;
                json_str(w, corner_name(c))?;
            }
            w.write_str(",\"origin\":[")?;
            json_num(w, l.origin.0)?;
            w.write_char(',')?;
            json_num(w, l.origin.1)?;
            w.write_str("],\"path\":")?;
            json_str(w, &l.path)?;
            w.write_char('}')?;
        }
        w.write_str("],\"tracks\":[")?;
        for (i, t) in self.tracks.iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            write!(
                w,
                "{{\"layer\":{},\"delayMs\":{},\"durationMs\":{},\"ease\":[",
                t.layer, t.delay_ms, t.duration_ms
            )?;
            for (j, v) in [t.ease.x1, t.ease.y1, t.ease.x2, t.ease.y2]
                .into_iter()
                .enumerate()
            {
                if j > 0 {
                    w.write_char(',')?;
                }
                json_num(w, v)?;
            }
            w.write_str("],\"iterations\":")?;
            match t.iterations {
                Some(n) => write!(w, "{n}")?,
                None => w.write_str("null")?,
            }
            w.write_str(",\"keys\":[")?;
            for (j, k) in t.keys.iter().enumerate() {
                if j > 0 {
                    w.write_char(',')?;
                }
                w.write_str("{\"offset\":")?;
                json_num(w, k.offset)?;
                w.write_str(",\"property\":")?;
                json_str(w, k.property.name())?;
                w.write_str(",\"value\":")?;
                match k.value {
                    Value::Number(v) => json_num(w, v)?,
                    Value::Color(c) => {
                        let mut css = String::new();
                        write!(css, "{c}")?;
                        json_str(w, &css)?;
                    }
                }
                w.write_char('}')?;
            }
            w.write_str("]}")?;
        }
        write!(w, "],\"totalMs\":{},\"trigger\":", self.total_ms)?;
        match &self.trigger {
            Trigger::Load => w.write_str("\"load\"")?,
            Trigger::Hover => w.write_str("\"hover\"")?,
            Trigger::Class(c) => {
                w.write_str("{\"class\":")?;
                json_str(w, c)?;
                w.write_char('}')?;
            }
        }
        w.write_str(",\"reducedMotion\":\"final-state\"}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, QrCode};

    #[test]
    fn curves_ease_out() {
        let quart = Ease::Quart.curve();
        assert_eq!(quart.at(0.0), 0.0);
        assert!((quart.at(1.0) - 1.0).abs() < 1e-9);
        // Ease-outs are ahead of linear everywhere in between.
        for i in 1..10 {
            let x = i as f64 / 10.0;
            assert!(quart.at(x) > x, "{x}");
        }
        assert!((CubicBezier::LINEAR.at(0.3) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn groups_partition_the_modules() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        for shape in [
            ModuleShape::SQUARE,
            ModuleShape::Dots { size: 0.9 },
            ModuleShape::Liquid { smoothness: 1.0 },
        ] {
            let style = Style::new().modules(shape).intro(Preset::Settle);
            let canvas = scene::canvas(&style, &qr);
            let layers = scene::layers(&style, &qr);
            let pieces = pieces(&style, &qr, &canvas, style.intro.as_ref().unwrap(), &layers);
            // Every data module is in exactly one piece.
            let Part::Modules(grid) = &layers[0].part else {
                panic!("data first");
            };
            let mut count = 0;
            for p in pieces.iter().filter(|p| p.layer == 0) {
                if let Unit::Cells(cells) = &p.unit {
                    for &(x, y) in cells {
                        assert!(grid.get(x as i64, y as i64));
                        count += 1;
                    }
                }
            }
            let dark = (0..qr.size() as i64)
                .flat_map(|y| (0..qr.size() as i64).map(move |x| (x, y)))
                .filter(|&(x, y)| grid.get(x, y))
                .count();
            assert_eq!(count, dark);
            let groups: Vec<u8> = pieces.iter().map(|p| p.group).collect();
            assert!(groups.iter().all(|&g| g < 12));
            assert!(groups.contains(&0) && groups.contains(&11));
        }
    }

    #[test]
    fn regions_are_four_connected() {
        let mut grid = Grid::new(4);
        for (x, y) in [(0, 0), (1, 0), (1, 1), (2, 2), (3, 2)] {
            grid.set(x, y);
        }
        let r = regions(&grid, 4, true);
        assert_eq!(
            r,
            alloc::vec![
                alloc::vec![(0, 0), (1, 0), (1, 1)],
                alloc::vec![(2, 2), (3, 2)]
            ]
        );
        assert_eq!(regions(&grid, 4, false).len(), 5);
    }

    #[test]
    fn stagger_fits_the_duration() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        for ms in [600, 1200, 2000] {
            let style = Style::new().intro(Intro::new(Preset::Settle).duration_ms(ms));
            let plan = style.plan(&qr).unwrap();
            assert_eq!(plan.total_ms, ms);
            assert!(plan.tracks.iter().all(|t| t.duration_ms <= 800));
            let first = plan.tracks.iter().map(|t| t.delay_ms).min().unwrap();
            assert_eq!(first, 0);
        }
    }

    #[test]
    fn frames_end_on_the_static_state() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        let style = Style::new().intro(Preset::Settle);
        let plan = style.plan(&qr).unwrap();
        let end = style.render_frame(&qr, plan.total_ms).unwrap();
        assert!(!end.contains("opacity=") && !end.contains("transform="));
        let start = style.render_frame(&qr, 0).unwrap();
        assert!(start.contains("opacity=\"0\""));
        assert!(!start.contains("@keyframes"));
    }

    #[test]
    fn css_is_inside_supports_and_has_no_end_keyframes() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let svg = Style::new().intro(Preset::Settle).render(&qr).unwrap();
        let style = &svg[svg.find("<style>").unwrap()..svg.find("</style>").unwrap()];
        let supports = style.find("@supports").unwrap();
        assert!(style.find("@keyframes").unwrap() > supports);
        assert!(style.contains("@media (prefers-reduced-motion:reduce)"));
        assert!(!style.contains("100%{") && !style.contains("to{"));
        assert!(style.contains("{0%{opacity:0;transform:scale(.92)}}"));
        assert!(svg.contains("class=\"qr qr"));
    }

    #[test]
    fn class_triggers_are_escaped() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let intro = Intro::new(Preset::Settle).trigger(Trigger::Class(String::from("1</style>")));
        let svg = Style::new().intro(intro).render(&qr).unwrap();
        assert!(!svg.contains("1</style>"));
        assert!(svg.contains(".\\31 \\3c \\2f style\\3e  ."));
        assert!(svg.contains("animation-play-state:paused"));
    }

    #[test]
    fn json_is_deterministic_and_complete() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let style = Style::new().intro(Preset::Settle);
        let a = style.plan(&qr).unwrap().to_json();
        assert_eq!(a, style.plan(&qr).unwrap().to_json());
        assert!(a.starts_with("{\"viewBox\":[29,29],\"palette\":{\"bg\":\"#fefcf8\""));
        assert!(a.contains("\"role\":\"finder-eye\",\"token\":\"finder.eye\""));
        assert!(a.ends_with(",\"trigger\":\"load\",\"reducedMotion\":\"final-state\"}"));
        let still = Style::new().plan(&qr).unwrap();
        assert!(still.tracks.is_empty());
    }
}
