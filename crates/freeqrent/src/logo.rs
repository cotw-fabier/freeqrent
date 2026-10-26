//! Logos in the middle of the code (SPEC §7.3): the logo value, its
//! placement, and the modules it covers.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::geometry::{Grid, Path, UNIT};
use crate::math;
use crate::matrix::{ModuleRole, QrMatrix};
use crate::sanitize::{self, SvgDoc, SvgProblem};
use crate::scene;
use crate::style::ModuleShape;

/// A raster image format a logo can be inlined as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RasterFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF.
    Gif,
    /// WebP.
    Webp,
}

impl RasterFormat {
    /// The MIME type, e.g. `image/png`.
    pub fn mime(self) -> &'static str {
        match self {
            RasterFormat::Png => "image/png",
            RasterFormat::Jpeg => "image/jpeg",
            RasterFormat::Gif => "image/gif",
            RasterFormat::Webp => "image/webp",
        }
    }

    /// The format whose signature `bytes` start with.
    pub fn sniff(bytes: &[u8]) -> Option<RasterFormat> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(RasterFormat::Png)
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(RasterFormat::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(RasterFormat::Gif)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(RasterFormat::Webp)
        } else {
            None
        }
    }
}

/// The outline of the logo's plate and of the modules cleared under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LogoShape {
    /// A square.
    Square,
    /// A square with corners of this radius, in modules.
    Rounded(f32),
    /// A circle.
    Circle,
}

/// How big the logo is (SPEC §7.3). The sizes are scales, the logo's
/// width as a fraction of the code's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogoSize {
    /// 0.14.
    Small,
    /// 0.2, the default.
    Medium,
    /// 0.26.
    Large,
    /// The largest scale, in hundredths up to 0.5, that the code's
    /// error-correction budget affords, and at least 0.26 (or the `scale`
    /// set after it). Fitting a code to the style asks for the minimum;
    /// the logo then grows into whatever the chosen code leaves.
    Largest,
}

impl LogoSize {
    /// Every size, smallest first.
    pub const ALL: [LogoSize; 4] = [
        LogoSize::Small,
        LogoSize::Medium,
        LogoSize::Large,
        LogoSize::Largest,
    ];

    /// The size's lowercase name, as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            LogoSize::Small => "small",
            LogoSize::Medium => "medium",
            LogoSize::Large => "large",
            LogoSize::Largest => "largest",
        }
    }
}

/// A border on the logo's plate (SPEC §7.3), painted in `logo.frame`. It
/// sits inside the plate, a quarter module in from its edge, and the plate
/// grows to hold it, so the error-correction budget counts it. Unless set,
/// its width grows with the logo: the default below for logos up to
/// [`FRAME_REFERENCE`] modules wide, in proportion past that.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogoFrame {
    /// A solid band along the plate's outline (default 0.8 modules wide).
    Ring,
    /// A ring of beads in the code's own module shape: dots for Dots,
    /// Liquid and Bars, squares for Square, rounded squares for Rounded
    /// (default 0.9 modules).
    Beads,
    /// A band with a scalloped outer edge, like a seal (default 0.9).
    Stamp,
    /// Four corner brackets, like a viewfinder (default 0.8). On a
    /// circle they are four arcs.
    Brackets,
    /// A circle inside the plate with a gap and a moon in it: the width
    /// is the circle's line (default 0.4), the moon is
    /// [`Logo::moon_size`] across (default 2.4). With
    /// [`Idle::LogoTurn`](crate::Idle::LogoTurn) the moon orbits.
    Orbit,
}

impl LogoFrame {
    /// Every frame.
    pub const ALL: [LogoFrame; 5] = [
        LogoFrame::Ring,
        LogoFrame::Beads,
        LogoFrame::Stamp,
        LogoFrame::Brackets,
        LogoFrame::Orbit,
    ];

    /// The frame's lowercase name, as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            LogoFrame::Ring => "ring",
            LogoFrame::Beads => "beads",
            LogoFrame::Stamp => "stamp",
            LogoFrame::Brackets => "brackets",
            LogoFrame::Orbit => "orbit",
        }
    }

    /// The width a frame gets unless set, in modules.
    pub fn default_width(self) -> f32 {
        match self {
            LogoFrame::Ring => 0.8,
            LogoFrame::Beads => 0.9,
            LogoFrame::Stamp => 0.9,
            LogoFrame::Brackets => 0.8,
            LogoFrame::Orbit => 0.4,
        }
    }
}

