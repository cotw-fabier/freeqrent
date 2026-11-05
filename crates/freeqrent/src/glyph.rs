//! Custom shapes (SPEC §7.1, §7.2): a picture drawn in place of the data
//! modules, a finder's ring or its eye. The picture is fitted (`meet`)
//! into the shape's box: one module, the ring's 7×7 or the eye's 3×3. By
//! default only its alpha counts, painted in the layer's token, so colors,
//! gradients and palette shifts apply as to any shape; with
//! [`Glyph::keep_colors`] it is drawn as it is.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::logo::{Content, RasterFormat};
use crate::matrix::Corner;
use crate::sanitize::{self, SvgProblem};

/// The largest clean SVG a glyph may be, in bytes: it is written once,
/// but referenced from every module.
pub const MAX_GLYPH_SVG: usize = 64 * 1024;
/// The largest raster a glyph may be, in bytes.
pub const MAX_GLYPH_RASTER: usize = 256 * 1024;

/// A custom shape: a vector or raster picture, validated when it's built
/// so [`Style`](crate::Style) stays infallible plain data.
///
/// ```
/// use freeqrent::{Glyph, QrCode, Style};
///
/// let heart = Glyph::svg(
///     r#"<svg viewBox="0 0 10 10"><path d="M5 9 1 5a2.4 2.4 0 0 1 4-3 2.4 2.4 0 0 1 4 3z"/></svg>"#,
/// )
/// .unwrap();
/// let style = Style::new().module_glyph(heart);
/// let qr = QrCode::builder().encode_text("https://example.com").unwrap();
/// assert!(style.check(&qr).ok());
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub(crate) content: Content,
    pub(crate) keep_colors: bool,
}

/// Why a glyph couldn't be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GlyphError {
    /// The text isn't an SVG document (no `<svg>` root).
    NotSvg,
    /// The SVG root has neither a viewBox nor a width and height.
    NoSize,
    /// The bytes don't start with the signature of this format.
    NotImage(RasterFormat),
    /// The format has no transparency (JPEG), so the shape would be a
    /// solid square.
    NoAlpha(RasterFormat),
    /// The clean SVG or the raster is larger than [`MAX_GLYPH_SVG`] or
    /// [`MAX_GLYPH_RASTER`] bytes.
    TooLarge {
        /// Its size.
        bytes: usize,
        /// The limit.
        max: usize,
    },
}

impl fmt::Display for GlyphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GlyphError::NotSvg => f.write_str("the shape isn't an SVG document"),
            GlyphError::NoSize => {
                f.write_str("the shape SVG has neither a viewBox nor a width and height")
            }
            GlyphError::NotImage(format) => write!(f, "the shape bytes aren't {}", format.mime()),
            GlyphError::NoAlpha(format) => write!(
                f,
                "{} has no transparency; use PNG, GIF, WebP or SVG for a shape",
                format.mime()
            ),
            GlyphError::TooLarge { bytes, max } => {
                write!(f, "the shape is {bytes} bytes; at most {max} fit")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for GlyphError {}

impl Glyph {
    /// A vector shape. The document is sanitized as a logo is (SPEC §7.3).
    pub fn svg(svg: &str) -> Result<Glyph, GlyphError> {
        let doc = sanitize::sanitize(svg).map_err(|p| match p {
            SvgProblem::NotSvg => GlyphError::NotSvg,
            SvgProblem::NoSize => GlyphError::NoSize,
        })?;
        let bytes = doc.to_document().len();
        if bytes > MAX_GLYPH_SVG {
            return Err(GlyphError::TooLarge {
                bytes,
                max: MAX_GLYPH_SVG,
            });
        }
        Ok(Glyph {
            content: Content::Svg(doc),
            keep_colors: false,
        })
    }

    /// A raster shape with transparency (PNG, GIF or WebP), inlined once
    /// as a base64 `data:` URI. The bytes must start with `format`'s
    /// signature; they aren't decoded.
    pub fn raster(bytes: impl Into<Vec<u8>>, format: RasterFormat) -> Result<Glyph, GlyphError> {
        let data: Vec<u8> = bytes.into();
        if RasterFormat::sniff(&data) != Some(format) {
            return Err(GlyphError::NotImage(format));
        }
        if format == RasterFormat::Jpeg {
            return Err(GlyphError::NoAlpha(format));
        }
        if data.len() > MAX_GLYPH_RASTER {
            return Err(GlyphError::TooLarge {
                bytes: data.len(),
                max: MAX_GLYPH_RASTER,
            });
        }
        Ok(Glyph {
            content: Content::Raster { format, data },
            keep_colors: false,
        })
    }

    /// Draws the picture in its own colors instead of as a silhouette in
    /// the layer's token. Its contrast can't be checked, so the report
    /// warns ([`Warning::GlyphColors`](crate::Warning::GlyphColors)).
    pub fn keep_colors(mut self, keep: bool) -> Glyph {
        self.keep_colors = keep;
        self
    }

    /// Whether the picture keeps its own colors.
    pub fn keeps_colors(&self) -> bool {
        self.keep_colors
    }

    /// The clean SVG document, for a vector glyph.
    pub fn sanitized_svg(&self) -> Option<String> {
        match &self.content {
            Content::Svg(doc) => Some(doc.to_document()),
            Content::Raster { .. } => None,
        }
    }

    /// The raster bytes and their format, for a raster glyph.
    pub fn raster_data(&self) -> Option<(&[u8], RasterFormat)> {
        match &self.content {
            Content::Raster { format, data } => Some((data, *format)),
            Content::Svg(_) => None,
        }
    }
}

/// Which part of the code a glyph draws (SPEC §8.5: plan layers name it,
/// and players draw the style's glyph for it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GlyphPart {
    /// Data modules, plain timing and round-frame filler.
    Modules,
    /// A finder's 7×7 ring.
    Ring,
    /// A finder's 3×3 eye.
    Eye,
}

