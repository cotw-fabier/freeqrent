//! The Style document (SPEC §10.1): a JSON object that describes a
//! [`Style`] and how to encode for it. A document is a base (a `preset`,
//! `art`, or the default style) plus the parts it gives, each replacing the
//! base's; `palette` tokens merge onto the base's palette. Every key is
//! optional, and unknown keys are refused.
//!
//! The value names here are the ones the CLI takes (SPEC §10.5), and the
//! schema (`schema/freeqrent.schema.json`) is generated from these tables.

use freeqrent::{
    AlignmentShape, AlignmentStyle, Axis, Border, ColorSpread, Corner, CubicBezier, Ease, Ecc,
    EciPolicy, Effect, Emphasis, EyeShape, FinderStyle, Fit, Frame, FrameRing, Halftone, Idle, Ink,
    Intro, Logo, LogoFrame, LogoIntro, LogoShape, LogoSize, LogoTiming, Mask, ModuleShape, Paint,
    Palette, Preset, QrBuilder, QrCode, RasterFormat, RingShape, StaggerOrder, Style, TargetSize,
    TimingStyle, Token, TrackingGrid, Trigger, Version,
};

use crate::error::{DocError, logo_error};
use crate::json::{self, Value};
use crate::read::{
    Obj, array_of, base64_of, bool_of, f32_of, image_of, name_of, pair_of, str_of, tagged, u8_of,
    u32_of, u64_of, unknown,
};

/// A parsed document: the style, and the encoder that fits codes to it.
#[derive(Clone, Debug)]
pub struct Document {
    /// The style.
    pub style: Style,
    /// The encoder options. Payloads are encoded with
    /// [`QrBuilder::encode_text_for`] or [`QrBuilder::encode_bytes_for`],
    /// so a logo grows the code instead of being refused.
    pub encoder: QrBuilder,
}

/// The top-level keys, in schema order.
pub const STYLE_KEYS: &[&str] = &[
    "preset",
    "art",
    "modules",
    "finders",
    "alignment",
    "timing",
    "palette",
    "dark",
    "quiet_zone",
    "allow_inverted",
    "frame",
    "border",
    "shadow",
    "color_spread",
    "effect",
    "seed",
    "logo",
    "halftone",
    "intro",
    "idle",
    "theme_transition_ms",
    "title",
    "description",
    "label",
    "id_prefix",
    "target_size",
    "minify",
    "encoding",
];

/// `modules.kind`.
pub const MODULE_SHAPES: &[&str] = &["square", "rounded", "dots", "liquid", "bars"];
/// Ring shapes; `rounded` takes a `radius`.
pub const RINGS: &[&str] = &["square", "rounded", "squircle", "leaf"];
/// Eye shapes; `rounded` takes a `radius`.
pub const EYES: &[&str] = &["square", "rounded", "circle", "squircle", "leaf"];
/// `finders.corners` keys.
pub const CORNERS: &[&str] = &["top-left", "top-right", "bottom-left"];
/// `alignment.emphasis`.
pub const EMPHASES: &[&str] = &["none", "accent", "outline"];
/// `timing`.
pub const TIMINGS: &[&str] = &["plain", "dotted", "dashed"];
/// `frame.kind`.
pub const FRAMES: &[&str] = &["square", "rounded", "circle", "none"];
/// `frame.ring`.
pub const FRAME_RINGS: &[&str] = &["none", "solid", "dashed"];
/// `effect.kind`; a bare name takes the defaults.
pub const EFFECTS: &[&str] = &["glow", "emboss", "sketch"];
/// `color_spread`.
pub const SPREADS: &[&str] = &["modules", "regions"];
/// `border.kind`; a bare name takes the default width.
pub const BORDERS: &[&str] = &["solid", "double"];
/// `logo.shape.kind`.
pub const LOGO_SHAPES: &[&str] = &["square", "rounded", "circle"];
/// `halftone.ink.kind`.
pub const INKS: &[&str] = &["mono", "color"];
/// `halftone.fit`.
pub const FITS: &[&str] = &["cover", "contain"];
/// `intro.ease.kind`.
pub const EASES: &[&str] = &["quart", "quint", "expo", "custom"];
/// `intro.stagger.kind`.
pub const STAGGERS: &[&str] = &["radial", "linear", "diagonal", "polar", "random"];
/// `intro.trigger.kind`.
pub const TRIGGERS: &[&str] = &["load", "hover", "class", "every"];
/// `target_size.kind`.
pub const TARGETS: &[&str] = &["pixels", "millimeters"];
/// `encoding.ecc`.
pub const ECCS: &[&str] = &["L", "M", "Q", "H"];
/// `encoding.tracking_grid`.
pub const GRIDS: &[&str] = &["sparse", "standard", "dense"];
/// `encoding.eci`.
pub const ECIS: &[&str] = &["auto", "never", "always"];

