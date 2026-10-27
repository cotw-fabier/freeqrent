//! Style documents for freeQRent (SPEC §10.1): the JSON boundary every
//! language binding shares. A binding sends a payload and a Style document
//! and gets SVG, a scan report, an animation plan or a palette back, as
//! text, or a structured error `{code, message, detail}`. New styles and
//! presets reach every language through the document, with no binding
//! changes.
//!
//! ```
//! use freeqrent_doc::{Payload, render};
//!
//! let svg = render(
//!     Payload::Text("https://example.com"),
//!     r#"{"preset":"ink","modules":{"kind":"dots","size":0.9},"intro":"ripple"}"#,
//! )?;
//! assert!(svg.starts_with("<svg"));
//!
//! let err = render(Payload::Text("hi"), r##"{"palette":{"fg":"#eee"}}"##).unwrap_err();
//! assert_eq!(err.code, "contrast_too_low");
//! # Ok::<(), freeqrent_doc::DocError>(())
//! ```
//!
//! The crate has no dependencies besides `freeqrent`, with the `std`,
//! `animate`, `palette-extract` and `halftone` features on and `kanji` off.
//! Every binding uses this feature set, so every language produces the
//! same bytes (SPEC §10.3).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
pub mod json;
mod read;
mod report;
pub mod schema;
pub mod style;

use freeqrent::{Palette, PaletteOpts, QrMatrix, Rgba8Image, Style};

pub use error::DocError;
pub use report::{extracted_json, palette_json, report_json};
pub use style::{Document, parse};

use crate::read::{Obj, array_of, bool_of, f32_of, str_of, u8_of, u64_of};

/// What a code carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Payload<'a> {
    /// Text, segmented into the shortest modes (UTF-8 bytes get an ECI
    /// header when they aren't ASCII).
    Text(&'a str),
    /// Raw bytes, in byte mode.
    Bytes(&'a [u8]),
}

/// The document's style and the code it encodes `payload` into.
pub fn prepare(payload: Payload<'_>, style_json: &str) -> Result<(Style, QrMatrix), DocError> {
    let doc = parse(style_json)?;
    let qr = match payload {
        Payload::Text(text) => doc.encoder.encode_text_for(text, &doc.style)?,
        Payload::Bytes(bytes) => doc.encoder.encode_bytes_for(bytes, &doc.style)?,
    };
    Ok((doc.style, qr))
}

/// The SVG document, or why the style can't be drawn (the first of the
/// report's errors).
pub fn render(payload: Payload<'_>, style_json: &str) -> Result<String, DocError> {
    let (style, qr) = prepare(payload, style_json)?;
    Ok(style.render(&qr)?)
}

/// The intro stopped at `t_ms`, as a still SVG with no animation: what
/// the CLI's `--frame-at` writes. A style without an intro gives its
/// finished code.
pub fn render_frame(payload: Payload<'_>, style_json: &str, t_ms: u32) -> Result<String, DocError> {
    let (style, qr) = prepare(payload, style_json)?;
    Ok(style.render_frame(&qr, t_ms)?)
}

/// The scan report as JSON ([`report_json`]). A style that would be refused
/// still has a report, with `ok` false; only a bad document or a payload
/// that doesn't fit is an error.
pub fn check(payload: Payload<'_>, style_json: &str) -> Result<String, DocError> {
    let (style, qr) = prepare(payload, style_json)?;
    Ok(report_json(&style.check(&qr), &qr))
}

/// The animation plan as JSON (SPEC §8.5), for players outside CSS such as
/// the Flutter widget.
pub fn plan(payload: Payload<'_>, style_json: &str) -> Result<String, DocError> {
    let (style, qr) = prepare(payload, style_json)?;
    Ok(style.plan(&qr)?.to_json())
}

/// Palette options: `{"clusters", "seed", "min_contrast", "allow_inverted",
/// "logo_colors": [css]}`, all optional. An empty text means `{}`.
fn palette_opts(opts_json: &str) -> Result<PaletteOpts, DocError> {
    let mut opts = PaletteOpts::new();
    if opts_json.trim().is_empty() {
        return Ok(opts);
    }
    let value = json::parse(opts_json).map_err(|e| {
        DocError::invalid("", format!("not JSON: {} at byte {}", e.reason, e.offset))
    })?;
    let mut o = Obj::new(&value, "")?;
    if let Some(k) = o.opt("clusters", u8_of)? {
        opts = opts.clusters(k);
    }
    if let Some(s) = o.opt("seed", u64_of)? {
        opts = opts.seed(s);
    }
    if let Some(d) = o.opt("min_contrast", f32_of)? {
        opts = opts.min_contrast(d);
    }
    if let Some(a) = o.opt("allow_inverted", bool_of)? {
        opts = opts.allow_inverted(a);
    }
    if let Some((v, path)) = o.get("logo_colors") {
        let mut colors = Vec::new();
        for (i, item) in array_of(v, &path)?.iter().enumerate() {
            let p = format!("{path}[{i}]");
            let css = str_of(item, &p)?;
            colors.push(
                freeqrent::Color::parse(css).map_err(|e| DocError::invalid(&p, e.to_string()))?,
            );
        }
        opts = opts.logo_colors(&colors);
    }
    o.finish()?;
    Ok(opts)
}

/// A palette from RGBA pixels, row by row, as JSON ([`extracted_json`]).
pub fn palette_from_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    opts_json: &str,
) -> Result<String, DocError> {
    let image =
        Rgba8Image::new(width, height, rgba.to_vec()).map_err(|e| error::image_error("", e))?;
    extracted_json(Palette::from_image(&image, palette_opts(opts_json)?))
}

/// A palette from a style sheet's colors, as JSON ([`extracted_json`]).
pub fn palette_from_css(css: &str, opts_json: &str) -> Result<String, DocError> {
    extracted_json(Palette::from_css_with(css, palette_opts(opts_json)?))
}

/// A palette from a page's colors (its style sheets and inline styles), as
/// JSON ([`extracted_json`]).
pub fn palette_from_html(html: &str, opts_json: &str) -> Result<String, DocError> {
    extracted_json(Palette::from_html_with(html, palette_opts(opts_json)?))
}

/// The preset names (`Style::presets`), as a JSON array.
pub fn presets() -> String {
    report::string_array(Style::presets())
}

/// The library version, the same in every package (SPEC §10.3).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// The README's example runs as a doctest.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
