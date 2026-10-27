//! Color tokens (SPEC §6): the named paints a style draws with, their
//! fallback chain, gradients, and the contrast math behind §6.3.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use core::str::FromStr;

use crate::color::{Color, ColorError, srgb_to_oklab};
use crate::math;
use crate::matrix::Corner;
use crate::num::write_scaled;

/// A named paint slot (SPEC §6.1). Each one falls back to another when
/// unset, ending at `fg` or `bg`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Token {
    /// Canvas background: a color (may be `transparent`) or a gradient
    /// across the card (SPEC §6.1).
    Bg,
    /// Data modules.
    Fg,
    /// Finder outer rings (default: `fg`).
    FinderOuter,
    /// Finder eyes (default: `fg`).
    FinderEye,
    /// Alignment patterns (default: `finder.outer`).
    Alignment,
    /// Timing patterns (default: `fg`).
    Timing,
    /// Decorative filler modules (default: `fg` at 35% opacity).
    Filler,
    /// The plate behind a logo (default: `bg`).
    LogoBackdrop,
    /// Frame stroke (default: `fg`).
    Frame,
    /// One finder's outer ring (default: `finder.outer`).
    CornerOuter(Corner),
    /// One finder's eye (default: `finder.eye`).
    CornerEye(Corner),
    /// The frame on a logo's plate (default: `fg`).
    LogoFrame,
    /// A frame's hard shadow (default: `fg`, SPEC §7.4).
    Shadow,
    /// A second data color (default: `fg`). Data modules are spread
    /// across `fg` and every data color that is set (SPEC §6.1).
    Fg2,
    /// A third data color (default: `fg`).
    Fg3,
    /// A fourth data color (default: `fg`).
    Fg4,
}

const CORNERS: [Corner; 3] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft];
const NUM_TOKENS: usize = 20;

impl Token {
    /// Every token, in a fixed order.
    pub const ALL: [Token; NUM_TOKENS] = [
        Token::Bg,
        Token::Fg,
        Token::FinderOuter,
        Token::FinderEye,
        Token::Alignment,
        Token::Timing,
        Token::Filler,
        Token::LogoBackdrop,
        Token::Frame,
        Token::CornerOuter(Corner::TopLeft),
        Token::CornerOuter(Corner::TopRight),
        Token::CornerOuter(Corner::BottomLeft),
        Token::CornerEye(Corner::TopLeft),
        Token::CornerEye(Corner::TopRight),
        Token::CornerEye(Corner::BottomLeft),
        Token::LogoFrame,
        Token::Shadow,
        Token::Fg2,
        Token::Fg3,
        Token::Fg4,
    ];

    fn index(self) -> usize {
        let corner = |c: Corner| CORNERS.iter().position(|&k| k == c).expect("corner");
        match self {
            Token::Bg => 0,
            Token::Fg => 1,
            Token::FinderOuter => 2,
            Token::FinderEye => 3,
            Token::Alignment => 4,
            Token::Timing => 5,
            Token::Filler => 6,
            Token::LogoBackdrop => 7,
            Token::Frame => 8,
            Token::CornerOuter(c) => 9 + corner(c),
            Token::CornerEye(c) => 12 + corner(c),
            Token::LogoFrame => 15,
            Token::Shadow => 16,
            Token::Fg2 => 17,
            Token::Fg3 => 18,
            Token::Fg4 => 19,
        }
    }

