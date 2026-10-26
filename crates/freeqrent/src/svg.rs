//! The SVG writer (SPEC §4 step 5, §4.1, §6.2): streams the document into
//! a `fmt::Write` sink with no tree in between.
//!
//! Paint has two levels. Attributes carry plain sRGB hex, which every
//! renderer understands. A `<style>` block wrapped in `@supports` adds the
//! token custom properties and wide-gamut values for browsers. Renderers
//! that don't know `@supports` skip the block; resvg, for one, paints
//! black when it meets `var()` or `oklch()` anywhere else.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::color::Color;
use crate::geometry::{self, Path, UNIT};
use crate::logo::{self, Content};
use crate::math;
use crate::matrix::{Corner, QrMatrix};
use crate::num::write_scaled;
use crate::palette::{Gradient, GradientKind, Paint, Palette, Token};
use crate::scanbox::{self, AlignmentShape};
use crate::scene::{self, Canvas, LayerRole, Part, Round};
use crate::style::{Frame, Style};

/// The paths to draw, one per (token, fill rule), in paint order; a
/// picture's sub-cells keep their own elements.
fn build_paths(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Vec<Element> {
    let mut paths: Vec<Element> = Vec::new();
    for layer in scene::layers(style, qr) {
        if matches!(layer.part, Part::Halftone) {
            paths.extend(halftone_elements(style, qr));
            continue;
        }
        let evenodd = layer.part.evenodd();
        let i = match paths.iter().position(|e| {
            e.token == layer.token && e.evenodd == evenodd && e.role != LayerRole::Halftone
        }) {
            Some(i) => i,
            None => {
                paths.push(Element::new(
                    layer.role(),
                    layer.token,
                    evenodd,
                    Path::new(),
                ));
                paths.len() - 1
            }
        };
        draw_part(style, qr, canvas, &layer.part, &mut paths[i].path);
    }
    paths.retain(|e| !e.path.is_empty());
    paths
}

/// The picture's elements: one per paint, in sub-cell units.
#[cfg(feature = "halftone")]
pub(crate) fn halftone_elements(style: &Style, qr: &QrMatrix) -> Vec<Element> {
    halftone_cells(style, qr).map_or_else(Vec::new, |cells| halftone_strokes(&cells, None))
}

/// The picture's sub-cells, if the style has one.
#[cfg(feature = "halftone")]
pub(crate) fn halftone_cells(style: &Style, qr: &QrMatrix) -> Option<crate::halftone::Subcells> {
    let h = style.halftone.as_ref()?;
    Some(crate::halftone::subcells(
        h,
        qr,
        &scene::removed(style, qr),
        style.palette.background(),
        style.inverted_ink(),
    ))
}

/// The picture's strokes, one element per paint and height; with `only`,
/// just the sub-cells of the modules it sets (an intro's tile of them).
#[cfg(feature = "halftone")]
pub(crate) fn halftone_strokes(
    cells: &crate::halftone::Subcells,
    only: Option<&geometry::Grid>,
) -> Vec<Element> {
    let masked;
    let cells = match only {
        Some(grid) => {
            let side = cells.side;
            let mut c = cells.clone();
            for (i, d) in c.dark.iter_mut().enumerate() {
                let (x, y) = ((i % side) / 3, (i / side) / 3);
                if !grid.get(x as i64, y as i64) {
                    *d = None;
                }
            }
            masked = c;
            &masked
        }
        None => cells,
    };
    crate::halftone::paths(cells)
        .into_iter()
        .map(|s| {
            let token = if s.dark { Token::Fg } else { Token::Bg };
            let mut e = Element::new(LayerRole::Halftone, token, false, s.path);
            e.color = s.paint;
            e.stroke = Some(s.width);
            e
        })
        .collect()
}

#[cfg(not(feature = "halftone"))]
pub(crate) fn halftone_elements(_: &Style, _: &QrMatrix) -> Vec<Element> {
    Vec::new()
}

/// Appends one layer part's geometry to `path`.
pub(crate) fn draw_part(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    part: &Part,
    path: &mut Path,
) {
    let origin = canvas.origin * UNIT;
    // Center of a pattern whose top-left module is `(x, y)` and side `n`.
    let center = |(x, y): (i64, i64), n: i64| {
        (
            origin + x * UNIT + n * UNIT / 2,
            origin + y * UNIT + n * UNIT / 2,
        )
    };
    match part {
        // Drawn as elements of their own (`halftone_elements`).
        Part::Halftone => {}
        Part::Ring => {
            if let Some(round) = &canvas.round {
                draw_ring(path, round, canvas.side * UNIT / 2);
            }
        }
        Part::Filler(grid) => geometry::draw_modules(&style.modules, grid, 0, path),
        Part::Modules(grid) => geometry::draw_modules(&style.modules, grid, origin, path),
        Part::Timing(grid) => {
            for (x, y, _, _) in qr.modules() {
                if grid.get(x as i64, y as i64) {
                    let (px, py) = (origin + x as i64 * UNIT, origin + y as i64 * UNIT);
                    scanbox::draw_timing_module(path, style.timing, px, py, y == 6);
                }
            }
        }
        Part::FinderOuter(corners) => {
            for &c in corners {
                let at = center(scene::finder_origin(c, qr.size()), 7);
                let (ring, _) = style.finders.at(c);
                let toward = scene::finder_toward(c);
                scanbox::draw_ring(path, ring, scanbox::FINDER, false, at, toward);
            }
        }
        Part::FinderEye(corners) => {
            for &c in corners {
                let at = center(scene::finder_origin(c, qr.size()), 7);
                let (_, eye) = style.finders.at(c);
                let toward = scene::finder_toward(c);
                scanbox::draw_eye(path, eye, scanbox::FINDER, false, at, toward);
            }
        }
        Part::Alignment(centers) => {
            let (ring, eye, scaled) = match style.alignment.shape {
                AlignmentShape::Match => {
                    let (ring, eye) = style.finders.at(Corner::TopLeft);
                    (ring, eye, true)
                }
                AlignmentShape::Own { outer, eye } => (outer, eye, false),
            };
            for &(cx, cy) in centers {
                let at = center((cx as i64 - 2, cy as i64 - 2), 5);
                let toward = scene::alignment_toward((cx, cy), qr.size());
                scanbox::draw_ring(path, ring, scanbox::ALIGNMENT, scaled, at, toward);
                scanbox::draw_eye(path, eye, scanbox::ALIGNMENT, scaled, at, toward);
            }
        }
    }
}

/// A round frame's ring, centered at `c` path units: an annulus, or
/// dashes as annular sectors, all filled (no strokes, so the paint form
/// stays the same as everywhere else).
fn draw_ring(path: &mut Path, round: &Round, c: i64) {
    let Some((inner, outer)) = round.ring else {
        return;
    };
    let u = |v: f64| math::round(v * UNIT as f64) as i64;
    if !round.dashed {
        path.circle(c, c, u(outer));
        path.circle(c, c, u(inner));
        return;
    }
    // Dashes about 1.2 modules long with 0.8 gaps, starting at the top.
    let mid = (inner + outer) / 2.0;
    let count = math::round(2.0 * core::f64::consts::PI * mid / 2.0).max(4.0);
    let step = 2.0 * core::f64::consts::PI / count;
    let at = |r: f64, a: f64| {
        let (sin, cos) = math::sin_cos(a - core::f64::consts::FRAC_PI_2);
        (c + u(r * cos), c + u(r * sin))
    };
    for i in 0..count as i64 {
        let (a0, a1) = (i as f64 * step, (i as f64 + 0.6) * step);
        let (p0, p1, p2, p3) = (at(outer, a0), at(outer, a1), at(inner, a1), at(inner, a0));
        path.move_to(p0.0, p0.1);
        path.arc_to(u(outer), false, true, p1.0, p1.1);
        path.line_to(p2.0, p2.1);
        path.arc_to(u(inner), false, false, p3.0, p3.1);
        path.close();
    }
}

/// One element of the document, in paint order.
#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub(crate) role: LayerRole,
    pub(crate) token: Token,
    pub(crate) evenodd: bool,
    pub(crate) path: Path,
    /// The stagger group, for split layers.
    #[cfg_attr(not(feature = "animate"), allow(dead_code))]
    pub(crate) group: Option<u8>,
    /// The finder corner, for finders drawn one corner at a time.
    #[cfg_attr(not(feature = "animate"), allow(dead_code))]
    pub(crate) corner: Option<Corner>,
    /// The transform origin: the center of the element's box, in modules.
    pub(crate) origin: (f64, f64),
    /// The `{prefix}-e{n}` class carrying its animation.
    pub(crate) anim: Option<usize>,
    /// Its state at a fixed time, for a frame render.
    pub(crate) pose: Option<Pose>,
    /// A fixed color instead of the token's paint (a picture's colors).
    pub(crate) color: Option<Color>,
    /// A picture's sub-cells (SPEC §7.7 B): the path is lines in fifths
    /// of a module, stroked this wide, drawn through a nested viewport over
    /// the code's modules.
    pub(crate) stroke: Option<u8>,
    /// A logo frame stroked along its path instead of filled.
    pub(crate) outline: Option<logo::Outline>,
    /// A picture's strokes that move with the element before them: drawn
    /// inside its animated group rather than in a group of their own.
    #[cfg_attr(not(feature = "animate"), allow(dead_code))]
    pub(crate) joins: bool,
}