impl GlyphPart {
    /// The part's name, as plans spell it.
    pub fn name(self) -> &'static str {
        match self {
            GlyphPart::Modules => "modules",
            GlyphPart::Ring => "ring",
            GlyphPart::Eye => "eye",
        }
    }

    /// The side of the part's box, in modules.
    pub fn side(self) -> u8 {
        match self {
            GlyphPart::Modules => 1,
            GlyphPart::Ring => 7,
            GlyphPart::Eye => 3,
        }
    }
}

/// The style's glyphs: one for the modules, and a ring and an eye per
/// finder corner (top-left, top-right, bottom-left).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Glyphs {
    pub(crate) modules: Option<Glyph>,
    pub(crate) ring: [Option<Glyph>; 3],
    pub(crate) eye: [Option<Glyph>; 3],
}

pub(crate) fn corner_index(corner: Corner) -> usize {
    match corner {
        Corner::TopLeft => 0,
        Corner::TopRight => 1,
        Corner::BottomLeft => 2,
    }
}

impl Glyphs {
    /// Whether any part is drawn by a glyph.
    pub(crate) fn any(&self) -> bool {
        self.modules.is_some() || self.any_finder()
    }

    /// Whether any finder ring or eye is drawn by a glyph.
    pub(crate) fn any_finder(&self) -> bool {
        self.ring.iter().chain(&self.eye).any(Option::is_some)
    }

    /// The glyph drawing `part` (at `corner`, for a finder part).
    pub(crate) fn get(&self, part: GlyphPart, corner: Option<Corner>) -> Option<&Glyph> {
        match (part, corner) {
            (GlyphPart::Modules, _) => self.modules.as_ref(),
            (GlyphPart::Ring, Some(c)) => self.ring[corner_index(c)].as_ref(),
            (GlyphPart::Eye, Some(c)) => self.eye[corner_index(c)].as_ref(),
            _ => None,
        }
    }

    /// Every glyph once, in a fixed order (modules, rings, eyes; corners
    /// top-left, top-right, bottom-left): what the document defines.
    pub(crate) fn distinct(&self) -> Vec<&Glyph> {
        let mut out: Vec<&Glyph> = Vec::new();
        let all = core::iter::once(&self.modules)
            .chain(&self.ring)
            .chain(&self.eye)
            .flatten();
        for g in all {
            if !out.iter().any(|o| o.content == g.content) {
                out.push(g);
            }
        }
        out
    }

    /// The index in [`Glyphs::distinct`] of the glyph drawing `part`.
    pub(crate) fn def(&self, part: GlyphPart, corner: Option<Corner>) -> Option<usize> {
        let g = self.get(part, corner)?;
        self.distinct().iter().position(|o| o.content == g.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEART: &str = r#"<svg viewBox="0 0 10 10"><path d="M5 9 1 5a2.4 2.4 0 0 1 4-3 2.4 2.4 0 0 1 4 3z"/></svg>"#;
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn builds_and_refuses() {
        assert!(Glyph::svg(HEART).is_ok());
        assert_eq!(Glyph::svg("<p/>"), Err(GlyphError::NotSvg));
        assert_eq!(Glyph::svg("<svg/>"), Err(GlyphError::NoSize));
        assert!(Glyph::raster(PNG, RasterFormat::Png).is_ok());
        assert_eq!(
            Glyph::raster(PNG, RasterFormat::Gif),
            Err(GlyphError::NotImage(RasterFormat::Gif))
        );
        assert_eq!(
            Glyph::raster([0xff, 0xd8, 0xff, 0], RasterFormat::Jpeg),
            Err(GlyphError::NoAlpha(RasterFormat::Jpeg))
        );
        let mut big = PNG.to_vec();
        big.resize(MAX_GLYPH_RASTER + 1, 0);
        assert!(matches!(
            Glyph::raster(big, RasterFormat::Png),
            Err(GlyphError::TooLarge { .. })
        ));
    }

    #[test]
    fn distinct_glyphs_share_a_definition() {
        let heart = Glyph::svg(HEART).unwrap();
        let dot = Glyph::raster(PNG, RasterFormat::Png).unwrap();
        let g = Glyphs {
            modules: Some(heart.clone()),
            ring: [None, Some(dot.clone()), Some(dot.clone().keep_colors(true))],
            eye: [Some(heart), None, None],
        };
        assert_eq!(g.distinct().len(), 2);
        assert_eq!(g.def(GlyphPart::Modules, None), Some(0));
        assert_eq!(g.def(GlyphPart::Eye, Some(Corner::TopLeft)), Some(0));
        assert_eq!(g.def(GlyphPart::Ring, Some(Corner::BottomLeft)), Some(1));
        assert_eq!(g.def(GlyphPart::Ring, Some(Corner::TopLeft)), None);
    }
}
