//! Style documents against the builder API: every key reaches the option it
//! names, errors name their path, and the boundary functions agree with the
//! core.

use freeqrent::{
    AlignmentShape, AlignmentStyle, Axis, Border, Caption, CaptionFont, ColorSpread, Corner,
    CubicBezier, Ease, Ecc, EciPolicy, Effect, Emphasis, EyeShape, FinderStyle, Fit, Frame,
    FrameRing, Halftone, Idle, Ink, Intro, Logo, LogoFrame, LogoIntro, LogoShape, LogoSize,
    LogoTiming, Mask, ModuleShape, Palette, Preset, QrCode, Rgba8Image, RingShape, StaggerOrder,
    Style, TargetSize, Texture, TimingStyle, TrackingGrid, Trigger, Version,
};
use freeqrent_doc::{Payload, parse};

const MARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#a3302a"/></svg>"##;
/// A 1×1 PNG.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn base64(bytes: &[u8]) -> String {
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

/// A 16×16 picture: a dark disc on a light page.
fn picture() -> Rgba8Image {
    let mut image = Rgba8Image::filled(16, 16, [245, 240, 230, 255]).unwrap();
    for y in 0..16i32 {
        for x in 0..16i32 {
            if (x - 8).pow(2) + (y - 8).pow(2) < 30 {
                image.set_pixel(x as u32, y as u32, [30, 40, 90, 255]);
            }
        }
    }
    image
}

fn image_json(image: &Rgba8Image) -> String {
    format!(
        r#"{{"width":{},"height":{},"rgba":"{}"}}"#,
        image.width(),
        image.height(),
        base64(image.data())
    )
}

fn style(doc: &str) -> Style {
    parse(doc).unwrap_or_else(|e| panic!("{doc}: {e}")).style
}

fn tokens(t: &[(&str, &str)]) -> Palette {
    Palette::from_tokens(t.iter().copied()).unwrap()
}