    /// The token's name as used in `Palette::from_tokens`, e.g. `fg` or
    /// `finder.top-left.eye`.
    pub fn name(self) -> &'static str {
        [
            "bg",
            "fg",
            "finder.outer",
            "finder.eye",
            "alignment",
            "timing",
            "filler",
            "logo.backdrop",
            "frame",
            "finder.top-left.outer",
            "finder.top-right.outer",
            "finder.bottom-left.outer",
            "finder.top-left.eye",
            "finder.top-right.eye",
            "finder.bottom-left.eye",
            "logo.frame",
            "shadow",
            "fg.2",
            "fg.3",
            "fg.4",
        ][self.index()]
    }

    /// Looks a token up by [`name`](Token::name).
    pub fn from_name(name: &str) -> Option<Token> {
        Token::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The CSS custom property a host page can set to re-theme the code:
    /// `--qr-` plus the name with dots as dashes.
    pub fn css_var(self) -> &'static str {
        [
            "--qr-bg",
            "--qr-fg",
            "--qr-finder-outer",
            "--qr-finder-eye",
            "--qr-alignment",
            "--qr-timing",
            "--qr-filler",
            "--qr-logo-backdrop",
            "--qr-frame",
            "--qr-finder-top-left-outer",
            "--qr-finder-top-right-outer",
            "--qr-finder-bottom-left-outer",
            "--qr-finder-top-left-eye",
            "--qr-finder-top-right-eye",
            "--qr-finder-bottom-left-eye",
            "--qr-logo-frame",
            "--qr-shadow",
            "--qr-fg-2",
            "--qr-fg-3",
            "--qr-fg-4",
        ][self.index()]
    }

    /// The token this one takes its paint from when unset.
    pub fn fallback(self) -> Option<Token> {
        match self {
            Token::Bg | Token::Fg => None,
            Token::FinderOuter
            | Token::FinderEye
            | Token::Timing
            | Token::Filler
            | Token::Frame
            | Token::LogoFrame
            | Token::Shadow
            | Token::Fg2
            | Token::Fg3
            | Token::Fg4 => Some(Token::Fg),
            Token::Alignment => Some(Token::FinderOuter),
            Token::LogoBackdrop => Some(Token::Bg),
            Token::CornerOuter(_) => Some(Token::FinderOuter),
            Token::CornerEye(_) => Some(Token::FinderEye),
        }
    }

    /// True for the tokens that accept gradients: `bg`, `fg` and the data
    /// colors, the finder tokens and `logo.frame`.
    pub fn accepts_gradient(self) -> bool {
        matches!(
            self,
            Token::Bg
                | Token::Fg
                | Token::Fg2
                | Token::Fg3
                | Token::Fg4
                | Token::LogoFrame
                | Token::FinderOuter
                | Token::FinderEye
                | Token::CornerOuter(_)
                | Token::CornerEye(_)
        )
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A solid color or a gradient.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Paint {
    /// One color.
    Solid(Color),
    /// A linear or radial gradient across the code.
    Gradient(Gradient),
}

impl From<Color> for Paint {
    fn from(color: Color) -> Paint {
        Paint::Solid(color)
    }
}

impl From<Gradient> for Paint {
    fn from(gradient: Gradient) -> Paint {
        Paint::Gradient(gradient)
    }
}

impl Paint {
    /// Parses a CSS color, `linear-gradient(…)` or `radial-gradient(…)`.
    pub fn parse(input: &str) -> Result<Paint, ColorError> {
        let trimmed = input.trim_start();
        let offset = input.len() - trimmed.len();
        let lower = |prefix: &str| {
            trimmed
                .as_bytes()
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
        };
        if lower("linear-gradient(") || lower("radial-gradient(") {
            Gradient::parse(trimmed.trim_end(), offset).map(Paint::Gradient)
        } else {
            Color::parse(input).map(Paint::Solid)
        }
    }

    fn map_alpha(&self, factor: f32) -> Paint {
        let scale = |c: Color| c.with_alpha(c.alpha() * factor);
        match self {
            Paint::Solid(c) => Paint::Solid(scale(*c)),
            Paint::Gradient(g) => Paint::Gradient(Gradient {
                kind: g.kind,
                stops: g
                    .stops
                    .iter()
                    .map(|s| GradientStop {
                        color: scale(s.color),
                        ..*s
                    })
                    .collect(),
            }),
        }
    }
}

impl fmt::Display for Paint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Paint::Solid(c) => c.fmt(f),
            Paint::Gradient(g) => g.fmt(f),
        }
    }
}

impl FromStr for Paint {
    type Err = ColorError;

    fn from_str(s: &str) -> Result<Paint, ColorError> {
        Paint::parse(s)
    }
}

/// Gradient geometry. Both kinds span the whole code (not each module),
/// like a CSS gradient on a square box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GradientKind {
    /// Along a line at `angle` hundredths of a degree, CSS convention:
    /// 0 points up, 9000 points right.
    Linear {
        /// Hundredths of a degree, 0–35999.
        angle: u16,
    },
    /// A circle from the center to the corners (`farthest-corner`).
    Radial,
}

/// A color at a position along a gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct GradientStop {
    /// The stop's color.
    pub color: Color,
    /// Position in thousandths, 0–1000, non-decreasing along the gradient.
    pub offset: u16,
}

/// A linear or radial gradient with two or more stops.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Gradient {
    kind: GradientKind,
    stops: Vec<GradientStop>,
}

impl Gradient {
    /// A linear gradient at `angle` degrees (CSS convention) through evenly
    /// spaced colors. Returns `None` for fewer than two colors.
    pub fn linear(angle: f32, colors: &[Color]) -> Option<Gradient> {
        Gradient::evenly(
            GradientKind::Linear {
                angle: normalize_angle(angle as f64),
            },
            colors,
        )
    }

    /// A radial gradient from the center out through evenly spaced colors.
    /// Returns `None` for fewer than two colors.
    pub fn radial(colors: &[Color]) -> Option<Gradient> {
        Gradient::evenly(GradientKind::Radial, colors)
    }

    fn evenly(kind: GradientKind, colors: &[Color]) -> Option<Gradient> {
        if colors.len() < 2 {
            return None;
        }
        let last = colors.len() - 1;
        let stops = colors
            .iter()
            .enumerate()
            .map(|(i, &color)| GradientStop {
                color,
                offset: (i * 1000 / last) as u16,
            })
            .collect();
        Some(Gradient { kind, stops })
    }

