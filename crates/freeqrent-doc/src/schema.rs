//! The JSON Schema (draft 2020-12) of Style documents and of everything the
//! boundary returns, generated from the name tables the parser reads
//! ([`crate::style`]) and the core's own name lists, so the two can't drift
//! apart. `schema/freeqrent.schema.json` is this output, checked in; the
//! bindings generate their types from it.

use freeqrent::Style;

use crate::json::{self, Value};
use crate::style::{
    BORDERS, CORNERS, EASES, ECCS, ECIS, EFFECTS, EMPHASES, EYES, FITS, FRAME_RINGS, FRAMES, GRIDS,
    INKS, LOGO_SHAPES, MODULE_SHAPES, RINGS, SPREADS, STAGGERS, STYLE_KEYS, TARGETS, TIMINGS,
    TRIGGERS, idles, intro_presets, logo_frames, logo_intros, logo_sizes, logo_timings, tokens,
};

fn s(text: &str) -> Value {
    Value::String(text.to_string())
}

fn n(x: impl ToString) -> Value {
    Value::Number(x.to_string())
}

fn obj(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn ty(t: &str) -> Value {
    obj(vec![("type", s(t))])
}

fn described(mut schema: Value, text: &str) -> Value {
    if let Value::Object(members) = &mut schema {
        members.insert(0, ("description".to_string(), s(text)));
    }
    schema
}

fn number() -> Value {
    ty("number")
}

fn int(max: u64) -> Value {
    obj(vec![
        ("type", s("integer")),
        ("minimum", n(0)),
        ("maximum", n(max)),
    ])
}

fn int_range(min: u64, max: u64) -> Value {
    obj(vec![
        ("type", s("integer")),
        ("minimum", n(min)),
        ("maximum", n(max)),
    ])
}

fn boolean() -> Value {
    ty("boolean")
}

fn string() -> Value {
    ty("string")
}

fn r(name: &str) -> Value {
    obj(vec![("$ref", s(&format!("#/$defs/{name}")))])
}

fn one_of(names: &[&str]) -> Value {
    obj(vec![(
        "enum",
        Value::Array(names.iter().map(|x| s(x)).collect()),
    )])
}

fn array(items: Value) -> Value {
    obj(vec![("type", s("array")), ("items", items)])
}

fn pair() -> Value {
    obj(vec![
        ("type", s("array")),
        ("items", number()),
        ("minItems", n(2)),
        ("maxItems", n(2)),
    ])
}

/// A closed object: these properties, these required.
fn props(properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut members = vec![("type", s("object")), ("properties", obj(properties))];
    if !required.is_empty() {
        members.push((
            "required",
            Value::Array(required.iter().map(|x| s(x)).collect()),
        ));
    }
    members.push(("additionalProperties", Value::Bool(false)));
    obj(members)
}

/// One `kind`-tagged variant.
fn variant(kind: &str, fields: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut properties = vec![("kind", obj(vec![("const", s(kind))]))];
    properties.extend(fields);
    let mut req = vec!["kind"];
    req.extend_from_slice(required);
    props(properties, &req)
}

/// A tagged union: the bare names of the variants without data, then
/// every variant as an object.
fn union(bare: &[&str], variants: Vec<Value>) -> Value {
    let mut options = Vec::new();
    if !bare.is_empty() {
        options.push(one_of(bare));
    }
    options.extend(variants);
    obj(vec![("oneOf", Value::Array(options))])
}

/// The variants of `names` that take no data, as `variant`s.
fn plain(names: &[&str], with_data: &[&str]) -> Vec<Value> {
    names
        .iter()
        .filter(|x| !with_data.contains(x))
        .map(|x| variant(x, vec![], &[]))
        .collect()
}

fn without<'a>(names: &[&'a str], with_data: &[&str]) -> Vec<&'a str> {
    names
        .iter()
        .copied()
        .filter(|x| !with_data.contains(x))
        .collect()
}

fn module_shape() -> Value {
    let field = |kind: &str, key: &str| variant(kind, vec![(key, number())], &[]);
    union(
        MODULE_SHAPES,
        vec![
            field("square", "gap"),
            field("rounded", "radius"),
            field("dots", "size"),
            field("liquid", "smoothness"),
            variant(
                "bars",
                vec![
                    ("axis", one_of(&["horizontal", "vertical"])),
                    ("radius", number()),
                ],
                &[],
            ),
        ],
    )
}

fn shape_with_radius(names: &[&str]) -> Value {
    let mut variants = plain(names, &["rounded"]);
    variants.push(variant("rounded", vec![("radius", number())], &[]));
    union(names, variants)
}

