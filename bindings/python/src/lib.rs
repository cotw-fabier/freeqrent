//! `freeqrent._native`: the boundary functions of `freeqrent-doc` (SPEC
//! §10.1) as a CPython extension module. Text goes in and text comes out;
//! the `freeqrent` package parses the JSON results and builds the Style
//! documents. Every render runs detached from the interpreter, so threads
//! render in parallel, and an error becomes a `freeqrent.FreeQrentError`
//! built from the error JSON `{code, message, detail}`.
//!
//! This crate writes no `unsafe` itself (PyO3's macros generate the C-API
//! glue).

#![forbid(unsafe_code)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use freeqrent_doc::{DocError, Payload};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::pybacked::{PyBackedBytes, PyBackedStr};
use pyo3::types::{PyByteArray, PyBytes, PyString};

/// A payload owned on the Rust side, so the render can run detached.
enum Owned {
    Text(PyBackedStr),
    Bytes(PyBackedBytes),
}

impl Owned {
    fn payload(&self) -> Payload<'_> {
        match self {
            Owned::Text(s) => Payload::Text(s),
            Owned::Bytes(b) => Payload::Bytes(b),
        }
    }
}

/// The `FreeQrentError` for `e`, built by the Python side from its JSON.
fn raise(py: Python<'_>, e: &DocError) -> PyErr {
    let built = py
        .import("freeqrent._errors")
        .and_then(|m| m.getattr("FreeQrentError"))
        .and_then(|cls| cls.call_method1("from_json", (e.to_json(),)));
    match built {
        Ok(err) => PyErr::from_value(err),
        Err(import_failed) => import_failed,
    }
}

fn internal(reason: &str) -> DocError {
    let mut e = DocError::invalid_argument(reason);
    e.code = "internal";
    e
}

/// Text that isn't UTF-8 (a str with lone surrogates) is `invalid_argument`,
/// as at the C ABI.
fn text_arg(py: Python<'_>, value: &Bound<'_, PyAny>, what: &str) -> PyResult<PyBackedStr> {
    let s = value
        .cast::<PyString>()
        .map_err(|_| PyTypeError::new_err(format!("{what} must be str")))?;
    PyBackedStr::try_from(s.clone()).map_err(|_| {
        raise(
            py,
            &DocError::invalid_argument(format!("{what} isn't valid UTF-8 text")),
        )
    })
}

fn payload_arg(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Owned> {
    if value.is_instance_of::<PyString>() {
        return Ok(Owned::Text(text_arg(py, value, "the text payload")?));
    }
    if value.is_instance_of::<PyBytes>() || value.is_instance_of::<PyByteArray>() {
        return Ok(Owned::Bytes(value.extract::<PyBackedBytes>()?));
    }
    Err(PyTypeError::new_err(
        "the payload must be str (text) or bytes (byte mode)",
    ))
}

/// Runs `f` detached from the interpreter, catching panics as `internal`.
fn run<F>(py: Python<'_>, f: F) -> PyResult<String>
where
    F: FnOnce() -> Result<String, DocError> + Send,
{
    let result = py.detach(|| {
        catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err(internal("the library panicked")))
    });
    result.map_err(|e| raise(py, &e))
}

/// The SVG for `payload` (str or bytes) in the Style document `style`.
#[pyfunction]
fn render(py: Python<'_>, payload: &Bound<'_, PyAny>, style: &Bound<'_, PyAny>) -> PyResult<String> {
    let payload = payload_arg(py, payload)?;
    let style = text_arg(py, style, "the style")?;
    run(py, || freeqrent_doc::render(payload.payload(), &style))
}

/// The intro stopped at `t_ms`, as a still SVG.
#[pyfunction]
fn render_frame(
    py: Python<'_>,
    payload: &Bound<'_, PyAny>,
    style: &Bound<'_, PyAny>,
    t_ms: u32,
) -> PyResult<String> {
    let payload = payload_arg(py, payload)?;
    let style = text_arg(py, style, "the style")?;
    run(py, || freeqrent_doc::render_frame(payload.payload(), &style, t_ms))
}

/// The scan report, as JSON.
#[pyfunction]
fn check(py: Python<'_>, payload: &Bound<'_, PyAny>, style: &Bound<'_, PyAny>) -> PyResult<String> {
    let payload = payload_arg(py, payload)?;
    let style = text_arg(py, style, "the style")?;
    run(py, || freeqrent_doc::check(payload.payload(), &style))
}

/// The animation plan, as JSON.
#[pyfunction]
fn plan(py: Python<'_>, payload: &Bound<'_, PyAny>, style: &Bound<'_, PyAny>) -> PyResult<String> {
    let payload = payload_arg(py, payload)?;
    let style = text_arg(py, style, "the style")?;
    run(py, || freeqrent_doc::plan(payload.payload(), &style))
}

/// A palette from 8-bit RGBA pixels, row by row, as JSON.
#[pyfunction]
fn palette_from_image(
    py: Python<'_>,
    rgba: &Bound<'_, PyAny>,
    width: u32,
    height: u32,
    opts: &Bound<'_, PyAny>,
) -> PyResult<String> {
    if !(rgba.is_instance_of::<PyBytes>() || rgba.is_instance_of::<PyByteArray>()) {
        return Err(PyTypeError::new_err("rgba must be bytes"));
    }
    let rgba = rgba.extract::<PyBackedBytes>()?;
    let opts = text_arg(py, opts, "the palette options")?;
    run(py, || {
        freeqrent_doc::palette_from_image(&rgba, width, height, &opts)
    })
}

/// A palette from a style sheet's colors, as JSON.
#[pyfunction]
fn palette_from_css(
    py: Python<'_>,
    css: &Bound<'_, PyAny>,
    opts: &Bound<'_, PyAny>,
) -> PyResult<String> {
    let css = text_arg(py, css, "the CSS")?;
    let opts = text_arg(py, opts, "the palette options")?;
    run(py, || freeqrent_doc::palette_from_css(&css, &opts))
}

/// A palette from a page's colors, as JSON.
#[pyfunction]
fn palette_from_html(
    py: Python<'_>,
    html: &Bound<'_, PyAny>,
    opts: &Bound<'_, PyAny>,
) -> PyResult<String> {
    let html = text_arg(py, html, "the HTML")?;
    let opts = text_arg(py, opts, "the palette options")?;
    run(py, || freeqrent_doc::palette_from_html(&html, &opts))
}

/// The preset names, as a JSON array.
#[pyfunction]
fn presets() -> String {
    freeqrent_doc::presets()
}

/// The library version, the same in every package.
#[pyfunction]
fn version() -> &'static str {
    freeqrent_doc::version()
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render, m)?)?;
    m.add_function(wrap_pyfunction!(render_frame, m)?)?;
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_function(wrap_pyfunction!(plan, m)?)?;
    m.add_function(wrap_pyfunction!(palette_from_image, m)?)?;
    m.add_function(wrap_pyfunction!(palette_from_css, m)?)?;
    m.add_function(wrap_pyfunction!(palette_from_html, m)?)?;
    m.add_function(wrap_pyfunction!(presets, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add("__version__", freeqrent_doc::version())?;
    Ok(())
}