    /// Linear or radial, with the angle.
    pub fn kind(&self) -> GradientKind {
        self.kind
    }

    /// The stops, in order.
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    fn parse(s: &str, offset: usize) -> Result<Gradient, ColorError> {
        let err = |at: usize| ColorError::Syntax {
            position: offset + at,
        };
        let open = s.find('(').ok_or(err(0))?;
        let radial = s[..open].eq_ignore_ascii_case("radial-gradient");
        let body = s[open + 1..].strip_suffix(')').ok_or(err(s.len()))?;
        let body_at = open + 1;

        let mut parts = split_top_level(body).peekable();
        let mut kind = if radial {
            GradientKind::Radial
        } else {
            GradientKind::Linear { angle: 18_000 }
        };
        if !radial
            && let Some(&(at, first)) = parts.peek()
            && let Some(angle) = parse_direction(first.trim())
        {
            kind = GradientKind::Linear {
                angle: angle.map_err(|_| err(body_at + at))?,
            };
            parts.next();
        }

        // Stops: a color, optionally followed by a percentage.
        let mut raw: Vec<(Color, Option<f64>)> = Vec::new();
        for (at, part) in parts {
            let part = part.trim();
            let at = body_at + at;
            let (color, pos) = match last_top_level_space(part) {
                Some(i) if part[i + 1..].ends_with('%') => {
                    let pct: f64 = part[i + 1..part.len() - 1]
                        .parse()
                        .map_err(|_| err(at + i + 1))?;
                    if !pct.is_finite() {
                        return Err(err(at + i + 1));
                    }
                    (&part[..i], Some(pct / 100.0))
                }
                _ => (part, None),
            };
            let color = Color::parse(color).map_err(|e| match e {
                ColorError::Syntax { position } => ColorError::Syntax {
                    position: offset + at + position,
                },
                other => other,
            })?;
            raw.push((color, pos));
        }
        if raw.len() < 2 {
            return Err(err(s.len()));
        }
        Ok(Gradient {
            kind,
            stops: fix_up_positions(&raw),
        })
    }
}

impl fmt::Display for Gradient {
    /// Canonical CSS with every stop position written out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            GradientKind::Linear { angle } => {
                f.write_str("linear-gradient(")?;
                write_scaled(f, angle as i64, 2)?;
                f.write_str("deg")?;
            }
            GradientKind::Radial => f.write_str("radial-gradient(")?,
        }
        for (i, stop) in self.stops.iter().enumerate() {
            if i > 0 || matches!(self.kind, GradientKind::Linear { .. }) {
                f.write_str(", ")?;
            }
            write!(f, "{} ", stop.color)?;
            write_scaled(f, stop.offset as i64, 1)?;
            f.write_char('%')?;
        }
        f.write_char(')')
    }
}

fn normalize_angle(degrees: f64) -> u16 {
    let d = degrees - 360.0 * math::floor(degrees / 360.0);
    (math::round(d * 100.0) as u32 % 36_000) as u16
}

/// `Some(Ok(angle))` for an angle or a `to <side>` phrase, `Some(Err)` for a
/// malformed `to …` phrase, `None` when `s` isn't a direction at all (so
/// it's the first stop).
fn parse_direction(s: &str) -> Option<Result<u16, ()>> {
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("to ") {
        let mut words: Vec<&str> = rest.split_ascii_whitespace().collect();
        crate::sort::sort_by(&mut words, |a, b| a.cmp(b));
        let angle = match words[..] {
            ["top"] => 0,
            ["right"] => 90,
            ["bottom"] => 180,
            ["left"] => 270,
            ["right", "top"] => 45,
            ["bottom", "right"] => 135,
            ["bottom", "left"] => 225,
            ["left", "top"] => 315,
            _ => return Some(Err(())),
        };
        return Some(Ok(angle * 100));
    }
    let (factor, num) = [
        ("deg", 1.0),
        ("grad", 0.9),
        ("rad", 180.0 / core::f64::consts::PI),
        ("turn", 360.0),
    ]
    .iter()
    .find_map(|&(unit, factor)| lower.strip_suffix(unit).map(|n| (factor, n)))?;
    let value: f64 = num.parse().ok()?;
    value
        .is_finite()
        .then(|| Ok(normalize_angle(value * factor)))
}

