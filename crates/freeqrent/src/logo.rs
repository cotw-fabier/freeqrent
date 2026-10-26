//! Logos in the middle of the code (SPEC §7.3): the logo value, its
//! placement, and the modules it covers.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::geometry::Grid;
use crate::matrix::{ModuleRole, QrMatrix};
use crate::sanitize::{self, SvgDoc, SvgProblem};
use crate::scene;

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

    /// The logo's width as a fraction of the code's.
    pub fn logo_scale(&self) -> f32 {
        self.scale
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
}

pub(crate) fn placement(logo: &Logo, qr: &QrMatrix, scale: f32) -> Placement {
    let n = qr.size() as f64;
    let content_half = n * scale as f64 / 2.0;
    let half = content_half + logo.padding as f64;
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
    }
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