/// The names of `items`, by `name`.
pub fn names<T: Copy>(items: &[T], name: fn(T) -> &'static str) -> Vec<&'static str> {
    items.iter().map(|&i| name(i)).collect()
}

/// `intro.preset`.
pub fn intro_presets() -> Vec<&'static str> {
    names(&Preset::ALL, Preset::name)
}
/// `intro.logo` and `intro.frame`.
pub fn logo_intros() -> Vec<&'static str> {
    names(&LogoIntro::ALL, LogoIntro::name)
}
/// `intro.logo_timing` and `intro.frame_timing`.
pub fn logo_timings() -> Vec<&'static str> {
    names(&LogoTiming::ALL, LogoTiming::name)
}
/// `idle` entries.
pub fn idles() -> Vec<&'static str> {
    names(&Idle::ALL, Idle::name)
}
/// `logo.size`.
pub fn logo_sizes() -> Vec<&'static str> {
    names(&LogoSize::ALL, LogoSize::name)
}
/// `logo.frame.kind`.
pub fn logo_frames() -> Vec<&'static str> {
    names(&LogoFrame::ALL, LogoFrame::name)
}
/// `palette` keys.
pub fn tokens() -> Vec<&'static str> {
    names(&Token::ALL, Token::name)
}

/// Parses a Style document.
pub fn parse(text: &str) -> Result<Document, DocError> {
    let value = json::parse(text).map_err(|e| {
        let mut d = crate::error::Detail::new();
        d.str("path", "");
        d.str("reason", &e.reason);
        d.int("offset", e.offset);
        DocError::new(
            "invalid_style",
            format!("not JSON: {} at byte {}", e.reason, e.offset),
            d,
        )
    })?;
    document(&value)
}

