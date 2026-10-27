//! freeQRent for JavaScript: the boundary functions of `freeqrent-doc`
//! (SPEC §10.1), exported with `wasm-bindgen`. The npm package's wrapper
//! (`bindings/js/src/api.js`) gives them their camelCase names, parses the
//! JSON results and turns errors into `FreeQrentError`.
//!
//! A payload crosses as bytes with a flag: `text` true means UTF-8 text,
//! segmented like the CLI's `TEXT`; false means byte mode. Every error
//! crosses as its JSON text, `{"code","message","detail"}`
//! (`DocError::to_json`), thrown as a string for the wrapper to parse.

#![forbid(unsafe_code)]

use freeqrent_doc::{DocError, Payload};
use wasm_bindgen::prelude::*;

/// The payload's bytes as text or as bytes. The wrapper encodes JS strings
/// as UTF-8, so text is always valid; it is checked anyway.
fn payload(data: &[u8], text: bool) -> Result<Payload<'_>, String> {
    if !text {
        return Ok(Payload::Bytes(data));
    }
    std::str::from_utf8(data)
        .map(Payload::Text)
        .map_err(|_| DocError::invalid_argument("the payload text isn't UTF-8").to_json())
}

fn out(result: Result<String, DocError>) -> Result<String, String> {
    result.map_err(|e| e.to_json())
}

/// The SVG, or the error JSON.
#[wasm_bindgen]
pub fn render(data: &[u8], text: bool, style: &str) -> Result<String, String> {
    out(freeqrent_doc::render(payload(data, text)?, style))
}

/// The intro stopped at `t_ms`, as a still SVG.
#[wasm_bindgen]
pub fn render_frame(data: &[u8], text: bool, style: &str, t_ms: u32) -> Result<String, String> {
    out(freeqrent_doc::render_frame(
        payload(data, text)?,
        style,
        t_ms,
    ))
}

/// The scan report as JSON.
#[wasm_bindgen]
pub fn check(data: &[u8], text: bool, style: &str) -> Result<String, String> {
    out(freeqrent_doc::check(payload(data, text)?, style))
}

/// The animation plan as JSON.
#[wasm_bindgen]
pub fn plan(data: &[u8], text: bool, style: &str) -> Result<String, String> {
    out(freeqrent_doc::plan(payload(data, text)?, style))
}

/// A palette result from RGBA pixels, row by row.
#[wasm_bindgen]
pub fn palette_from_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    opts: &str,
) -> Result<String, String> {
    out(freeqrent_doc::palette_from_image(rgba, width, height, opts))
}

/// A palette result from a style sheet.
#[wasm_bindgen]
pub fn palette_from_css(css: &str, opts: &str) -> Result<String, String> {
    out(freeqrent_doc::palette_from_css(css, opts))
}

/// A palette result from an HTML page.
#[wasm_bindgen]
pub fn palette_from_html(html: &str, opts: &str) -> Result<String, String> {
    out(freeqrent_doc::palette_from_html(html, opts))
}

/// The preset names as a JSON array.
#[wasm_bindgen]
pub fn presets() -> String {
    freeqrent_doc::presets()
}

/// The library version.
#[wasm_bindgen]
pub fn version() -> String {
    freeqrent_doc::version().to_string()
}