/// The logo width, in modules, up to which a frame keeps its default
/// width; wider logos get proportionally wider frames.
pub const FRAME_REFERENCE: f32 = 8.0;
/// The widest a frame or moon gets by default, in modules.
const FRAME_AUTO_MAX: f64 = 2.5;
/// An Orbit's moon unless set, in modules: three times the line it rode
/// before (W20261026105648).
pub const DEFAULT_MOON: f32 = 2.4;

/// Why a logo couldn't be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogoError {
    /// The text isn't an SVG document (no `<svg>` root).
    NotSvg,
    /// The SVG root has neither a viewBox nor a width and height.
    NoSize,
    /// The bytes don't start with the signature of this format.
    NotImage(RasterFormat),
}

impl fmt::Display for LogoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogoError::NotSvg => f.write_str("the logo isn't an SVG document"),
            LogoError::NoSize => {
                f.write_str("the logo SVG has neither a viewBox nor a width and height")
            }
            LogoError::NotImage(format) => {
                write!(f, "the logo bytes aren't {}", format.mime())
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LogoError {}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    Svg(SvgDoc),
    Raster { format: RasterFormat, data: Vec<u8> },
}

/// A logo for the middle of a code: clean content plus placement options.
/// Plain data, cheap to clone apart from the image itself.
///
/// ```
/// use freeqrent::{Ecc, Logo, LogoShape, QrCode, Style};
///
/// let logo = Logo::svg(r#"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5"/></svg>"#)
///     .unwrap()
///     .scale(0.2)
///     .shape(LogoShape::Circle);
/// let style = Style::new().logo(logo);
/// let qr = QrCode::builder().ecc(Ecc::Q).encode_text_for("https://example.com", &style).unwrap();
/// assert!(style.check(&qr).ok());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Logo {
    pub(crate) content: Content,
    pub(crate) scale: f32,
    pub(crate) shape: LogoShape,
    pub(crate) backdrop: bool,
    pub(crate) padding: f32,
    pub(crate) clear_modules: bool,
    /// [`LogoSize::Largest`]: grow past `scale` as far as the budget goes.
    pub(crate) grow: bool,
    pub(crate) frame: Option<LogoFrame>,
    pub(crate) frame_width: Option<f32>,
    pub(crate) moon: Option<f32>,
}

impl Logo {
    fn new(content: Content) -> Logo {
        Logo {
            content,
            scale: 0.2,
            shape: LogoShape::Square,
            backdrop: true,
            padding: 0.5,
            clear_modules: true,
            grow: false,
            frame: None,
            frame_width: None,
            moon: None,
        }
    }

    /// A vector logo. The document is sanitized now: only shapes, paint,
    /// gradients, clip paths, masks, text and references inside the logo
    /// survive (SPEC §7.3).
    pub fn svg(svg: &str) -> Result<Logo, LogoError> {
        let doc = sanitize::sanitize(svg).map_err(|p| match p {
            SvgProblem::NotSvg => LogoError::NotSvg,
            SvgProblem::NoSize => LogoError::NoSize,
        })?;
        Ok(Logo::new(Content::Svg(doc)))
    }

    /// A raster logo, inlined as a base64 `data:` URI. The bytes must
    /// start with `format`'s signature; they aren't decoded.
    pub fn raster(bytes: impl Into<Vec<u8>>, format: RasterFormat) -> Result<Logo, LogoError> {
        let data = bytes.into();
        if RasterFormat::sniff(&data) != Some(format) {
            return Err(LogoError::NotImage(format));
        }
        Ok(Logo::new(Content::Raster { format, data }))
    }

    /// The logo's width as a fraction of the code's (default 0.2). Larger
    /// logos cover more codewords; [`Style::check`](crate::Style::check)
    /// says how far it can go.
    pub fn scale(mut self, scale: f32) -> Logo {
        self.scale = scale;
        self
    }

    /// A named size (SPEC §7.3). `Small`, `Medium` and `Large` set the
    /// scale; `Largest` sets Large's as the minimum and grows the logo as
    /// far as the code's budget affords. A [`scale`](Logo::scale) after it
    /// changes the minimum.
    pub fn size(mut self, size: LogoSize) -> Logo {
        self.scale = match size {
            LogoSize::Small => 0.14,
            LogoSize::Medium => 0.2,
            LogoSize::Large | LogoSize::Largest => 0.26,
        };
        self.grow = size == LogoSize::Largest;
        self
    }