fn document(v: &Value) -> Result<Document, DocError> {
    let mut o = Obj::new(v, "")?;
    let preset = o.opt("preset", |v, p| {
        let name = str_of(v, p)?;
        Style::preset(name).ok_or_else(|| unknown(p, name, Style::presets()))
    })?;
    let art = o.opt("art", halftone)?;
    if art.is_some() && (preset.is_some() || o.has("halftone")) {
        return Err(DocError::invalid(
            "art",
            "art sets the whole style; use halftone with preset",
        ));
    }
    let is_art = art.is_some();
    let mut style = match (art, preset) {
        (Some(h), _) => Style::art(h),
        (None, Some(p)) => p,
        (None, None) => Style::new(),
    };

    if let Some(shape) = o.opt("modules", modules)? {
        style = style.modules(shape);
    }
    if let Some((value, path)) = o.get("finders") {
        let base = style.finder_style();
        style = style.finders(finders(value, &path, base)?);
    }
    if let Some((value, path)) = o.get("alignment") {
        let base = style.alignment_style();
        style = style.alignment(alignment(value, &path, base)?);
    }
    if let Some(t) = o.opt("timing", |v, p| {
        Ok(match name_of(v, p, TIMINGS)? {
            "plain" => TimingStyle::Plain,
            "dotted" => TimingStyle::Dotted,
            _ => TimingStyle::Dashed,
        })
    })? {
        style = style.timing(t);
    }
    let dark = o.opt("dark", bool_of)?.unwrap_or(false);
    let mut palette = style.colors().clone();
    if let Some((value, path)) = o.get("palette") {
        merge_palette(&mut palette, value, &path)?;
    }
    if dark {
        palette = palette.dark_twin();
    }
    style = style.palette(palette);
    if let Some(q) = o.opt("quiet_zone", u8_of)? {
        style = style.quiet_zone(q);
    }
    let inverted = o.opt("allow_inverted", bool_of)?;
    let allow = inverted.unwrap_or(style.inverted_allowed()) || dark;
    style = style.allow_inverted(allow);
    if let Some(f) = o.opt("frame", frame)? {
        style = style.frame(f);
    }
    if let Some(b) = o.opt("border", border)? {
        style = style.border(b);
    }
    if let Some(offset) = o.opt("shadow", f32_of)? {
        style = style.shadow(offset);
    }
    if let Some(spread) = o.opt("color_spread", |v, p| {
        Ok(match name_of(v, p, SPREADS)? {
            "modules" => ColorSpread::Modules,
            _ => ColorSpread::Regions,
        })
    })? {
        style = style.color_spread(spread);
    }
    if let Some(e) = o.opt("effect", effect)? {
        style = style.effect(e);
    }
    if let Some(seed) = o.opt("seed", u64_of)? {
        style = style.seed(seed);
    }
    if let Some(h) = o.opt("halftone", halftone)? {
        style = style.halftone(h);
    }
    let has_logo = o.has("logo");
    if let Some(logo) = o.opt("logo", logo)? {
        style = style.logo(logo);
    }
    if let Some(i) = o.opt("intro", |v, p| intro(v, p, dark, has_logo))? {
        style = style.intro(i);
    }
    if let Some((value, path)) = o.get("idle") {
        for (i, item) in array_of(value, &path)?.iter().enumerate() {
            let p = format!("{path}[{i}]");
            let name = name_of(item, &p, &idles())?;
            let idle = Idle::ALL
                .into_iter()
                .find(|x| x.name() == name)
                .expect("listed");
            style = style.idle(idle);
        }
    }
    if let Some(ms) = o.opt("theme_transition_ms", u32_of)? {
        style = style.theme_transition(ms);
    }
    if let Some(s) = o.opt("title", str_of)? {
        style = style.title(s);
    }
    if let Some(s) = o.opt("description", str_of)? {
        style = style.description(s);
    }
    if let Some(s) = o.opt("label", str_of)? {
        style = style.label(s);
    }
    if let Some(s) = o.opt("id_prefix", str_of)? {
        style = style.id_prefix(s);
    }
    if let Some(t) = o.opt("target_size", target_size)? {
        style = style.target_size(t);
    }
    if let Some(m) = o.opt("minify", bool_of)? {
        style = style.minify(m);
    }
    let encoder = match o.get("encoding") {
        Some((v, p)) => encoding(v, &p, is_art)?,
        None if is_art => QrCode::builder().tracking_grid(TrackingGrid::Standard),
        None => QrCode::builder(),
    };
    o.finish()?;
    Ok(Document { style, encoder })
}

/// Reads `key` as a number, or `default` when it's absent.
fn num_or(o: &mut Obj<'_>, key: &str, default: f32) -> Result<f32, DocError> {
    Ok(o.opt(key, f32_of)?.unwrap_or(default))
}

/// The object of a tagged union that may have none, for reading defaults.
fn or_empty<'a>(o: Option<Obj<'a>>, empty: &'a Value) -> Obj<'a> {
    o.unwrap_or_else(|| Obj::new(empty, "").expect("an object"))
}

static EMPTY: Value = Value::Object(Vec::new());

fn modules(v: &Value, path: &str) -> Result<ModuleShape, DocError> {
    let (kind, o) = tagged(v, path, MODULE_SHAPES)?;
    let mut o = or_empty(o, &EMPTY);
    let shape = match kind {
        "square" => ModuleShape::Square {
            gap: num_or(&mut o, "gap", 0.0)?,
        },
        "rounded" => ModuleShape::Rounded {
            radius: num_or(&mut o, "radius", 0.35)?,
        },
        "dots" => ModuleShape::Dots {
            size: num_or(&mut o, "size", 0.85)?,
        },
        "liquid" => ModuleShape::Liquid {
            smoothness: num_or(&mut o, "smoothness", 0.8)?,
        },
        _ => ModuleShape::Bars {
            axis: match o.opt("axis", |v, p| name_of(v, p, &["horizontal", "vertical"]))? {
                Some("horizontal") => Axis::Horizontal,
                _ => Axis::Vertical,
            },
            radius: num_or(&mut o, "radius", 0.5)?,
        },
    };
    o.finish()?;
    Ok(shape)
}

