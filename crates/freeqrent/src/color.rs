//! CSS colors (SPEC §6.1): parsing, OKLab conversion, gamut mapping, and a
//! canonical short serialization. A parsed color remembers the space it was
//! written in, so `oklch()` input stays `oklch()` in the output, but its
//! text is always regenerated: nothing from the input reaches the SVG
//! verbatim.

use core::fmt::{self, Write};
use core::str::FromStr;

use crate::math;
use crate::num::write_scaled;

/// A CSS color. Components are stored at fixed precision so that equal
/// colors compare equal and serialization round-trips exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    space: Space,
    /// Thousandths, 0–1000.
    alpha: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Space {
    Srgb([u8; 3]),
    /// L in hundredths of a percent, C in ten-thousandths, hue in
    /// hundredths of a degree (0–35999).
    Oklch {
        l: u16,
        c: u16,
        h: u16,
    },
    /// L in hundredths of a percent, a and b in ten-thousandths.
    Oklab {
        l: u16,
        a: i16,
        b: i16,
    },
}

/// Why a color or token string was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ColorError {
    /// Malformed input; `position` is the byte offset where parsing failed.
    Syntax {
        /// Byte offset into the input.
        position: usize,
    },
    /// An identifier that isn't a CSS named color.
    UnknownName,
    /// A token name that isn't in the token set (SPEC §6.1).
    UnknownToken,
    /// A gradient given for a token that only takes solid colors.
    GradientNotAllowed,
}

impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColorError::Syntax { position } => write!(f, "invalid color syntax at byte {position}"),
            ColorError::UnknownName => f.write_str("unknown color name"),
            ColorError::UnknownToken => f.write_str("unknown color token"),
            ColorError::GradientNotAllowed => f.write_str("this token takes a solid color only"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ColorError {}

impl Color {
    /// Fully transparent (`transparent`).
    pub const TRANSPARENT: Color = Color {
        space: Space::Srgb([0, 0, 0]),
        alpha: 0,
    };

    /// An opaque sRGB color.
    pub const fn srgb(r: u8, g: u8, b: u8) -> Color {
        Color {
            space: Space::Srgb([r, g, b]),
            alpha: 1000,
        }
    }

    /// An opaque OKLCH color: lightness 0–1, chroma ≥ 0, hue in degrees.
    /// Chroma above 1 is clamped; colors outside sRGB are gamut-mapped
    /// for fallbacks and contrast math.
    pub fn oklch(l: f32, c: f32, h: f32) -> Color {
        let hue = (h as f64) - 360.0 * math::floor(h as f64 / 360.0);
        Color {
            space: Space::Oklch {
                l: fixed(l as f64 * 100.0, 100.0, 0.0, 10_000.0) as u16,
                c: fixed(c as f64, 10_000.0, 0.0, 10_000.0) as u16,
                h: (fixed(hue, 100.0, 0.0, 36_000.0) as u16) % 36_000,
            },
            alpha: 1000,
        }
    }

    /// The same color with alpha set (0–1, clamped).
    pub fn with_alpha(self, alpha: f32) -> Color {
        Color {
            alpha: fixed(alpha as f64, 1000.0, 0.0, 1000.0) as u16,
            ..self
        }
    }

    /// Parses any supported CSS color: `#rgb[a]`, `#rrggbb[aa]`, `rgb()`,
    /// `rgba()`, `hsl()`, `hsla()`, `oklch()`, `oklab()`, a named color, or
    /// `transparent`.
    pub fn parse(input: &str) -> Result<Color, ColorError> {
        let start = input.len() - input.trim_start().len();
        parse_color(input.trim(), start)
    }

    /// Alpha, 0–1.
    pub fn alpha(self) -> f32 {
        self.alpha as f32 / 1000.0
    }

    /// True when alpha is below 1.
    pub fn is_translucent(self) -> bool {
        self.alpha < 1000
    }

    /// The same color, fully opaque.
    pub fn opaque(self) -> Color {
        Color {
            alpha: 1000,
            ..self
        }
    }

    /// The nearest sRGB color, 8 bits per channel. OKLCH and OKLab colors
    /// outside sRGB lose chroma at constant lightness and hue until they fit.
    pub fn to_srgb8(self) -> [u8; 3] {
        match self.space {
            Space::Srgb(rgb) => rgb,
            _ => {
                let rgb = self.to_srgb();
                rgb.map(|c| math::round(c * 255.0) as u8)
            }
        }
    }

    /// The nearest opaque sRGB color: the literal fallback for renderers
    /// without CSS variables or OKLCH.
    pub(crate) fn opaque_srgb(self) -> Color {
        let [r, g, b] = self.to_srgb8();
        Color::srgb(r, g, b)
    }

    /// Gamma-encoded sRGB in 0–1 (gamut-mapped).
    pub(crate) fn to_srgb(self) -> [f64; 3] {
        match self.space {
            Space::Srgb(rgb) => rgb.map(|c| c as f64 / 255.0),
            Space::Oklch { l, c, h } => {
                let (sin, cos) = math::sin_cos(math::radians(h as f64 / 100.0));
                let c = c as f64 / 10_000.0;
                gamut_map(l as f64 / 10_000.0, c * cos, c * sin)
            }
            Space::Oklab { l, a, b } => gamut_map(
                l as f64 / 10_000.0,
                a as f64 / 10_000.0,
                b as f64 / 10_000.0,
            ),
        }
    }

    /// OKLab `[L, a, b]` of the (gamut-mapped) color, ignoring alpha.
    pub fn to_oklab(self) -> [f32; 3] {
        let [l, a, b] = srgb_to_oklab(self.to_srgb());
        [l as f32, a as f32, b as f32]
    }

    /// Writes the color without its alpha, in canonical form. The SVG
    /// writer carries alpha separately (`fill-opacity`).
    pub(crate) fn write_opaque<W: Write + ?Sized>(self, w: &mut W) -> fmt::Result {
        match self.space {
            Space::Srgb(rgb) => write_hex(w, rgb),
            Space::Oklch { l, c, h } => {
                w.write_str("oklch(")?;
                write_scaled(w, l as i64, 2)?;
                w.write_str("% ")?;
                write_scaled(w, c as i64, 4)?;
                w.write_char(' ')?;
                write_scaled(w, h as i64, 2)?;
                w.write_char(')')
            }
            Space::Oklab { l, a, b } => {
                w.write_str("oklab(")?;
                write_scaled(w, l as i64, 2)?;
                w.write_str("% ")?;
                write_scaled(w, a as i64, 4)?;
                w.write_char(' ')?;
                write_scaled(w, b as i64, 4)?;
                w.write_char(')')
            }
        }
    }
}

impl fmt::Display for Color {
    /// Canonical CSS: hex for sRGB (`rgb(… / a)` when translucent), and
    /// `oklch()` / `oklab()` with short numbers otherwise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Color::TRANSPARENT {
            return f.write_str("transparent");
        }
        match (self.space, self.is_translucent()) {
            (Space::Srgb([r, g, b]), true) => {
                write!(f, "rgb({r} {g} {b} / ")?;
                write_scaled(f, self.alpha as i64, 3)?;
                f.write_char(')')
            }
            (_, false) => self.write_opaque(f),
            (_, true) => {
                let mut buf = alloc::string::String::new();
                self.write_opaque(&mut buf)?;
                f.write_str(&buf[..buf.len() - 1])?;
                f.write_str(" / ")?;
                write_scaled(f, self.alpha as i64, 3)?;
                f.write_char(')')
            }
        }
    }
}

impl FromStr for Color {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<Color, ColorError> {
        Color::parse(s)
    }
}

/// Rounds `value · scale` and clamps it into `[min, max]`.
fn fixed(value: f64, scale: f64, min: f64, max: f64) -> f64 {
    let v = math::round(value * scale);
    if v.is_nan() { min } else { v.clamp(min, max) }
}

fn write_hex<W: Write + ?Sized>(w: &mut W, [r, g, b]: [u8; 3]) -> fmt::Result {
    let short = [r, g, b].iter().all(|c| c >> 4 == c & 0xf);
    if short {
        write!(w, "#{:x}{:x}{:x}", r & 0xf, g & 0xf, b & 0xf)
    } else {
        write!(w, "#{r:02x}{g:02x}{b:02x}")
    }
}

// ---------------------------------------------------------------- OKLab

pub(crate) fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        math::powf((c + 0.055) / 1.055, 2.4)
    }
}

pub(crate) fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * math::powf(c, 1.0 / 2.4) - 0.055
    }
}

/// Gamma-encoded sRGB to OKLab (Björn Ottosson's matrices).
pub(crate) fn srgb_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(srgb_to_linear);
    let l = math::cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
    let m = math::cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
    let s = math::cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// OKLab to linear sRGB, unclamped.
fn oklab_to_linear(l: f64, a: f64, b: f64) -> [f64; 3] {
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
}

