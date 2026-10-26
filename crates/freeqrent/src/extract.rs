//! Auto color tokens (SPEC §6.4): palettes derived from pixels. Colors are
//! clustered in OKLab, then assigned to `bg`, `fg` and `finder.eye` so the
//! result always clears the contrast rules of §6.3.

use alloc::vec::Vec;
use core::fmt;

use crate::check::CONTRAST_WARNING;
use crate::color::{Color, linear_to_srgb, srgb_to_linear, srgb_to_oklab};
use crate::image::Rgba8Image;
use crate::math;
use crate::palette::{Palette, Token};
use crate::rng::SplitMix64;

/// The longest side images are averaged down to before clustering.
const SAMPLE_SIDE: u32 = 64;
/// Cells with less coverage than this are ignored, so transparent
/// surroundings don't count as a color.
const MIN_COVERAGE: f64 = 0.05;
/// k-means stops after this many rounds even if it hasn't settled.
const ROUNDS: usize = 24;
/// The share a swatch needs to be a background.
const BG_SHARE: f32 = 0.10;
/// The lightness a swatch needs to be a background.
const BG_LIGHTNESS: f32 = 0.85;
/// The most chroma a background may have: pastels yes, vivid colors no.
const BG_CHROMA: f32 = 0.12;
/// The most lightness a dark background may have when inverted palettes
/// are allowed: darker than this leaves room for an ink
/// [`INVERTED_CONTRAST`] lighter.
const BG_DARKNESS: f32 = 0.25;
/// The default minimum contrast. Measured (W20261026073538): v1 codes on
/// backgrounds from L 0.80 to 0.95 fail ZXing on the blurred, noisy render
/// up to ΔL 0.64 and pass from 0.66; near-white backgrounds pass at 0.60.
pub const EXTRACT_CONTRAST: f32 = 0.66;
/// The least contrast an inverted palette keeps. Measured
/// (W20261026141235): v1 codes, light on backgrounds from L 0.04 to 0.28,
/// read inverted, fail ZXing on the blurred, noisy render at ΔL 0.66 on
/// some backgrounds and pass on all of them from 0.68.
pub const INVERTED_CONTRAST: f32 = 0.70;
/// The share a swatch needs to be drawn with at all; smaller clusters are
/// mostly anti-aliased edges.
const INK_SHARE: f32 = 0.02;
/// Chroma from which a swatch counts as a hue rather than a gray.
const CHROMATIC: f32 = 0.04;
/// Chroma an accent needs.
const ACCENT_CHROMA: f32 = 0.06;
/// How far (OKLab ΔE) an accent must be from `fg` to be worth a token.
/// A brand blue or teal darkened to pass lands about 0.09 from a
/// near-black ink and still reads as its hue.
const ACCENT_DISTANCE: f32 = 0.06;
/// The OKLab lightness difference `logo.frame` keeps from `bg` when it's
/// extracted (SPEC §6.4). The frame sits on the logo's plate and is never
/// read; see [`FRAME_FLOOR`](crate::FRAME_FLOOR) for what was measured.
const FRAME_CONTRAST: f32 = 0.30;

/// Options for [`Palette::from_image`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaletteOpts {
    clusters: u8,
    seed: u64,
    min_contrast: f32,
    allow_inverted: bool,
    logo: [Option<Color>; LOGO_COLORS],
}

/// How many of a logo's colors [`PaletteOpts::logo_colors`] keeps.
const LOGO_COLORS: usize = 4;

impl Default for PaletteOpts {
    fn default() -> PaletteOpts {
        PaletteOpts {
            clusters: 6,
            seed: 0,
            min_contrast: EXTRACT_CONTRAST,
            allow_inverted: false,
            logo: [None; LOGO_COLORS],
        }
    }
}

impl PaletteOpts {
    /// Six clusters, seed 0, and a minimum contrast of
    /// [`EXTRACT_CONTRAST`].
    pub fn new() -> PaletteOpts {
        PaletteOpts::default()
    }

    /// How many colors to cluster the image into, 2–12 (clamped).
    pub fn clusters(mut self, k: u8) -> PaletteOpts {
        self.clusters = k.clamp(2, 12);
        self
    }

    /// Seeds the k-means++ start, so a different seed can find a different
    /// split of the same image.
    pub fn seed(mut self, seed: u64) -> PaletteOpts {
        self.seed = seed;
        self
    }