fn ring(v: &Value, path: &str) -> Result<RingShape, DocError> {
    let (kind, o) = tagged(v, path, RINGS)?;
    let mut o = or_empty(o, &EMPTY);
    let shape = match kind {
        "square" => RingShape::Square,
        "rounded" => RingShape::Rounded(num_or(&mut o, "radius", 1.5)?),
        "squircle" => RingShape::Squircle,
        _ => RingShape::Leaf,
    };
    o.finish()?;
    Ok(shape)
}

fn eye(v: &Value, path: &str) -> Result<EyeShape, DocError> {
    let (kind, o) = tagged(v, path, EYES)?;
    let mut o = or_empty(o, &EMPTY);
    let shape = match kind {
        "square" => EyeShape::Square,
        "rounded" => EyeShape::Rounded(num_or(&mut o, "radius", 0.75)?),
        "circle" => EyeShape::Circle,
        "squircle" => EyeShape::Squircle,
        _ => EyeShape::Leaf,
    };
    o.finish()?;
    Ok(shape)
}

/// The eye that goes with a ring when only the ring is given, as in the
/// CLI.
fn matching_eye(ring: RingShape) -> EyeShape {
    match ring {
        RingShape::Rounded(r) => EyeShape::Rounded((r / 2.0).min(1.5)),
        RingShape::Squircle => EyeShape::Squircle,
        RingShape::Leaf => EyeShape::Leaf,
        _ => EyeShape::Square,
    }
}

/// `{"ring", "eye"}`: either may be left out; a ring alone brings its
/// matching eye.
fn ring_and_eye(
    o: &mut Obj<'_>,
    base: (RingShape, EyeShape),
) -> Result<(RingShape, EyeShape), DocError> {
    let r = o.opt("ring", ring)?;
    let e = o.opt("eye", eye)?;
    Ok(match (r, e) {
        (None, None) => base,
        (Some(r), None) => (r, matching_eye(r)),
        (None, Some(e)) => (base.0, e),
        (Some(r), Some(e)) => (r, e),
    })
}

fn finders(v: &Value, path: &str, base: FinderStyle) -> Result<FinderStyle, DocError> {
    let mut o = Obj::new(v, path)?;
    let given = o.has("ring") || o.has("eye");
    let (r, e) = ring_and_eye(&mut o, (base.outer, base.eye))?;
    let mut style = if !given { base } else { FinderStyle::new(r, e) };
    if let Some((value, cpath)) = o.get("corners") {
        for (name, value, p) in Obj::new(value, &cpath)?.entries() {
            let corner = match name {
                "top-left" => Corner::TopLeft,
                "top-right" => Corner::TopRight,
                "bottom-left" => Corner::BottomLeft,
                _ => return Err(unknown(&p, name, CORNERS)),
            };
            let mut c = Obj::new(value, &p)?;
            let (r, e) = ring_and_eye(&mut c, style.at(corner))?;
            c.finish()?;
            style = style.corner(corner, r, e);
        }
    }
    o.finish()?;
    Ok(style)
}

fn alignment(v: &Value, path: &str, base: AlignmentStyle) -> Result<AlignmentStyle, DocError> {
    let mut o = Obj::new(v, path)?;
    let shape = match o.opt("shape", |v, p| tagged(v, p, &["match", "own"]))? {
        None => base.shape,
        Some(("match", s)) => {
            if let Some(s) = s {
                s.finish()?;
            }
            AlignmentShape::Match
        }
        Some((_, s)) => {
            let Some(mut s) = s else {
                return Err(DocError::invalid(
                    &format!("{path}.shape"),
                    "\"own\" needs an object with a ring",
                ));
            };
            let outer = s.req("ring", ring)?;
            let eye = match s.opt("eye", eye)? {
                Some(e) => e,
                None => match outer {
                    RingShape::Rounded(_) => EyeShape::Circle,
                    other => matching_eye(other),
                },
            };
            s.finish()?;
            AlignmentShape::Own { outer, eye }
        }
    };
    let emphasis = match o.opt("emphasis", |v, p| name_of(v, p, EMPHASES))? {
        None => base.emphasis,
        Some("none") => Emphasis::None,
        Some("accent") => Emphasis::Accent,
        Some(_) => Emphasis::Outline,
    };
    o.finish()?;
    Ok(AlignmentStyle::new(shape, emphasis))
}