/// OKLab to gamma-encoded sRGB, reducing chroma at constant L and hue
/// until the color fits.
fn gamut_map(l: f64, a: f64, b: f64) -> [f64; 3] {
    const EPS: f64 = 1e-6;
    let l = l.clamp(0.0, 1.0);
    let fits = |rgb: &[f64; 3]| rgb.iter().all(|&c| (-EPS..=1.0 + EPS).contains(&c));
    let mut rgb = oklab_to_linear(l, a, b);
    if !fits(&rgb) {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..32 {
            let mid = (lo + hi) / 2.0;
            if fits(&oklab_to_linear(l, a * mid, b * mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        rgb = oklab_to_linear(l, a * lo, b * lo);
    }
    rgb.map(|c| linear_to_srgb(c.clamp(0.0, 1.0)))
}

// -------------------------------------------------------------- parsing

fn parse_color(s: &str, offset: usize) -> Result<Color, ColorError> {
    let err = |at: usize| ColorError::Syntax {
        position: offset + at,
    };
    if s.is_empty() {
        return Err(err(0));
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex).ok_or(err(0));
    }
    let Some(open) = s.find('(') else {
        return parse_name(s);
    };
    let name = &s[..open];
    let body = s[open + 1..].strip_suffix(')').ok_or(err(s.len()))?;
    let args = Args::parse(body, offset + open + 1)?;
    let lower = |n: &str| name.eq_ignore_ascii_case(n);
    if lower("rgb") || lower("rgba") {
        args.expect_len(3, err(open))?;
        let channel = |v: Value| match v.unit {
            Unit::Percent => v.num * 255.0 / 100.0,
            _ => v.num,
        };
        let rgb = [0, 1, 2].map(|i| fixed(channel(args.values[i]), 1.0, 0.0, 255.0) as u8);
        Ok(Color::srgb(rgb[0], rgb[1], rgb[2]).with_alpha(args.alpha()))
    } else if lower("hsl") || lower("hsla") {
        args.expect_len(3, err(open))?;
        let h = args.values[0].degrees().ok_or(err(open))?;
        let s = (args.values[1].num / 100.0).clamp(0.0, 1.0);
        let l = (args.values[2].num / 100.0).clamp(0.0, 1.0);
        let rgb = hsl_to_srgb(h, s, l).map(|c| fixed(c, 255.0, 0.0, 255.0) as u8);
        Ok(Color::srgb(rgb[0], rgb[1], rgb[2]).with_alpha(args.alpha()))
    } else if lower("oklch") {
        args.expect_len(3, err(open))?;
        let l = args.values[0].lightness();
        let c = args.values[1].ab_component().max(0.0);
        let h = args.values[2].degrees().ok_or(err(open))?;
        Ok(Color::oklch(l as f32, c as f32, h as f32).with_alpha(args.alpha()))
    } else if lower("oklab") {
        args.expect_len(3, err(open))?;
        Ok(Color {
            space: Space::Oklab {
                l: fixed(args.values[0].lightness() * 100.0, 100.0, 0.0, 10_000.0) as u16,
                a: fixed(args.values[1].ab_component(), 10_000.0, -10_000.0, 10_000.0) as i16,
                b: fixed(args.values[2].ab_component(), 10_000.0, -10_000.0, 10_000.0) as i16,
            },
            alpha: 1000,
        }
        .with_alpha(args.alpha()))
    } else {
        Err(err(0))
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok();
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let (rgb, alpha) = match hex.len() {
        3 | 4 => {
            let d = |i| digit(i).map(|v| v * 17);
            (
                [d(0)?, d(1)?, d(2)?],
                if hex.len() == 4 { d(3)? } else { 255 },
            )
        }
        6 | 8 => (
            [pair(0)?, pair(2)?, pair(4)?],
            if hex.len() == 8 { pair(6)? } else { 255 },
        ),
        _ => return None,
    };
    Some(Color::srgb(rgb[0], rgb[1], rgb[2]).with_alpha(alpha as f32 / 255.0))
}

fn parse_name(s: &str) -> Result<Color, ColorError> {
    if s.eq_ignore_ascii_case("transparent") {
        return Ok(Color::TRANSPARENT);
    }
    let mut buf = [0u8; 24];
    if s.len() > buf.len() || !s.is_ascii() {
        return Err(ColorError::UnknownName);
    }
    for (dst, src) in buf.iter_mut().zip(s.bytes()) {
        *dst = src.to_ascii_lowercase();
    }
    let key = &buf[..s.len()];
    NAMED_COLORS
        .binary_search_by(|(name, _)| name.as_bytes().cmp(key))
        .map(|i| {
            let rgb = NAMED_COLORS[i].1;
            Color::srgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
        })
        .map_err(|_| ColorError::UnknownName)
}

/// HSL (hue in degrees, s and l in 0–1) to gamma-encoded sRGB in 0–1.
fn hsl_to_srgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    let h = h - 360.0 * math::floor(h / 360.0);
    let a = s * l.min(1.0 - l);
    [0.0, 8.0, 4.0].map(|n| {
        let k = n + h / 30.0;
        let k = k - 12.0 * math::floor(k / 12.0);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    None,
    Percent,
    Deg,
    Rad,
    Grad,
    Turn,
}

#[derive(Clone, Copy, Debug)]
struct Value {
    num: f64,
    unit: Unit,
}

impl Value {
    fn degrees(self) -> Option<f64> {
        Some(match self.unit {
            Unit::None | Unit::Deg => self.num,
            Unit::Rad => self.num * 180.0 / core::f64::consts::PI,
            Unit::Grad => self.num * 0.9,
            Unit::Turn => self.num * 360.0,
            Unit::Percent => return None,
        })
    }

    /// OKLCH/OKLab lightness: a number in 0–1 or a percentage.
    fn lightness(self) -> f64 {
        match self.unit {
            Unit::Percent => self.num / 100.0,
            _ => self.num,
        }
    }

    /// OKLCH chroma or OKLab a/b: 100% means 0.4.
    fn ab_component(self) -> f64 {
        match self.unit {
            Unit::Percent => self.num * 0.004,
            _ => self.num,
        }
    }
}

/// The arguments of a color function: three components and an optional
/// alpha, in either the legacy comma form or the modern space/slash form.
struct Args {
    values: [Value; 3],
    count: usize,
    alpha: Option<Value>,
}

impl Args {
    fn parse(body: &str, offset: usize) -> Result<Args, ColorError> {
        let err = |at: usize| ColorError::Syntax {
            position: offset + at,
        };
        let zero = Value {
            num: 0.0,
            unit: Unit::None,
        };
        let mut args = Args {
            values: [zero; 3],
            count: 0,
            alpha: None,
        };
        let commas = body.contains(',');
        let mut slash = false;
        let bytes = body.as_bytes();
        let mut i = 0;
        loop {
            while i < bytes.len()
                && (bytes[i].is_ascii_whitespace() || (commas && bytes[i] == b','))
            {
                i += 1;
            }
            if i == bytes.len() {
                break;
            }
            if bytes[i] == b'/' {
                if commas || slash || args.count != 3 {
                    return Err(err(i));
                }
                slash = true;
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !matches!(bytes[i], b',' | b'/')
            {
                i += 1;
            }
            let value = parse_value(&body[start..i]).ok_or(err(start))?;
            if args.count < 3 && !slash {
                args.values[args.count] = value;
                args.count += 1;
            } else if args.alpha.is_none() && (slash || commas) {
                args.alpha = Some(value);
            } else {
                return Err(err(start));
            }
        }
        if slash && args.alpha.is_none() {
            return Err(err(body.len()));
        }
        Ok(args)
    }

    fn expect_len(&self, n: usize, err: ColorError) -> Result<(), ColorError> {
        if self.count == n { Ok(()) } else { Err(err) }
    }

    fn alpha(&self) -> f32 {
        match self.alpha {
            None => 1.0,
            Some(Value {
                num,
                unit: Unit::Percent,
            }) => (num / 100.0) as f32,
            Some(v) => v.num as f32,
        }
    }
}

/// A number with an optional unit, or the keyword `none` (zero).
fn parse_value(s: &str) -> Option<Value> {
    if s.eq_ignore_ascii_case("none") {
        return Some(Value {
            num: 0.0,
            unit: Unit::None,
        });
    }
    let split = s
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(s.len());
    // Keep an exponent ("1e3") with the number.
    let split = match s.as_bytes().get(split) {
        Some(b'e' | b'E')
            if s[split + 1..]
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit() || b == b'-' || b == b'+') =>
        {
            s[split + 1..]
                .find(|c: char| c.is_ascii_alphabetic() || c == '%')
                .map_or(s.len(), |i| split + 1 + i)
        }
        _ => split,
    };
    let (num, unit) = s.split_at(split);
    let num: f64 = num.parse().ok()?;
    if !num.is_finite() {
        return None;
    }
    let unit = match unit.to_ascii_lowercase().as_str() {
        "" => Unit::None,
        "%" => Unit::Percent,
        "deg" => Unit::Deg,
        "rad" => Unit::Rad,
        "grad" => Unit::Grad,
        "turn" => Unit::Turn,
        _ => return None,
    };
    Some(Value { num, unit })
}

/// CSS Color 4 named colors, sorted by name.
#[rustfmt::skip]
static NAMED_COLORS: [(&str, u32); 148] = [
    ("aliceblue", 0xf0f8ff), ("antiquewhite", 0xfaebd7), ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4), ("azure", 0xf0ffff), ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4), ("black", 0x000000), ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff), ("blueviolet", 0x8a2be2), ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887), ("cadetblue", 0x5f9ea0), ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e), ("coral", 0xff7f50), ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc), ("crimson", 0xdc143c), ("cyan", 0x00ffff),
    ("darkblue", 0x00008b), ("darkcyan", 0x008b8b), ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9), ("darkgreen", 0x006400), ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b), ("darkmagenta", 0x8b008b), ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00), ("darkorchid", 0x9932cc), ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a), ("darkseagreen", 0x8fbc8f), ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f), ("darkslategrey", 0x2f4f4f), ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3), ("deeppink", 0xff1493), ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969), ("dimgrey", 0x696969), ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222), ("floralwhite", 0xfffaf0), ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff), ("gainsboro", 0xdcdcdc), ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700), ("goldenrod", 0xdaa520), ("gray", 0x808080),
    ("green", 0x008000), ("greenyellow", 0xadff2f), ("grey", 0x808080),
    ("honeydew", 0xf0fff0), ("hotpink", 0xff69b4), ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082), ("ivory", 0xfffff0), ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa), ("lavenderblush", 0xfff0f5), ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd), ("lightblue", 0xadd8e6), ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff), ("lightgoldenrodyellow", 0xfafad2), ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90), ("lightgrey", 0xd3d3d3), ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a), ("lightseagreen", 0x20b2aa), ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899), ("lightslategrey", 0x778899), ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0), ("lime", 0x00ff00), ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6), ("magenta", 0xff00ff), ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa), ("mediumblue", 0x0000cd), ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db), ("mediumseagreen", 0x3cb371), ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a), ("mediumturquoise", 0x48d1cc), ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970), ("mintcream", 0xf5fffa), ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5), ("navajowhite", 0xffdead), ("navy", 0x000080),
    ("oldlace", 0xfdf5e6), ("olive", 0x808000), ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500), ("orangered", 0xff4500), ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa), ("palegreen", 0x98fb98), ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093), ("papayawhip", 0xffefd5), ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f), ("pink", 0xffc0cb), ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6), ("purple", 0x800080), ("rebeccapurple", 0x663399),
    ("red", 0xff0000), ("rosybrown", 0xbc8f8f), ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513), ("salmon", 0xfa8072), ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57), ("seashell", 0xfff5ee), ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0), ("skyblue", 0x87ceeb), ("slateblue", 0x6a5acd),
    ("slategray", 0x708090), ("slategrey", 0x708090), ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f), ("steelblue", 0x4682b4), ("tan", 0xd2b48c),
    ("teal", 0x008080), ("thistle", 0xd8bfd8), ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0), ("violet", 0xee82ee), ("wheat", 0xf5deb3),
    ("white", 0xffffff), ("whitesmoke", 0xf5f5f5), ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn c(s: &str) -> Color {
        Color::parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
    }

    #[test]
    fn named_table_is_sorted() {
        assert!(NAMED_COLORS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn hex_forms() {
        assert_eq!(c("#abc"), Color::srgb(0xaa, 0xbb, 0xcc));
        assert_eq!(c("#A1B2C3"), Color::srgb(0xa1, 0xb2, 0xc3));
        assert_eq!(c("#a1b2c380").alpha(), 0.502);
        assert_eq!(c("#abcd").alpha(), 0.867);
        assert!(Color::parse("#abcde").is_err());
        assert!(Color::parse("#ggg").is_err());
    }

    #[test]
    fn functional_forms() {
        assert_eq!(c("rgb(255, 0, 128)"), Color::srgb(255, 0, 128));
        assert_eq!(c("rgb(100% 0% 50%)"), Color::srgb(255, 0, 128));
        assert_eq!(c("rgba(1,2,3,.5)"), Color::srgb(1, 2, 3).with_alpha(0.5));
        assert_eq!(c("rgb(1 2 3 / 25%)"), Color::srgb(1, 2, 3).with_alpha(0.25));
        assert_eq!(c("hsl(120, 100%, 25%)"), Color::srgb(0, 128, 0));
        assert_eq!(c("hsl(0.5turn 100% 50%)"), Color::srgb(0, 255, 255));
        assert_eq!(c("HSLA(240deg 100% 50% / 1)"), Color::srgb(0, 0, 255));
        assert_eq!(c("  RebeccaPurple "), Color::srgb(0x66, 0x33, 0x99));
        assert_eq!(c("transparent"), Color::TRANSPARENT);
        assert_eq!(c("oklch(62% 0.19 35)"), Color::oklch(0.62, 0.19, 35.0));
        assert_eq!(c("oklch(.62 47.5% 395)"), Color::oklch(0.62, 0.19, 35.0));
        assert_eq!(c("oklch(50% .1 1e2)"), Color::oklch(0.5, 0.1, 100.0));
    }

    #[test]
    fn rejects_malformed() {
        for bad in [
            "",
            "rgb(1,2)",
            "rgb(1 2 3 4)",
            "rgb(1, 2, 3 / 1)",
            "rgb(1 2 / 3)",
            "rgb(1 2 3 /)",
            "hsl(1% 2% 3%)",
            "foo(1 2 3)",
            "rgb(1 2 3",
            "notacolor",
            "rgb(a b c)",
            "oklch(1 2 3) x",
            "rgb(inf 0 0)",
        ] {
            assert!(Color::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(Color::parse("blurple"), Err(ColorError::UnknownName));
        assert_eq!(
            Color::parse(" rgb(1 2 x)"),
            Err(ColorError::Syntax { position: 9 })
        );
    }

    #[test]
    fn canonical_output_round_trips() {
        for (input, canonical) in [
            ("#AABBCC", "#abc"),
            ("#a1b2c3", "#a1b2c3"),
            ("rgb(1 2 3 / .5)", "rgb(1 2 3 / .5)"),
            ("oklch(32% 0.08 265)", "oklch(32% .08 265)"),
            ("oklch(62.5% .19 35.5 / 50%)", "oklch(62.5% .19 35.5 / .5)"),
            ("oklab(50% -0.1 0.05)", "oklab(50% -.1 .05)"),
            ("transparent", "transparent"),
        ] {
            let color = c(input);
            assert_eq!(color.to_string(), canonical);
            assert_eq!(c(canonical), color);
        }
    }

    #[test]
    fn oklab_reference_values() {
        // From Björn Ottosson's OKLab post: the sRGB primaries and white.
        let close =
            |got: [f32; 3], want: [f32; 3]| got.iter().zip(want).all(|(g, w)| (g - w).abs() < 2e-4);
        assert!(close(c("#fff").to_oklab(), [1.0, 0.0, 0.0]));
        assert!(close(c("#f00").to_oklab(), [0.62796, 0.22486, 0.12585]));
        assert!(close(c("#0f0").to_oklab(), [0.86644, -0.23389, 0.1795]));
        assert!(close(c("#00f").to_oklab(), [0.45201, -0.03246, -0.31153]));
        assert!(close(c("#000").to_oklab(), [0.0, 0.0, 0.0]));
    }

    #[test]
    fn oklch_maps_into_srgb() {
        // oklch(62.8% .2577 29.23) is sRGB red.
        assert_eq!(c("oklch(62.8% .2577 29.23)").to_srgb8(), [255, 0, 0]);
        // Far out of gamut: chroma drops, L and hue hold.
        let [r, g, b] = c("oklch(70% 0.5 150)").to_srgb8();
        let lab = Color::srgb(r, g, b).to_oklab();
        assert!((lab[0] - 0.70).abs() < 0.01, "{lab:?}");
        assert!(lab[1] < 0.0 && lab[2] > 0.0, "hue kept: {lab:?}");
    }

    #[test]
    fn srgb_round_trips_through_oklab() {
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let rgb = [r, g, b].map(|v| v as f64 / 255.0);
                    let [l, a, bb] = srgb_to_oklab(rgb);
                    // The published matrices carry 10 digits, so the inverse is good to
                    // ~1e-6 (8-bit resolution is 4e-3).
                    let back = gamut_map(l, a, bb);
                    assert!(
                        back.iter().zip(rgb).all(|(x, y)| (x - y).abs() < 1e-5),
                        "{rgb:?} -> {back:?}"
                    );
                }
            }
        }
    }
}