fn style() -> Value {
    let properties: Vec<(&str, Value)> = vec![
        (
            "preset",
            described(
                one_of(Style::presets()),
                "The base style: a preset (Style::preset).",
            ),
        ),
        (
            "art",
            described(
                r("Halftone"),
                "The base style: art mode for this picture (Style::art). Not with preset or halftone.",
            ),
        ),
        ("modules", r("ModuleShape")),
        ("finders", r("Finders")),
        ("alignment", r("Alignment")),
        ("timing", one_of(TIMINGS)),
        (
            "palette",
            described(
                r("Palette"),
                "Token paints, merged onto the base's palette.",
            ),
        ),
        (
            "dark",
            described(
                boolean(),
                "Render the palette's dark twin and allow inverted colors.",
            ),
        ),
        ("quiet_zone", int(255)),
        ("allow_inverted", boolean()),
        ("frame", r("Frame")),
        ("border", r("Border")),
        (
            "shadow",
            described(
                number(),
                "A hard shadow's offset down and right, in modules (0.25–3).",
            ),
        ),
        (
            "color_spread",
            described(
                one_of(SPREADS),
                "How data modules share fg and the data colors fg.2 to fg.4.",
            ),
        ),
        ("effect", r("Effect")),
        ("seed", int(u64::MAX)),
        ("logo", r("Logo")),
        ("halftone", r("Halftone")),
        ("intro", r("Intro")),
        ("idle", array(one_of(&idles()))),
        ("theme_transition_ms", int(u32::MAX as u64)),
        ("title", string()),
        ("description", string()),
        ("label", string()),
        ("id_prefix", string()),
        ("target_size", r("TargetSize")),
        ("minify", boolean()),
        ("encoding", r("Encoding")),
    ];
    debug_assert!(
        properties
            .iter()
            .map(|(k, _)| *k)
            .eq(STYLE_KEYS.iter().copied())
    );
    described(
        props(properties, &[]),
        "A Style document (SPEC §10.1). Every key is optional.",
    )
}

fn ring_and_eye() -> Value {
    props(vec![("ring", r("Ring")), ("eye", r("Eye"))], &[])
}

fn finders() -> Value {
    let corners = props(CORNERS.iter().map(|c| (*c, r("RingAndEye"))).collect(), &[]);
    props(
        vec![("ring", r("Ring")), ("eye", r("Eye")), ("corners", corners)],
        &[],
    )
}

fn alignment() -> Value {
    let shape = union(
        &["match"],
        vec![
            variant("match", vec![], &[]),
            variant(
                "own",
                vec![("ring", r("Ring")), ("eye", r("Eye"))],
                &["ring"],
            ),
        ],
    );
    props(vec![("shape", shape), ("emphasis", one_of(EMPHASES))], &[])
}

fn palette() -> Value {
    obj(vec![
        ("type", s("object")),
        ("propertyNames", one_of(&tokens())),
        (
            "additionalProperties",
            described(
                string(),
                "A CSS paint: a color, or a gradient where the token takes one.",
            ),
        ),
    ])
}

fn frame() -> Value {
    let mut variants = plain(FRAMES, &["rounded", "circle"]);
    variants.push(variant("rounded", vec![("radius", number())], &[]));
    variants.push(variant(
        "circle",
        vec![("filler_density", number()), ("ring", one_of(FRAME_RINGS))],
        &[],
    ));
    union(FRAMES, variants)
}

fn effect() -> Value {
    described(
        union(
            EFFECTS,
            vec![
                variant(
                    "glow",
                    vec![("radius", number()), ("strength", number())],
                    &[],
                ),
                variant("emboss", vec![("depth", number())], &[]),
                variant("sketch", vec![("roughness", number())], &[]),
            ],
        ),
        "A filter on the code: a glow in its own colors (radius 0.05–0.5 modules, default 0.3; strength 0–0.25, default 0.25) an emboss (depth 0.05–0.12, default 0.1), or a hand-drawn sketch (roughness 0.02–0.15 modules, default 0.08, seeded).",
    )
}

fn border() -> Value {
    union(
        BORDERS,
        BORDERS
            .iter()
            .map(|b| variant(b, vec![("width", number())], &[]))
            .collect(),
    )
}