/// Token paints by name, set onto `palette`.
fn merge_palette(palette: &mut Palette, v: &Value, path: &str) -> Result<(), DocError> {
    for (name, value, p) in Obj::new(v, path)?.entries() {
        let token = Token::from_name(name).ok_or_else(|| unknown(&p, name, &tokens()))?;
        let css = str_of(value, &p)?;
        let paint = Paint::parse(css).map_err(|e| DocError::invalid(&p, e.to_string()))?;
        palette
            .set(token, paint)
            .map_err(|e| DocError::invalid(&p, e.to_string()))?;
    }
    Ok(())
}

/// A palette from token paints alone.
pub(crate) fn palette_of(v: &Value, path: &str) -> Result<Palette, DocError> {
    let mut palette = Palette::new();
    merge_palette(&mut palette, v, path)?;
    Ok(palette)
}

fn frame(v: &Value, path: &str) -> Result<Frame, DocError> {
    let (kind, o) = tagged(v, path, FRAMES)?;
    let mut o = or_empty(o, &EMPTY);
    let frame = match kind {
        "square" => Frame::Square,
        "rounded" => Frame::RoundedSquare(num_or(&mut o, "radius", 2.0)?),
        "circle" => Frame::Circle {
            filler_density: num_or(&mut o, "filler_density", 0.5)?,
            ring: match o.opt("ring", |v, p| name_of(v, p, FRAME_RINGS))? {
                None | Some("none") => FrameRing::None,
                Some("solid") => FrameRing::Solid,
                Some(_) => FrameRing::Dashed,
            },
        },
        _ => Frame::None,
    };
    o.finish()?;
    Ok(frame)
}

fn effect(v: &Value, path: &str) -> Result<Effect, DocError> {
    let (kind, o) = tagged(v, path, EFFECTS)?;
    let mut o = or_empty(o, &EMPTY);
    let effect = match kind {
        "glow" => Effect::Glow {
            radius: num_or(&mut o, "radius", 0.3)?,
            strength: num_or(&mut o, "strength", 0.25)?,
        },
        "emboss" => Effect::Emboss {
            depth: num_or(&mut o, "depth", 0.1)?,
        },
        _ => Effect::Sketch {
            roughness: num_or(&mut o, "roughness", 0.08)?,
        },
    };
    o.finish()?;
    Ok(effect)
}

fn border(v: &Value, path: &str) -> Result<Border, DocError> {
    let (kind, o) = tagged(v, path, BORDERS)?;
    let mut o = or_empty(o, &EMPTY);
    let border = match kind {
        "solid" => Border::Solid {
            width: num_or(&mut o, "width", 0.5)?,
        },
        _ => Border::Double {
            width: num_or(&mut o, "width", 1.0)?,
        },
    };
    o.finish()?;
    Ok(border)
}

fn target_size(v: &Value, path: &str) -> Result<TargetSize, DocError> {
    let (kind, o) = tagged(v, path, TARGETS)?;
    let Some(mut o) = o else {
        return Err(DocError::invalid(
            path,
            format!("{kind:?} needs an object with a size"),
        ));
    };
    let t = match kind {
        "pixels" => TargetSize::Pixels(o.req("px", u32_of)?),
        _ => TargetSize::Millimeters(o.req("mm", f32_of)?),
    };
    o.finish()?;
    Ok(t)
}

