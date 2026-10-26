//! Decodes PNG and JPEG files into the core's [`Rgba8Image`] (SPEC §2,
//! §6.4), so `freeqrent` itself stays free of dependencies. Pixels are
//! taken as sRGB: gamma chunks and ICC profiles are ignored, and 16-bit
//! PNGs are reduced to 8 bits. Animated PNGs give their first frame.
//!
//! ```no_run
//! use freeqrent::{Palette, PaletteOpts};
//!
//! let bytes = std::fs::read("logo.png").unwrap();
//! let image = freeqrent_image::decode(&bytes).unwrap();
//! let (palette, report) = Palette::from_image(&image, PaletteOpts::new()).unwrap();
//! ```

use std::fmt;
#[cfg(any(feature = "png", feature = "jpeg"))]
use std::io::Cursor;

pub use freeqrent::{RasterFormat, Rgba8Image};
#[cfg(any(feature = "png", feature = "jpeg"))]
use zune_core::colorspace::ColorSpace;
#[cfg(any(feature = "png", feature = "jpeg"))]
use zune_core::options::DecoderOptions;

/// The largest image [`decode`] accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Widest, in pixels.
    pub max_width: u32,
    /// Tallest, in pixels.
    pub max_height: u32,
    /// Most pixels in all (each costs four bytes once decoded).
    pub max_pixels: u64,
}

impl Default for Limits {
    /// 16,384 a side and 40 megapixels (160 MB decoded).
    fn default() -> Limits {
        Limits {
            max_width: 16_384,
            max_height: 16_384,
            max_pixels: 40_000_000,
        }
    }
}

/// Why an image couldn't be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The bytes don't start with a PNG or JPEG signature.
    UnknownFormat,
    /// A format this crate reads, but whose feature isn't enabled, or one
    /// it doesn't read (GIF, WebP).
    Unsupported(RasterFormat),
    /// The image is larger than the [`Limits`].
    TooLarge {
        /// Width in pixels.
        width: u64,
        /// Height in pixels.
        height: u64,
    },
    /// The decoder rejected the data.
    Malformed(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnknownFormat => f.write_str("not a PNG or JPEG file"),
            DecodeError::Unsupported(format) => {
                write!(f, "{} images aren't supported here", format.mime())
            }
            DecodeError::TooLarge { width, height } => {
                write!(f, "the image is too large ({width}×{height})")
            }
            DecodeError::Malformed(why) => write!(f, "malformed image: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decodes a PNG or JPEG, recognized by its signature, within the default
/// [`Limits`].
pub fn decode(bytes: &[u8]) -> Result<Rgba8Image, DecodeError> {
    decode_with(bytes, Limits::default())
}

/// [`decode`] with chosen limits.
pub fn decode_with(bytes: &[u8], limits: Limits) -> Result<Rgba8Image, DecodeError> {
    let _ = limits; // unused when neither format is enabled
    match RasterFormat::sniff(bytes) {
        #[cfg(feature = "png")]
        Some(RasterFormat::Png) => png(bytes, limits),
        #[cfg(feature = "jpeg")]
        Some(RasterFormat::Jpeg) => jpeg(bytes, limits),
        Some(format) => Err(DecodeError::Unsupported(format)),
        None => Err(DecodeError::UnknownFormat),
    }
}

#[cfg(any(feature = "png", feature = "jpeg"))]
fn check(width: usize, height: usize, limits: Limits) -> Result<(u32, u32), DecodeError> {
    let (w, h) = (width as u64, height as u64);
    if w > limits.max_width as u64 || h > limits.max_height as u64 || w * h > limits.max_pixels {
        return Err(DecodeError::TooLarge {
            width: w,
            height: h,
        });
    }
    Ok((width as u32, height as u32))
}

#[cfg(any(feature = "png", feature = "jpeg"))]
/// The decoders' own size caps are lifted; [`check`] applies the limits
/// once the headers are read, before any pixels are allocated, so a
/// refusal is always `TooLarge`.
fn options() -> DecoderOptions {
    DecoderOptions::default()
        .set_max_width(u32::MAX as usize)
        .set_max_height(u32::MAX as usize)
}

/// Expands decoded samples in `space` to RGBA.
#[cfg(any(feature = "png", feature = "jpeg"))]
fn to_rgba(
    space: ColorSpace,
    data: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<Rgba8Image, DecodeError> {
    let rgba = match space {
        ColorSpace::RGBA => data,
        ColorSpace::RGB => data
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        ColorSpace::LumaA => data
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        ColorSpace::Luma => data.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        other => {
            return Err(DecodeError::Malformed(format!(
                "unexpected color space {other:?}"
            )));
        }
    };
    Rgba8Image::new(width, height, rgba).map_err(|e| DecodeError::Malformed(e.to_string()))
}

#[cfg(feature = "png")]
fn png(bytes: &[u8], limits: Limits) -> Result<Rgba8Image, DecodeError> {
    use zune_png::PngDecoder;
    let opts = options()
        .png_set_strip_to_8bit(true)
        .png_set_add_alpha_channel(true);
    let mut decoder = PngDecoder::new_with_options(Cursor::new(bytes), opts);
    let malformed = |e: zune_png::error::PngDecodeErrors| DecodeError::Malformed(format!("{e:?}"));
    decoder.decode_headers().map_err(malformed)?;
    let (w, h) = decoder.dimensions().expect("headers decoded");
    let (width, height) = check(w, h, limits)?;
    let space = decoder.colorspace().expect("headers decoded");
    let data = decoder
        .decode()
        .map_err(malformed)?
        .u8()
        .ok_or_else(|| DecodeError::Malformed(String::from("samples wider than 8 bits")))?;
    to_rgba(space, data, width, height)
}

#[cfg(feature = "jpeg")]
fn jpeg(bytes: &[u8], limits: Limits) -> Result<Rgba8Image, DecodeError> {
    use zune_jpeg::JpegDecoder;
    let opts = options().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(Cursor::new(bytes), opts);
    let malformed = |e: zune_jpeg::errors::DecodeErrors| DecodeError::Malformed(format!("{e:?}"));
    decoder.decode_headers().map_err(malformed)?;
    let info = decoder.info().expect("headers decoded");
    let (width, height) = check(info.width as usize, info.height as usize, limits)?;
    let space = decoder.output_colorspace().expect("headers decoded");
    let data = decoder.decode().map_err(malformed)?;
    to_rgba(space, data, width, height)
}