    /// A border on the plate (none by default).
    pub fn frame(mut self, frame: LogoFrame) -> Logo {
        self.frame = Some(frame);
        self
    }

    /// The frame's width in modules (0.1–2.5). Unless set it is
    /// [`LogoFrame::default_width`], grown in proportion for logos wider
    /// than [`FRAME_REFERENCE`] modules.
    pub fn frame_width(mut self, width: f32) -> Logo {
        self.frame_width = Some(width);
        self
    }

    /// The diameter of an [`Orbit`](LogoFrame::Orbit) frame's moon, in
    /// modules (0.4–4; default [`DEFAULT_MOON`], grown like the width).
    /// Other frames ignore it.
    pub fn moon_size(mut self, size: f32) -> Logo {
        self.moon = Some(size);
        self
    }

    /// The frame, if any.
    pub fn logo_frame(&self) -> Option<LogoFrame> {
        self.frame
    }

    /// The plate and excavation outline (default square).
    pub fn shape(mut self, shape: LogoShape) -> Logo {
        self.shape = shape;
        self
    }

    /// Draws a plate in the `logo.backdrop` token behind the logo
    /// (default on).
    pub fn backdrop(mut self, backdrop: bool) -> Logo {
        self.backdrop = backdrop;
        self
    }

    /// Space between the logo and the modules, in modules (default 0.5,
    /// at most 4).
    pub fn padding(mut self, padding: f32) -> Logo {
        self.padding = padding;
        self
    }

    /// Leaves out the modules under the plate (default on). Off, they are
    /// drawn and the logo covers them; the budget counts them either way.
    pub fn clear_modules(mut self, clear: bool) -> Logo {
        self.clear_modules = clear;
        self
    }

    /// The logo's width as a fraction of the code's (the minimum, with
    /// [`LogoSize::Largest`]; [`Style::check`](crate::Style::check) reports
    /// what it grew to).
    pub fn logo_scale(&self) -> f32 {
        self.scale
    }

    /// The solid colors a vector logo paints with (`fill`, `stroke` and
    /// gradient stops, after sanitizing), in document order, each once;
    /// empty for a raster. Palette extraction can keep the logo's frame
    /// apart from them ([`PaletteOpts::logo_colors`](crate::PaletteOpts::logo_colors)).
    pub fn colors(&self) -> Vec<crate::Color> {
        match &self.content {
            Content::Svg(doc) => doc.paints(),
            Content::Raster { .. } => Vec::new(),
        }
    }

    /// The sanitized document of a vector logo, standalone.
    pub fn sanitized_svg(&self) -> Option<String> {
        match &self.content {
            Content::Svg(doc) => Some(doc.to_document()),
            Content::Raster { .. } => None,
        }
    }
}

/// Where the plate sits, in modules from the code's top-left corner.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placement {
    /// The code's center, on both axes.
    pub(crate) center: f64,
    /// Half the plate's side.
    pub(crate) half: f64,
    /// The plate's corner radius.
    pub(crate) radius: f64,
    /// Half the side of the box the content is fitted into.
    pub(crate) content_half: f64,
    /// The band the frame takes, when there is one: its width, or an
    /// Orbit's moon when that is wider.
    pub(crate) frame: Option<f64>,
    /// The frame's own width: its line, for an Orbit.
    pub(crate) line: f64,
    /// An Orbit's moon diameter.
    pub(crate) moon: f64,
}

/// How far a frame sits in from the plate's edge, in modules, so a light
/// gap always parts it from the modules around the plate.
pub(crate) const FRAME_INSET: f64 = 0.25;

impl Logo {
    /// The frame's line width and moon diameter in modules, for a logo
    /// `side` modules wide, when there is a frame.
    pub(crate) fn frame_extent(&self, side: f64) -> Option<(f64, f64)> {
        let frame = self.frame?;
        let grow = (side / FRAME_REFERENCE as f64).max(1.0);
        let auto = |v: f32| (v as f64 * grow).min(FRAME_AUTO_MAX.max(v as f64));
        let line = self
            .frame_width
            .map_or_else(|| auto(frame.default_width()), |w| w as f64);
        let moon = match frame {
            LogoFrame::Orbit => self.moon.map_or_else(|| auto(DEFAULT_MOON), |m| m as f64),
            _ => 0.0,
        };
        Some((line, moon))
    }
}