fn logo(v: &Value, path: &str) -> Result<Logo, DocError> {
    let mut o = Obj::new(v, path)?;
    let mut logo = match (o.has("svg"), o.has("raster")) {
        (true, false) => {
            let (svg, p) = o.get("svg").expect("present");
            Logo::svg(str_of(svg, &p)?).map_err(|e| logo_error(&p, e))?
        }
        (false, true) => {
            let (raster, p) = o.get("raster").expect("present");
            let bytes = base64_of(raster, &p)?;
            let format = RasterFormat::sniff(&bytes)
                .ok_or_else(|| DocError::invalid(&p, "not a PNG, JPEG, GIF or WebP image"))?;
            Logo::raster(bytes, format).map_err(|e| logo_error(&p, e))?
        }
        _ => {
            return Err(DocError::invalid(
                path,
                "a logo needs exactly one of svg or raster",
            ));
        }
    };
    if let Some(size) = o.opt("size", |v, p| name_of(v, p, &logo_sizes()))? {
        logo = logo.size(
            *LogoSize::ALL
                .iter()
                .find(|s| s.name() == size)
                .expect("listed"),
        );
    }
    if let Some(scale) = o.opt("scale", f32_of)? {
        logo = logo.scale(scale);
    }
    if let Some((value, p)) = o.get("frame") {
        let (kind, f) = tagged(value, &p, &logo_frames())?;
        let frame = *LogoFrame::ALL
            .iter()
            .find(|f| f.name() == kind)
            .expect("listed");
        logo = logo.frame(frame);
        if let Some(mut f) = f {
            if let Some(w) = f.opt("width", f32_of)? {
                logo = logo.frame_width(w);
            }
            if let Some(m) = f.opt("moon", f32_of)? {
                logo = logo.moon_size(m);
            }
            f.finish()?;
        }
    }
    if let Some(shape) = o.opt("shape", |v, p| {
        let (kind, s) = tagged(v, p, LOGO_SHAPES)?;
        let mut s = or_empty(s, &EMPTY);
        let shape = match kind {
            "square" => LogoShape::Square,
            "rounded" => LogoShape::Rounded(num_or(&mut s, "radius", 1.0)?),
            _ => LogoShape::Circle,
        };
        s.finish()?;
        Ok(shape)
    })? {
        logo = logo.shape(shape);
    }
    if let Some(p) = o.opt("padding", f32_of)? {
        logo = logo.padding(p);
    }
    if let Some(b) = o.opt("backdrop", bool_of)? {
        logo = logo.backdrop(b);
    }
    if let Some(c) = o.opt("clear_modules", bool_of)? {
        logo = logo.clear_modules(c);
    }
    o.finish()?;
    Ok(logo)
}

fn halftone(v: &Value, path: &str) -> Result<Halftone, DocError> {
    let mut o = Obj::new(v, path)?;
    let mut h = Halftone::new(o.req("image", image_of)?);
    if let Some(ink) = o.opt("ink", |v, p| {
        let (kind, i) = tagged(v, p, INKS)?;
        let mut i = or_empty(i, &EMPTY);
        let ink = match kind {
            "mono" => Ink::Mono,
            _ => Ink::Color {
                colors: i.opt("colors", u8_of)?.unwrap_or(6),
            },
        };
        i.finish()?;
        Ok(ink)
    })? {
        h = h.ink(ink);
    }
    if let Some((dark, light)) = o.opt("bands", pair_of)? {
        h = h.bands(dark, light);
    }
    if let Some(c) = o.opt("clarity", |v, p| match v {
        Value::Number(_) => f32_of(v, p).map(freeqrent::Clarity::Uniform),
        _ => image_of(v, p).map(freeqrent::Clarity::Map),
    })? {
        h = h.clarity(c);
    }
    if let Some(fit) = o.opt("fit", |v, p| name_of(v, p, FITS))? {
        h = h.fit(if fit == "cover" {
            Fit::Cover
        } else {
            Fit::Contain
        });
    }
    if let Some((x, y)) = o.opt("focal", pair_of)? {
        h = h.focal(x, y);
    }
    if let Some(map) = o.opt("priority", image_of)? {
        h = h.priority(map);
    }
    if let Some(steer) = o.opt("steer", bool_of)? {
        h = h.steer(steer);
    }
    o.finish()?;
    Ok(h)
}