impl Element {
    pub(crate) fn new(role: LayerRole, token: Token, evenodd: bool, path: Path) -> Element {
        Element {
            role,
            token,
            evenodd,
            path,
            group: None,
            corner: None,
            origin: (0.0, 0.0),
            anim: None,
            pose: None,
            color: None,
            stroke: None,
            outline: None,
            joins: false,
        }
    }
}

/// An element's animated properties at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    pub(crate) opacity: f64,
    pub(crate) scale: f64,
    pub(crate) translate: (f64, f64),
    /// Clockwise turn about the origin, in degrees.
    pub(crate) rotate: f64,
    /// Vertical scale on top of `scale`, for flips (negative mirrors).
    pub(crate) scale_y: f64,
    /// How much of an outline is drawn, 0–1.
    pub(crate) draw: f64,
    /// A fill that replaces the token's paint.
    pub(crate) fill: Option<Color>,
}

impl Pose {
    #[cfg_attr(not(feature = "animate"), allow(dead_code))]
    pub(crate) const NATURAL: Pose = Pose {
        opacity: 1.0,
        scale: 1.0,
        translate: (0.0, 0.0),
        rotate: 0.0,
        scale_y: 1.0,
        draw: 1.0,
        fill: None,
    };
}

/// Whether the background is drawn.
pub(crate) fn draws_background(style: &Style) -> bool {
    style.palette.background().alpha() > 0.0 && style.frame != Frame::None
}

/// The background's outline as a path (the document writes it as a
/// `<rect>` or `<circle>`).
pub(crate) fn background_path(style: &Style, canvas: &Canvas) -> Path {
    let mut path = Path::new();
    let n = canvas.side * UNIT;
    match (&canvas.round, style.frame) {
        (Some(round), _) => {
            path.circle(n / 2, n / 2, math::round(round.radius * UNIT as f64) as i64)
        }
        (None, Frame::RoundedSquare(r)) => path.rounded_rect(0, 0, n, n, [geometry::units(r); 4]),
        _ => path.rect(0, 0, n, n),
    }
    path
}