pub(crate) fn placement(logo: &Logo, qr: &QrMatrix, scale: f32) -> Placement {
    let n = qr.size() as f64;
    let content_half = n * scale as f64 / 2.0;
    let (line, moon) = logo.frame_extent(2.0 * content_half).unwrap_or((0.0, 0.0));
    let frame = logo.frame.map(|_| line.max(moon));
    let half = content_half + logo.padding as f64 + frame.map_or(0.0, |w| w + FRAME_INSET);
    let radius = match logo.shape {
        LogoShape::Square => 0.0,
        LogoShape::Rounded(r) => (r as f64).clamp(0.0, half),
        LogoShape::Circle => half,
    };
    Placement {
        center: n / 2.0,
        half,
        radius,
        content_half,
        frame,
        line,
        moon,
    }
}

/// A frame drawn as a stroke along its path instead of filled: its width
/// in path units, the path's length in modules, and a dash pattern (empty
/// for a solid stroke) with its offset, in modules.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Outline {
    pub(crate) width: i64,
    pub(crate) length: f64,
    pub(crate) dashes: Vec<f64>,
    pub(crate) offset: f64,
}

/// A frame's drawing: a filled path, or an outline stroked along it.
#[derive(Clone, Debug)]
pub(crate) struct FrameShape {
    pub(crate) path: Path,
    pub(crate) outline: Option<Outline>,
}

/// The rounded square of half-side `h` and corner radius `r` (in path
/// units) as the frame paths walk it: from the end of the top-left corner,
/// clockwise. Returns its straight edge and corner arc lengths.
fn sides(h: i64, r: i64) -> (f64, f64) {
    (
        (2 * (h - r)) as f64,
        core::f64::consts::FRAC_PI_2 * r as f64,
    )
}

/// The point `s` path units along that walk, relative to the center.
fn along(h: i64, r: i64, s: f64) -> (f64, f64) {
    let (edge, arc) = sides(h, r);
    let side = edge + arc;
    let s = s - 4.0 * side * math::floor(s / (4.0 * side));
    let k = math::floor(s / side) as usize % 4;
    let t = s - k as f64 * side;
    let (h, r) = (h as f64, r as f64);
    let (x, y) = if t < edge || r == 0.0 {
        (-(h - r) + t.min(edge), -h)
    } else {
        let (sin, cos) = math::sin_cos(-core::f64::consts::FRAC_PI_2 + (t - edge) / r);
        (h - r + r * cos, -(h - r) + r * sin)
    };
    // A quarter turn clockwise per side (y points down).
    (0..k).fold((x, y), |(x, y), _| (-y, x))
}

