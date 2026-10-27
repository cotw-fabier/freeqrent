//! The C ABI, called the way a C host calls it: the whole conformance suite
//! through the exported functions, and the edge cases (NULL pointers,
//! text that isn't UTF-8, lengths, threads).

#[path = "../../freeqrent-doc/tests/support/mod.rs"]
mod support;

use std::ffi::{CStr, CString, c_char};
use std::ptr::{null, null_mut};

use freeqrent_doc::json::{self, Value};
use freeqrent_ffi::*;

/// Takes a returned string: its text, after checking `out_len`.
fn take(out: *mut c_char, len: usize) -> String {
    assert!(!out.is_null());
    // SAFETY: the library returned a NUL-terminated string.
    let text = unsafe { CStr::from_ptr(out) }.to_str().unwrap().to_string();
    assert_eq!(text.len(), len);
    // SAFETY: it came from the library and is freed once.
    unsafe { freeqrent_free(out) };
    text
}

type PayloadFn =
    unsafe extern "C" fn(*const u8, usize, i32, *const c_char, *mut *mut c_char, *mut usize) -> i32;

fn call_payload(f: PayloadFn, data: &[u8], kind: i32, style: Option<&str>) -> (i32, String) {
    let style = style.map(|s| CString::new(s).unwrap());
    let (mut out, mut len) = (null_mut(), 0usize);
    // SAFETY: valid pointers for the call.
    let code = unsafe {
        f(
            data.as_ptr(),
            data.len(),
            kind,
            style.as_ref().map_or(null(), |s| s.as_ptr()),
            &mut out,
            &mut len,
        )
    };
    (code, take(out, len))
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Object(m) => m
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .unwrap_or(&Value::Null),
        _ => &Value::Null,
    }
}

fn compact(v: &Value) -> String {
    let mut out = String::new();
    json::write_compact(&mut out, v);
    out
}

fn string(v: &Value) -> &str {
    match v {
        Value::String(s) => s,
        other => panic!("{other:?}"),
    }
}