/// The logo's plate and its center, in modules.
pub(crate) fn logo_plate(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Option<(Path, f64)> {
    let logo = style.logo.as_ref()?;
    let p = logo::placement(logo, qr, crate::check::logo_scale(style, qr));
    let o = canvas.origin as f64;
    let u = |v: f64| math::round(v * UNIT as f64) as i64;
    let mut plate = Path::new();
    let (at, side) = (u(o + p.center - p.half), u(2.0 * p.half));
    plate.rounded_rect(at, at, side, side, [u(p.radius); 4]);
    Some((plate, o + p.center))
}

/// The outline the logo's content is clipped to: the plate, or with a
/// frame the window inside it, and its corner radius.
fn logo_window(p: &logo::Placement, origin: f64) -> (Path, f64) {
    let inset = p.frame.map_or(0.0, |w| w + logo::FRAME_INSET);
    let (half, radius) = (p.half - inset, (p.radius - inset).max(0.0));
    let u = |v: f64| math::round(v * UNIT as f64) as i64;
    let mut path = Path::new();
    let (at, side) = (u(origin + p.center - half), u(2.0 * half));
    path.rounded_rect(at, at, side, side, [u(radius); 4]);
    (path, radius)
}

/// The elements of a code that doesn't animate: one path per (token,
/// fill rule), between the background and the logo.
pub(crate) fn static_elements(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Vec<Element> {
    let mut elements = Vec::new();
    let mid = canvas.side as f64 / 2.0;
    if draws_background(style) {
        elements.push(Element::new(
            LayerRole::Background,
            Token::Bg,
            false,
            background_path(style, canvas),
        ));
    }
    elements.extend(build_paths(style, qr, canvas));
    for e in &mut elements {
        e.origin = (mid, mid);
    }
    elements.extend(logo_elements(style, qr, canvas));
    elements
}

/// The glint band's width and slant, as shares of the plate's side.
pub(crate) const GLINT_BAND: f64 = 0.5;
pub(crate) const GLINT_SLANT: f64 = 0.3;

/// The logo's layers (SPEC §7.3, §8): the plate with the logo, its
/// silhouette for the Ink intro, the frame, and the glint band, each
/// turning about the plate's center.
pub(crate) fn logo_elements(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Vec<Element> {
    let (Some(logo), Some((plate, center))) = (&style.logo, logo_plate(style, qr, canvas)) else {
        return Vec::new();
    };
    #[cfg(feature = "animate")]
    let (ink, glint, art) = (
        style
            .intro
            .as_ref()
            .is_some_and(|i| i.entrance().is_some() && i.logo == crate::animate::LogoIntro::Ink),
        style.idle.contains(&crate::Idle::LogoGlint),
        style.idle.contains(&crate::Idle::LogoSpin) || style.idle.contains(&crate::Idle::LogoFlip),
    );
    #[cfg(not(feature = "animate"))]
    let (ink, glint, art) = (false, false, false);
    let mut out = Vec::new();
    out.push(Element::new(
        LayerRole::Logo,
        Token::LogoBackdrop,
        false,
        plate.clone(),
    ));
    let p = logo::placement(logo, qr, crate::check::logo_scale(style, qr));
    if art {
        let (window, _) = logo_window(&p, canvas.origin as f64);
        out.push(Element::new(
            LayerRole::LogoArt,
            Token::LogoBackdrop,
            false,
            window,
        ));
    }
    if ink {
        let (window, _) = logo_window(&p, canvas.origin as f64);
        out.push(Element::new(LayerRole::LogoInk, Token::Fg, false, window));
    }
    if let Some(frame) = logo::frame_shape(logo, &p, canvas.origin as f64, &style.modules) {
        let mut e = Element::new(LayerRole::LogoFrame, Token::LogoFrame, false, frame.path);
        e.outline = frame.outline;
        out.push(e);
    }
    if glint {
        let u = |v: f64| math::round(v * UNIT as f64) as i64;
        let (left, side) = (u(center - p.half), u(2.0 * p.half));
        let (band, slant) = (u(2.0 * p.half * GLINT_BAND), u(2.0 * p.half * GLINT_SLANT));
        let mut path = Path::new();
        path.move_to(left - band, left);
        path.line_to(left, left);
        path.line_to(left - slant, left + side);
        path.line_to(left - band - slant, left + side);
        path.close();
        out.push(Element::new(LayerRole::LogoGlint, Token::Bg, false, path));
    }
    for e in &mut out {
        e.origin = (center, center);
    }
    out
}

#[cfg(feature = "animate")]
type Motion = crate::animate::Motion;
/// Without the `animate` feature nothing moves.
#[cfg(not(feature = "animate"))]
enum Motion {}

/// The elements to write, and their animation if the style has one. With
/// `at`, the animation is evaluated at that time instead (a frame).
fn layout(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    at: Option<u32>,
) -> (Vec<Element>, Option<Motion>) {
    #[cfg(feature = "animate")]
    if let Some((elements, motion)) = crate::animate::build(style, qr, canvas, at) {
        return (elements, Some(motion));
    }
    let _ = at;
    (static_elements(style, qr, canvas), None)
}

pub(crate) fn write<W: Write + ?Sized>(
    style: &Style,
    qr: &QrMatrix,
    at: Option<u32>,
    w: &mut W,
) -> fmt::Result {
    let palette = &style.palette;
    let canvas = scene::canvas(style, qr);
    let (elements, motion) = layout(style, qr, &canvas, at);
    let n = canvas.side;
    let prefix = id_prefix(style, qr);
    let backdrop = style.logo.as_ref().is_some_and(|l| l.backdrop);

    // Solid tokens that get a class, and the gradients to define.
    let mut solids: Vec<Token> = Vec::new();
    let mut gradients: Vec<Token> = Vec::new();
    // A picture's strokes: solid tokens whose rules set `stroke`, and
    // gradients restated in the strokes' space (fifths of a module).
    let mut stroked: Vec<Token> = Vec::new();
    let mut stroke_gradients: Vec<Token> = Vec::new();
    let painted = elements
        .iter()
        .filter(|e| {
            !matches!(
                e.role,
                LayerRole::Shimmer | LayerRole::LogoGlint | LayerRole::LogoArt
            )
        })
        .filter(|e| e.role != LayerRole::Logo || backdrop)
        .filter(|e| e.color.is_none())
        .map(|e| (e.token, e.stroke.is_some(), e.outline.is_some()));
    for (token, cells, outline) in painted {
        // An outline's gradient is in the document's space, like a fill's.
        let (list, token) = match palette.resolve(token) {
            Paint::Solid(_) if cells || outline => (&mut stroked, token),
            Paint::Solid(_) => (&mut solids, token),
            Paint::Gradient(_) if cells => (&mut stroke_gradients, paint_source(palette, token)),
            Paint::Gradient(_) => (&mut gradients, paint_source(palette, token)),
        };
        if !list.contains(&token) {
            list.push(token);
        }
    }

    let mut out = Out {
        w,
        pretty: !style.minify,
    };
    out.w
        .write_str("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 ")?;
    write!(out.w, "{n} {n}\" class=\"qr")?;
    #[cfg(feature = "animate")]
    if motion.as_ref().is_some_and(|m| m.live()) {
        write!(out.w, " {prefix}")?;
    }
    out.w.write_str("\" role=\"img\" aria-label=\"")?;
    escape(out.w, style.label.as_deref().unwrap_or("QR code"))?;
    out.w.write_str("\">")?;
    if let Some(title) = &style.title {
        out.line()?;
        out.w.write_str("<title>")?;
        escape(out.w, title)?;
        out.w.write_str("</title>")?;
    }
    if let Some(desc) = &style.description {
        out.line()?;
        out.w.write_str("<desc>")?;
        escape(out.w, desc)?;
        out.w.write_str("</desc>")?;
    }

    out.line()?;
    #[cfg(feature = "animate")]
    let fade = style.theme_transition;
    #[cfg(not(feature = "animate"))]
    let fade = None;
    write_style(
        out.w,
        palette,
        &prefix,
        (&solids, &stroked),
        &gradients,
        fade,
        motion.as_ref(),
    )?;

    if !gradients.is_empty() || !stroke_gradients.is_empty() {
        out.line()?;
        out.w.write_str("<defs>")?;
        let defs = gradients.iter().map(|&t| (t, false));
        for (source, fifths) in defs.chain(stroke_gradients.iter().map(|&t| (t, true))) {
            let Some(Paint::Gradient(g)) = palette.get(source) else {
                unreachable!("gradient sources hold gradients");
            };
            out.nested_line()?;
            let at = (canvas.origin, qr.size());
            write_gradient(out.w, g, &prefix, source, at, fifths)?;
        }
        out.line()?;
        out.w.write_str("</defs>")?;
    }

    #[cfg(feature = "animate")]
    let masked = motion.as_ref().and_then(|m| m.reveal_layers());
    for (i, e) in elements.iter().enumerate() {
        out.line()?;
        #[cfg(feature = "animate")]
        if let (Some((first, _)), Some(m)) = (masked, &motion)
            && first == i
        {
            m.write_mask(out.w, &prefix, n)?;
            // Live, the stylesheet applies the mask, so renderers that skip
            // it draw the code unmasked; a frame needs the attribute.
            if m.live() {
                write!(out.w, "<g class=\"{prefix}-m\">")?;
            } else {
                write!(out.w, "<g mask=\"url(#{prefix}-reveal)\">")?;
            }
            out.line()?;
        }
        let _ = i;
        match e.role {
            LayerRole::Background => {
                match (&canvas.round, style.frame) {
                    (Some(round), _) => {
                        out.w.write_str("<circle cx=\"")?;
                        write_scaled(out.w, n * UNIT / 2, 3)?;
                        out.w.write_str("\" cy=\"")?;
                        write_scaled(out.w, n * UNIT / 2, 3)?;
                        out.w.write_str("\" r=\"")?;
                        write_scaled(out.w, math::round(round.radius * UNIT as f64) as i64, 3)?;
                        out.w.write_char('"')?;
                    }
                    (None, Frame::RoundedSquare(r)) if geometry::units(r) > 0 => {
                        write!(out.w, "<rect width=\"{n}\" height=\"{n}\" rx=\"")?;
                        write_scaled(out.w, geometry::units(r), 3)?;
                        out.w.write_char('"')?;
                    }
                    _ => write!(out.w, "<rect width=\"{n}\" height=\"{n}\"")?,
                }
                write_element_paint(out.w, palette, e, &prefix)?;
                write_pose(out.w, e)?;
                out.w.write_str("/>")?;
            }
            LayerRole::Logo => {
                let ink = elements.iter().any(|e| e.role == LayerRole::LogoInk);
                let art = elements.iter().find(|e| e.role == LayerRole::LogoArt);
                write_logo(&mut out, style, qr, &canvas, &prefix, e, art, ink)?
            }
            // Drawn inside the logo's own group.
            LayerRole::LogoArt => {}
            LayerRole::LogoFrame => write_frame(out.w, palette, e, &prefix)?,
            LayerRole::LogoInk => {
                write!(
                    out.w,
                    "<mask id=\"{prefix}-logo-ink\" mask-type=\"alpha\"><use href=\"#{prefix}-logo-art\"/></mask><path mask=\"url(#{prefix}-logo-ink)\""
                )?;
                write_element_paint(out.w, palette, e, &prefix)?;
                // Its natural state is invisible: the intro only flashes it.
                match e.pose {
                    Some(_) => write_pose(out.w, e)?,
                    None => out.w.write_str(" opacity=\"0\"")?,
                }
                write!(out.w, " d=\"{}\"/>", e.path.data())?;
            }
            LayerRole::LogoGlint => {
                let plate = logo_plate(style, qr, &canvas)
                    .map(|(p, _)| p)
                    .unwrap_or_default();
                write!(
                    out.w,
                    "<clipPath id=\"{prefix}-glint-clip\"><path d=\"{}\"/></clipPath><linearGradient id=\"{prefix}-glint\"><stop offset=\"0\" stop-color=\"#fff\" stop-opacity=\"0\"/><stop offset=\".5\" stop-color=\"#fff\" stop-opacity=\".45\"/><stop offset=\"1\" stop-color=\"#fff\" stop-opacity=\"0\"/></linearGradient><g clip-path=\"url(#{prefix}-glint-clip)\"><path",
                    plate.data()
                )?;
                if let Some(n) = e.anim {
                    write!(out.w, " class=\"{prefix}-e{n}\"")?;
                }
                write!(out.w, " fill=\"url(#{prefix}-glint)\"")?;
                write_pose(out.w, e)?;
                write!(out.w, " d=\"{}\"/></g>", e.path.data())?;
            }
            LayerRole::Shimmer => {
                write!(
                    out.w,
                    "<linearGradient id=\"{prefix}-shimmer\"><stop offset=\"0\" stop-color=\"#fff\" stop-opacity=\"0\"/><stop offset=\".5\" stop-color=\"#fff\" stop-opacity=\".18\"/><stop offset=\"1\" stop-color=\"#fff\" stop-opacity=\"0\"/></linearGradient><path"
                )?;
                if let Some(n) = e.anim {
                    write!(out.w, " class=\"{prefix}-e{n}\"")?;
                }
                write!(out.w, " fill=\"url(#{prefix}-shimmer)\"")?;
                write_pose(out.w, e)?;
                write!(out.w, " d=\"{}\"/>", e.path.data())?;
            }
            _ if e.stroke.is_some() => {
                let next_joins = elements.get(i + 1).is_some_and(|n| n.joins);
                write_subcells(out.w, palette, e, &prefix, &canvas, qr.size(), next_joins)?
            }
            _ => {
                out.w.write_str("<path")?;
                write_element_paint(out.w, palette, e, &prefix)?;
                if e.evenodd {
                    out.w.write_str(" fill-rule=\"evenodd\"")?;
                }
                write_pose(out.w, e)?;
                write!(out.w, " d=\"{}\"/>", e.path.data())?;
            }
        }
        #[cfg(feature = "animate")]
        if masked.is_some_and(|(_, last)| last == i) {
            out.line()?;
            out.w.write_str("</g>")?;
        }
    }
    if out.pretty {
        out.w.write_char('\n')?;
    }
    out.w.write_str("</svg>")?;
    if out.pretty {
        out.w.write_char('\n')?;
    }
    Ok(())
}

/// A path in sub-cells: a nested viewport over the code's modules, so the
/// path's integer coordinates scale exactly. Motion goes on a group around
/// it, so transform origins stay in the document's units.
/// With `next_joins`, the element after this one moves with it, so the
/// group carrying the motion stays open; a joining element writes no group
/// of its own and closes its owner's when nothing joins after it.
fn write_subcells<W: Write + ?Sized>(
    w: &mut W,
    palette: &Palette,
    e: &Element,
    prefix: &str,
    canvas: &Canvas,
    size: u16,
    next_joins: bool,
) -> fmt::Result {
    let opens = !e.joins && (e.anim.is_some() || e.pose.is_some_and(|p| p != Pose::NATURAL));
    if opens {
        w.write_str("<g")?;
        if let Some(n) = e.anim {
            write!(w, " class=\"{prefix}-e{n}\"")?;
        }
        write_pose(w, e)?;
        w.write_char('>')?;
    }
    let (o, cells) = (canvas.origin, size as usize * geometry::FIFTHS);
    write!(
        w,
        "<svg x=\"{o}\" y=\"{o}\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 {cells} {cells}\"><path"
    )?;
    let plain = Element {
        anim: None,
        pose: e.pose.map(|p| Pose {
            fill: p.fill,
            ..Pose::NATURAL
        }),
        ..e.clone()
    };
    let color = plain.pose.and_then(|p| p.fill).or(e.color);
    match (color, palette.resolve(e.token)) {
        (Some(c), _) | (None, Paint::Solid(c)) => {
            if color.is_none() {
                write!(w, " class=\"{prefix}-")?;
                write_dashed(w, e.token.name())?;
                w.write_str("-s\"")?;
            }
            w.write_str(" stroke=\"")?;
            write_hex(w, c)?;
            w.write_char('"')?;
            write_opacity(w, "stroke-opacity", c)?;
        }
        (None, Paint::Gradient(_)) => {
            write!(w, " stroke=\"url(#{prefix}-")?;
            write_dashed(w, paint_source(palette, e.token).name())?;
            w.write_str("-q)\"")?;
        }
    }
    if let Some(width) = e.stroke.filter(|&k| k != 1) {
        write!(w, " stroke-width=\"{width}\"")?;
    }
    write!(w, " d=\"{}\"/></svg>", e.path.data())?;
    if (opens || e.joins) && !next_joins {
        w.write_str("</g>")?;
    }
    Ok(())
}

/// A logo frame: filled like any path, or stroked along it with its dash
/// pattern, drawn as far as its pose says.
fn write_frame<W: Write + ?Sized>(
    w: &mut W,
    palette: &Palette,
    e: &Element,
    prefix: &str,
) -> fmt::Result {
    w.write_str("<path")?;
    let Some(outline) = &e.outline else {
        write_element_paint(w, palette, e, prefix)?;
        if e.evenodd {
            w.write_str(" fill-rule=\"evenodd\"")?;
        }
        write_pose(w, e)?;
        return write!(w, " d=\"{}\"/>", e.path.data());
    };
    match palette.resolve(e.token) {
        Paint::Solid(color) => {
            write!(w, " class=\"{prefix}-")?;
            write_dashed(w, e.token.name())?;
            w.write_str("-s")?;
            if let Some(n) = e.anim {
                write!(w, " {prefix}-e{n}")?;
            }
            w.write_str("\" fill=\"none\" stroke=\"")?;
            write_hex(w, color)?;
            w.write_char('"')?;
            write_opacity(w, "stroke-opacity", color)?;
        }
        Paint::Gradient(_) => {
            if let Some(n) = e.anim {
                write!(w, " class=\"{prefix}-e{n}\"")?;
            }
            write!(w, " fill=\"none\" stroke=\"url(#{prefix}-")?;
            write_dashed(w, paint_source(palette, e.token).name())?;
            w.write_str(")\"")?;
        }
    }
    let k = |v: f64| math::round(v * 1000.0) as i64;
    w.write_str(" stroke-width=\"")?;
    write_scaled(w, outline.width, 3)?;
    w.write_char('"')?;
    if !outline.dashes.is_empty() {
        w.write_str(" stroke-dasharray=\"")?;
        for (i, &d) in outline.dashes.iter().enumerate() {
            if i > 0 {
                w.write_char(' ')?;
            }
            write_scaled(w, k(d), 3)?;
        }
        w.write_char('"')?;
    }
    // Drawing on moves the dashes back from the path's end.
    let undrawn = e.pose.map_or(0.0, |p| 1.0 - p.draw) * outline.dashes.first().unwrap_or(&0.0);
    let offset = k(outline.offset + undrawn);
    if offset != 0 {
        w.write_str(" stroke-dashoffset=\"")?;
        write_scaled(w, offset, 3)?;
        w.write_char('"')?;
    }
    write_pose(w, e)?;
    write!(w, " d=\"{}\"/>", e.path.data())
}

/// The paint of an element: its token's, or a frame's fixed fill.
fn write_element_paint<W: Write + ?Sized>(
    w: &mut W,
    palette: &Palette,
    e: &Element,
    prefix: &str,
) -> fmt::Result {
    if let Some(color) = e.pose.and_then(|p| p.fill).or(e.color) {
        w.write_str(" fill=\"")?;
        write_hex(w, color)?;
        w.write_char('"')?;
        return write_opacity(w, "fill-opacity", color);
    }
    write_paint(w, palette, e.token, prefix, e.anim)
}

/// A frame's opacity and transform, when they aren't the natural state.
fn write_pose<W: Write + ?Sized>(w: &mut W, e: &Element) -> fmt::Result {
    let Some(pose) = e.pose else {
        return Ok(());
    };
    let k = |v: f64| math::round(v * 1000.0) as i64;
    if k(pose.opacity) < 1000 {
        w.write_str(" opacity=\"")?;
        write_scaled(w, k(pose.opacity.max(0.0)), 3)?;
        w.write_char('"')?;
    }
    let (s, (tx, ty)) = (pose.scale, pose.translate);
    if k(pose.rotate) != 0 || k(pose.scale_y) != 1000 {
        // Scale, turn, then translate, about the origin: CSS
        // `translate(..) rotate(..) scale(..)` with that transform origin.
        let (ox, oy) = e.origin;
        let (sin, cos) = math::sin_cos(math::radians(pose.rotate));
        let sy = s * pose.scale_y;
        let (a, b, c, d) = (s * cos, s * sin, -sy * sin, sy * cos);
        w.write_str(" transform=\"matrix(")?;
        let values = [
            a,
            b,
            c,
            d,
            ox + tx - a * ox - c * oy,
            oy + ty - b * ox - d * oy,
        ];
        for (i, v) in values.into_iter().enumerate() {
            if i > 0 {
                w.write_char(' ')?;
            }
            write_scaled(w, k(v), 3)?;
        }
        return w.write_str(")\"");
    }
    if k(s) != 1000 || k(tx) != 0 || k(ty) != 0 {
        // Scale about the origin, then translate: the CSS
        // `translate(..) scale(..)` with that transform origin.
        let (ox, oy) = e.origin;
        w.write_str(" transform=\"matrix(")?;
        for (i, v) in [s, 0.0, 0.0, s, ox * (1.0 - s) + tx, oy * (1.0 - s) + ty]
            .into_iter()
            .enumerate()
        {
            if i > 0 {
                w.write_char(' ')?;
            }
            write_scaled(w, k(v), 3)?;
        }
        w.write_str(")\"")?;
    }
    Ok(())
}

/// The logo (SPEC §7.3): a clip path for round plates, the plate, then
/// the content fitted into its box and clipped to the plate, so nothing
/// paints outside the modules the budget counted. An animated logo is
/// wrapped in a group that carries its motion. Content that moves on its
/// own (`art`, [`LayerRole::LogoArt`]) gets a group of its own inside the
/// clip, so it moves with the plate and apart from it, and never past the
/// window.
#[allow(clippy::too_many_arguments)]
fn write_logo<W: Write + ?Sized>(
    out: &mut Out<'_, W>,
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    prefix: &str,
    e: &Element,
    art: Option<&Element>,
    ink: bool,
) -> fmt::Result {
    let Some(logo) = &style.logo else {
        return Ok(());
    };
    let p = logo::placement(logo, qr, crate::check::logo_scale(style, qr));
    let o = canvas.origin as f64;
    let u = |v: f64| math::round(v * UNIT as f64) as i64;
    let plate = &e.path;
    let group = e.anim.is_some() || e.pose.is_some();
    if group {
        out.w.write_str("<g")?;
        if let Some(n) = e.anim {
            write!(out.w, " class=\"{prefix}-e{n}\"")?;
        }
        write_pose(out.w, e)?;
        out.w.write_char('>')?;
        out.line()?;
    }
    // Without a frame the window is the plate itself.
    let (window, radius) = logo_window(&p, o);
    let clip = radius > 0.0 || art.is_some();
    if clip {
        write!(
            out.w,
            "<clipPath id=\"{prefix}-logo-clip\"><path d=\"{}\"/></clipPath>",
            window.data()
        )?;
        out.line()?;
    }
    if logo.backdrop {
        out.w.write_str("<path")?;
        let plain = Element {
            anim: None,
            ..e.clone()
        };
        write_element_paint(out.w, &style.palette, &plain, prefix)?;
        write!(out.w, " d=\"{}\"/>", plate.data())?;
        out.line()?;
    }
    if clip {
        write!(out.w, "<g clip-path=\"url(#{prefix}-logo-clip)\">")?;
    }
    let moves = art.filter(|a| a.anim.is_some() || a.pose.is_some());
    if let Some(a) = moves {
        out.w.write_str("<g")?;
        if let Some(n) = a.anim {
            write!(out.w, " class=\"{prefix}-e{n}\"")?;
        }
        write_pose(out.w, a)?;
        out.w.write_char('>')?;
    }
    let box_attrs = |w: &mut W| -> fmt::Result {
        let (at, side) = (u(o + p.center - p.content_half), u(2.0 * p.content_half));
        for (name, v) in [("x", at), ("y", at), ("width", side), ("height", side)] {
            write!(w, " {name}=\"")?;
            write_scaled(w, v, 3)?;
            w.write_char('"')?;
        }
        w.write_str(" preserveAspectRatio=\"xMidYMid meet\"")
    };
    match &logo.content {
        Content::Svg(doc) => {
            out.w.write_str("<svg")?;
            if ink {
                write!(out.w, " id=\"{prefix}-logo-art\"")?;
            }
            box_attrs(out.w)?;
            out.w.write_str(" viewBox=\"")?;
            doc.write_view_box(out.w)?;
            out.w.write_str("\">")?;
            doc.write_body(out.w, Some(prefix))?;
            out.w.write_str("</svg>")?;
        }
        Content::Raster { format, data } => {
            out.w.write_str("<image")?;
            if ink {
                write!(out.w, " id=\"{prefix}-logo-art\"")?;
            }
            box_attrs(out.w)?;
            write!(out.w, " href=\"data:{};base64,", format.mime())?;
            logo::base64(out.w, data)?;
            out.w.write_str("\"/>")?;
        }
    }
    if moves.is_some() {
        out.w.write_str("</g>")?;
    }
    if clip {
        out.w.write_str("</g>")?;
    }
    if group {
        out.w.write_str("</g>")?;
    }
    Ok(())
}

struct Out<'a, W: Write + ?Sized> {
    w: &'a mut W,
    pretty: bool,
}

impl<W: Write + ?Sized> Out<'_, W> {
    /// Starts a child element's line in pretty mode.
    fn line(&mut self) -> fmt::Result {
        if self.pretty {
            self.w.write_str("\n  ")?;
        }
        Ok(())
    }

    /// Starts a grandchild's line (inside `<defs>`) in pretty mode.
    fn nested_line(&mut self) -> fmt::Result {
        if self.pretty {
            self.w.write_str("\n    ")?;
        }
        Ok(())
    }
}

/// The token whose explicit value `token` resolves to.
fn paint_source(palette: &Palette, token: Token) -> Token {
    let mut t = token;
    while palette.get(t).is_none() {
        t = t.fallback().expect("bg and fg are always set");
    }
    t
}

/// `<style>` with one rule per solid token reading its custom property,
/// and wide-gamut gradient stops, all inside `@supports`. When any value is
/// OKLCH/OKLab the condition also asks for `oklch()`, so a browser without
/// it keeps the hex attributes instead of failing the whole rule.
fn write_style<W: Write + ?Sized>(
    w: &mut W,
    palette: &Palette,
    prefix: &str,
    (solids, stroked): (&[Token], &[Token]),
    gradients: &[Token],
    fade: Option<u32>,
    motion: Option<&Motion>,
) -> fmt::Result {
    let wide = |c: Color| c.opaque_srgb() != c.opaque();
    let mut needs_oklch =
        solids
            .iter()
            .chain(stroked)
            .any(|&t| match palette.get(paint_source(palette, t)) {
                Some(Paint::Solid(c)) => wide(*c),
                _ => false,
            });
    for &g in gradients {
        if let Some(Paint::Gradient(g)) = palette.get(g) {
            needs_oklch |= g.stops().iter().any(|s| wide(s.color));
        }
    }

    w.write_str("<style>@supports (color:var(--a))")?;
    if needs_oklch {
        w.write_str(" and (color:oklch(0% 0 0))")?;
    }
    w.write_char('{')?;
    let rules = solids.iter().map(|&t| (t, "", "fill"));
    let rules: Vec<(Token, &str, &str)> = rules
        .chain(stroked.iter().map(|&t| (t, "-s", "stroke")))
        .collect();
    for &(token, suffix, property) in &rules {
        write!(w, ".{prefix}-")?;
        write_dashed(w, token.name())?;
        write!(w, "{suffix}{{{property}:")?;
        write_var_chain(w, palette, token)?;
        if let Some(ms) = fade {
            write!(w, ";transition:{property} {ms}ms cubic-bezier(.25,1,.5,1)")?;
        }
        w.write_char('}')?;
    }
    if fade.is_some() && !rules.is_empty() {
        w.write_str("@media (prefers-reduced-motion:reduce){")?;
        for (i, &(token, suffix, _)) in rules.iter().enumerate() {
            w.write_str(if i == 0 { "." } else { ",." })?;
            write!(w, "{prefix}-")?;
            write_dashed(w, token.name())?;
            w.write_str(suffix)?;
        }
        w.write_str("{transition:none}}")?;
    }
    for &source in gradients {
        let Some(Paint::Gradient(g)) = palette.get(source) else {
            continue;
        };
        for (i, stop) in g.stops().iter().enumerate().filter(|(_, s)| wide(s.color)) {
            write!(w, ".{prefix}-")?;
            write_dashed(w, source.name())?;
            write!(w, "-s{i}{{stop-color:")?;
            stop.color.write_opaque(w)?;
            w.write_char('}')?;
        }
    }
    // Animation lives in the same block, so renderers that skip it show
    // the finished code (SPEC §8.2).
    if let Some(motion) = motion {
        #[cfg(feature = "animate")]
        motion.write_css(w, prefix)?;
        #[cfg(not(feature = "animate"))]
        match *motion {}
    }
    w.write_str("}</style>")
}

/// The paint attributes of an element: a class and hex fill for solid
/// colors, or a gradient reference.
fn write_paint<W: Write + ?Sized>(
    w: &mut W,
    palette: &Palette,
    token: Token,
    prefix: &str,
    anim: Option<usize>,
) -> fmt::Result {
    match palette.resolve(token) {
        Paint::Solid(color) => {
            write!(w, " class=\"{prefix}-")?;
            write_dashed(w, token.name())?;
            if let Some(n) = anim {
                write!(w, " {prefix}-e{n}")?;
            }
            w.write_str("\" fill=\"")?;
            write_hex(w, color)?;
            w.write_char('"')?;
            write_opacity(w, "fill-opacity", color)
        }
        Paint::Gradient(_) => {
            if let Some(n) = anim {
                write!(w, " class=\"{prefix}-e{n}\"")?;
            }
            write!(w, " fill=\"url(#{prefix}-")?;
            write_dashed(w, paint_source(palette, token).name())?;
            w.write_str(")\"")
        }
    }
}

fn write_opacity<W: Write + ?Sized>(w: &mut W, attr: &str, color: Color) -> fmt::Result {
    if color.is_translucent() {
        write!(w, " {attr}=\"")?;
        write_scaled(w, math::round(color.alpha() as f64 * 1000.0) as i64, 3)?;
        w.write_char('"')?;
    }
    Ok(())
}

/// `var(--qr-token,<fallback>)`, recursing through unset tokens. Alpha
/// stays in the `fill-opacity` attribute, so values here are opaque.
fn write_var_chain<W: Write + ?Sized>(w: &mut W, palette: &Palette, token: Token) -> fmt::Result {
    write!(w, "var({},", token.css_var())?;
    match palette.get(token) {
        Some(Paint::Solid(c)) => c.write_opaque(w)?,
        Some(Paint::Gradient(_)) => unreachable!("solid paints resolve through solid tokens"),
        None => write_var_chain(
            w,
            palette,
            token.fallback().expect("bg and fg are always set"),
        )?,
    }
    w.write_char(')')
}

fn write_hex<W: Write + ?Sized>(w: &mut W, color: Color) -> fmt::Result {
    color.opaque_srgb().write_opaque(w)
}

fn write_dashed<W: Write + ?Sized>(w: &mut W, name: &str) -> fmt::Result {
    for c in name.chars() {
        w.write_char(if c == '.' { '-' } else { c })?;
    }
    Ok(())
}

/// With `fifths`, the gradient is for a picture's strokes, whose user
/// space is fifths of a module from the code's corner: it gets a `-q` id
/// and a transform into that space, so it lands where the fills' does.
fn write_gradient<W: Write + ?Sized>(
    w: &mut W,
    gradient: &Gradient,
    prefix: &str,
    source: Token,
    (origin, size): (i64, u16),
    fifths: bool,
) -> fmt::Result {
    // Gradients span the code area, not the quiet zone.
    let side = size as f64;
    let c = origin as f64 + side / 2.0;
    let attr = |w: &mut W, name: &str, v: f64| -> fmt::Result {
        write!(w, " {name}=\"")?;
        write_scaled(w, math::round(v * UNIT as f64) as i64, 3)?;
        w.write_char('"')
    };
    let tag = match gradient.kind() {
        GradientKind::Linear { .. } => "linearGradient",
        GradientKind::Radial => "radialGradient",
    };
    write!(w, "<{tag} id=\"{prefix}-")?;
    write_dashed(w, source.name())?;
    if fifths {
        let k = geometry::FIFTHS as i64;
        let o = -k * origin;
        write!(w, "-q\" gradientTransform=\"matrix({k} 0 0 {k} {o} {o})")?;
    }
    w.write_str("\" gradientUnits=\"userSpaceOnUse\"")?;
    match gradient.kind() {
        GradientKind::Linear { angle } => {
            // CSS geometry: 0deg points up, clockwise; the line is long
            // enough that the corners get the end colors.
            let (sin, cos) = math::sin_cos(math::radians(angle as f64 / 100.0));
            let half = side / 2.0 * (sin.abs() + cos.abs());
            let (dx, dy) = (sin * half, -cos * half);
            attr(w, "x1", c - dx)?;
            attr(w, "y1", c - dy)?;
            attr(w, "x2", c + dx)?;
            attr(w, "y2", c + dy)?;
        }
        GradientKind::Radial => {
            attr(w, "cx", c)?;
            attr(w, "cy", c)?;
            attr(w, "r", side * core::f64::consts::FRAC_1_SQRT_2)?;
        }
    }
    w.write_char('>')?;
    for (i, stop) in gradient.stops().iter().enumerate() {
        w.write_str("<stop")?;
        if stop.color.opaque_srgb() != stop.color.opaque() {
            write!(w, " class=\"{prefix}-")?;
            write_dashed(w, source.name())?;
            write!(w, "-s{i}\"")?;
        }
        w.write_str(" offset=\"")?;
        write_scaled(w, stop.offset as i64, 3)?;
        w.write_str("\" stop-color=\"")?;
        write_hex(w, stop.color)?;
        w.write_char('"')?;
        write_opacity(w, "stop-opacity", stop.color)?;
        w.write_str("/>")?;
    }
    write!(w, "</{tag}>")
}

/// The prefix for classes and IDs. A `<style>` in an inline SVG applies to
/// the whole page, so by default the prefix is a hash (FNV-1a) of the code,
/// quiet zone and palette: two different codes never share a class, and
/// identical ones share identical rules. `Style::id_prefix` overrides it.
fn id_prefix(style: &Style, qr: &QrMatrix) -> String {
    if let Some(p) = &style.id_prefix {
        let clean: String = p
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        return if clean.starts_with(|c: char| c.is_ascii_alphabetic()) {
            clean
        } else {
            alloc::format!("qr{clean}")
        };
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: u8| {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    eat(style.quiet_zone);
    // Frames move the code on the canvas, and gradients with it. The
    // square frame adds nothing, so existing prefixes stay the same.
    if style.frame != Frame::Square {
        let canvas = scene::canvas(style, qr);
        for b in canvas.side.to_le_bytes() {
            eat(b);
        }
    }
    for (_, _, dark, _) in qr.modules() {
        eat(dark as u8);
    }
    // A logo brings its own IDs, so two codes with different logos must
    // not share a prefix.
    if let Some(logo) = &style.logo {
        let mut text = String::new();
        let _ = write!(
            text,
            "{:?}{:?}{}{}{}",
            logo.shape, logo.scale, logo.padding, logo.backdrop, logo.clear_modules
        );
        // Frames and growth were added later: only hashed when used, so
        // existing prefixes stay the same.
        if logo.grow || logo.frame.is_some() {
            let _ = write!(text, "{}{:?}{:?}", logo.grow, logo.frame, logo.frame_width);
        }
        if let Some(moon) = logo.moon {
            let _ = write!(text, "{moon:?}");
        }
        text.bytes().for_each(&mut eat);
        match &logo.content {
            Content::Svg(doc) => doc.to_document().bytes().for_each(&mut eat),
            Content::Raster { data, .. } => data.iter().copied().for_each(&mut eat),
        }
    }
    // An intro adds classes and keyframes, so two animations of one code
    // on a page must not share a prefix either.
    #[cfg(feature = "animate")]
    if style.intro.is_some() || !style.idle.is_empty() || style.theme_transition.is_some() {
        let mut text = String::new();
        let _ = write!(
            text,
            "{:?}{:?}{:?}",
            style.intro, style.idle, style.theme_transition
        );
        text.bytes().for_each(&mut eat);
    }
    let mut text = String::new();
    for (token, paint) in style.palette.tokens() {
        let _ = write!(text, "{token}={paint};");
    }
    text.bytes().for_each(&mut eat);
    alloc::format!("qr{:08x}", (hash >> 32) as u32 ^ hash as u32)
}

/// XML-escapes text for element content and double-quoted attributes.
fn escape<W: Write + ?Sized>(w: &mut W, text: &str) -> fmt::Result {
    for c in text.chars() {
        match c {
            '&' => w.write_str("&amp;")?,
            '<' => w.write_str("&lt;")?,
            '>' => w.write_str("&gt;")?,
            '"' => w.write_str("&quot;")?,
            '\'' => w.write_str("&apos;")?,
            // Characters XML 1.0 forbids are dropped.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => w.write_char(c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, ModuleShape, QrCode, Style};

    fn qr() -> QrMatrix {
        QrCode::encode("HELLO", Ecc::M).unwrap()
    }

    fn prefix_of(svg: &str) -> &str {
        let at = svg.find("{.").unwrap() + 2;
        &svg[at..at + 10]
    }

    #[test]
    fn document_shape() {
        let svg = Style::new().render(&qr()).unwrap();
        let p = prefix_of(&svg);
        assert!(p.starts_with("qr"));
        assert!(svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 29 29\" class=\"qr\" role=\"img\" aria-label=\"QR code\"><style>@supports (color:var(--a)){"
        ));
        assert!(svg.contains(&alloc::format!(".{p}-bg{{fill:var(--qr-bg,#fefcf8)}}")));
        assert!(svg.contains(&alloc::format!(
            ".{p}-finder-outer{{fill:var(--qr-finder-outer,var(--qr-fg,#17161c))}}"
        )));
        assert!(svg.contains(&alloc::format!(
            "<rect width=\"29\" height=\"29\" class=\"{p}-bg\" fill=\"#fefcf8\"/>"
        )));
        // Data first, then timing, then the finder layers.
        let data = svg
            .find(&alloc::format!(
                "<path class=\"{p}-fg\" fill=\"#17161c\" d=\"m"
            ))
            .unwrap();
        let timing = svg
            .find(&alloc::format!("<path class=\"{p}-timing\""))
            .unwrap();
        let finder = svg
            .find(&alloc::format!(
                "<path class=\"{p}-finder-outer\" fill=\"#17161c\" fill-rule=\"evenodd\""
            ))
            .unwrap();
        assert!(data < timing && timing < finder);
        assert!(svg.ends_with("</svg>"));
        assert!(!svg.contains('\n'));
        assert!(!svg.contains("<script") && !svg.contains("<foreignObject"));
        // Nothing outside the @supports block uses var() or oklch().
        let body = &svg[svg.find("</style>").unwrap()..];
        assert!(!body.contains("var(") && !body.contains("oklch"));
    }

    #[test]
    fn pretty_output_is_one_element_per_line() {
        let svg = Style::new().minify(false).render(&qr()).unwrap();
        let lines: Vec<&str> = svg.lines().collect();
        assert!(lines[0].starts_with("<svg "));
        assert!(
            lines[1..lines.len() - 1]
                .iter()
                .all(|l| l.starts_with("  <"))
        );
        assert_eq!(*lines.last().unwrap(), "</svg>");
    }

    #[test]
    fn text_is_escaped() {
        let svg = Style::new()
            .title("a<b>&\"c'\u{1}")
            .label("x\"onload=\"y")
            .render(&qr())
            .unwrap();
        assert!(svg.contains("<title>a&lt;b&gt;&amp;&quot;c&apos;</title>"));
        assert!(svg.contains("aria-label=\"x&quot;onload=&quot;y\""));
    }

    #[test]
    fn wide_gamut_values_stay_inside_supports() {
        let palette = Palette::from_tokens([("finder.top-right.eye", "oklch(50% .2 30)")]).unwrap();
        let svg = Style::new().palette(palette).render(&qr()).unwrap();
        let p = prefix_of(&svg);
        assert!(svg.contains("@supports (color:var(--a)) and (color:oklch(0% 0 0)){"));
        assert!(svg.contains(&alloc::format!(
            ".{p}-finder-top-right-eye{{fill:var(--qr-finder-top-right-eye,oklch(50% .2 30))}}"
        )));
        assert!(svg.contains(&alloc::format!(
            ".{p}-finder-top-left-eye{{fill:var(--qr-finder-top-left-eye,var(--qr-finder-eye,var(--qr-fg,#17161c)))}}"
        )));
        assert!(!svg.contains(&alloc::format!(".{p}-finder-eye{{")));
        let body = &svg[svg.find("</style>").unwrap()..];
        assert!(!body.contains("oklch"));
    }

    #[test]
    fn translucent_and_transparent() {
        let palette =
            Palette::from_tokens([("bg", "transparent"), ("fg", "rgb(0 0 0 / .8)")]).unwrap();
        let svg = Style::new().palette(palette).render(&qr()).unwrap();
        let p = prefix_of(&svg);
        assert!(!svg.contains("<rect"));
        assert!(!svg.contains("-bg{"));
        assert!(svg.contains(&alloc::format!(
            "class=\"{p}-fg\" fill=\"#000\" fill-opacity=\".8\""
        )));
        assert!(svg.contains(&alloc::format!(".{p}-fg{{fill:var(--qr-fg,#000)}}")));
    }

    #[test]
    fn gradients_share_the_prefix() {
        let palette =
            Palette::from_tokens([("fg", "linear-gradient(90deg, #102030, oklch(35% .1 300))")])
                .unwrap();
        let svg = Style::new().palette(palette.clone()).render(&qr()).unwrap();
        let p = prefix_of(&svg);
        assert!(svg.contains(&alloc::format!(
            "<linearGradient id=\"{p}-fg\" gradientUnits=\"userSpaceOnUse\" x1=\"4\" y1=\"14.5\" x2=\"25\" y2=\"14.5\">"
        )));
        assert!(svg.contains(&alloc::format!(
            ".{p}-fg-s1{{stop-color:oklch(35% .1 300)}}"
        )));
        assert!(svg.contains(&alloc::format!("<stop class=\"{p}-fg-s1\" offset=\"1\"")));
        // Finders fall back to fg, so they reuse its gradient.
        assert_eq!(svg.matches("<linearGradient").count(), 1);
        assert_eq!(svg.matches(&alloc::format!("url(#{p}-fg)")).count(), 4);
        let other = Style::new()
            .palette(palette.clone())
            .render(&QrCode::encode("WORLD", Ecc::M).unwrap())
            .unwrap();
        assert_ne!(prefix_of(&other), p);
        let fixed = Style::new()
            .palette(palette)
            .id_prefix("9 x!")
            .render(&qr())
            .unwrap();
        assert!(fixed.contains("id=\"qr9x-fg\""));
    }

    #[test]
    fn frames_change_the_background() {
        use crate::{Frame, FrameRing};
        let svg = |frame| Style::new().frame(frame).render(&qr()).unwrap();
        assert!(!svg(Frame::None).contains("<rect"));
        assert!(
            svg(Frame::RoundedSquare(2.5)).contains("<rect width=\"29\" height=\"29\" rx=\"2.5\"")
        );
        let round = svg(Frame::Circle {
            filler_density: 0.5,
            ring: FrameRing::Solid,
        });
        assert!(round.contains("<circle cx="));
        assert!(!round.contains("<rect"));
        let p = prefix_of(&round);
        assert!(round.contains(&alloc::format!("<path class=\"{p}-frame\"")));
        assert!(round.contains(&alloc::format!(
            "class=\"{p}-filler\" fill=\"#17161c\" fill-opacity=\".35\""
        )));
        // A round frame moves the code, so it gets its own prefix.
        assert_ne!(p, prefix_of(&svg(Frame::Square)));
        assert_eq!(
            prefix_of(&svg(Frame::Square)),
            prefix_of(&Style::new().render(&qr()).unwrap())
        );
    }

    #[test]
    fn logos_are_clipped_prefixed_and_change_the_prefix() {
        use crate::{Logo, LogoShape, Version};
        let qr = QrCode::builder()
            .ecc(Ecc::H)
            .version(Version::new(5).unwrap())
            .encode_text("HELLO")
            .unwrap();
        let logo = Logo::svg(r##"<svg viewBox="0 0 2 2"><clipPath id="c"><rect width="1" height="1"/></clipPath><rect clip-path="url(#c)" width="2" height="2"/></svg>"##)
            .unwrap()
            .scale(0.2);
        let square = Style::new().logo(logo.clone()).render(&qr).unwrap();
        let p = prefix_of(&square);
        assert!(square.contains(&alloc::format!("<clipPath id=\"{p}-logo-c\">")));
        assert!(square.contains(&alloc::format!("clip-path=\"url(#{p}-logo-c)\"")));
        assert!(!square.contains("-logo-clip"));
        assert!(square.contains(&alloc::format!(
            "<path class=\"{p}-logo-backdrop\" fill=\"#fefcf8\""
        )));
        let round = Style::new()
            .logo(logo.clone().shape(LogoShape::Circle).backdrop(false))
            .render(&qr)
            .unwrap();
        let q = prefix_of(&round);
        assert_ne!(p, q);
        assert!(round.contains(&alloc::format!(
            "<g clip-path=\"url(#{q}-logo-clip)\"><svg "
        )));
        assert!(!round.contains("logo-backdrop\" fill"));
        assert_ne!(p, prefix_of(&Style::new().render(&qr).unwrap()));
    }

    #[test]
    fn render_is_deterministic() {
        let style = Style::new().modules(ModuleShape::Dots { size: 0.9 });
        assert_eq!(style.render(&qr()).unwrap(), style.render(&qr()).unwrap());
    }
}
