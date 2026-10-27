//! freeQRent's C ABI (SPEC §10.2): the `freeqrent-doc` boundary as plain C
//! functions, for Dart, Go and any other FFI host. The header is
//! `include/freeqrent.h`.
//!
//! Every call has the same shape: it writes one string to `*out` and
//! returns [`FREEQRENT_OK`] (`*out` is the result) or [`FREEQRENT_ERROR`]
//! (`*out` is the error as JSON, `{"code","message","detail"}`). When
//! `out_len` isn't NULL it gets the string's length in bytes, not counting
//! the terminating NUL. Strings are UTF-8, and every string the library
//! returns is freed with [`freeqrent_free`]. Only a NULL `out` returns
//! [`FREEQRENT_INVALID`], with nothing written.
//!
//! The functions are pure and safe to call from any thread at once.

use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};

use freeqrent_doc::{DocError, Payload};

/// The call worked; `*out` holds the result.
pub const FREEQRENT_OK: i32 = 0;
/// The call failed; `*out` holds the error as JSON.
pub const FREEQRENT_ERROR: i32 = 1;
/// `out` was NULL; nothing was written.
pub const FREEQRENT_INVALID: i32 = 2;

/// A payload of UTF-8 text.
pub const FREEQRENT_TEXT: i32 = 0;
/// A payload of raw bytes.
pub const FREEQRENT_BYTES: i32 = 1;

/// Hands `text` to the caller as a C string.
///
/// # Safety
/// `out` is non-null and writable; `out_len` is null or writable.
unsafe fn give(text: String, out: *mut *mut c_char, out_len: *mut usize) {
    // SVG drops NULs and JSON escapes them, so this only fails on a bug;
    // the error has no NUL either.
    let c = CString::new(text).unwrap_or_else(|_| {
        CString::new(internal("a result held a NUL byte").to_json()).expect("no NUL")
    });
    let len = c.as_bytes().len();
    // SAFETY: the caller promised `out` is writable and `out_len` null or
    // writable.
    unsafe {
        *out = c.into_raw();
        if !out_len.is_null() {
            *out_len = len;
        }
    }
}

fn internal(reason: &str) -> DocError {
    let mut e = DocError::invalid_argument(reason);
    e.code = "internal";
    e
}

/// Runs `f`, catching panics, and hands its result or error out.
///
/// # Safety
/// `out` is null or writable; `out_len` is null or writable.
unsafe fn deliver(
    out: *mut *mut c_char,
    out_len: *mut usize,
    f: impl FnOnce() -> Result<String, DocError>,
) -> i32 {
    if out.is_null() {
        return FREEQRENT_INVALID;
    }
    let (code, text) = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(text)) => (FREEQRENT_OK, text),
        Ok(Err(e)) => (FREEQRENT_ERROR, e.to_json()),
        Err(_) => (FREEQRENT_ERROR, internal("the library panicked").to_json()),
    };
    // SAFETY: `out` is non-null, and the caller promised both are writable.
    unsafe { give(text, out, out_len) };
    code
}

/// The bytes at `ptr`, or empty for a NULL pointer with length 0.
///
/// # Safety
/// `ptr` is null, or valid for reads of `len` bytes for the call.
unsafe fn bytes<'a>(ptr: *const u8, len: usize, what: &str) -> Result<&'a [u8], DocError> {
    if ptr.is_null() {
        return if len == 0 {
            Ok(&[])
        } else {
            Err(DocError::invalid_argument(format!("{what} is NULL")))
        };
    }
    // SAFETY: the caller promised `ptr` is valid for `len` bytes.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// A NUL-terminated UTF-8 string, or `default` for NULL.
///
/// # Safety
/// `ptr` is null or a valid NUL-terminated string for the call.
unsafe fn text<'a>(ptr: *const c_char, default: &'a str, what: &str) -> Result<&'a str, DocError> {
    if ptr.is_null() {
        return Ok(default);
    }
    // SAFETY: the caller promised a NUL-terminated string.
    let c = unsafe { CStr::from_ptr(ptr) };
    c.to_str()
        .map_err(|_| DocError::invalid_argument(format!("{what} isn't UTF-8")))
}

/// # Safety
/// As for [`bytes`].
unsafe fn payload<'a>(ptr: *const u8, len: usize, kind: i32) -> Result<Payload<'a>, DocError> {
    // SAFETY: passed through from the caller's promise.
    let raw = unsafe { bytes(ptr, len, "payload") }?;
    match kind {
        FREEQRENT_TEXT => std::str::from_utf8(raw)
            .map(Payload::Text)
            .map_err(|_| DocError::invalid_argument("the text payload isn't UTF-8")),
        FREEQRENT_BYTES => Ok(Payload::Bytes(raw)),
        _ => Err(DocError::invalid_argument(format!(
            "payload kind {kind}; expected FREEQRENT_TEXT (0) or FREEQRENT_BYTES (1)"
        ))),
    }
}