fn unbase64(s: &str) -> Vec<u8> {
    let abc = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.trim_end_matches('=').chars() {
        acc = (acc << 6) | abc.find(c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// One case through the ABI: `(code, text)`.
fn run(case: &Value) -> (i32, String) {
    let opts = || CString::new(compact(field(case, "opts"))).unwrap();
    let (mut out, mut len) = (null_mut(), 0usize);
    match string(field(case, "fn")) {
        f @ ("render" | "check" | "plan") => {
            let function: PayloadFn = match f {
                "render" => freeqrent_render,
                "check" => freeqrent_check,
                _ => freeqrent_plan,
            };
            let Value::Object(p) = field(case, "payload") else {
                panic!()
            };
            let (data, kind) = match &p[0] {
                (k, Value::String(t)) if k == "text" => (t.as_bytes().to_vec(), FREEQRENT_TEXT),
                (_, Value::String(b)) => (unbase64(b), FREEQRENT_BYTES),
                _ => panic!(),
            };
            call_payload(function, &data, kind, Some(&compact(field(case, "style"))))
        }
        "render_frame" => {
            let Value::Object(p) = field(case, "payload") else {
                panic!()
            };
            let Value::String(t) = &p[0].1 else { panic!() };
            let Value::Number(t_ms) = field(case, "t_ms") else {
                panic!()
            };
            let style = CString::new(compact(field(case, "style"))).unwrap();
            // SAFETY: valid pointers for the call.
            let code = unsafe {
                freeqrent_render_frame(
                    t.as_ptr(),
                    t.len(),
                    FREEQRENT_TEXT,
                    style.as_ptr(),
                    t_ms.parse().unwrap(),
                    &mut out,
                    &mut len,
                )
            };
            (code, take(out, len))
        }
        "palette_from_css" | "palette_from_html" => {
            let text = CString::new(string(field(case, "text"))).unwrap();
            let f = if string(field(case, "fn")) == "palette_from_css" {
                freeqrent_palette_from_css
            } else {
                freeqrent_palette_from_html
            };
            // SAFETY: valid pointers for the call.
            let code = unsafe { f(text.as_ptr(), opts().as_ptr(), &mut out, &mut len) };
            (code, take(out, len))
        }
        "palette_from_image" => {
            let image = field(case, "image");
            let num = |k: &str| match field(image, k) {
                Value::Number(n) => n.parse::<u32>().unwrap(),
                _ => panic!(),
            };
            let rgba = unbase64(string(field(image, "rgba")));
            // SAFETY: valid pointers for the call.
            let code = unsafe {
                freeqrent_palette_from_image(
                    rgba.as_ptr(),
                    rgba.len(),
                    num("width"),
                    num("height"),
                    opts().as_ptr(),
                    &mut out,
                    &mut len,
                )
            };
            (code, take(out, len))
        }
        "presets" => {
            // SAFETY: valid pointers for the call.
            let code = unsafe { freeqrent_presets(&mut out, &mut len) };
            (code, take(out, len))
        }
        other => panic!("{other}"),
    }
}

#[test]
fn the_conformance_suite_passes_through_the_abi() {
    let suite =
        json::parse(&std::fs::read_to_string(support::root().join(support::SUITE)).unwrap())
            .unwrap();
    let Value::Array(cases) = field(&suite, "cases") else {
        panic!()
    };
    for case in cases {
        let (code, text) = run(case);
        let expect = field(case, "expect");
        let got = if code == FREEQRENT_OK {
            format!(
                r#"{{"sha256":"{}","length":{}}}"#,
                support::sha256::hex(text.as_bytes()),
                text.len()
            )
        } else {
            assert_eq!(code, FREEQRENT_ERROR);
            let error = json::parse(&text).expect("errors are JSON");
            format!(r#"{{"error":"{}"}}"#, string(field(&error, "code")))
        };
        assert_eq!(
            &json::parse(&got).unwrap(),
            expect,
            "{:?}",
            field(case, "name")
        );
    }
}

#[test]
fn edge_cases() {
    // NULL out: nothing to write to.
    // SAFETY: a NULL `out` is allowed.
    let code = unsafe { freeqrent_presets(null_mut(), null_mut()) };
    assert_eq!(code, FREEQRENT_INVALID);
    // NULL style means `{}`, and NULL out_len is fine.
    let (code, svg) = call_payload(freeqrent_render, b"hello", FREEQRENT_TEXT, None);
    assert_eq!(code, FREEQRENT_OK);
    assert_eq!(
        svg,
        freeqrent_doc::render(freeqrent_doc::Payload::Text("hello"), "{}").unwrap()
    );
    let mut out = null_mut();
    // SAFETY: valid pointers; NULL out_len is allowed.
    let code = unsafe {
        freeqrent_render(
            b"hi".as_ptr(),
            2,
            FREEQRENT_TEXT,
            null(),
            &mut out,
            null_mut(),
        )
    };
    assert_eq!(code, FREEQRENT_OK);
    // SAFETY: from the library, freed once.
    unsafe { freeqrent_free(out) };
    // SAFETY: NULL is ignored.
    unsafe { freeqrent_free(null_mut()) };

    let error = |(code, text): (i32, String)| {
        assert_eq!(code, FREEQRENT_ERROR, "{text}");
        let e = json::parse(&text).unwrap();
        (
            string(field(&e, "code")).to_string(),
            string(field(&e, "message")).to_string(),
        )
    };
    assert_eq!(
        error(call_payload(
            freeqrent_render,
            b"\xff\xfe",
            FREEQRENT_TEXT,
            None
        ))
        .0,
        "invalid_argument"
    );
    assert_eq!(
        call_payload(freeqrent_render, b"\xff\xfe", FREEQRENT_BYTES, None).0,
        FREEQRENT_OK
    );
    assert_eq!(
        error(call_payload(freeqrent_render, b"x", 7, None)).0,
        "invalid_argument"
    );
    assert_eq!(
        error(call_payload(
            freeqrent_check,
            b"x",
            FREEQRENT_TEXT,
            Some("{\"modules\":3}")
        ))
        .0,
        "invalid_style"
    );
    // A NULL payload is empty only with length 0.
    let mut out = null_mut();
    let mut len = 0;
    // SAFETY: NULL data with length 0 is allowed.
    let code = unsafe { freeqrent_render(null(), 0, FREEQRENT_TEXT, null(), &mut out, &mut len) };
    assert_eq!(code, FREEQRENT_OK);
    take(out, len);
    // SAFETY: NULL data with a length is refused before it's read.
    let code = unsafe { freeqrent_render(null(), 4, FREEQRENT_TEXT, null(), &mut out, &mut len) };
    assert_eq!(code, FREEQRENT_ERROR);
    assert!(take(out, len).contains("\"invalid_argument\""));
    // Style text that isn't UTF-8.
    let bad = [0xffu8, 0];
    // SAFETY: a NUL-terminated (non-UTF-8) string.
    let code = unsafe {
        freeqrent_render(
            b"x".as_ptr(),
            1,
            FREEQRENT_TEXT,
            bad.as_ptr().cast(),
            &mut out,
            &mut len,
        )
    };
    assert_eq!(code, FREEQRENT_ERROR);
    assert!(take(out, len).contains("the style isn't UTF-8"));

    // Memory for hosts that can't allocate here themselves.
    assert!(freeqrent_alloc(0).is_null());
    let p = freeqrent_alloc(5);
    // SAFETY: five fresh bytes, zeroed, freed once with their length.
    unsafe {
        assert_eq!(std::slice::from_raw_parts(p, 5), &[0; 5]);
        freeqrent_dealloc(p, 5);
        freeqrent_dealloc(null_mut(), 0);
    }

    // SAFETY: a static NUL-terminated string.
    let version = unsafe { CStr::from_ptr(freeqrent_version()) };
    assert_eq!(version.to_str().unwrap(), freeqrent_doc::version());
}

#[test]
fn calls_from_many_threads_agree() {
    let style = r#"{"preset":"candy","intro":"ripple"}"#;
    let want = freeqrent_doc::render(freeqrent_doc::Payload::Text("https://example.com/t"), style)
        .unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(move || {
                (0..10)
                    .map(|_| {
                        call_payload(
                            freeqrent_render,
                            b"https://example.com/t",
                            FREEQRENT_TEXT,
                            Some(style),
                        )
                        .1
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    for t in threads {
        assert!(t.join().unwrap().iter().all(|svg| *svg == want));
    }
}