/// The frame of `logo` placed at `p`, on a canvas whose code starts
/// `origin` modules in, drawn for a code of `shape` modules (SPEC §7.3).
pub(crate) fn frame_shape(
    logo: &Logo,
    p: &Placement,
    origin: f64,
    shape: &ModuleShape,
) -> Option<FrameShape> {
    let (frame, width) = (logo.frame?, p.frame?);
    let u = |v: f64| math::round(v * UNIT as f64) as i64;
    let c = u(origin + p.center);
    let outer = p.half - FRAME_INSET;
    let outer_r = (p.radius - FRAME_INSET).clamp(0.0, outer);
    // The band's middle line, where strokes and beads sit.
    let (h, r) = (u(outer - width / 2.0), u((outer_r - width / 2.0).max(0.0)));
    let (edge, arc) = sides(h, r);
    let length = 4.0 * (edge + arc);
    let w = u(p.line);
    let mut path = Path::new();
    let square = |path: &mut Path, h: i64, r: i64| {
        path.rounded_rect(c - h, c - h, 2 * h, 2 * h, [r; 4]);
    };
    // Points around the middle line, a multiple of four, starting on
    // the top-left corner's middle so the corners match.
    let ring = |spacing: f64| {
        let n = 4 * (math::round(length / (4.0 * spacing)) as usize).max(2);
        (0..n).map(move |i| {
            let (x, y) = along(h, r, -arc / 2.0 + i as f64 * length / n as f64);
            (c + math::round(x) as i64, c + math::round(y) as i64)
        })
    };
    let outline = match frame {
        LogoFrame::Ring => {
            square(&mut path, h, r);
            Some(Outline {
                width: w,
                length: length / UNIT as f64,
                dashes: Vec::new(),
                offset: 0.0,
            })
        }
        LogoFrame::Brackets => {
            square(&mut path, h, r);
            // Each corner's arc and a fifth of each edge beside it, at most
            // half of the side, so a circle gets four 45° arcs.
            let dash = (arc + 0.44 * edge).min((arc + edge) / 2.0);
            let m = |v: f64| v / UNIT as f64;
            Some(Outline {
                width: w,
                length: m(length),
                dashes: alloc::vec![m(dash), m(arc + edge - dash)],
                offset: m(arc / 2.0 + dash / 2.0),
            })
        }
        LogoFrame::Beads => {
            for (x, y) in ring(1.6 * w as f64) {
                match *shape {
                    ModuleShape::Square { .. } => path.rect(x - w / 2, y - w / 2, w, w),
                    ModuleShape::Rounded { radius } => {
                        let k = math::round(radius as f64 * w as f64) as i64;
                        path.rounded_rect(x - w / 2, y - w / 2, w, w, [k; 4]);
                    }
                    _ => path.circle(x, y, w / 2),
                }
            }
            None
        }
        LogoFrame::Stamp => {
            for (x, y) in ring(0.95 * w as f64) {
                path.circle(x, y, w / 2);
            }
            // A smooth inner edge: the band from the middle line in,
            // its hole drawn the other way round.
            square(&mut path, h, r);
            let (hi, ri) = (u(outer - width), u((outer_r - width).max(0.0)));
            path.rounded_rect_ccw(c - hi, c - hi, 2 * hi, 2 * hi, ri.min(hi));
            None
        }
        LogoFrame::Orbit => {
            // Inscribed in the plate whatever its shape, so it can turn;
            // the line and the moon share the band's middle.
            let radius = u(outer - width / 2.0) as f64;
            let thick = w as f64;
            let moon_d = u(p.moon);
            let at = -core::f64::consts::FRAC_PI_4;
            let gap = (moon_d as f64 * 0.85) / radius;
            let point = |angle: f64, rr: f64| {
                let (sin, cos) = math::sin_cos(angle);
                (
                    c + math::round(rr * cos) as i64,
                    c + math::round(rr * sin) as i64,
                )
            };
            if gap < core::f64::consts::FRAC_PI_2 {
                let (ro, ri) = (radius + thick / 2.0, radius - thick / 2.0);
                let start = point(at + gap, ro);
                path.move_to(start.0, start.1);
                let end = point(at - gap, ro);
                path.arc_to(math::round(ro) as i64, true, true, end.0, end.1);
                let inner = point(at - gap, ri);
                path.line_to(inner.0, inner.1);
                let back = point(at + gap, ri);
                path.arc_to(math::round(ri) as i64, true, false, back.0, back.1);
                path.close();
            }
            let moon = point(at, radius);
            path.circle(moon.0, moon.1, moon_d / 2);
            None
        }
    };
    Some(FrameShape { path, outline })
}

/// What a logo covers.
#[derive(Clone, Debug)]
pub(crate) struct Cover {
    /// Data, ECC and remainder modules under the plate.
    pub(crate) modules: Grid,
    /// Alignment patterns under the plate, left out whole.
    pub(crate) alignment: Vec<(u16, u16)>,
    /// False when the plate touches a finder, timing, format or version
    /// module, the dark module, or the bottom-right alignment pattern.
    pub(crate) valid: bool,
}

/// Whether the cell at `(x, y)` overlaps the plate's interior.
fn overlaps(p: &Placement, x: u16, y: u16) -> bool {
    const EPS: f64 = 1e-6;
    let d = |i: u16| (i as f64 + 0.5 - p.center).abs();
    let (dx, dy) = (d(x), d(y));
    if p.radius <= 0.0 {
        return dx < 0.5 + p.half - EPS && dy < 0.5 + p.half - EPS;
    }
    // A rounded square is its inner square grown by the radius.
    let inner = p.half - p.radius;
    let gx = (dx - 0.5 - inner).max(0.0);
    let gy = (dy - 0.5 - inner).max(0.0);
    gx * gx + gy * gy < p.radius * p.radius - EPS
}