#[test]
fn the_empty_document_is_the_default_style() {
    let doc = parse("{}").unwrap();
    assert_eq!(doc.style, Style::new());
    assert_eq!(doc.encoder, QrCode::builder());
    assert_eq!(style(r#"{"preset":null,"modules":null}"#), Style::new());
}

#[test]
fn presets_are_bases() {
    for name in Style::presets() {
        assert_eq!(
            style(&format!(r#"{{"preset":"{name}"}}"#)),
            Style::preset(name).unwrap()
        );
    }
    let ink = Style::preset("ink").unwrap();
    let palette = ink
        .colors()
        .clone()
        .with(
            freeqrent::Token::Fg,
            freeqrent::Color::parse("#123").unwrap(),
        )
        .unwrap();
    assert_eq!(
        style(r##"{"preset":"ink","modules":"square","palette":{"fg":"#123"}}"##),
        ink.modules(ModuleShape::SQUARE).palette(palette)
    );
}

#[test]
fn shapes_and_scan_boxes() {
    for (json, shape) in [
        (r#""square""#, ModuleShape::Square { gap: 0.0 }),
        (
            r#"{"kind":"square","gap":0.1}"#,
            ModuleShape::Square { gap: 0.1 },
        ),
        (r#""rounded""#, ModuleShape::Rounded { radius: 0.35 }),
        (
            r#"{"kind":"dots","size":0.8}"#,
            ModuleShape::Dots { size: 0.8 },
        ),
        (r#""liquid""#, ModuleShape::Liquid { smoothness: 0.8 }),
        (
            r#""bars""#,
            ModuleShape::Bars {
                axis: Axis::Vertical,
                radius: 0.5,
            },
        ),
        (
            r#"{"kind":"bars","axis":"horizontal","radius":0.3}"#,
            ModuleShape::Bars {
                axis: Axis::Horizontal,
                radius: 0.3,
            },
        ),
    ] {
        assert_eq!(
            style(&format!(r#"{{"modules":{json}}}"#)),
            Style::new().modules(shape),
            "{json}"
        );
    }
    let finders = FinderStyle::new(RingShape::Rounded(2.0), EyeShape::Rounded(1.0))
        .corner(Corner::TopRight, RingShape::Leaf, EyeShape::Leaf)
        .corner(Corner::BottomLeft, RingShape::Square, EyeShape::Circle);
    assert_eq!(
        style(
            r#"{"finders":{"ring":{"kind":"rounded","radius":2},
                "corners":{"top-right":{"ring":"leaf"},"bottom-left":{"ring":"square","eye":"circle"}}}}"#
        ),
        Style::new().finders(finders)
    );
    // An eye alone keeps the base's ring.
    assert_eq!(
        style(r#"{"preset":"ink","finders":{"eye":"squircle"}}"#),
        Style::preset("ink")
            .unwrap()
            .finders(FinderStyle::new(RingShape::Square, EyeShape::Squircle))
    );
    assert_eq!(
        style(
            r#"{"alignment":{"shape":{"kind":"own","ring":{"kind":"rounded","radius":2.5}},"emphasis":"outline"},"timing":"dashed"}"#
        ),
        Style::new()
            .alignment(AlignmentStyle::new(
                AlignmentShape::Own {
                    outer: RingShape::Rounded(2.5),
                    eye: EyeShape::Circle
                },
                Emphasis::Outline,
            ))
            .timing(TimingStyle::Dashed)
    );
    assert_eq!(
        style(r#"{"alignment":{"emphasis":"none"}}"#),
        Style::new().alignment(AlignmentStyle::new(AlignmentShape::Match, Emphasis::None))
    );
}

#[test]
fn colors_frames_and_text() {
    let palette = tokens(&[
        ("bg", "#0d1b12"),
        ("fg", "linear-gradient(135deg, #7dffa8, #fff)"),
    ]);
    assert_eq!(
        style(
            r##"{"palette":{"bg":"#0d1b12","fg":"linear-gradient(135deg, #7dffa8, #fff)"},
                "allow_inverted":true,"quiet_zone":2,"seed":18446744073709551615,
                "frame":{"kind":"circle","ring":"dashed"},"title":"T","description":"D",
                "label":"L","id_prefix":"q","target_size":{"kind":"millimeters","mm":30},
                "minify":false,"theme_transition_ms":400,"idle":["shimmer","breathe"]}"##
        ),
        Style::new()
            .palette(palette)
            .allow_inverted(true)
            .quiet_zone(2)
            .seed(u64::MAX)
            .frame(Frame::Circle {
                filler_density: 0.5,
                ring: FrameRing::Dashed
            })
            .title("T")
            .description("D")
            .label("L")
            .id_prefix("q")
            .target_size(TargetSize::Millimeters(30.0))
            .minify(false)
            .theme_transition(400)
            .idle(Idle::Shimmer)
            .idle(Idle::Breathe)
    );
    let dark = tokens(&[("fg", "#1d3b72")]).dark_twin();
    assert_eq!(
        style(r##"{"palette":{"fg":"#1d3b72"},"dark":true}"##),
        Style::new().palette(dark).allow_inverted(true)
    );
    assert_eq!(
        style(r#"{"frame":{"kind":"rounded"},"target_size":{"kind":"pixels","px":300}}"#),
        Style::new()
            .frame(Frame::RoundedSquare(2.0))
            .target_size(TargetSize::Pixels(300))
    );
    assert_eq!(
        style(r#"{"frame":"none"}"#),
        Style::new().frame(Frame::None)
    );
    assert_eq!(
        style(r#"{"border":"solid","shadow":1.5}"#),
        Style::new()
            .border(Border::Solid { width: 0.5 })
            .shadow(1.5)
    );
    assert_eq!(
        style(r#"{"border":{"kind":"double","width":2}}"#),
        Style::new().border(Border::Double { width: 2.0 })
    );
    assert_eq!(
        style(r#"{"border":"double"}"#).border_style(),
        Some(Border::Double { width: 1.0 })
    );
    assert_eq!(
        style(r#"{"effect":"glow"}"#).effect_style(),
        Some(Effect::Glow {
            radius: 0.3,
            strength: 0.25
        })
    );
    assert_eq!(
        style(r#"{"effect":{"kind":"emboss","depth":0.08}}"#),
        Style::new().effect(Effect::Emboss { depth: 0.08 })
    );
    assert_eq!(
        style(r#"{"effect":"sketch"}"#).effect_style(),
        Some(Effect::Sketch { roughness: 0.08 })
    );
    assert_eq!(
        style(r#"{"caption":"Scan me"}"#),
        Style::new().caption(Caption::new("Scan me"))
    );
    assert_eq!(
        style(r#"{"caption":{"text":"MENU","size":3,"font":"mono","bold":false,"tracking":20}}"#),
        Style::new().caption(
            Caption::new("MENU")
                .size(3.0)
                .font(CaptionFont::Mono)
                .bold(false)
                .tracking(20)
        )
    );
    assert_eq!(
        style(
            r#"{"textures":["grain",{"kind":"ink","fade":0.1},{"kind":"scanlines","opacity":0.2}]}"#
        ),
        Style::new()
            .texture(Texture::Grain { amount: 0.08 })
            .texture(Texture::Ink { fade: 0.1 })
            .texture(Texture::Scanlines { opacity: 0.2 })
    );
    assert_eq!(
        style(r##"{"palette":{"fg.2":"#7a0000"},"color_spread":"regions"}"##),
        Style::new()
            .palette(tokens(&[("fg.2", "#7a0000")]))
            .color_spread(ColorSpread::Regions)
    );
}

#[test]
fn logos() {
    let doc = format!(
        r#"{{"logo":{{"svg":{},"size":"large","scale":0.2,"frame":{{"kind":"orbit","width":0.5,"moon":2}},
            "shape":{{"kind":"rounded","radius":2}},"padding":0.1,"backdrop":false,"clear_modules":false}}}}"#,
        freeqrent_doc_json(MARK)
    );
    let logo = Logo::svg(MARK)
        .unwrap()
        .size(LogoSize::Large)
        .scale(0.2)
        .frame(LogoFrame::Orbit)
        .frame_width(0.5)
        .moon_size(2.0)
        .shape(LogoShape::Rounded(2.0))
        .padding(0.1)
        .backdrop(false)
        .clear_modules(false);
    assert_eq!(style(&doc), Style::new().logo(logo));
    let raster = freeqrent_doc_b64(PNG);
    assert_eq!(
        style(&format!(
            r#"{{"logo":{{"raster":"{PNG}","frame":"ring","shape":"circle"}}}}"#
        )),
        Style::new().logo(
            Logo::raster(raster, freeqrent::RasterFormat::Png)
                .unwrap()
                .frame(LogoFrame::Ring)
                .shape(LogoShape::Circle)
        )
    );
}

fn freeqrent_doc_json(s: &str) -> String {
    let mut out = String::new();
    freeqrent_doc::json::write_str(&mut out, s);
    out
}

fn freeqrent_doc_b64(s: &str) -> Vec<u8> {
    const ABC: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim_end_matches('=').chars() {
        acc = (acc << 6) | ABC.find(c).unwrap() as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

#[test]
fn pictures() {
    let p = picture();
    let doc = format!(
        r#"{{"halftone":{{"image":{img},"ink":{{"kind":"color","colors":3}},"bands":[0.3,0.9],
            "clarity":0.2,"fit":"contain","focal":[0.4,0.6],"priority":{img},"steer":false}}}}"#,
        img = image_json(&p)
    );
    let h = Halftone::new(p.clone())
        .ink(Ink::Color { colors: 3 })
        .bands(0.3, 0.9)
        .clarity(freeqrent::Clarity::Uniform(0.2))
        .fit(Fit::Contain)
        .focal(0.4, 0.6)
        .priority(p.clone())
        .steer(false);
    assert_eq!(style(&doc), Style::new().halftone(h));
    assert_eq!(
        style(&format!(
            r#"{{"halftone":{{"image":{img},"clarity":{img}}}}}"#,
            img = image_json(&p)
        )),
        Style::new().halftone(Halftone::new(p.clone()).clarity(freeqrent::Clarity::Map(p.clone())))
    );
    // Art mode takes the standard grid unless the encoding says otherwise.
    let doc = parse(&format!(r#"{{"art":{{"image":{}}}}}"#, image_json(&p))).unwrap();
    assert_eq!(doc.style, Style::art(Halftone::new(p.clone())));
    assert_eq!(
        doc.encoder,
        QrCode::builder().tracking_grid(TrackingGrid::Standard)
    );
    let doc = parse(&format!(
        r#"{{"art":{{"image":{}}},"encoding":{{"version":9}}}}"#,
        image_json(&p)
    ))
    .unwrap();
    assert_eq!(
        doc.encoder,
        QrCode::builder().version(Version::new(9).unwrap())
    );
}

#[test]
fn intros() {
    assert_eq!(
        style(r#"{"intro":"ripple"}"#),
        Style::new().intro(Preset::Ripple)
    );
    let doc = format!(
        r#"{{"logo":{{"svg":{}}},"intro":{{"preset":"lock-on","duration_ms":900,"delay_ms":100,
            "ease":{{"kind":"custom","x1":0.2,"y1":1,"x2":0.4,"y2":1}},"stagger":{{"kind":"random","seed":7}},
            "groups":4,"trigger":{{"kind":"every","ms":5000}},"logo":"spin","logo_timing":"before",
            "frame":"draw","frame_timing":"with"}}}}"#,
        freeqrent_doc_json(MARK)
    );
    let intro = Intro::new(Preset::LockOn)
        .duration_ms(900)
        .delay_ms(100)
        .ease(Ease::Custom(CubicBezier::new(0.2, 1.0, 0.4, 1.0)))
        .stagger(StaggerOrder::Random(7))
        .groups(4)
        .trigger(Trigger::Every(5000))
        .logo(LogoIntro::Spin)
        .logo_timing(LogoTiming::Before)
        .frame(LogoIntro::Draw)
        .frame_timing(LogoTiming::With);
    assert_eq!(
        style(&doc),
        Style::new().logo(Logo::svg(MARK).unwrap()).intro(intro)
    );
    assert_eq!(
        style(
            r#"{"intro":{"ease":"expo","stagger":"polar","trigger":{"kind":"class","name":"in"}}}"#
        ),
        Style::new().intro(
            Intro::new(Preset::Settle)
                .ease(Ease::Expo)
                .stagger(StaggerOrder::Polar)
                .trigger(Trigger::Class("in".into()))
        )
    );
    assert_eq!(
        style(r##"{"intro":{"palette_from":{"fg":"#a33"},"duration_ms":800,"trigger":"hover"}}"##),
        Style::new().intro(
            Intro::palette_from(&tokens(&[("fg", "#a33")]))
                .duration_ms(800)
                .trigger(Trigger::Hover)
        )
    );
}

#[test]
fn encoding() {
    let doc = parse(r#"{"encoding":{"ecc":"Q","min_version":3,"max_version":9,"boost_ecc":false,"mask":5,"eci":"always"}}"#).unwrap();
    assert_eq!(
        doc.encoder,
        QrCode::builder()
            .ecc(Ecc::Q)
            .min_version(Version::new(3).unwrap())
            .max_version(Version::new(9).unwrap())
            .boost_ecc(false)
            .mask(Mask::new(5).unwrap())
            .eci(EciPolicy::Always)
    );
    let doc = parse(r#"{"encoding":{"tracking_grid":"dense"}}"#).unwrap();
    assert_eq!(
        doc.encoder,
        QrCode::builder().tracking_grid(TrackingGrid::Dense)
    );
}

#[test]
fn errors_name_their_path() {
    let img = image_json(&picture());
    for (doc, code, path) in [
        ("[]".to_string(), "invalid_style", ""),
        ("{".to_string(), "invalid_style", ""),
        (r#"{"shape":"dots"}"#.into(), "invalid_style", "shape"),
        (r#"{"preset":"nope"}"#.into(), "invalid_style", "preset"),
        (
            r#"{"modules":{"kind":"hex"}}"#.into(),
            "invalid_style",
            "modules.kind",
        ),
        (
            r#"{"modules":{"kind":"dots","radius":1}}"#.into(),
            "invalid_style",
            "modules.radius",
        ),
        (
            r#"{"modules":{"size":1}}"#.into(),
            "invalid_style",
            "modules.kind",
        ),
        (r#"{"modules":4}"#.into(), "invalid_style", "modules"),
        (
            r#"{"finders":{"corners":{"middle":{}}}}"#.into(),
            "invalid_style",
            "finders.corners.middle",
        ),
        (
            r#"{"finders":{"ring":"rounded","eye":{"kind":"leaf","radius":1}}}"#.into(),
            "invalid_style",
            "finders.eye.radius",
        ),
        (
            r#"{"alignment":{"shape":"own"}}"#.into(),
            "invalid_style",
            "alignment.shape",
        ),
        (
            r##"{"palette":{"ink":"#000"}}"##.into(),
            "invalid_style",
            "palette.ink",
        ),
        (
            r#"{"palette":{"fg":"blurple"}}"#.into(),
            "invalid_style",
            "palette.fg",
        ),
        (
            r#"{"palette":{"timing":"linear-gradient(90deg, #000, #333)"}}"#.into(),
            "invalid_style",
            "palette.timing",
        ),
        (
            r#"{"quiet_zone":256}"#.into(),
            "invalid_style",
            "quiet_zone",
        ),
        (r#"{"quiet_zone":-1}"#.into(), "invalid_style", "quiet_zone"),
        (r#"{"seed":1.5}"#.into(), "invalid_style", "seed"),
        (r#"{"dark":"yes"}"#.into(), "invalid_style", "dark"),
        (r#"{"idle":["spin"]}"#.into(), "invalid_style", "idle[0]"),
        (r#"{"logo":{}}"#.into(), "invalid_style", "logo"),
        (
            r#"{"logo":{"svg":"<html/>"}}"#.into(),
            "invalid_logo",
            "logo.svg",
        ),
        (
            r#"{"logo":{"raster":"AAAA"}}"#.into(),
            "invalid_style",
            "logo.raster",
        ),
        (
            r#"{"logo":{"raster":"*"}}"#.into(),
            "invalid_style",
            "logo.raster",
        ),
        (
            r#"{"intro":{"logo":"spin"}}"#.into(),
            "invalid_style",
            "intro.logo",
        ),
        (
            r#"{"intro":{"palette_from":{},"preset":"ripple"}}"#.into(),
            "invalid_style",
            "intro.preset",
        ),
        (
            r#"{"intro":{"trigger":"every"}}"#.into(),
            "invalid_style",
            "intro.trigger",
        ),
        (
            r#"{"intro":{"trigger":{"kind":"class","name":""}}}"#.into(),
            "invalid_style",
            "intro.trigger.name",
        ),
        (
            r#"{"intro":{"ease":"custom"}}"#.into(),
            "invalid_style",
            "intro.ease",
        ),
        (
            r#"{"intro":{"duration_ms":"1s"}}"#.into(),
            "invalid_style",
            "intro.duration_ms",
        ),
        (
            r#"{"halftone":{}}"#.into(),
            "invalid_style",
            "halftone.image",
        ),
        (
            r#"{"halftone":{"image":{"width":2,"height":2,"rgba":"AAAA"}}}"#.into(),
            "invalid_image",
            "halftone.image",
        ),
        (
            format!(r#"{{"halftone":{{"image":{img},"bands":[1]}}}}"#),
            "invalid_style",
            "halftone.bands",
        ),
        (
            format!(r#"{{"art":{{"image":{img}}},"preset":"ink"}}"#),
            "invalid_style",
            "art",
        ),
        (
            r#"{"encoding":{"version":41}}"#.into(),
            "invalid_style",
            "encoding.version",
        ),
        (
            r#"{"encoding":{"version":2,"tracking_grid":"sparse"}}"#.into(),
            "invalid_style",
            "encoding",
        ),
        (
            r#"{"encoding":{"mask":8}}"#.into(),
            "invalid_style",
            "encoding.mask",
        ),
        (
            r#"{"encoding":{"ecc":"m"}}"#.into(),
            "invalid_style",
            "encoding.ecc",
        ),
        (
            r#"{"target_size":"pixels"}"#.into(),
            "invalid_style",
            "target_size",
        ),
    ] {
        let e = parse(&doc).expect_err(&doc);
        assert_eq!(e.code, code, "{doc}: {e}");
        let want = format!(r#"{{"path":"{path}""#);
        assert!(
            e.detail.starts_with(&want),
            "{doc}: {} (want path {path:?})",
            e.detail
        );
    }
}

#[test]
fn boundary_functions_match_the_core() {
    let doc = r#"{"preset":"candy","intro":"settle","encoding":{"ecc":"Q"}}"#;
    let text = "https://example.com/doc";
    let style = Style::preset("candy").unwrap().intro(Preset::Settle);
    let qr = QrCode::builder()
        .ecc(Ecc::Q)
        .encode_text_for(text, &style)
        .unwrap();
    let qr_text = qr.clone();
    assert_eq!(
        freeqrent_doc::render(Payload::Text(text), doc).unwrap(),
        style.render(&qr).unwrap()
    );
    assert_eq!(
        freeqrent_doc::plan(Payload::Text(text), doc).unwrap(),
        style.plan(&qr).unwrap().to_json()
    );

    let qr = QrCode::builder()
        .ecc(Ecc::Q)
        .encode_bytes_for(b"\x00\xff", &style)
        .unwrap();
    assert_eq!(
        freeqrent_doc::render(Payload::Bytes(b"\x00\xff"), doc).unwrap(),
        style.render(&qr).unwrap()
    );

    let report = freeqrent_doc::check(Payload::Text(text), doc).unwrap();
    assert!(
        report.starts_with(r#"{"ok":true,"errors":[],"warnings":[{"code":"rounded_finder""#),
        "{report}"
    );
    let code = report
        .split(r#""logo_scale":null,"steering":null,"code":"#)
        .nth(1)
        .expect("the report ends with the code");
    assert_eq!(
        code,
        format!(
            r#"{{"version":{},"ecc":"{:?}","size":{},"mask":{}}}}}"#,
            qr_text.version().number(),
            qr_text.ecc(),
            qr_text.size(),
            qr_text.mask().pattern()
        )
    );

    let refused =
        freeqrent_doc::check(Payload::Text(text), r##"{"palette":{"fg":"#ddd"}}"##).unwrap();
    assert!(
        refused.starts_with(r#"{"ok":false,"errors":[{"code":"contrast_too_low""#),
        "{refused}"
    );
    let e =
        freeqrent_doc::render(Payload::Text(text), r##"{"palette":{"fg":"#ddd"}}"##).unwrap_err();
    assert_eq!(e.code, "contrast_too_low");
    assert!(
        e.detail.starts_with(r#"{"token":"fg","delta_l":0."#),
        "{}",
        e.detail
    );

    let e = freeqrent_doc::render(Payload::Text(&"9".repeat(8000)), "{}").unwrap_err();
    assert_eq!(e.code, "data_too_long");
    assert_eq!(
        e.to_json().split(r#""detail":"#).nth(1).unwrap(),
        r#"{"bits_needed":26685,"bits_available":18672,"max_version":40,"ecc":"M"}}"#
    );

    assert_eq!(
        freeqrent_doc::presets(),
        r#"["classic","ink","candy","terminal","sunset","bubble","ribbon","leaf"]"#
    );
    assert_eq!(freeqrent_doc::version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn palettes() {
    let css = ":root { --brand: #1d3b72; --paper: #f8f5ee; }";
    let json = freeqrent_doc::palette_from_css(css, "").unwrap();
    let (palette, _) = Palette::from_css(css).unwrap();
    assert!(
        json.starts_with(&format!(
            r##"{{"palette":{},"swatches":[{{"color":"#"##,
            freeqrent_doc::palette_json(&palette)
        )),
        "{json}"
    );
    // The palette goes straight back into a document.
    let palette_part = &json[11..json.find(r#","swatches""#).unwrap()];
    assert_eq!(
        style(&format!(r#"{{"palette":{palette_part}}}"#)).colors(),
        &palette
    );

    let html = r#"<style>body{background:#f8f5ee;color:#1d3b72}</style>"#;
    assert!(
        freeqrent_doc::palette_from_html(
            html,
            r##"{"allow_inverted":false,"logo_colors":["#c33"]}"##
        )
        .is_ok()
    );
    assert_eq!(
        freeqrent_doc::palette_from_css("", "{}").unwrap_err().code,
        "no_colors"
    );
    assert_eq!(
        freeqrent_doc::palette_from_css(css, r#"{"k":3}"#)
            .unwrap_err()
            .code,
        "invalid_style"
    );

    let p = picture();
    let json = freeqrent_doc::palette_from_image(
        p.data(),
        16,
        16,
        r#"{"clusters":3,"seed":2,"min_contrast":0.6}"#,
    )
    .unwrap();
    let (palette, _) = Palette::from_image(
        &p,
        freeqrent::PaletteOpts::new()
            .clusters(3)
            .seed(2)
            .min_contrast(0.6),
    )
    .unwrap();
    assert!(json.starts_with(&format!(
        r#"{{"palette":{}"#,
        freeqrent_doc::palette_json(&palette)
    )));
    let e = freeqrent_doc::palette_from_image(&[0; 12], 2, 2, "").unwrap_err();
    assert_eq!(e.code, "invalid_image");
    assert_eq!(e.detail, r#"{"path":"","expected":16,"actual":12}"#);
}

/// Documents render codes that scan, held to their own report's bar.
#[test]
fn documents_render_codes_that_scan() {
    let img = image_json(&picture());
    let docs = [
        r#"{}"#.to_string(),
        r#"{"preset":"bubble","intro":"ripple","idle":["finder-pulse"]}"#.into(),
        r#"{"preset":"terminal","modules":{"kind":"dots","size":0.9}}"#.into(),
        r##"{"palette":{"fg":"#1d3b72"},"dark":true,"frame":{"kind":"rounded","radius":1}}"##
            .into(),
        format!(
            r#"{{"preset":"leaf","logo":{{"svg":{},"frame":"ring"}},"encoding":{{"ecc":"Q"}}}}"#,
            freeqrent_doc_json(MARK)
        ),
        format!(r#"{{"halftone":{{"image":{img}}}}}"#),
        format!(r#"{{"art":{{"image":{img},"ink":"mono"}}}}"#),
    ];
    let text = "https://example.com/scan";
    for doc in &docs {
        let (style, qr) = freeqrent_doc::prepare(Payload::Text(text), doc).unwrap();
        freeqrent_verify::verify_style(&style, &qr, text)
            .unwrap_or_else(|e| panic!("{doc}: {e:?}"));
    }
}

/// `render_frame` is the core's still frame, and the frame after the
/// intro scans, held to the style's own bar.
#[test]
fn frames_are_the_cores_still_frames() {
    let text = "https://example.com/frame";
    let doc = r#"{"preset":"candy","intro":{"preset":"ripple","duration_ms":900}}"#;
    let (style, qr) = freeqrent_doc::prepare(Payload::Text(text), doc).unwrap();
    for t in [0, 300, 900, 4000] {
        assert_eq!(
            freeqrent_doc::render_frame(Payload::Text(text), doc, t).unwrap(),
            style.render_frame(&qr, t).unwrap()
        );
    }
    let total = style.plan(&qr).unwrap().total_ms;
    let after = freeqrent_doc::render_frame(Payload::Text(text), doc, total).unwrap();
    assert!(!after.contains("@keyframes"));
    let bar = freeqrent_verify::Bar::for_report(&style.check(&qr));
    freeqrent_verify::verify_with(&after, text, bar).unwrap();
    let e = freeqrent_doc::render_frame(Payload::Text(text), r##"{"palette":{"fg":"#ccc"}}"##, 0)
        .unwrap_err();
    assert_eq!(e.code, "contrast_too_low");
}
