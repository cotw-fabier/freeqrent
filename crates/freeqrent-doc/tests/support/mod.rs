//! The conformance suite (SPEC §10.3): its inputs, the runner every
//! binding mirrors, and the generator that writes `conformance/cases.json`.
//! Shared by the conformance test and the `generate` example.
#![allow(dead_code)]

pub mod sha256;
pub mod validate;

use freeqrent_doc::json::{self, Value};
use freeqrent_doc::{DocError, Payload};

/// The repository root.
pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub const SUITE: &str = "conformance/cases.json";
pub const SCHEMA: &str = "schema/freeqrent.schema.json";

const MARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#a3302a"/><path d="M3 5h4" stroke="#fff"/></svg>"##;
/// A 1×1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().fold(0u32, |a, &b| (a << 8) | b as u32) << (8 * (3 - chunk.len()));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn unbase64(s: &str) -> Vec<u8> {
    const ABC: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim_end_matches('=').chars() {
        acc = (acc << 6) | ABC.find(c).expect("base64") as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

fn quote(s: &str) -> String {
    let mut out = String::new();
    json::write_str(&mut out, s);
    out
}

/// A 16×16 picture: a dark disc on a light page.
fn disc() -> String {
    picture(|x, y| {
        if (x - 8).pow(2) + (y - 8).pow(2) < 30 {
            [30, 40, 90, 255]
        } else {
            [245, 240, 230, 255]
        }
    })
}

/// A 16×16 picture in four colors: a sunset of bands.
fn bands() -> String {
    picture(|_, y| match y / 4 {
        0 => [250, 220, 160, 255],
        1 => [240, 150, 90, 255],
        2 => [170, 60, 80, 255],
        _ => [40, 30, 70, 255],
    })
}

fn picture(f: impl Fn(i32, i32) -> [u8; 4]) -> String {
    let mut rgba = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            rgba.extend(f(x, y));
        }
    }
    format!(r#"{{"width":16,"height":16,"rgba":"{}"}}"#, base64(&rgba))
}

const URL: &str = "https://example.com/conformance";

