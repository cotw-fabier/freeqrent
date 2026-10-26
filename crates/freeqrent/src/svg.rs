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
use crate::scene::{self, Canvas, Part, Round};
use crate::style::{Frame, Style};

/// The paths to draw, one per (token, fill rule), in paint order.
fn build_paths(style: &Style, qr: &QrMatrix, canvas: &Canvas) -> Vec<(Token, bool, Path)> {
    let origin = canvas.origin * UNIT;
    let mut paths: Vec<(Token, bool, Path)> = Vec::new();
    for layer in scene::layers(style, qr) {
        let evenodd = matches!(
            layer.part,
            Part::FinderOuter(_) | Part::Alignment(_) | Part::Ring
        );
        let i = match paths
            .iter()
            .position(|(t, e, _)| *t == layer.token && *e == evenodd)
        {
            Some(i) => i,
            None => {
                paths.push((layer.token, evenodd, Path::new()));
                paths.len() - 1
            }
        };
        let path = &mut paths[i].2;
        // Center of a pattern whose top-left module is `(x, y)` and side `n`.
        let center = |(x, y): (i64, i64), n: i64| {
            (
                origin + x * UNIT + n * UNIT / 2,
                origin + y * UNIT + n * UNIT / 2,
            )
        };
        match &layer.part {
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
    paths.retain(|(_, _, p)| !p.is_empty());
    paths
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

pub(crate) fn write<W: Write + ?Sized>(style: &Style, qr: &QrMatrix, w: &mut W) -> fmt::Result {
    let palette = &style.palette;
    let canvas = scene::canvas(style, qr);
    let paths = build_paths(style, qr, &canvas);
    let n = canvas.side;
    let prefix = id_prefix(style, qr);
    let draw_bg = palette.background().alpha() > 0.0 && style.frame != Frame::None;

    // Solid tokens that get a class, and the gradients to define.
    let mut solids: Vec<Token> = Vec::new();
    let mut gradients: Vec<Token> = Vec::new();
    let backdrop = style.logo.as_ref().is_some_and(|l| l.backdrop);
    let painted = draw_bg
        .then_some(Token::Bg)
        .into_iter()
        .chain(paths.iter().map(|(t, _, _)| *t))
        .chain(backdrop.then_some(Token::LogoBackdrop));
    for token in painted {
        match palette.resolve(token) {
            Paint::Solid(_) => solids.push(token),
            Paint::Gradient(_) => {
                let source = paint_source(palette, token);
                if !gradients.contains(&source) {
                    gradients.push(source);
                }
            }
        }
    }

    let mut out = Out {
        w,
        pretty: !style.minify,
    };
    out.w
        .write_str("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 ")?;
    write!(out.w, "{n} {n}\" class=\"qr\" role=\"img\" aria-label=\"")?;
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
    write_style(out.w, palette, &prefix, &solids, &gradients)?;

    if !gradients.is_empty() {
        out.line()?;
        out.w.write_str("<defs>")?;
        for &source in &gradients {
            let Some(Paint::Gradient(g)) = palette.get(source) else {
                unreachable!("gradient sources hold gradients");
            };
            out.nested_line()?;
            write_gradient(out.w, g, &prefix, source, canvas.origin, qr.size())?;
        }
        out.line()?;
        out.w.write_str("</defs>")?;
    }

    if draw_bg {
        out.line()?;
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
        write_paint(out.w, palette, Token::Bg, &prefix)?;
        out.w.write_str("/>")?;
    }
    for (token, evenodd, path) in &paths {
        out.line()?;
        out.w.write_str("<path")?;
        write_paint(out.w, palette, *token, &prefix)?;
        if *evenodd {
            out.w.write_str(" fill-rule=\"evenodd\"")?;
        }
        write!(out.w, " d=\"{}\"/>", path.data())?;
    }
    write_logo(&mut out, style, qr, &canvas, &prefix)?;
    if out.pretty {
        out.w.write_char('\n')?;
    }
    out.w.write_str("</svg>")?;
    if out.pretty {
        out.w.write_char('\n')?;
    }
    Ok(())
}

/// The logo (SPEC §7.3): a clip path for round plates, the plate, then
/// the content fitted into its box and clipped to the plate, so nothing
/// paints outside the modules the budget counted.
fn write_logo<W: Write + ?Sized>(
    out: &mut Out<'_, W>,
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    prefix: &str,
) -> fmt::Result {
    let Some(logo) = &style.logo else {
        return Ok(());
    };
    let p = logo::placement(logo, qr, logo.scale);
    let o = canvas.origin as f64;
    let u = |v: f64| math::round(v * UNIT as f64) as i64;
    let mut plate = Path::new();
    let (at, side) = (u(o + p.center - p.half), u(2.0 * p.half));
    plate.rounded_rect(at, at, side, side, [u(p.radius); 4]);
    let clip = p.radius > 0.0;
    if clip {
        out.line()?;
        write!(
            out.w,
            "<clipPath id=\"{prefix}-logo-clip\"><path d=\"{}\"/></clipPath>",
            plate.data()
        )?;
    }
    if logo.backdrop {
        out.line()?;
        out.w.write_str("<path")?;
        write_paint(out.w, &style.palette, Token::LogoBackdrop, prefix)?;
        write!(out.w, " d=\"{}\"/>", plate.data())?;
    }
    out.line()?;
    if clip {
        write!(out.w, "<g clip-path=\"url(#{prefix}-logo-clip)\">")?;
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
            box_attrs(out.w)?;
            out.w.write_str(" viewBox=\"")?;
            doc.write_view_box(out.w)?;
            out.w.write_str("\">")?;
            doc.write_body(out.w, Some(prefix))?;
            out.w.write_str("</svg>")?;
        }
        Content::Raster { format, data } => {
            out.w.write_str("<image")?;
            box_attrs(out.w)?;
            write!(out.w, " href=\"data:{};base64,", format.mime())?;
            logo::base64(out.w, data)?;
            out.w.write_str("\"/>")?;
        }
    }
    if clip {
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
    solids: &[Token],
    gradients: &[Token],
) -> fmt::Result {
    let wide = |c: Color| c.opaque_srgb() != c.opaque();
    let mut needs_oklch = solids
        .iter()
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
    for &token in solids {
        write!(w, ".{prefix}-")?;
        write_dashed(w, token.name())?;
        w.write_str("{fill:")?;
        write_var_chain(w, palette, token)?;
        w.write_char('}')?;
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
    w.write_str("}</style>")
}

/// The paint attributes of an element: a class and hex fill for solid
/// colors, or a gradient reference.
fn write_paint<W: Write + ?Sized>(
    w: &mut W,
    palette: &Palette,
    token: Token,
    prefix: &str,
) -> fmt::Result {
    match palette.resolve(token) {
        Paint::Solid(color) => {
            write!(w, " class=\"{prefix}-")?;
            write_dashed(w, token.name())?;
            w.write_str("\" fill=\"")?;
            write_hex(w, color)?;
            w.write_char('"')?;
            write_opacity(w, "fill-opacity", color)
        }
        Paint::Gradient(_) => {
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

fn write_gradient<W: Write + ?Sized>(
    w: &mut W,
    gradient: &Gradient,
    prefix: &str,
    source: Token,
    origin: i64,
    size: u16,
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
        text.bytes().for_each(&mut eat);
        match &logo.content {
            Content::Svg(doc) => doc.to_document().bytes().for_each(&mut eat),
            Content::Raster { data, .. } => data.iter().copied().for_each(&mut eat),
        }
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