fn ease(v: &Value, path: &str) -> Result<Ease, DocError> {
    let (kind, o) = tagged(v, path, EASES)?;
    Ok(match (kind, o) {
        ("custom", None) => {
            return Err(DocError::invalid(
                path,
                "\"custom\" needs an object with x1, y1, x2, y2",
            ));
        }
        ("custom", Some(mut o)) => {
            let curve = CubicBezier::new(
                o.req("x1", f32_of)?,
                o.req("y1", f32_of)?,
                o.req("x2", f32_of)?,
                o.req("y2", f32_of)?,
            );
            o.finish()?;
            Ease::Custom(curve)
        }
        (kind, o) => {
            if let Some(o) = o {
                o.finish()?;
            }
            match kind {
                "quart" => Ease::Quart,
                "quint" => Ease::Quint,
                _ => Ease::Expo,
            }
        }
    })
}

fn stagger(v: &Value, path: &str) -> Result<StaggerOrder, DocError> {
    let (kind, o) = tagged(v, path, STAGGERS)?;
    let mut o = or_empty(o, &EMPTY);
    let order = match kind {
        "radial" => StaggerOrder::Radial,
        "linear" => StaggerOrder::Linear,
        "diagonal" => StaggerOrder::Diagonal,
        "polar" => StaggerOrder::Polar,
        _ => StaggerOrder::Random(o.opt("seed", u64_of)?.unwrap_or(0)),
    };
    o.finish()?;
    Ok(order)
}

fn trigger(v: &Value, path: &str) -> Result<Trigger, DocError> {
    let (kind, o) = tagged(v, path, TRIGGERS)?;
    let needs =
        |what: &str| DocError::invalid(path, format!("{kind:?} needs an object with {what}"));
    let t = match (kind, o) {
        ("class", None) => return Err(needs("a name")),
        ("every", None) => return Err(needs("ms")),
        ("class", Some(mut o)) => {
            let name = o.req("name", str_of)?;
            if name.is_empty() {
                return Err(DocError::invalid(
                    &format!("{path}.name"),
                    "empty class name",
                ));
            }
            o.finish()?;
            Trigger::Class(name.to_string())
        }
        ("every", Some(mut o)) => {
            let ms = o.req("ms", u32_of)?;
            o.finish()?;
            Trigger::Every(ms)
        }
        (kind, o) => {
            if let Some(o) = o {
                o.finish()?;
            }
            if kind == "load" {
                Trigger::Load
            } else {
                Trigger::Hover
            }
        }
    };
    Ok(t)
}

fn logo_intro(v: &Value, path: &str) -> Result<LogoIntro, DocError> {
    let name = name_of(v, path, &logo_intros())?;
    Ok(LogoIntro::ALL
        .into_iter()
        .find(|i| i.name() == name)
        .expect("listed"))
}

fn logo_timing(v: &Value, path: &str) -> Result<LogoTiming, DocError> {
    let name = name_of(v, path, &logo_timings())?;
    Ok(LogoTiming::ALL
        .into_iter()
        .find(|t| t.name() == name)
        .expect("listed"))
}