    /// The OKLab lightness difference `fg` and `finder.eye` must keep from
    /// `bg`, 0.60–0.90 (clamped; NaN keeps the default). Colors short of it
    /// are darkened.
    pub fn min_contrast(mut self, delta_l: f32) -> PaletteOpts {
        if !delta_l.is_nan() {
            self.min_contrast = delta_l.clamp(CONTRAST_WARNING, 0.90);
        }
        self
    }

    /// Lets a dark source keep its polarity (SPEC §6.4): when its
    /// background is dark, `bg` is that dark color and the inks are light,
    /// lightened where needed to at least [`INVERTED_CONTRAST`]. Render the
    /// result with [`Style::allow_inverted`](crate::Style::allow_inverted).
    /// Off by default, since some older scanners can't read inverted codes.
    pub fn allow_inverted(mut self, allow: bool) -> PaletteOpts {
        self.allow_inverted = allow;
        self
    }

    /// The colors of the logo the palette will frame ([`Logo::colors`]):
    /// `logo.frame` is chosen at least as far from them as from `fg` and
    /// the eyes, so the frame doesn't take the logo's own color, and is
    /// left unset when nothing qualifies. The first four chromatic ones
    /// count; grays never compete with an accent.
    ///
    /// [`Logo::colors`]: crate::Logo::colors
    pub fn logo_colors(mut self, colors: &[Color]) -> PaletteOpts {
        self.logo = [None; LOGO_COLORS];
        let chromatic = colors.iter().filter(|&&c| chroma(c) >= ACCENT_CHROMA);
        for (slot, &color) in self.logo.iter_mut().zip(chromatic) {
            *slot = Some(color);
        }
        self
    }

    /// Whether inverted palettes are allowed.
    pub fn inverted_allowed(&self) -> bool {
        self.allow_inverted
    }

    /// The cluster count.
    pub fn cluster_count(&self) -> u8 {
        self.clusters
    }

    /// The minimum contrast.
    pub fn contrast(&self) -> f32 {
        self.min_contrast
    }
}

/// A color found in the source, with its share of it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Swatch {
    /// The color.
    pub color: Color,
    /// Its share of the source, 0–1. For an image, the share of covered
    /// pixels; the shares add up to 1.
    pub share: f32,
}

/// Where a token's color came from.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Origin {
    /// Taken as is from [`PaletteReport::swatches`]`[index]`.
    Swatch(usize),
    /// A tinted near-white made from a swatch's hue (or neutral when
    /// `None`), because no swatch was light and common enough to be a
    /// background. For `fg`, the default ink, darkened to pass (lightened
    /// on a dark `bg`), because the source had nothing but its background.
    Tinted(Option<usize>),
    /// A swatch darkened at constant hue until it had enough contrast.
    Darkened {
        /// The swatch.
        swatch: usize,
        /// Its contrast against `bg` before darkening.
        delta_l: f32,
    },
    /// A swatch lightened at constant hue until it had enough contrast
    /// against a dark `bg` ([`PaletteOpts::allow_inverted`]).
    Lightened {
        /// The swatch.
        swatch: usize,
        /// Its contrast against `bg` before lightening (zero when it was
        /// darker).
        delta_l: f32,
    },
}

/// One token the extraction set.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Choice {
    /// The token.
    pub token: Token,
    /// The color it got.
    pub color: Color,
    /// Why.
    pub origin: Origin,
}

/// What [`Palette::from_image`] found and how it used it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PaletteReport {
    /// The source's colors, most common first.
    pub swatches: Vec<Swatch>,
    /// The tokens that were set: `bg`, `fg`, and `finder.eye` and
    /// `logo.frame` when the source has an accent.
    pub choices: Vec<Choice>,
}