/// Splits on commas outside parentheses, with each part's byte offset.
fn split_top_level(s: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                parts.push((start, &s[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push((start, &s[start..]));
    parts.into_iter()
}

fn last_top_level_space(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut last = None;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b if b.is_ascii_whitespace() && depth == 0 => last = Some(i),
            _ => {}
        }
    }
    last
}

/// CSS stop fix-up: a missing first/last position is 0%/100%, positions
/// never go backwards, and runs of missing positions are spread evenly.
fn fix_up_positions(raw: &[(Color, Option<f64>)]) -> Vec<GradientStop> {
    let n = raw.len();
    let mut pos: Vec<Option<f64>> = raw.iter().map(|&(_, p)| p).collect();
    pos[0] = Some(pos[0].unwrap_or(0.0));
    pos[n - 1] = Some(pos[n - 1].unwrap_or(1.0));
    let mut max = 0.0f64;
    for p in pos.iter_mut().flatten() {
        *p = p.clamp(0.0, 1.0).max(max);
        max = *p;
    }
    let mut i = 0;
    while i < n {
        if pos[i].is_some() {
            i += 1;
            continue;
        }
        let before = pos[i - 1].expect("first is set");
        let end = (i..n).find(|&j| pos[j].is_some()).expect("last is set");
        let after = pos[end].expect("set");
        let gap = (end - i + 1) as f64;
        for (k, p) in pos[i..end].iter_mut().enumerate() {
            *p = Some(before + (after - before) * (k + 1) as f64 / gap);
        }
        i = end;
    }
    raw.iter()
        .zip(pos)
        .map(|(&(color, _), p)| GradientStop {
            color,
            offset: math::round(p.expect("filled") * 1000.0) as u16,
        })
        .collect()
}

/// A palette entry that failed to parse, with the token it was for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PaletteError {
    /// The token name as given.
    pub token: String,
    /// What was wrong.
    pub error: ColorError,
}

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "token {:?}: {}", self.token, self.error)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PaletteError {}

/// How far a paint stands out from the background (SPEC §6.3).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Contrast {
    /// The smallest OKLab lightness difference between the paint
    /// (composited over the background) and the background, 0–1. For a
    /// gradient, the worst point along it. Zero when a gradient crosses the
    /// background's lightness.
    pub delta_l: f32,
    /// True when the paint is lighter than the background everywhere.
    pub inverted: bool,
}

/// Where [`Palette::dark_twin`] puts the background's lightness.
const DARK_BG: f32 = 0.16;

/// A set of token paints. Unset tokens follow [`Token::fallback`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Palette {
    paints: [Option<Paint>; NUM_TOKENS],
}

/// The tokens of 1.0. Later ones show in `Debug` only when set, so the
/// text stays the same for palettes that don't use them: an intro's
/// palette feeds the ID prefix through it (SPEC §6.2).
const FIRST_TOKENS: usize = 16;

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let used = self.paints[FIRST_TOKENS..]
            .iter()
            .rposition(Option::is_some)
            .map_or(FIRST_TOKENS, |i| FIRST_TOKENS + i + 1);
        f.debug_struct("Palette")
            .field("paints", &&self.paints[..used])
            .finish()
    }
}

impl Default for Palette {
    /// A warm white background and a near-black foreground.
    fn default() -> Palette {
        let mut paints: [Option<Paint>; NUM_TOKENS] = Default::default();
        paints[Token::Bg.index()] = Some(Paint::Solid(Color::srgb(0xfe, 0xfc, 0xf8)));
        paints[Token::Fg.index()] = Some(Paint::Solid(Color::srgb(0x17, 0x16, 0x1c)));
        Palette { paints }
    }
}

impl Palette {
    /// The default palette.
    pub fn new() -> Palette {
        Palette::default()
    }