/// A preset name, or `{"preset", …}`, or `{"palette_from", …}` for a
/// palette shift (SPEC §6.5).
fn intro(v: &Value, path: &str, dark: bool, has_logo: bool) -> Result<Intro, DocError> {
    let preset_of = |v: &Value, p: &str| {
        let name = name_of(v, p, &intro_presets())?;
        Ok(Preset::ALL
            .into_iter()
            .find(|x| x.name() == name)
            .expect("listed"))
    };
    if let Value::String(_) = v {
        return preset_of(v, path).map(Intro::new);
    }
    let mut o = Obj::new(v, path)?;
    let shift = o.opt("palette_from", |v, p| {
        let old = palette_of(v, p)?;
        Ok(if dark { old.dark_twin() } else { old })
    })?;
    let mut intro = match shift {
        Some(old) => {
            for key in [
                "preset",
                "stagger",
                "groups",
                "logo",
                "logo_timing",
                "frame",
                "frame_timing",
                "delay_ms",
            ] {
                if o.has(key) {
                    return Err(DocError::invalid(
                        &format!("{path}.{key}"),
                        "a palette shift takes only duration_ms, ease and trigger",
                    ));
                }
            }
            Intro::palette_from(&old)
        }
        None => Intro::new(o.opt("preset", preset_of)?.unwrap_or(Preset::Settle)),
    };
    if let Some(ms) = o.opt("duration_ms", u32_of)? {
        intro = intro.duration_ms(ms);
    }
    if let Some(ms) = o.opt("delay_ms", u32_of)? {
        intro = intro.delay_ms(ms);
    }
    if let Some(e) = o.opt("ease", ease)? {
        intro = intro.ease(e);
    }
    if let Some(s) = o.opt("stagger", stagger)? {
        intro = intro.stagger(s);
    }
    if let Some(g) = o.opt("groups", u8_of)? {
        intro = intro.groups(g);
    }
    if let Some(t) = o.opt("trigger", trigger)? {
        intro = intro.trigger(t);
    }
    for key in ["logo", "logo_timing", "frame", "frame_timing"] {
        if o.has(key) && !has_logo {
            return Err(DocError::invalid(&format!("{path}.{key}"), "needs a logo"));
        }
    }
    if let Some(l) = o.opt("logo", logo_intro)? {
        intro = intro.logo(l);
    }
    if let Some(t) = o.opt("logo_timing", logo_timing)? {
        intro = intro.logo_timing(t);
    }
    if let Some(f) = o.opt("frame", logo_intro)? {
        intro = intro.frame(f);
    }
    if let Some(t) = o.opt("frame_timing", logo_timing)? {
        intro = intro.frame_timing(t);
    }
    o.finish()?;
    Ok(intro)
}

fn version_of(v: &Value, path: &str) -> Result<Version, DocError> {
    let n = u8_of(v, path)?;
    Version::new(n).ok_or_else(|| DocError::invalid(path, "versions are 1 to 40"))
}

/// `encoding`: how payloads are encoded. Art mode defaults to the standard
/// tracking grid unless a version or grid is given, as in the CLI.
fn encoding(v: &Value, path: &str, art: bool) -> Result<QrBuilder, DocError> {
    let mut o = Obj::new(v, path)?;
    let mut b = QrCode::builder();
    if let Some(ecc) = o.opt("ecc", |v, p| name_of(v, p, ECCS))? {
        b = b.ecc(match ecc {
            "L" => Ecc::L,
            "M" => Ecc::M,
            "Q" => Ecc::Q,
            _ => Ecc::H,
        });
    }
    let version = o.opt("version", version_of)?;
    let grid = o.opt("tracking_grid", |v, p| {
        Ok(match name_of(v, p, GRIDS)? {
            "sparse" => TrackingGrid::Sparse,
            "standard" => TrackingGrid::Standard,
            _ => TrackingGrid::Dense,
        })
    })?;
    let range = (
        o.opt("min_version", version_of)?,
        o.opt("max_version", version_of)?,
    );
    let pinned = version.is_some() || grid.is_some() || range != (None, None);
    if version.is_some() as u8 + grid.is_some() as u8 + (range != (None, None)) as u8 > 1 {
        return Err(DocError::invalid(
            path,
            "give one of version, tracking_grid, or min_version/max_version",
        ));
    }
    if let Some(v) = version {
        b = b.version(v);
    }
    if let Some(g) = grid {
        b = b.tracking_grid(g);
    }
    if let Some(min) = range.0 {
        b = b.min_version(min);
    }
    if let Some(max) = range.1 {
        b = b.max_version(max);
    }
    if art && !pinned {
        b = b.tracking_grid(TrackingGrid::Standard);
    }
    if let Some(boost) = o.opt("boost_ecc", bool_of)? {
        b = b.boost_ecc(boost);
    }
    if let Some((value, p)) = o.get("mask") {
        let n = u8_of(value, &p)?;
        b = b.mask(Mask::new(n).ok_or_else(|| DocError::invalid(&p, "masks are 0 to 7"))?);
    }
    if let Some(eci) = o.opt("eci", |v, p| name_of(v, p, ECIS))? {
        b = b.eci(match eci {
            "auto" => EciPolicy::Auto,
            "never" => EciPolicy::Never,
            _ => EciPolicy::Always,
        });
    }
    o.finish()?;
    Ok(b)
}