fn logo() -> Value {
    let frames = logo_frames();
    let frame = union(
        &frames,
        frames
            .iter()
            .map(|f| variant(f, vec![("width", number()), ("moon", number())], &[]))
            .collect(),
    );
    let mut schema = props(
        vec![
            (
                "svg",
                described(string(), "An SVG document; it is sanitized."),
            ),
            (
                "raster",
                described(r("Base64"), "PNG, JPEG, GIF or WebP bytes."),
            ),
            ("size", one_of(&logo_sizes())),
            ("scale", number()),
            ("frame", frame),
            ("shape", shape_with_radius(LOGO_SHAPES)),
            ("padding", number()),
            ("backdrop", boolean()),
            ("clear_modules", boolean()),
        ],
        &[],
    );
    if let Value::Object(members) = &mut schema {
        members.push((
            "oneOf".to_string(),
            Value::Array(vec![
                obj(vec![("required", Value::Array(vec![s("svg")]))]),
                obj(vec![("required", Value::Array(vec![s("raster")]))]),
            ]),
        ));
    }
    schema
}

fn image() -> Value {
    described(
        props(
            vec![
                ("width", int(u32::MAX as u64)),
                ("height", int(u32::MAX as u64)),
                ("rgba", described(r("Base64"), "8-bit RGBA, row by row.")),
            ],
            &["width", "height", "rgba"],
        ),
        "Raw pixels.",
    )
}

fn halftone() -> Value {
    let mut inks = plain(INKS, &["color"]);
    inks.push(variant("color", vec![("colors", int(255))], &[]));
    props(
        vec![
            ("image", r("Image")),
            ("ink", union(INKS, inks)),
            ("bands", pair()),
            (
                "clarity",
                obj(vec![("oneOf", Value::Array(vec![number(), r("Image")]))]),
            ),
            ("fit", one_of(FITS)),
            ("focal", pair()),
            ("priority", r("Image")),
            ("steer", boolean()),
        ],
        &["image"],
    )
}

fn intro() -> Value {
    let entrance = props(
        vec![
            ("preset", one_of(&intro_presets())),
            ("duration_ms", int(u32::MAX as u64)),
            ("delay_ms", int(u32::MAX as u64)),
            ("ease", r("Ease")),
            ("stagger", r("Stagger")),
            ("groups", int(255)),
            ("trigger", r("Trigger")),
            ("logo", one_of(&logo_intros())),
            ("logo_timing", one_of(&logo_timings())),
            ("frame", one_of(&logo_intros())),
            ("frame_timing", one_of(&logo_timings())),
        ],
        &[],
    );
    let shift = described(
        props(
            vec![
                ("palette_from", r("Palette")),
                ("duration_ms", int(u32::MAX as u64)),
                ("ease", r("Ease")),
                ("trigger", r("Trigger")),
            ],
            &["palette_from"],
        ),
        "A palette shift: the code's colors cross-fade from palette_from.",
    );
    obj(vec![(
        "oneOf",
        Value::Array(vec![one_of(&intro_presets()), entrance, shift]),
    )])
}

fn ease() -> Value {
    let mut variants = plain(EASES, &["custom"]);
    variants.push(variant(
        "custom",
        vec![
            ("x1", number()),
            ("y1", number()),
            ("x2", number()),
            ("y2", number()),
        ],
        &["x1", "y1", "x2", "y2"],
    ));
    union(&without(EASES, &["custom"]), variants)
}

fn stagger() -> Value {
    let mut variants = plain(STAGGERS, &["random"]);
    variants.push(variant("random", vec![("seed", int(u64::MAX))], &[]));
    union(STAGGERS, variants)
}

fn trigger() -> Value {
    let mut variants = plain(TRIGGERS, &["class", "every"]);
    variants.push(variant(
        "class",
        vec![(
            "name",
            obj(vec![("type", s("string")), ("minLength", n(1))]),
        )],
        &["name"],
    ));
    variants.push(variant(
        "every",
        vec![("ms", int(u32::MAX as u64))],
        &["ms"],
    ));
    union(&without(TRIGGERS, &["class", "every"]), variants)
}

fn target_size() -> Value {
    debug_assert_eq!(TARGETS, ["pixels", "millimeters"]);
    union(
        &[],
        vec![
            variant("pixels", vec![("px", int(u32::MAX as u64))], &["px"]),
            variant("millimeters", vec![("mm", number())], &["mm"]),
        ],
    )
}

fn encoding() -> Value {
    props(
        vec![
            ("ecc", one_of(ECCS)),
            ("version", int_range(1, 40)),
            ("min_version", int_range(1, 40)),
            ("max_version", int_range(1, 40)),
            ("tracking_grid", one_of(GRIDS)),
            ("boost_ecc", boolean()),
            ("mask", int(7)),
            ("eci", one_of(ECIS)),
        ],
        &[],
    )
}