/// One input: a name, a function, its arguments as JSON members.
fn case(name: &str, function: &str, args: &str) -> String {
    let sep = if args.is_empty() { "" } else { "," };
    format!(r#"{{"name":{},"fn":"{function}"{sep}{args}}}"#, quote(name))
}

fn render(name: &str, style: &str) -> String {
    case(
        name,
        "render",
        &format!(r#""payload":{{"text":"{URL}"}},"style":{style}"#),
    )
}

/// Every input, in suite order.
pub fn inputs() -> Vec<String> {
    let logo = quote(MARK);
    let disc = disc();
    let bands = bands();
    let mut cases = vec![render("default", "{}")];
    for preset in freeqrent::Style::presets() {
        cases.push(render(
            &format!("preset {preset}"),
            &format!(r#"{{"preset":"{preset}"}}"#),
        ));
    }
    cases.extend([
        render("square gaps", r#"{"modules":{"kind":"square","gap":0.12}}"#),
        render("rounded", r#"{"modules":"rounded"}"#),
        render("dots", r#"{"modules":{"kind":"dots","size":0.8}}"#),
        render("liquid", r#"{"modules":{"kind":"liquid","smoothness":1}}"#),
        render("bars", r#"{"modules":{"kind":"bars","axis":"horizontal","radius":0.4}}"#),
        render(
            "finders and corners",
            r#"{"finders":{"ring":"squircle","eye":"circle","corners":{"top-right":{"ring":"leaf","eye":"leaf"}}}}"#,
        ),
        render(
            "outlined alignment",
            r#"{"alignment":{"shape":{"kind":"own","ring":{"kind":"rounded","radius":2.5}},"emphasis":"outline"},"timing":"dashed","encoding":{"min_version":7}}"#,
        ),
        render(
            "gradient palette",
            r#"{"modules":"rounded","palette":{"bg":"oklch(97% .015 85)","fg":"linear-gradient(135deg, #1d2b64, oklch(30% .14 330))","finder.eye":"oklch(45% .15 30)"}}"#,
        ),
        render(
            "round frame",
            r#"{"modules":{"kind":"dots","size":0.85},"frame":{"kind":"circle","filler_density":0.4,"ring":"dashed"},"seed":7}"#,
        ),
        render("rounded frame", r#"{"frame":{"kind":"rounded","radius":1.5}}"#),
        render(
            "border and shadow",
            r##"{"frame":{"kind":"rounded","radius":2},"border":{"kind":"double","width":1.5},"shadow":1.5,"palette":{"frame":"#b8860b","shadow":"#1a1a1a"}}"##,
        ),
        render(
            "data colors by region",
            r##"{"modules":"liquid","color_spread":"regions","palette":{"fg.2":"oklch(35% .16 27)","fg.3":"oklch(35% .18 265)"},"seed":5}"##,
        ),
        render(
            "data colors rippling",
            r##"{"modules":"dots","palette":{"fg.2":"#7a0000","fg.3":"#1e3a8a","fg.4":"#065f46"},"intro":"ripple"}"##,
        ),
        render(
            "gradient background",
            r##"{"palette":{"bg":"radial-gradient(oklch(97% .02 300), oklch(90% .05 300))","fg":"#2e1065"},"frame":{"kind":"rounded","radius":3},"shadow":1}"##,
        ),
        render(
            "neon glow settling",
            r##"{"palette":{"bg":"#0a0a0f","fg":"#7dffb0"},"allow_inverted":true,"effect":{"kind":"glow","radius":0.4},"intro":"settle"}"##,
        ),
        render(
            "clay emboss",
            r##"{"palette":{"bg":"#e0e5ec","fg":"#2b333b"},"modules":"dots","effect":"emboss","frame":{"kind":"rounded","radius":4}}"##,
        ),
        render(
            "sketch with leaf finders",
            r##"{"modules":{"kind":"rounded","radius":0.3},"finders":{"ring":"leaf"},"effect":{"kind":"sketch","roughness":0.12},"seed":11}"##,
        ),
        case("plan glow", "plan", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"effect":"glow","intro":"ripple"}}"#)),
        render(
            "round border settling",
            r#"{"modules":"dots","frame":{"kind":"circle","ring":"solid"},"border":"solid","shadow":1,"seed":3,"intro":"settle"}"#,
        ),
        render("dark twin", r##"{"preset":"ink","dark":true}"##),
        render(
            "svg logo",
            &format!(r#"{{"modules":"liquid","logo":{{"svg":{logo},"frame":"ring","shape":"circle"}},"encoding":{{"ecc":"Q"}}}}"#),
        ),
        render(
            "raster logo",
            &format!(r#"{{"logo":{{"raster":"{PNG}","size":"small","shape":{{"kind":"rounded","radius":1}}}},"encoding":{{"ecc":"H"}}}}"#),
        ),
        render(
            "orbit logo turning",
            &format!(r#"{{"logo":{{"svg":{logo},"frame":{{"kind":"orbit","moon":2}}}},"idle":["logo-turn","logo-glint"],"encoding":{{"ecc":"Q"}}}}"#),
        ),
        render("halftone", &format!(r#"{{"halftone":{{"image":{disc}}}}}"#)),
        render(
            "halftone in color",
            &format!(r#"{{"halftone":{{"image":{bands},"ink":{{"kind":"color","colors":3}},"clarity":0.3}}}}"#),
        ),
        render("art", &format!(r#"{{"art":{{"image":{bands},"ink":"mono"}}}}"#)),
        render(
            "settle with options",
            r#"{"modules":"dots","intro":{"preset":"settle","duration_ms":900,"delay_ms":100,"groups":6,"ease":"expo","stagger":{"kind":"random","seed":3}}}"#,
        ),
        render("ripple replayed", r#"{"preset":"bubble","intro":{"preset":"ripple","trigger":{"kind":"every","ms":5000}}}"#),
        render("lock-on on hover", r#"{"intro":{"preset":"lock-on","trigger":"hover"}}"#),
        render(
            "sweep with a logo",
            &format!(r#"{{"logo":{{"svg":{logo}}},"intro":{{"preset":"sweep","logo":"stamp","logo_timing":"before"}},"encoding":{{"ecc":"Q"}}}}"#),
        ),
        render("raster", r#"{"intro":{"preset":"raster","ease":{"kind":"custom","x1":0.2,"y1":1,"x2":0.4,"y2":1}}}"#),
        render("iris on a class", r#"{"intro":{"preset":"iris","trigger":{"kind":"class","name":"shown"}}}"#),
        render("assemble", r#"{"preset":"leaf","intro":"assemble"}"#),
        render("develop", &format!(r#"{{"halftone":{{"image":{disc}}},"intro":"develop"}}"#)),
        render("palette shift", r##"{"preset":"candy","intro":{"palette_from":{"fg":"#1d3b72"},"duration_ms":800}}"##),
        render("idle loops", r#"{"preset":"ribbon","intro":"settle","idle":["finder-pulse","shimmer","breathe"],"theme_transition_ms":300}"#),
        render(
            "text and size",
            r#"{"title":"Menu","description":"Today's menu","label":"https://example.com/menu","id_prefix":"m","target_size":{"kind":"millimeters","mm":25},"minify":false}"#,
        ),
        render("pinned encoding", r#"{"encoding":{"ecc":"L","version":10,"mask":3,"boost_ecc":false,"eci":"never"}}"#),
        case("unicode text", "render", r#""payload":{"text":"Grüße, café ☕ 😀"},"style":{"preset":"sunset"}"#),
        case(
            "bytes",
            "render",
            &format!(r#""payload":{{"bytes":"{}"}},"style":{{}}"#, base64(&[0, 1, 2, 0xfe, 0xff, b'Q', b'R'])),
        ),
        case("check clean", "check", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"preset":"ink"}}"#)),
        case("check warnings", "check", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"preset":"terminal","modules":{{"kind":"dots","size":0.7}}}}"#)),
        case("check refused", "check", &format!(r##""payload":{{"text":"{URL}"}},"style":{{"palette":{{"fg":"#bbb"}}}}"##)),
        case(
            "check logo",
            "check",
            &format!(r#""payload":{{"text":"{URL}"}},"style":{{"logo":{{"svg":{logo},"scale":0.2}},"encoding":{{"ecc":"H"}}}}"#),
        ),
        case("check halftone", "check", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"halftone":{{"image":{disc}}}}}"#)),
        case("plan ripple", "plan", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"preset":"ink","intro":"ripple","idle":["shimmer"]}}"#)),
        case(
            "plan sweep with a logo",
            "plan",
            &format!(r#""payload":{{"text":"{URL}"}},"style":{{"logo":{{"svg":{logo},"frame":"beads"}},"intro":{{"preset":"sweep","frame":"draw"}},"encoding":{{"ecc":"Q"}}}}"#),
        ),
        case("error unknown key", "render", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"shape":"dots"}}"#)),
        case("error shadow", "render", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"frame":"none","shadow":1}}"#)),
        case("error contrast", "render", &format!(r##""payload":{{"text":"{URL}"}},"style":{{"palette":{{"fg":"#ccc"}}}}"##)),
        case("error too long", "render", &format!(r#""payload":{{"text":"{}"}},"style":{{"encoding":{{"ecc":"H"}}}}"#, "x".repeat(3000))),
        case("error logo", "render", &format!(r#""payload":{{"text":"{URL}"}},"style":{{"logo":{{"svg":"<html></html>"}}}}"#)),
        case(
            "error logo too large",
            "render",
            &format!(r#""payload":{{"text":"{URL}"}},"style":{{"logo":{{"svg":{logo},"scale":0.5}},"encoding":{{"version":5}}}}"#),
        ),
        case("palette from css", "palette_from_css", r#""text":":root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }","opts":{}"#),
        case(
            "palette from dark css",
            "palette_from_css",
            r#""text":"body { background: #0f1420; color: #e8e2d0; } a { color: #7dd3fc; }","opts":{"allow_inverted":true}"#,
        ),
        case(
            "palette from html",
            "palette_from_html",
            r##""text":"<html><head><style>body{background:#fffaf0;color:#2b2118}h1{color:#9a3412}</style></head><body style=\"border-color:#14532d\"></body></html>","opts":{"logo_colors":["#a3302a"]}"##,
        ),
        case("palette from image", "palette_from_image", &format!(r#""image":{bands},"opts":{{"clusters":4,"seed":1}}"#)),
        case("palette with no colors", "palette_from_css", r#""text":"","opts":{}"#),
        case("presets", "presets", ""),
    ]);
    cases
}

/// Runs one case, as every binding does: the result's text, or the error.
pub fn run(case: &Value) -> Result<String, DocError> {
    let get = |key: &str| match case {
        Value::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    };
    let text = |v: Option<&Value>| match v {
        Some(Value::String(s)) => s.clone(),
        other => panic!("expected a string, got {other:?}"),
    };
    let compact = |v: Option<&Value>| {
        let mut out = String::new();
        json::write_compact(&mut out, v.expect("present"));
        out
    };
    let function = text(get("fn"));
    match function.as_str() {
        "render" | "check" | "plan" => {
            let style = compact(get("style"));
            let Some(Value::Object(payload)) = get("payload") else {
                panic!("payload")
            };
            let bytes;
            let payload = match &payload[..] {
                [(k, Value::String(t))] if k == "text" => Payload::Text(t),
                [(k, Value::String(b))] if k == "bytes" => {
                    bytes = unbase64(b);
                    Payload::Bytes(&bytes)
                }
                other => panic!("payload {other:?}"),
            };
            match function.as_str() {
                "render" => freeqrent_doc::render(payload, &style),
                "check" => freeqrent_doc::check(payload, &style),
                _ => freeqrent_doc::plan(payload, &style),
            }
        }
        "palette_from_css" => {
            freeqrent_doc::palette_from_css(&text(get("text")), &compact(get("opts")))
        }
        "palette_from_html" => {
            freeqrent_doc::palette_from_html(&text(get("text")), &compact(get("opts")))
        }
        "palette_from_image" => {
            let Some(Value::Object(image)) = get("image") else {
                panic!("image")
            };
            let field = |key: &str| {
                image
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v)
                    .expect(key)
            };
            let num = |key: &str| match field(key) {
                Value::Number(n) => n.parse::<u32>().expect("width"),
                _ => panic!("{key}"),
            };
            let rgba = match field("rgba") {
                Value::String(s) => unbase64(s),
                _ => panic!("rgba"),
            };
            freeqrent_doc::palette_from_image(
                &rgba,
                num("width"),
                num("height"),
                &compact(get("opts")),
            )
        }
        "presets" => Ok(freeqrent_doc::presets()),
        other => panic!("unknown function {other}"),
    }
}

/// What a case expects: `{"sha256", "length"}` of the result's UTF-8, or
/// `{"error": code}`.
pub fn expectation(result: &Result<String, DocError>) -> String {
    match result {
        Ok(text) => format!(
            r#"{{"sha256":"{}","length":{}}}"#,
            sha256::hex(text.as_bytes()),
            text.len()
        ),
        Err(e) => format!(r#"{{"error":"{}"}}"#, e.code),
    }
}

/// The suite file's text.
pub fn generate() -> String {
    let mut cases = Vec::new();
    for input in inputs() {
        let mut value = json::parse(&input).unwrap_or_else(|e| panic!("{input}: {e:?}"));
        let expect = json::parse(&expectation(&run(&value))).expect("expectation");
        if let Value::Object(m) = &mut value {
            m.push(("expect".into(), expect));
        }
        cases.push(value);
    }
    let suite = Value::Object(vec![
        (
            "about".into(),
            Value::String(
                "freeQRent conformance suite (SPEC §10.3). Each binding runs every case and must get output whose SHA-256 and length match, or the error code. Generated by `cargo run -p freeqrent-doc --example generate`; don't edit by hand.".into(),
            ),
        ),
        ("version".into(), Value::String(freeqrent_doc::version().into())),
        ("cases".into(), Value::Array(cases)),
    ]);
    let mut out = String::new();
    json::write_pretty(&mut out, &suite);
    out
}
