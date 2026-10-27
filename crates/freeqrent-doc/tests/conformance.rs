//! The conformance suite's Rust runner (SPEC §10.3). It reads
//! `conformance/cases.json` the way every binding does and checks each
//! case's output; it also checks the file is what the generator writes now,
//! that every document fits the schema, and that every code it renders
//! scans.

mod support;

use freeqrent_doc::Payload;
use freeqrent_doc::json::{self, Value};
use support::validate::validate;

fn read(path: &str) -> String {
    std::fs::read_to_string(support::root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
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

fn cases() -> Vec<Value> {
    match field(&json::parse(&read(support::SUITE)).expect("JSON"), "cases") {
        Value::Array(cases) => cases.clone(),
        _ => panic!("cases"),
    }
}

#[test]
fn sha256_vectors() {
    assert_eq!(
        support::sha256::hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        support::sha256::hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        support::sha256::hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        support::sha256::hex(&[b'a'; 1000]),
        "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
    );
}

#[test]
fn the_suite_is_current() {
    assert!(
        read(support::SUITE) == support::generate(),
        "conformance/cases.json is stale: run `cargo run -p freeqrent-doc --example generate` and say why the hashes changed in the wave log"
    );
}

#[test]
fn every_case_passes() {
    let cases = cases();
    assert!(cases.len() >= 50, "{} cases", cases.len());
    for case in &cases {
        let mut input = case.clone();
        if let Value::Object(m) = &mut input {
            m.retain(|(k, _)| k != "expect");
        }
        let got = json::parse(&support::expectation(&support::run(&input))).unwrap();
        assert_eq!(&got, field(case, "expect"), "{:?}", field(case, "name"));
    }
}

#[test]
fn documents_and_results_fit_the_schema() {
    let schema = json::parse(&read(support::SCHEMA)).expect("schema JSON");
    let def = |name: &str| json::parse(&format!(r##"{{"$ref":"#/$defs/{name}"}}"##)).unwrap();
    for case in cases() {
        let name = field(&case, "name").clone();
        let result = support::run(&case);
        let problems = match field(&case, "fn") {
            Value::String(f)
                if ["render", "render_frame", "check", "plan"].contains(&f.as_str()) =>
            {
                let style = field(&case, "style");
                let problems = validate(&schema, &schema, style, "style");
                // A document the parser takes fits the schema, and one the
                // schema refuses is refused.
                let refused = matches!(&result, Err(e) if e.code == "invalid_style");
                assert_eq!(problems.is_empty(), !refused, "{name:?}: {problems:?}");
                match (f.as_str(), &result) {
                    ("check", Ok(report)) => validate(
                        &schema,
                        &def("Report"),
                        &json::parse(report).unwrap(),
                        "report",
                    ),
                    _ => vec![],
                }
            }
            Value::String(f) if f.starts_with("palette_from") => {
                let mut p = validate(&schema, &def("PaletteOpts"), field(&case, "opts"), "opts");
                if let Ok(found) = &result {
                    p.extend(validate(
                        &schema,
                        &def("PaletteResult"),
                        &json::parse(found).unwrap(),
                        "result",
                    ));
                }
                p
            }
            _ => vec![],
        };
        assert!(problems.is_empty(), "{name:?}: {problems:?}");
        if let Err(e) = &result {
            let error = json::parse(&e.to_json()).unwrap();
            let p = validate(&schema, &def("Error"), &error, "error");
            assert!(p.is_empty(), "{name:?}: {p:?}");
        }
    }
}

/// Every code the suite renders scans, held to its own report's bar.
#[test]
fn rendered_cases_scan() {
    let mut verified = 0;
    for case in cases() {
        if field(&case, "fn") != &Value::String("render".into())
            || field(field(&case, "expect"), "error") != &Value::Null
        {
            continue;
        }
        let mut style = String::new();
        json::write_compact(&mut style, field(&case, "style"));
        let (text, bytes) = match field(&case, "payload") {
            Value::Object(m) => match &m[0] {
                (k, Value::String(t)) if k == "text" => (t.clone(), None),
                (_, Value::String(b)) => (String::new(), Some(b.clone())),
                _ => panic!(),
            },
            _ => panic!(),
        };
        // Byte payloads aren't text; the decoders read them as Latin-1.
        if bytes.is_some() {
            continue;
        }
        let (s, qr) = freeqrent_doc::prepare(Payload::Text(&text), &style).unwrap();
        freeqrent_verify::verify_style(&s, &qr, &text)
            .unwrap_or_else(|e| panic!("{:?}: {e:?}", field(&case, "name")));
        verified += 1;
    }
    assert!(verified >= 35, "{verified}");
}