fn palette_opts() -> Value {
    props(
        vec![
            ("clusters", int(255)),
            ("seed", int(u64::MAX)),
            ("min_contrast", number()),
            ("allow_inverted", boolean()),
            ("logo_colors", array(string())),
        ],
        &[],
    )
}

fn nullable(t: &str) -> Value {
    obj(vec![("type", Value::Array(vec![s(t), s("null")]))])
}

fn error() -> Value {
    described(
        props(
            vec![
                ("code", string()),
                ("message", string()),
                ("detail", ty("object")),
            ],
            &["code", "message", "detail"],
        ),
        "An error, or one of a report's errors and warnings.",
    )
}

fn report() -> Value {
    let steering = props(
        vec![
            ("steerable_modules", int(u64::MAX)),
            ("code_modules", int(u64::MAX)),
            ("matched_modules", int(u64::MAX)),
        ],
        &["steerable_modules", "code_modules", "matched_modules"],
    );
    let code = props(
        vec![
            ("version", int_range(1, 40)),
            ("ecc", one_of(ECCS)),
            ("size", int(177)),
            ("mask", int(7)),
        ],
        &["version", "ecc", "size", "mask"],
    );
    let keys = [
        "ok",
        "errors",
        "warnings",
        "min_contrast",
        "ecc_budget_used",
        "logo_scale",
        "steering",
        "code",
    ];
    props(
        vec![
            ("ok", boolean()),
            ("errors", array(r("Error"))),
            ("warnings", array(r("Error"))),
            ("min_contrast", nullable("number")),
            ("ecc_budget_used", nullable("number")),
            ("logo_scale", nullable("number")),
            (
                "steering",
                obj(vec![("oneOf", Value::Array(vec![ty("null"), steering]))]),
            ),
            ("code", code),
        ],
        &keys,
    )
}

fn palette_result() -> Value {
    let swatch = props(
        vec![("color", string()), ("share", number())],
        &["color", "share"],
    );
    let origin = union(
        &[],
        vec![
            variant("swatch", vec![("swatch", int(u64::MAX))], &["swatch"]),
            variant("tinted", vec![("swatch", nullable("integer"))], &["swatch"]),
            variant(
                "darkened",
                vec![("swatch", int(u64::MAX)), ("delta_l", number())],
                &["swatch", "delta_l"],
            ),
            variant(
                "lightened",
                vec![("swatch", int(u64::MAX)), ("delta_l", number())],
                &["swatch", "delta_l"],
            ),
        ],
    );
    let choice = props(
        vec![
            ("token", one_of(&tokens())),
            ("color", string()),
            ("origin", origin),
        ],
        &["token", "color", "origin"],
    );
    props(
        vec![
            ("palette", r("Palette")),
            ("swatches", array(swatch)),
            ("choices", array(choice)),
        ],
        &["palette", "swatches", "choices"],
    )
}

/// The whole schema: a Style document at the root, every part in `$defs`.
pub fn schema() -> Value {
    let base64 = described(
        obj(vec![
            ("type", s("string")),
            ("contentEncoding", s("base64")),
            ("pattern", s("^[A-Za-z0-9+/]*={0,2}$")),
        ]),
        "Standard base64 (RFC 4648); padding optional.",
    );
    let defs = obj(vec![
        ("Style", style()),
        ("ModuleShape", module_shape()),
        ("Ring", shape_with_radius(RINGS)),
        ("Eye", shape_with_radius(EYES)),
        ("RingAndEye", ring_and_eye()),
        ("Finders", finders()),
        ("Alignment", alignment()),
        ("Palette", palette()),
        ("Frame", frame()),
        ("Border", border()),
        ("Effect", effect()),
        ("Logo", logo()),
        ("Base64", base64),
        ("Image", image()),
        ("Halftone", halftone()),
        ("Intro", intro()),
        ("Ease", ease()),
        ("Stagger", stagger()),
        ("Trigger", trigger()),
        ("TargetSize", target_size()),
        ("Encoding", encoding()),
        ("PaletteOpts", palette_opts()),
        ("Error", error()),
        ("Report", report()),
        ("PaletteResult", palette_result()),
    ]);
    obj(vec![
        ("$schema", s("https://json-schema.org/draft/2020-12/schema")),
        (
            "$id",
            s("https://github.com/freeqrent/freeqrent/schema/freeqrent.schema.json"),
        ),
        ("title", s("freeQRent Style document")),
        ("x-freeqrent-version", s(crate::version())),
        ("$ref", s("#/$defs/Style")),
        ("$defs", defs),
    ])
}

/// [`schema`] as the checked-in file's text.
pub fn schema_json() -> String {
    let mut out = String::new();
    json::write_pretty(&mut out, &schema());
    out
}