type PayloadFn = fn(Payload<'_>, &str) -> Result<String, DocError>;

/// # Safety
/// As for the public payload functions.
unsafe fn with_payload(
    f: PayloadFn,
    data: *const u8,
    len: usize,
    kind: i32,
    style_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller's promises about every pointer pass through.
    unsafe {
        deliver(out, out_len, || {
            f(
                payload(data, len, kind)?,
                text(style_json, "{}", "the style")?,
            )
        })
    }
}

/// Renders `payload` (`len` bytes of `kind` [`FREEQRENT_TEXT`] or
/// [`FREEQRENT_BYTES`]) with the Style document `style_json` (NULL means
/// `{}`): the SVG, or why the style can't be drawn.
///
/// # Safety
/// `data` is NULL with `len` 0, or valid for reads of `len` bytes;
/// `style_json` is NULL or a NUL-terminated string; `out` is NULL or
/// writable; `out_len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_render(
    data: *const u8,
    len: usize,
    kind: i32,
    style_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: as documented above.
    unsafe {
        with_payload(
            freeqrent_doc::render,
            data,
            len,
            kind,
            style_json,
            out,
            out_len,
        )
    }
}

/// The scan report for `payload` and `style_json`, as JSON. A style
/// [`freeqrent_render`] would refuse still gets a report, with `ok` false.
///
/// # Safety
/// As for [`freeqrent_render`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_check(
    data: *const u8,
    len: usize,
    kind: i32,
    style_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: as documented above.
    unsafe {
        with_payload(
            freeqrent_doc::check,
            data,
            len,
            kind,
            style_json,
            out,
            out_len,
        )
    }
}

/// The animation plan for `payload` and `style_json`, as JSON (SPEC §8.5).
///
/// # Safety
/// As for [`freeqrent_render`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_plan(
    data: *const u8,
    len: usize,
    kind: i32,
    style_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: as documented above.
    unsafe {
        with_payload(
            freeqrent_doc::plan,
            data,
            len,
            kind,
            style_json,
            out,
            out_len,
        )
    }
}

/// A palette from `width` × `height` RGBA pixels (`rgba_len` bytes, row by
/// row), with options `opts_json` (NULL means none), as JSON.
///
/// # Safety
/// `rgba` is NULL with `rgba_len` 0, or valid for reads of `rgba_len`
/// bytes; `opts_json` is NULL or a NUL-terminated string; `out` is NULL or
/// writable; `out_len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_palette_from_image(
    rgba: *const u8,
    rgba_len: usize,
    width: u32,
    height: u32,
    opts_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: as documented above.
    unsafe {
        deliver(out, out_len, || {
            freeqrent_doc::palette_from_image(
                bytes(rgba, rgba_len, "rgba")?,
                width,
                height,
                text(opts_json, "", "the options")?,
            )
        })
    }
}

/// A palette from a style sheet's colors, as JSON.
///
/// # Safety
/// `css` and `opts_json` are NULL or NUL-terminated strings (a NULL `css`
/// is empty); `out` is NULL or writable; `out_len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_palette_from_css(
    css: *const c_char,
    opts_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: as documented above.
    unsafe {
        deliver(out, out_len, || {
            freeqrent_doc::palette_from_css(
                text(css, "", "the style sheet")?,
                text(opts_json, "", "the options")?,
            )
        })
    }
}

/// A palette from a page's colors, as JSON.
///
/// # Safety
/// As for [`freeqrent_palette_from_css`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_palette_from_html(
    html: *const c_char,
    opts_json: *const c_char,
    out: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: as documented above.
    unsafe {
        deliver(out, out_len, || {
            freeqrent_doc::palette_from_html(
                text(html, "", "the page")?,
                text(opts_json, "", "the options")?,
            )
        })
    }
}

/// The preset names as a JSON array.
///
/// # Safety
/// `out` is NULL or writable; `out_len` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_presets(out: *mut *mut c_char, out_len: *mut usize) -> i32 {
    // SAFETY: as documented above.
    unsafe { deliver(out, out_len, || Ok(freeqrent_doc::presets())) }
}

/// The library version, e.g. `"1.0.0"`: a static string, not to be freed.
#[unsafe(no_mangle)]
pub extern "C" fn freeqrent_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// Frees a string this library returned. NULL is ignored.
///
/// # Safety
/// `s` is NULL or a string from this library, not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeqrent_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: the caller promised `s` came from `CString::into_raw` here.
        drop(unsafe { CString::from_raw(s) });
    }
}