    /// The default palette with the given `(token name, paint)` pairs
    /// applied in order.
    pub fn from_tokens<'a, I>(tokens: I) -> Result<Palette, PaletteError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut palette = Palette::default();
        for (name, value) in tokens {
            let wrap = |error| PaletteError {
                token: String::from(name),
                error,
            };
            let token = Token::from_name(name).ok_or_else(|| wrap(ColorError::UnknownToken))?;
            let paint = Paint::parse(value).map_err(wrap)?;
            palette.set(token, paint).map_err(wrap)?;
        }
        Ok(palette)
    }

    /// Sets a token. Gradients are refused for tokens that don't take them.
    pub fn set(&mut self, token: Token, paint: impl Into<Paint>) -> Result<(), ColorError> {
        let paint = paint.into();
        if matches!(paint, Paint::Gradient(_)) && !token.accepts_gradient() {
            return Err(ColorError::GradientNotAllowed);
        }
        self.paints[token.index()] = Some(paint);
        Ok(())
    }

    /// [`set`](Palette::set), by value.
    pub fn with(mut self, token: Token, paint: impl Into<Paint>) -> Result<Palette, ColorError> {
        self.set(token, paint)?;
        Ok(self)
    }

    /// Clears a token so it falls back again. `bg` and `fg` return to
    /// their defaults.
    pub fn unset(&mut self, token: Token) {
        self.paints[token.index()] = Palette::default().paints[token.index()].clone();
    }

    /// The paint set explicitly for `token`, if any.
    pub fn get(&self, token: Token) -> Option<&Paint> {
        self.paints[token.index()].as_ref()
    }

    /// The paint `token` resolves to through the fallback chain.
    pub fn resolve(&self, token: Token) -> Paint {
        if let Some(paint) = self.get(token) {
            return paint.clone();
        }
        let fallback = token.fallback().expect("bg and fg are always set");
        let paint = self.resolve(fallback);
        if token == Token::Filler {
            paint.map_alpha(0.35)
        } else {
            paint
        }
    }

    /// Explicitly set tokens, in [`Token::ALL`] order.
    pub fn tokens(&self) -> impl Iterator<Item = (Token, &Paint)> {
        Token::ALL
            .into_iter()
            .filter_map(|t| self.get(t).map(|p| (t, p)))
    }

    /// The explicitly set solid tokens as CSS custom properties, e.g.
    /// `--qr-bg:#fefcf8;--qr-fg:#17161c`, to re-theme an inline code from
    /// script (`svg.style.cssText = vars`). Values are opaque because the
    /// document keeps alpha in `fill-opacity`; gradients are left out.
    pub fn to_css_vars(&self) -> String {
        let mut out = String::new();
        for (token, paint) in self.tokens() {
            if let Paint::Solid(c) = paint {
                if !out.is_empty() {
                    out.push(';');
                }
                out.push_str(token.css_var());
                out.push(':');
                c.write_opaque(&mut out).expect("writing to a String");
            }
        }
        out
    }

    /// The palette's dark twin (SPEC §6.3): every set color, gradient
    /// stops included, with its luma mirrored to `k − Y′` at the same
    /// OKLCH chroma and hue and the same alpha. `Y′` is Rec. 601 luma of
    /// the gamma-encoded sRGB color, which is what cameras and
    /// `freeqrent-verify` threshold, so a gray palette's twin is its
    /// photographic negative and reads the same, inverted. `k` puts the
    /// background at the luma of OKLab L 0.16, or lower when the darkest
    /// color would otherwise pass white. Only the polarity flips, so the
    /// codes need [`Style::allow_inverted`]; lightness differences in
    /// OKLab move a little (they grow for mid-tones), and translucent
    /// paints and gradients are blended in sRGB, so [`Style::check`] has
    /// the final word. A palette whose `fg` is already lighter than `bg`
    /// is its own twin. Colors come out as `oklch()`.
    ///
    /// [`Style::allow_inverted`]: crate::Style::allow_inverted
    /// [`Style::check`]: crate::Style::check
    pub fn dark_twin(&self) -> Palette {
        if self.contrast(Token::Fg).inverted {
            return self.clone();
        }
        let mut darkest = 1.0f64;
        for paint in self.paints.iter().flatten() {
            match paint {
                Paint::Solid(c) => darkest = darkest.min(luma(*c)),
                Paint::Gradient(g) => {
                    for stop in &g.stops {
                        darkest = darkest.min(luma(stop.color));
                    }
                }
            }
        }
        let floor = luma(Color::oklch(DARK_BG, 0.0, 0.0));
        let k = (luma(self.background()) + floor).min(1.0 + darkest);
        let mirror = |c: Color| {
            let [_, a, b] = c.to_oklab();
            let chroma = math::sqrt((a * a + b * b) as f64) as f32;
            let hue = (math::atan2(b as f64, a as f64) * (180.0 / core::f64::consts::PI)) as f32;
            let target = (k - luma(c)).clamp(0.0, 1.0);
            // Luma rises with lightness at a fixed chroma and hue (chroma
            // gives way at the ends, by gamut mapping).
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..32 {
                let mid = (lo + hi) / 2.0;
                if luma(Color::oklch(mid, chroma, hue)) < target {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            Color::oklch((lo + hi) / 2.0, chroma, hue).with_alpha(c.alpha())
        };
        let mut out = self.clone();
        for paint in out.paints.iter_mut().flatten() {
            match paint {
                Paint::Solid(c) => *c = mirror(*c),
                Paint::Gradient(g) => {
                    for stop in &mut g.stops {
                        stop.color = mirror(stop.color);
                    }
                }
            }
        }
        out
    }

    /// The background color; for a gradient, its average along the
    /// gradient (what a picture is traced against, and what a dark twin
    /// is placed by).
    pub fn background(&self) -> Color {
        match self.get(Token::Bg) {
            Some(Paint::Solid(c)) => *c,
            Some(Paint::Gradient(g)) => {
                let samples = gradient_samples(g);
                let n = samples.len() as f64;
                let mut sum = [0.0; 4];
                for (rgb, a) in &samples {
                    for i in 0..3 {
                        sum[i] += rgb[i] * a;
                    }
                    sum[3] += a;
                }
                let a = sum[3] / n;
                let byte = |i: usize| {
                    let v = if sum[3] > 0.0 { sum[i] / sum[3] } else { 0.0 };
                    math::round(v.clamp(0.0, 1.0) * 255.0) as u8
                };
                Color::srgb(byte(0), byte(1), byte(2)).with_alpha(a as f32)
            }
            None => unreachable!("bg is always set"),
        }
    }

    /// [`contrast`](Palette::contrast) of `token` keeping only `share` of
    /// its paint (uneven ink, SPEC §7.5).
    pub(crate) fn contrast_faded(&self, token: Token, share: f32) -> Contrast {
        let mut faded = self.clone();
        let paint = self.resolve(token).map_alpha(share);
        faded.paints[token.index()] = Some(paint);
        faded.contrast(token)
    }

    /// Contrast of `token`'s resolved paint against the background. A
    /// translucent background is judged over white; against a gradient,
    /// every point of the paint is judged against every point of the
    /// background, so the result holds wherever they meet.
    pub fn contrast(&self, token: Token) -> Contrast {
        let white = [1.0; 3];
        let paint = self.resolve(token);
        let backdrops = match self.get(Token::Bg) {
            Some(Paint::Gradient(g)) => gradient_samples(g)
                .into_iter()
                .map(|(rgb, a)| [0, 1, 2].map(|i| rgb[i] * a + (1.0 - a)))
                .collect(),
            _ => alloc::vec![composite(self.background(), white)],
        };
        let all: Vec<Contrast> = backdrops
            .into_iter()
            .map(|bg| contrast(&paint, bg))
            .collect();
        let delta_l = all.iter().map(|c| c.delta_l).fold(f32::INFINITY, f32::min);
        let inverted = all[0].inverted;
        if all.iter().any(|c| c.inverted != inverted) {
            return Contrast {
                delta_l: 0.0,
                inverted: false,
            };
        }
        Contrast { delta_l, inverted }
    }
}

/// Rec. 601 luma of the gamma-encoded sRGB color, 0–1, ignoring alpha:
/// what a camera's binarizer (and `freeqrent-verify`) thresholds.
fn luma(color: Color) -> f64 {
    let [r, g, b] = color.to_srgb();
    0.299 * r + 0.587 * g + 0.114 * b
}

/// `color` over an opaque gamma-encoded backdrop, the way browsers blend.
fn composite(color: Color, backdrop: [f64; 3]) -> [f64; 3] {
    let a = color.alpha() as f64;
    let rgb = color.to_srgb();
    [0, 1, 2].map(|i| rgb[i] * a + backdrop[i] * (1.0 - a))
}

fn contrast(paint: &Paint, bg: [f64; 3]) -> Contrast {
    let bg_l = srgb_to_oklab(bg)[0];
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut visit = |rgb: [f64; 3]| {
        let d = bg_l - srgb_to_oklab(rgb)[0];
        lo = lo.min(d);
        hi = hi.max(d);
    };
    match paint {
        Paint::Solid(c) => visit(composite(*c, bg)),
        Paint::Gradient(g) => {
            // Renderers interpolate premultiplied sRGB, which can wander
            // in lightness between stops, so sample along each span.
            for pair in g.stops.windows(2) {
                let (a, b) = (pair[0].color, pair[1].color);
                let (pa, pb) = (premultiplied(a), premultiplied(b));
                for k in 0..=8 {
                    let t = k as f64 / 8.0;
                    let alpha = a.alpha() as f64 * (1.0 - t) + b.alpha() as f64 * t;
                    visit([0, 1, 2].map(|i| pa[i] * (1.0 - t) + pb[i] * t + bg[i] * (1.0 - alpha)));
                }
            }
        }
    }
    if lo > 0.0 {
        Contrast {
            delta_l: lo as f32,
            inverted: false,
        }
    } else if hi < 0.0 {
        Contrast {
            delta_l: -hi as f32,
            inverted: true,
        }
    } else {
        Contrast {
            delta_l: 0.0,
            inverted: false,
        }
    }
}

/// Points along a gradient, as renderers interpolate it (premultiplied
/// sRGB, 8 steps per span): straight sRGB and alpha.
fn gradient_samples(g: &Gradient) -> Vec<([f64; 3], f64)> {
    let mut out = Vec::new();
    for pair in g.stops.windows(2) {
        let (a, b) = (pair[0].color, pair[1].color);
        let (pa, pb) = (premultiplied(a), premultiplied(b));
        for k in 0..=8 {
            let t = k as f64 / 8.0;
            let alpha = a.alpha() as f64 * (1.0 - t) + b.alpha() as f64 * t;
            let rgb = [0, 1, 2].map(|i| {
                let v = pa[i] * (1.0 - t) + pb[i] * t;
                if alpha > 0.0 { v / alpha } else { 0.0 }
            });
            out.push((rgb, alpha));
        }
    }
    out
}

fn premultiplied(c: Color) -> [f64; 3] {
    let a = c.alpha() as f64;
    c.to_srgb().map(|v| v * a)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dark_twins_keep_contrast_and_flip_polarity() {
        let light = Palette::from_tokens([
            ("bg", "#fefcf8"),
            ("fg", "#17161c"),
            ("finder.eye", "#c2410c"),
            ("finder.outer", "linear-gradient(45deg, #1e3a8a, #0f766e)"),
            ("alignment", "rgb(20 30 80 / 80%)"),
        ])
        .unwrap();
        let dark = light.dark_twin();
        assert!((dark.background().to_oklab()[0] - DARK_BG).abs() < 0.005);
        for token in [
            Token::Fg,
            Token::FinderEye,
            Token::FinderOuter,
            Token::Alignment,
        ] {
            let (a, b) = (light.contrast(token), dark.contrast(token));
            assert!(!a.inverted && b.inverted, "{token}: {a:?} {b:?}");
            // Mirrored luma never loses OKLab contrast; mid-tones gain.
            assert!(b.delta_l >= a.delta_l - 0.01, "{token}: {a:?} {b:?}");
        }
        // Luma is mirrored: the twin of a gray is its negative, lifted.
        let lifted = luma(Color::oklch(DARK_BG, 0.0, 0.0));
        let k = luma(light.background()) + lifted;
        let fg = |p: &Palette| match p.get(Token::Fg) {
            Some(Paint::Solid(c)) => luma(*c),
            _ => unreachable!(),
        };
        assert!((fg(&dark) - (k - fg(&light))).abs() < 0.002);
        // Hue and alpha hold; unset tokens stay unset.
        let hue = |p: &Palette, t: Token| {
            let Some(Paint::Solid(c)) = p.get(t) else {
                unreachable!()
            };
            let [_, a, b] = c.to_oklab();
            (math::atan2(b as f64, a as f64), c.alpha())
        };
        let (h0, a0) = hue(&light, Token::Alignment);
        let (h1, a1) = hue(&dark, Token::Alignment);
        assert!((h0 - h1).abs() < 0.02 && a0 == a1);
        assert!(dark.get(Token::Timing).is_none());
        // A dark palette is its own twin.
        assert_eq!(dark.dark_twin(), dark);
        // A near-black ink keeps the background below L 0.16, so it
        // doesn't pass white.
        let deep = Palette::from_tokens([("bg", "#ffffff"), ("fg", "#000000")]).unwrap();
        let twin = deep.dark_twin();
        assert!(twin.background().to_oklab()[0] < 0.01);
        assert!(twin.contrast(Token::Fg).delta_l > 0.99);
        // The red eye that lost ZXing with mirrored OKLab lightness keeps
        // its luma gap.
        let eye = Palette::from_tokens([("finder.top-left.eye", "#b03a2e")]).unwrap();
        let twin = eye.dark_twin();
        let gap = |p: &Palette| {
            let Some(Paint::Solid(c)) = p.get(Token::CornerEye(Corner::TopLeft)) else {
                unreachable!()
            };
            (luma(p.background()) - luma(*c)).abs()
        };
        assert!((gap(&eye) - gap(&twin)).abs() < 0.002);
    }

    #[test]
    fn css_vars_list_solid_tokens() {
        let p = super::Palette::from_tokens([
            ("fg", "oklch(30% .1 265)"),
            ("finder.eye", "rgb(200 0 0 / .5)"),
            ("finder.outer", "linear-gradient(90deg, #000, #333)"),
        ])
        .unwrap();
        assert_eq!(
            p.to_css_vars(),
            "--qr-bg:#fefcf8;--qr-fg:oklch(30% .1 265);--qr-finder-eye:#c80000"
        );
    }

    use super::*;
    use alloc::string::ToString;

    #[test]
    fn token_names_round_trip() {
        for t in Token::ALL {
            assert_eq!(Token::from_name(t.name()), Some(t));
            assert_eq!(
                t.css_var(),
                alloc::format!("--qr-{}", t.name().replace('.', "-"))
            );
        }
        assert_eq!(Token::from_name("finder"), None);
    }

    #[test]
    fn fallback_chain() {
        let p = Palette::from_tokens([("fg", "#123"), ("finder.outer", "#c33")]).unwrap();
        assert_eq!(p.resolve(Token::Timing), Paint::parse("#123").unwrap());
        assert_eq!(p.resolve(Token::Alignment), Paint::parse("#c33").unwrap());
        assert_eq!(
            p.resolve(Token::CornerOuter(Corner::TopRight)),
            Paint::parse("#c33").unwrap()
        );
        assert_eq!(
            p.resolve(Token::CornerEye(Corner::TopLeft)),
            Paint::parse("#123").unwrap()
        );
        assert_eq!(p.resolve(Token::LogoBackdrop), Paint::Solid(p.background()));
        assert_eq!(
            p.resolve(Token::Filler),
            Paint::Solid(Color::srgb(0x11, 0x22, 0x33).with_alpha(0.35))
        );
    }

    #[test]
    fn palette_errors_name_the_token() {
        let err = Palette::from_tokens([("fg", "#12")]).unwrap_err();
        assert_eq!(err.token, "fg");
        assert!(matches!(err.error, ColorError::Syntax { .. }));
        let err = Palette::from_tokens([("fgg", "#123")]).unwrap_err();
        assert_eq!(err.error, ColorError::UnknownToken);
        let err = Palette::from_tokens([("timing", "linear-gradient(#000, #333)")]).unwrap_err();
        assert_eq!(err.token, "timing");
        assert_eq!(err.error, ColorError::GradientNotAllowed);
    }

    #[test]
    fn gradient_backgrounds_judge_the_worst_point() {
        let fg = |bg: &str| {
            Palette::from_tokens([("bg", bg), ("fg", "#000")])
                .unwrap()
                .contrast(Token::Fg)
        };
        // The darker end decides.
        let grey = fg("#999").delta_l;
        let gradient = fg("linear-gradient(#fff, #999)");
        assert!((gradient.delta_l - grey).abs() < 0.01 && !gradient.inverted);
        // A background crossing the ink's lightness has no contrast at all.
        assert_eq!(fg("linear-gradient(#fff, #000)").delta_l, 0.0);
        // A picture or a dark twin sees the average.
        let p = Palette::from_tokens([("bg", "linear-gradient(#ffffff, #000000)")]).unwrap();
        let [r, g, b] = p.background().to_srgb();
        assert!((r - 0.5).abs() < 0.01 && r == g && g == b);
    }

    #[test]
    fn gradients_parse_and_round_trip() {
        for (input, canonical) in [
            (
                "linear-gradient(#000, #333)",
                "linear-gradient(180deg, #000 0%, #333 100%)",
            ),
            (
                "linear-gradient(to top right, red, oklch(40% .1 250) 30%, blue)",
                "linear-gradient(45deg, #f00 0%, oklch(40% .1 250) 30%, #00f 100%)",
            ),
            (
                "linear-gradient(0.25turn, #111 10%, #222, #333, #444 70%)",
                "linear-gradient(90deg, #111 10%, #222 30%, #333 50%, #444 70%)",
            ),
            (
                "radial-gradient(rgb(1 2 3) 20%, #456)",
                "radial-gradient(#010203 20%, #456 100%)",
            ),
            (
                "linear-gradient(-90deg, #111 60%, #222 40%)",
                "linear-gradient(270deg, #111 60%, #222 60%)",
            ),
        ] {
            let paint = Paint::parse(input).unwrap_or_else(|e| panic!("{input}: {e}"));
            assert_eq!(paint.to_string(), canonical);
            assert_eq!(Paint::parse(canonical).unwrap(), paint);
        }
        for bad in [
            "linear-gradient(#000)",
            "linear-gradient(to middle, #000, #fff)",
            "linear-gradient(45deg, #000, #fff",
            "radial-gradient(#000, #fff x%)",
            "linear-gradient(45deg, #000, nope)",
            // Found by fuzzing: a multi-byte char across the prefix check.
            "\0\0\0\0AAAAA\0\0\0A\0r\u{706}",
        ] {
            assert!(Paint::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn contrast_of_default_and_inverted() {
        let p = Palette::default();
        let c = p.contrast(Token::Fg);
        assert!(c.delta_l > 0.75 && !c.inverted, "{c:?}");
        let inv = Palette::from_tokens([("bg", "#111"), ("fg", "#eee")]).unwrap();
        let c = inv.contrast(Token::Fg);
        assert!(c.inverted && c.delta_l > 0.7, "{c:?}");
    }

    #[test]
    fn translucent_fg_loses_contrast() {
        let solid = Palette::from_tokens([("fg", "#000")])
            .unwrap()
            .contrast(Token::Fg);
        let faded = Palette::from_tokens([("fg", "rgb(0 0 0 / .3)")])
            .unwrap()
            .contrast(Token::Fg);
        assert!(
            faded.delta_l < solid.delta_l * 0.5,
            "{faded:?} vs {solid:?}"
        );
    }

    #[test]
    fn gradient_contrast_is_the_worst_point() {
        let p =
            Palette::from_tokens([("bg", "#fff"), ("fg", "linear-gradient(#000, #999)")]).unwrap();
        let grey = Palette::from_tokens([("bg", "#fff"), ("fg", "#999")]).unwrap();
        assert!((p.contrast(Token::Fg).delta_l - grey.contrast(Token::Fg).delta_l).abs() < 1e-6);
        // A gradient through the background's lightness has no contrast.
        let crossing =
            Palette::from_tokens([("bg", "#888"), ("fg", "linear-gradient(#000, #fff)")]).unwrap();
        assert_eq!(crossing.contrast(Token::Fg).delta_l, 0.0);
    }
}
