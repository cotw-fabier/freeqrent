//! The schema against the parser: the checked-in file is the generated one,
//! documents the parser takes fit it, and its validator refuses what the
//! parser refuses for type and name reasons.

mod support;

use freeqrent_doc::json::{self, Value};
use support::validate::validate;

fn schema() -> Value {
    json::parse(&freeqrent_doc::schema::schema_json()).expect("the schema is JSON")
}

#[test]
fn the_schema_file_is_current() {
    let file = std::fs::read_to_string(support::root().join(support::SCHEMA)).unwrap_or_default();
    assert!(
        file == freeqrent_doc::schema::schema_json(),
        "schema/freeqrent.schema.json is stale: run `cargo run -p freeqrent-doc --example generate`"
    );
}

/// Documents both the parser and the schema take.
#[test]
fn good_documents_fit() {
    let schema = schema();
    let image = r#"{"width":1,"height":1,"rgba":"AAAA/w=="}"#;
    for doc in [
        "{}".to_string(),
        r#"{"preset":"ink","modules":{"kind":"dots","size":0.9},"finders":{"ring":"rounded","corners":{"top-left":{"eye":{"kind":"rounded","radius":1}}}}}"#.into(),
        r#"{"alignment":{"shape":{"kind":"own","ring":"square","eye":"circle"},"emphasis":"none"},"timing":"dotted"}"#.into(),
        r##"{"palette":{"fg":"#123","finder.top-left.eye":"red"},"dark":true,"quiet_zone":3,"allow_inverted":false,"seed":12}"##.into(),
        r#"{"frame":{"kind":"circle","ring":"solid"},"logo":{"svg":"<svg viewBox=\"0 0 4 4\"/>","frame":{"kind":"orbit","moon":2},"shape":"circle","size":"medium"}}"#.into(),
        format!(r#"{{"halftone":{{"image":{image},"ink":{{"kind":"color","colors":4}},"bands":[0.3,0.9],"clarity":{image},"fit":"cover","focal":[0.5,0.5],"priority":{image},"steer":true}}}}"#),
        format!(r#"{{"art":{{"image":{image}}},"encoding":{{"tracking_grid":"dense","ecc":"M"}}}}"#),
        r#"{"intro":{"preset":"iris","ease":{"kind":"custom","x1":0,"y1":0,"x2":1,"y2":1},"stagger":"radial","trigger":{"kind":"class","name":"x"}}}"#.into(),
        r#"{"intro":{"palette_from":{"fg":"red"},"trigger":{"kind":"every","ms":4000}},"idle":["breathe"]}"#.into(),
        r#"{"intro":"develop","target_size":{"kind":"pixels","px":200},"title":"t","minify":false,"theme_transition_ms":0}"#.into(),
    ] {
        let problems = validate(&schema, &schema, &json::parse(&doc).unwrap(), "");
        assert!(problems.is_empty(), "{doc}: {problems:?}");
        let parsed = freeqrent_doc::parse(&doc);
        assert!(parsed.is_ok(), "{doc}: {parsed:?}");
    }
}

/// Documents both refuse: wrong names, types, keys and shapes.
#[test]
fn bad_documents_fail_both() {
    let schema = schema();
    for doc in [
        r#"{"shape":"dots"}"#,
        r#"{"preset":"nope"}"#,
        r#"{"modules":"hex"}"#,
        r#"{"modules":{"kind":"dots","radius":1}}"#,
        r#"{"modules":{"size":1}}"#,
        r#"{"finders":{"corners":{"middle":{}}}}"#,
        r#"{"finders":{"eye":{"kind":"leaf","radius":1}}}"#,
        r#"{"alignment":{"shape":"own"}}"#,
        r##"{"palette":{"ink":"#000"}}"##,
        r#"{"palette":{"fg":1}}"#,
        r#"{"quiet_zone":256}"#,
        r#"{"quiet_zone":1.5}"#,
        r#"{"dark":"yes"}"#,
        r#"{"idle":["spin"]}"#,
        r#"{"idle":"shimmer"}"#,
        r#"{"logo":{}}"#,
        r#"{"logo":{"svg":"<svg/>","raster":"AA=="}}"#,
        r#"{"logo":{"raster":"*"}}"#,
        r#"{"intro":{"palette_from":{},"preset":"ripple"}}"#,
        r#"{"intro":{"trigger":"every"}}"#,
        r#"{"intro":{"trigger":{"kind":"class","name":""}}}"#,
        r#"{"intro":{"ease":"custom"}}"#,
        r#"{"intro":{"duration_ms":"1s"}}"#,
        r#"{"intro":"bloom"}"#,
        r#"{"halftone":{}}"#,
        r#"{"halftone":{"image":{"width":1,"height":1,"rgba":"AAAA/w=="},"bands":[1]}}"#,
        r#"{"encoding":{"version":41}}"#,
        r#"{"encoding":{"mask":8}}"#,
        r#"{"encoding":{"ecc":"m"}}"#,
        r#"{"target_size":"pixels"}"#,
        r#"{"target_size":{"kind":"pixels"}}"#,
    ] {
        let problems = validate(&schema, &schema, &json::parse(doc).unwrap(), "");
        assert!(!problems.is_empty(), "the schema takes {doc}");
        let e = freeqrent_doc::parse(doc).expect_err(doc);
        assert_eq!(e.code, "invalid_style", "{doc}");
    }
}

/// The schema's names are the parser's and the core's.
#[test]
fn names_match_the_tables() {
    let text = freeqrent_doc::schema::schema_json();
    for name in freeqrent::Style::presets()
        .iter()
        .copied()
        .chain(freeqrent_doc::style::tokens())
        .chain(freeqrent_doc::style::idles())
        .chain(freeqrent_doc::style::intro_presets())
        .chain(freeqrent_doc::style::logo_intros())
        .chain(freeqrent_doc::style::STYLE_KEYS.iter().copied())
    {
        assert!(text.contains(&format!("\"{name}\"")), "{name}");
    }
}