pub(crate) fn cover(logo: &Logo, qr: &QrMatrix, scale: f32) -> Cover {
    let p = placement(logo, qr, scale);
    let size = qr.size();
    let mut modules = Grid::new(size as usize);
    let mut alignment = Vec::new();
    let mut valid = true;
    let bottom_right = scene::alignment_centers(qr)
        .into_iter()
        .max_by_key(|&(x, y)| x as u32 + y as u32);
    for (x, y, _, role) in qr.modules() {
        if !overlaps(&p, x, y) {
            continue;
        }
        match role {
            ModuleRole::Data | ModuleRole::Ecc | ModuleRole::Remainder => {
                modules.set(x as usize, y as usize)
            }
            ModuleRole::Alignment { center } if Some(center) != bottom_right => {
                if !alignment.contains(&center) {
                    alignment.push(center);
                }
            }
            _ => valid = false,
        }
    }
    Cover {
        modules,
        alignment,
        valid,
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub(crate) fn base64<W: fmt::Write + ?Sized>(w: &mut W, data: &[u8]) -> fmt::Result {
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                w.write_char(BASE64[(n >> (18 - 6 * i) & 63) as usize] as char)?;
            } else {
                w.write_char('=')?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, QrCode, Version};

    #[test]
    fn base64_matches_rfc_4648() {
        let enc = |s: &str| {
            let mut out = String::new();
            base64(&mut out, s.as_bytes()).unwrap();
            out
        };
        assert_eq!(enc(""), "");
        assert_eq!(enc("f"), "Zg==");
        assert_eq!(enc("fo"), "Zm8=");
        assert_eq!(enc("foo"), "Zm9v");
        assert_eq!(enc("foob"), "Zm9vYg==");
        assert_eq!(enc("fooba"), "Zm9vYmE=");
        assert_eq!(enc("foobar"), "Zm9vYmFy");
    }

    #[test]
    fn rasters_are_signature_checked() {
        let png = b"\x89PNG\r\n\x1a\n rest";
        assert!(Logo::raster(&png[..], RasterFormat::Png).is_ok());
        assert_eq!(
            Logo::raster(&png[..], RasterFormat::Jpeg),
            Err(LogoError::NotImage(RasterFormat::Jpeg))
        );
        assert_eq!(
            RasterFormat::sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(RasterFormat::Webp)
        );
        assert_eq!(RasterFormat::sniff(b"<svg"), None);
    }

    fn logo() -> Logo {
        Logo::svg(r#"<svg viewBox="0 0 1 1"/>"#).unwrap()
    }

    #[test]
    fn covers_whole_modules_under_the_plate() {
        // Version 5: 37 modules, center 18.5. Scale 0.2 + padding 0.5 is
        // a plate 8.4 wide: it reaches x 14.3–22.7, so cells 14–22.
        let qr = QrCode::builder()
            .version(Version::new(5).unwrap())
            .encode_text("HELLO")
            .unwrap();
        let c = cover(&logo(), &qr, 0.2);
        assert!(c.valid);
        let covered: Vec<u16> = (0..37).filter(|&x| c.modules.get(x as i64, 18)).collect();
        assert_eq!(covered, (14..=22).collect::<Vec<_>>());
        let circle = cover(&logo().shape(LogoShape::Circle), &qr, 0.2);
        assert!(circle.modules.get(18, 18) && !circle.modules.get(14, 14));
    }

    #[test]
    fn function_patterns_stop_the_logo() {
        // Version 1: 21 modules, with format information in row and
        // column 8 and the dark module at (8, 13). A plate 2.68 wide covers
        // cells 9–11; 5.2 wide reaches 7–13.
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        assert!(cover(&logo(), &qr, 0.08).valid);
        assert!(!cover(&logo(), &qr, 0.2).valid);
    }

    #[test]
    fn vector_logos_name_their_colors() {
        let logo = Logo::svg(
            r##"<svg viewBox="0 0 10 10"><defs><linearGradient id="g"><stop offset="0" stop-color="#123456"/></linearGradient></defs><rect fill="#c2410c" stroke="none" width="5" height="5"/><rect fill="url(#g)" width="1" height="1"/><path fill="#c2410c" stroke="teal" d="M0 0h1v1z"/></svg>"##,
        )
        .unwrap();
        let parse = |s: &str| crate::Color::parse(s).unwrap();
        assert_eq!(
            logo.colors(),
            [parse("#123456"), parse("#c2410c"), parse("teal")]
        );
        let raster = Logo::raster(b"\x89PNG\r\n\x1a\n".to_vec(), RasterFormat::Png).unwrap();
        assert!(raster.colors().is_empty());
    }

    #[test]
    fn sizes_set_the_scale() {
        assert_eq!(logo().size(LogoSize::Small).scale, 0.14);
        assert_eq!(logo().size(LogoSize::Large).scale, 0.26);
        let largest = logo().size(LogoSize::Largest);
        assert!(largest.grow && largest.scale == 0.26);
        assert!(!largest.size(LogoSize::Medium).grow);
    }

    #[test]
    fn frames_grow_the_plate_and_stay_inside_it() {
        let qr = QrCode::builder()
            .version(Version::new(7).unwrap())
            .encode_text("HELLO")
            .unwrap();
        let plain = placement(&logo(), &qr, 0.2);
        for frame in LogoFrame::ALL {
            let l = logo().frame(frame);
            let p = placement(&l, &qr, 0.2);
            // A version 7 logo at 0.2 is 9 modules wide: the defaults
            // grow by 9/8.
            let grow = 2.0 * p.content_half / FRAME_REFERENCE as f64;
            assert!((grow - 1.125).abs() < 1e-6);
            // Up to the cap: an Orbit's moon (2.4 × 1.125) reaches it.
            let w = match frame {
                LogoFrame::Orbit => DEFAULT_MOON as f64 * grow,
                _ => frame.default_width() as f64 * grow,
            }
            .min(FRAME_AUTO_MAX);
            assert!(
                (p.half - plain.half - w - FRAME_INSET).abs() < 1e-6,
                "{frame:?}"
            );
            let shape = frame_shape(&l, &p, 4.0, &ModuleShape::SQUARE).unwrap();
            assert!(!shape.path.is_empty(), "{frame:?}");
            if let Some(o) = shape.outline {
                // A stroke on the band's middle line, w wide, ends at the
                // inset: a rounded square's perimeter at that line.
                let side = 2.0 * (p.half - FRAME_INSET - w / 2.0);
                assert!(
                    (o.length - 4.0 * side).abs() < 0.01,
                    "{frame:?} {}",
                    o.length
                );
                if !o.dashes.is_empty() {
                    // Four corners' worth of dashes fill the outline.
                    let period: f64 = o.dashes.iter().sum();
                    assert!((4.0 * period - o.length).abs() < 0.01, "{frame:?}");
                }
            }
        }
    }

    #[test]
    fn set_widths_and_moons_win() {
        let qr = QrCode::builder()
            .version(Version::new(7).unwrap())
            .encode_text("HELLO")
            .unwrap();
        let plain = placement(&logo(), &qr, 0.2);
        let orbit = logo().frame(LogoFrame::Orbit).frame_width(0.3);
        let p = placement(&orbit.clone().moon_size(1.0), &qr, 0.2);
        assert_eq!(
            (p.line as f32, p.moon as f32, p.frame.map(|f| f as f32)),
            (0.3, 1.0, Some(1.0))
        );
        // A thin moon leaves the line the band.
        let p = placement(&orbit.frame_width(1.2).moon_size(0.6), &qr, 0.2);
        assert_eq!(p.frame.map(|f| f as f32), Some(1.2));
        assert!((p.half - plain.half - 1.2 - FRAME_INSET).abs() < 1e-6);
        // Small logos keep the defaults.
        let small = placement(&logo().frame(LogoFrame::Ring), &qr, 0.1);
        assert_eq!(small.line, LogoFrame::Ring.default_width() as f64);
    }

    #[test]
    fn inner_alignment_patterns_are_removed_whole() {
        // Version 7 has an alignment pattern in the middle (22, 22) and the
        // bottom-right one at (38, 38).
        let qr = QrCode::builder()
            .version(Version::new(7).unwrap())
            .encode_text("HELLO")
            .unwrap();
        let c = cover(&logo(), &qr, 0.1);
        assert!(c.valid);
        assert_eq!(c.alignment, [(22, 22)]);
    }
}