/// Why no palette could be derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractError {
    /// The source has no colors: every pixel is transparent.
    NoColors,
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractError::NoColors => f.write_str("no colors to take a palette from"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ExtractError {}

impl Palette {
    /// Derives `bg`, `fg` and (when there is an accent) `finder.eye` and
    /// `logo.frame` from
    /// an image (SPEC §6.4). The image is averaged down to at most 64×64,
    /// clustered in OKLab with seeded k-means++, and the clusters are
    /// assigned so every painted token has at least
    /// [`PaletteOpts::min_contrast`] against `bg`. Deterministic.
    pub fn from_image(
        image: &Rgba8Image,
        opts: PaletteOpts,
    ) -> Result<(Palette, PaletteReport), ExtractError> {
        let samples = downsample(image);
        let swatches = kmeans(&samples, opts.clusters as usize, opts.seed);
        if swatches.is_empty() {
            return Err(ExtractError::NoColors);
        }
        let roles = alloc::vec![Roles::default(); swatches.len()];
        Ok(assign(swatches, &roles, opts))
    }
}

/// An OKLab color with a weight.
#[derive(Clone, Copy, Debug)]
struct Sample {
    lab: [f64; 3],
    weight: f64,
}

/// Box-averages the image in linear light, weighted by alpha, down to at
/// most [`SAMPLE_SIDE`] cells a side.
fn downsample(image: &Rgba8Image) -> Vec<Sample> {
    let (w, h) = (image.width(), image.height());
    let step = w.max(h).div_ceil(SAMPLE_SIDE).max(1);
    let linear: Vec<f64> = (0..=255u8)
        .map(|c| srgb_to_linear(c as f64 / 255.0))
        .collect();
    let data = image.data();
    let mut samples = Vec::new();
    for cy in 0..h.div_ceil(step) {
        for cx in 0..w.div_ceil(step) {
            let mut sum = [0.0f64; 3];
            let (mut alpha, mut count) = (0.0f64, 0.0f64);
            for y in cy * step..((cy + 1) * step).min(h) {
                for x in cx * step..((cx + 1) * step).min(w) {
                    let i = (y as usize * w as usize + x as usize) * 4;
                    let a = data[i + 3] as f64 / 255.0;
                    for (s, &c) in sum.iter_mut().zip(&data[i..i + 3]) {
                        *s += linear[c as usize] * a;
                    }
                    alpha += a;
                    count += 1.0;
                }
            }
            let coverage = alpha / count;
            if coverage < MIN_COVERAGE {
                continue;
            }
            let rgb = sum.map(|s| linear_to_srgb((s / alpha).clamp(0.0, 1.0)));
            samples.push(Sample {
                lab: srgb_to_oklab(rgb),
                weight: coverage,
            });
        }
    }
    samples
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

/// Picks an index with probability proportional to `weights`.
fn pick(weights: &[f64], rng: &mut SplitMix64) -> Option<usize> {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = rng.next_f64() * total;
    for (i, &w) in weights.iter().enumerate() {
        if r < w {
            return Some(i);
        }
        r -= w;
    }
    weights.iter().rposition(|&w| w > 0.0)
}

/// Weighted k-means++ (Arthur & Vassilvitskii 2007) then Lloyd rounds.
/// Returns the clusters as swatches, most common first.
fn kmeans(samples: &[Sample], k: usize, seed: u64) -> Vec<Swatch> {
    let total: f64 = samples.iter().map(|s| s.weight).sum();
    if samples.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let mut rng = SplitMix64::new(seed);
    let weights: Vec<f64> = samples.iter().map(|s| s.weight).collect();
    let mut centers = Vec::with_capacity(k);
    centers.push(samples[pick(&weights, &mut rng).expect("total > 0")].lab);
    while centers.len() < k {
        let d: Vec<f64> = samples
            .iter()
            .map(|s| {
                let near = centers
                    .iter()
                    .map(|&c| dist2(s.lab, c))
                    .fold(f64::INFINITY, f64::min);
                near * s.weight
            })
            .collect();
        match pick(&d, &mut rng) {
            Some(i) => centers.push(samples[i].lab),
            // Every sample sits on a center: fewer colors than k.
            None => break,
        }
    }

    let nearest = |lab: [f64; 3], centers: &[[f64; 3]]| {
        let mut best = (0, f64::INFINITY);
        for (j, &c) in centers.iter().enumerate() {
            let d = dist2(lab, c);
            if d < best.1 {
                best = (j, d);
            }
        }
        best.0
    };
    let mut owner = alloc::vec![usize::MAX; samples.len()];
    let mut mass = alloc::vec![0.0; centers.len()];
    for _ in 0..ROUNDS {
        let mut changed = false;
        for (o, s) in owner.iter_mut().zip(samples) {
            let j = nearest(s.lab, &centers);
            changed |= *o != j;
            *o = j;
        }
        let mut sums = alloc::vec![[0.0f64; 3]; centers.len()];
        mass = alloc::vec![0.0; centers.len()];
        for (&j, s) in owner.iter().zip(samples) {
            for (sum, v) in sums[j].iter_mut().zip(s.lab) {
                *sum += v * s.weight;
            }
            mass[j] += s.weight;
        }
        for (j, c) in centers.iter_mut().enumerate() {
            if mass[j] > 0.0 {
                *c = sums[j].map(|v| v / mass[j]);
            }
        }
        if !changed {
            break;
        }
    }

    let mut swatches: Vec<Swatch> = centers
        .iter()
        .zip(&mass)
        .filter(|&(_, &m)| m > 0.0)
        .map(|(&lab, &m)| Swatch {
            color: lab_color(lab),
            share: (m / total) as f32,
        })
        .collect();
    crate::sort::sort_by(&mut swatches, |a, b| {
        b.share
            .total_cmp(&a.share)
            .then(b.color.to_oklab()[0].total_cmp(&a.color.to_oklab()[0]))
    });
    swatches
}

/// An OKLab color as `oklch()`.
fn lab_color([l, a, b]: [f64; 3]) -> Color {
    let c = math::sqrt(a * a + b * b);
    let h = math::atan2(b, a) * (180.0 / core::f64::consts::PI);
    Color::oklch(l as f32, c as f32, h as f32)
}

fn chroma(color: Color) -> f32 {
    let [_, a, b] = color.to_oklab();
    math::sqrt((a * a + b * b) as f64) as f32
}

fn distance(x: Color, y: Color) -> f32 {
    let (p, q) = (x.to_oklab(), y.to_oklab());
    let d: f32 = (0..3).map(|i| (p[i] - q[i]) * (p[i] - q[i])).sum();
    math::sqrt(d as f64) as f32
}

/// `color`'s contrast against `bg`, or zero when it is on the wrong side:
/// lighter, or darker when `inverted`.
fn contrast(bg: Color, color: Color, inverted: bool) -> f32 {
    let palette = Palette::new()
        .with(Token::Bg, bg)
        .and_then(|p| p.with(Token::Fg, color))
        .expect("solid colors");
    let c = palette.contrast(Token::Fg);
    if c.inverted == inverted {
        c.delta_l
    } else {
        0.0
    }
}

/// `color` at the OKLCH lightness nearest its own (same chroma and hue)
/// that has `min` contrast against `bg`: the highest one, or the lowest
/// when `inverted`.
fn shade(bg: Color, color: Color, min: f32, inverted: bool) -> Color {
    let [l, a, b] = color.to_oklab();
    let c = math::sqrt((a * a + b * b) as f64) as f32;
    let h = (math::atan2(b as f64, a as f64) * (180.0 / core::f64::consts::PI)) as f32;
    let at = |l: f32| Color::oklch(l, c, h);
    // `pass` passes and `fail` doesn't (or is where the search starts).
    let (mut pass, mut fail) = if inverted { (1.0f32, l) } else { (0.0f32, l) };
    for _ in 0..24 {
        let mid = (pass + fail) / 2.0;
        if contrast(bg, at(mid), inverted) >= min {
            pass = mid;
        } else {
            fail = mid;
        }
    }
    // Stored lightness is rounded; step away from bg until it passes.
    let step = if inverted { 0.0005 } else { -0.0005 };
    let mut out = at(pass);
    let mut l = pass;
    while contrast(bg, out, inverted) < min && (0.0..=1.0).contains(&(l + step)) {
        l += step;
        out = at(l);
    }
    out
}

/// Whether the source is dark: its most strongly marked low-chroma
/// background is darker than mid-gray, or, when it marks none, its colors
/// are on average (by share).
fn dark_first(swatches: &[Swatch], roles: &[Roles]) -> bool {
    let lightness = |i: usize| swatches[i].color.to_oklab()[0];
    let n = swatches.len();
    match best(
        (0..n).filter(|&i| roles[i].bg > 0.0 && chroma(swatches[i].color) <= BG_CHROMA),
        |i| roles[i].bg,
    ) {
        Some(i) => lightness(i) < 0.5,
        None => {
            let total: f32 = swatches.iter().map(|s| s.share).sum();
            let mean: f32 = (0..n).map(|i| lightness(i) * swatches[i].share).sum();
            total > 0.0 && mean / total < 0.5
        }
    }
}

/// How strongly a source marks a swatch for each role: CSS says which
/// colors are backgrounds, text and links; pixels don't.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Roles {
    pub(crate) bg: f32,
    pub(crate) text: f32,
    pub(crate) accent: f32,
}

/// The index with the largest `key` among `items`, first on ties.
fn best<K: Fn(usize) -> f32>(items: impl Iterator<Item = usize>, key: K) -> Option<usize> {
    let mut out: Option<(usize, f32)> = None;
    for i in items {
        let k = key(i);
        if out.is_none_or(|(_, best)| k > best) {
            out = Some((i, k));
        }
    }
    out.map(|(i, _)| i)
}

/// Assigns swatches to tokens (SPEC §6.4). Where `roles` marks swatches
/// for a role, the most strongly marked eligible one wins; otherwise the
/// pixel rules apply. A dark source, when `opts` allows inverted palettes,
/// gets the same rules mirrored: a dark `bg` and light inks.
pub(crate) fn assign(
    swatches: Vec<Swatch>,
    roles: &[Roles],
    opts: PaletteOpts,
) -> (Palette, PaletteReport) {
    let inverted = opts.allow_inverted && dark_first(&swatches, roles);
    let min = if inverted {
        opts.min_contrast.max(INVERTED_CONTRAST)
    } else {
        opts.min_contrast
    };
    // Lightness toward the background's end of the scale: high on a light
    // page, low on a dark one.
    let toward_bg = |l: f32| if inverted { -l } else { l };
    let n = swatches.len();
    let lightness = |i: usize| swatches[i].color.to_oklab()[0];
    let hue = |i: usize| chroma(swatches[i].color);
    let common = |i: usize| swatches[i].share >= INK_SHARE;
    let accent_hint = || {
        best(
            (0..n).filter(|&i| roles[i].accent > 0.0 && hue(i) >= CHROMATIC),
            |i| roles[i].accent,
        )
    };
    let dominant_hue = accent_hint().or_else(|| (0..n).find(|&i| hue(i) >= CHROMATIC && common(i)));

    // bg: the background the source names, else the lightest common
    // swatch (darkest, inverted), else a near-white tinted by the dominant
    // hue (a near-black, inverted).
    let light = |i: usize| {
        let l = lightness(i);
        hue(i) <= BG_CHROMA
            && if inverted {
                l <= BG_DARKNESS
            } else {
                l >= BG_LIGHTNESS
            }
    };
    let bg_index = best((0..n).filter(|&i| light(i) && roles[i].bg > 0.0), |i| {
        roles[i].bg
    })
    .or_else(|| {
        best(
            (0..n).filter(|&i| light(i) && swatches[i].share >= BG_SHARE),
            |i| toward_bg(lightness(i)),
        )
    });
    let (bg, bg_origin) = match bg_index {
        Some(i) => (swatches[i].color, Origin::Swatch(i)),
        None => {
            let (c, h) = match dominant_hue {
                Some(i) => {
                    let [_, a, b] = swatches[i].color.to_oklab();
                    let c = math::sqrt((a * a + b * b) as f64) as f32;
                    let h = math::atan2(b as f64, a as f64) * (180.0 / core::f64::consts::PI);
                    ((c * 0.2).min(0.015), h as f32)
                }
                None => (0.0, 0.0),
            };
            let l = if inverted { 0.16 } else { 0.97 };
            (Color::oklch(l, c, h), Origin::Tinted(dominant_hue))
        }
    };

    let marked = |i: usize| roles[i] != Roles::default();
    let ink: Vec<usize> = (0..n)
        .filter(|&i| Some(i) != bg_index && (common(i) || marked(i)))
        .collect();
    let passes = |i: usize| contrast(bg, swatches[i].color, inverted) >= min;
    let settle_at = |i: usize, min: f32| {
        let color = swatches[i].color;
        let delta_l = contrast(bg, color, inverted);
        if delta_l >= min {
            (color, Origin::Swatch(i))
        } else if inverted {
            (
                shade(bg, color, min, true),
                Origin::Lightened { swatch: i, delta_l },
            )
        } else {
            (
                shade(bg, color, min, false),
                Origin::Darkened { swatch: i, delta_l },
            )
        }
    };
    let settle = |i: usize| settle_at(i, min);

    // fg: the text color the source names, else the darkest swatch
    // (lightest, inverted); if neither passes, the named text color or the
    // dominant hue darkened (lightened).
    let text = |i: usize| roles[i].text;
    let darkest = best(ink.iter().copied(), |i| -toward_bg(lightness(i)));
    let fg_index = best(
        ink.iter().copied().filter(|&i| text(i) > 0.0 && passes(i)),
        text,
    )
    .or(darkest.filter(|&i| passes(i)))
    .or_else(|| best(ink.iter().copied().filter(|&i| text(i) > 0.0), text))
    .or(dominant_hue.filter(|&d| Some(d) != bg_index))
    .or(darkest);
    let (fg, fg_origin) = match fg_index {
        Some(i) => settle(i),
        // Nothing but the background: the default ink (the default
        // background, inverted), darkened (lightened) to pass.
        None => {
            let token = if inverted { Token::Bg } else { Token::Fg };
            let crate::palette::Paint::Solid(c) = Palette::new().resolve(token) else {
                unreachable!("the default bg and fg are solid")
            };
            (shade(bg, c, min, inverted), Origin::Tinted(None))
        }
    };

    // finder.eye: the accent the source names, else the most prominent
    // hue that isn't fg.
    let candidates = || {
        ink.iter()
            .copied()
            .filter(|&i| Some(i) != fg_index && hue(i) >= ACCENT_CHROMA)
    };
    let accent_index = best(candidates().filter(|&i| roles[i].accent > 0.0), |i| {
        roles[i].accent
    })
    .or_else(|| best(candidates(), |i| swatches[i].share * hue(i)))
    .filter(|&i| distance(settle(i).0, fg) >= ACCENT_DISTANCE);
    let accent = accent_index.map(settle);

    // logo.frame: an off color for the logo's border, the second most
    // prominent hue when the source has one apart from fg, the eyes and the
    // logo, else the eyes' accent if it's apart from the logo. The frame
    // is never read, so it only darkens (lightens) to FRAME_CONTRAST.
    let framing = |i: usize| settle_at(i, FRAME_CONTRAST.min(min));
    let off_logo = |c: Color| {
        opts.logo
            .iter()
            .flatten()
            .all(|&l| distance(c, l) >= ACCENT_DISTANCE)
    };
    let second = best(
        candidates().filter(|&i| {
            Some(i) != accent_index && {
                let c = framing(i).0;
                distance(c, fg) >= ACCENT_DISTANCE
                    && accent.is_none_or(|(eye, _)| distance(c, eye) >= ACCENT_DISTANCE)
                    && off_logo(c)
            }
        }),
        |i| roles[i].accent + swatches[i].share * hue(i),
    )
    .filter(|_| accent.is_some())
    .map(framing);
    let frame = second.or_else(|| accent_index.map(framing).filter(|&(c, _)| off_logo(c)));

    let mut palette = Palette::new();
    let mut choices = Vec::new();
    let mut put = |token: Token, (color, origin): (Color, Origin)| {
        palette.set(token, color).expect("solid colors");
        choices.push(Choice {
            token,
            color,
            origin,
        });
    };
    put(Token::Bg, (bg, bg_origin));
    put(Token::Fg, (fg, fg_origin));
    if let Some(eye) = accent {
        put(Token::FinderEye, eye);
    }
    if let Some(frame) = frame {
        put(Token::LogoFrame, frame);
    }
    (palette, PaletteReport { swatches, choices })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stripes(colors: &[([u8; 4], u32)]) -> Rgba8Image {
        let height: u32 = colors.iter().map(|c| c.1).sum();
        let mut data = Vec::new();
        for &(rgba, rows) in colors {
            for _ in 0..rows * 32 {
                data.extend_from_slice(&rgba);
            }
        }
        Rgba8Image::new(32, height, data).unwrap()
    }

    fn token(report: &PaletteReport, token: Token) -> Option<Choice> {
        report.choices.iter().copied().find(|c| c.token == token)
    }

    #[test]
    fn picks_the_obvious_roles() {
        // White page, navy ink, an orange accent.
        let img = stripes(&[
            ([250, 248, 244, 255], 64),
            ([20, 30, 80, 255], 24),
            ([230, 90, 20, 255], 16),
        ]);
        let (palette, report) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        assert_eq!(report.swatches.len(), 3);
        assert!(matches!(
            token(&report, Token::Bg).unwrap().origin,
            Origin::Swatch(0)
        ));
        assert!(matches!(
            token(&report, Token::Fg).unwrap().origin,
            Origin::Swatch(1)
        ));
        let eye = token(&report, Token::FinderEye).unwrap();
        assert!(matches!(eye.origin, Origin::Darkened { swatch: 2, .. }));
        for t in [Token::Fg, Token::FinderEye] {
            let c = palette.contrast(t);
            assert!(!c.inverted && c.delta_l >= EXTRACT_CONTRAST, "{t}: {c:?}");
        }
        // The accent keeps its hue while it darkens.
        let hue = |c: Color| {
            let [_, a, b] = c.to_oklab();
            math::atan2(b as f64, a as f64)
        };
        let orange = report.swatches[2].color;
        assert!((hue(eye.color) - hue(orange)).abs() < 0.05);
        // One accent: the logo's frame takes it too, at its own lightness,
        // since the frame only needs FRAME_CONTRAST.
        let frame = token(&report, Token::LogoFrame).unwrap();
        assert!(matches!(frame.origin, Origin::Swatch(2)), "{frame:?}");
        assert_eq!(frame.color, orange);
    }

    #[test]
    fn a_second_accent_frames_the_logo() {
        // White page, navy ink, orange and teal accents.
        let img = stripes(&[
            ([250, 248, 244, 255], 60),
            ([20, 30, 80, 255], 24),
            ([230, 90, 20, 255], 12),
            ([20, 150, 140, 255], 8),
        ]);
        let (palette, report) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        let eye = token(&report, Token::FinderEye).unwrap();
        let frame = token(&report, Token::LogoFrame).unwrap();
        assert!(matches!(eye.origin, Origin::Darkened { swatch: 2, .. }));
        assert!(matches!(
            frame.origin,
            Origin::Swatch(3) | Origin::Darkened { swatch: 3, .. }
        ));
        let c = palette.contrast(Token::LogoFrame);
        assert!(!c.inverted && c.delta_l >= FRAME_CONTRAST, "{c:?}");
    }

    #[test]
    fn frames_keep_apart_from_the_logo() {
        // Orange and teal accents: the orange takes the eyes and the teal
        // the frame; with a teal logo the frame falls back to the orange,
        // and with an orange and teal logo it's left to follow fg.
        let img = stripes(&[
            ([250, 248, 244, 255], 60),
            ([20, 30, 80, 255], 24),
            ([230, 90, 20, 255], 12),
            ([20, 150, 140, 255], 8),
        ]);
        let frame = |logo: &[Color]| {
            let opts = PaletteOpts::new().logo_colors(logo);
            let (_, report) = Palette::from_image(&img, opts).unwrap();
            token(&report, Token::LogoFrame).map(|c| c.origin)
        };
        let teal = Color::parse("rgb(20 150 140)").unwrap();
        let orange = Color::parse("rgb(230 90 20)").unwrap();
        let white = Color::parse("white").unwrap();
        assert!(matches!(frame(&[]), Some(Origin::Swatch(3))));
        // A gray logo color doesn't compete with an accent.
        assert!(matches!(frame(&[white]), Some(Origin::Swatch(3))));
        assert!(matches!(frame(&[teal]), Some(Origin::Swatch(2))));
        assert_eq!(frame(&[white, teal, orange]), None);
    }

    #[test]
    fn transparent_pixels_are_ignored() {
        // A dark mark on a transparent canvas: no light swatch, so bg is a
        // tinted near-white.
        let img = stripes(&[([0, 0, 0, 0], 80), ([40, 20, 120, 255], 20)]);
        let (palette, report) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        assert_eq!(report.swatches.len(), 1);
        assert!((report.swatches[0].share - 1.0).abs() < 1e-6);
        assert!(matches!(
            token(&report, Token::Bg).unwrap().origin,
            Origin::Tinted(Some(0))
        ));
        assert!(palette.background().to_oklab()[0] > 0.95);
        let empty = Rgba8Image::filled(8, 8, [255, 0, 0, 0]).unwrap();
        assert_eq!(
            Palette::from_image(&empty, PaletteOpts::new()),
            Err(ExtractError::NoColors)
        );
    }

    #[test]
    fn dark_sources_keep_their_polarity_when_allowed() {
        // A near-black page, light gray ink, a cyan accent.
        let img = stripes(&[
            ([14, 17, 22, 255], 64),
            ([228, 230, 232, 255], 24),
            ([0, 150, 200, 255], 16),
        ]);
        let opts = PaletteOpts::new().allow_inverted(true);
        let (palette, report) = Palette::from_image(&img, opts).unwrap();
        assert!(matches!(
            token(&report, Token::Bg).unwrap().origin,
            Origin::Swatch(0)
        ));
        // The light gray is the ink; the cyan is lightened to pass.
        let fg = token(&report, Token::Fg).unwrap();
        assert!(matches!(fg.origin, Origin::Swatch(1)), "{report:?}");
        let eye = token(&report, Token::FinderEye).unwrap();
        assert!(matches!(eye.origin, Origin::Lightened { swatch: 2, .. }));
        for t in [Token::Fg, Token::FinderEye] {
            let c = palette.contrast(t);
            assert!(c.inverted && c.delta_l >= INVERTED_CONTRAST, "{t}: {c:?}");
        }
        let c = palette.contrast(Token::LogoFrame);
        assert!(c.inverted && c.delta_l >= FRAME_CONTRAST, "{c:?}");
        // Without the option, the same image is dark on light.
        let (palette, _) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        assert!(palette.background().to_oklab()[0] > 0.85);
        assert!(!palette.contrast(Token::Fg).inverted);
        // A light image is unchanged by it.
        let light = stripes(&[([250, 248, 244, 255], 64), ([20, 30, 80, 255], 24)]);
        assert_eq!(
            Palette::from_image(&light, opts).unwrap(),
            Palette::from_image(&light, PaletteOpts::new()).unwrap()
        );
    }

    #[test]
    fn dim_sources_get_a_tinted_near_black() {
        // Mid-dark blues, none dark enough to be a background.
        let img = stripes(&[
            ([40, 55, 90, 255], 50),
            ([70, 90, 130, 255], 30),
            ([150, 160, 170, 255], 20),
        ]);
        let opts = PaletteOpts::new().allow_inverted(true);
        let (palette, report) = Palette::from_image(&img, opts).unwrap();
        assert!(matches!(
            token(&report, Token::Bg).unwrap().origin,
            Origin::Tinted(Some(_))
        ));
        assert!(palette.background().to_oklab()[0] < BG_DARKNESS);
        let c = palette.contrast(Token::Fg);
        assert!(c.inverted && c.delta_l >= INVERTED_CONTRAST, "{c:?}");
    }

    #[test]
    fn pale_images_get_a_darkened_ink() {
        let img = stripes(&[([255, 255, 255, 255], 50), ([250, 180, 200, 255], 50)]);
        let (palette, report) = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        let fg = token(&report, Token::Fg).unwrap();
        assert!(matches!(fg.origin, Origin::Darkened { swatch: 1, delta_l } if delta_l < 0.3));
        assert!(palette.contrast(Token::Fg).delta_l >= EXTRACT_CONTRAST);
        // A single pink isn't a second accent.
        assert!(token(&report, Token::FinderEye).is_none());
    }

    #[test]
    fn deterministic_and_seeded() {
        let mut img = Rgba8Image::filled(97, 61, [0; 4]).unwrap();
        let mut rng = SplitMix64::new(7);
        for y in 0..61 {
            for x in 0..97 {
                let v = rng.next_u64();
                img.set_pixel(x, y, [v as u8, (v >> 8) as u8, (v >> 16) as u8, 255]);
            }
        }
        let a = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        let b = Palette::from_image(&img, PaletteOpts::new()).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.1.swatches.len(), 6);
        let total: f32 = a.1.swatches.iter().map(|s| s.share).sum();
        assert!((total - 1.0).abs() < 1e-4);
        let c = Palette::from_image(&img, PaletteOpts::new().seed(3).clusters(4)).unwrap();
        assert_eq!(c.1.swatches.len(), 4);
        for (palette, _) in [a, c] {
            assert!(palette.contrast(Token::Fg).delta_l >= EXTRACT_CONTRAST);
        }
    }

    #[test]
    fn options_clamp() {
        let o = PaletteOpts::new()
            .clusters(40)
            .min_contrast(0.1)
            .min_contrast(f32::NAN);
        assert_eq!(o.cluster_count(), 12);
        assert_eq!(o.contrast(), CONTRAST_WARNING);
        assert_eq!(PaletteOpts::new().contrast(), EXTRACT_CONTRAST);
        assert_eq!(PaletteOpts::new().min_contrast(2.0).contrast(), 0.90);
    }
}
