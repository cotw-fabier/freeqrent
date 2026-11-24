//! Static scannability rules (SPEC §9, first row): what `Style::check`
//! reports and what makes `Style::render` refuse.

use alloc::vec::Vec;
use core::fmt;

use crate::Ecc;
use crate::geometry::Grid;
use crate::glyph::GlyphPart;
#[cfg(feature = "halftone")]
use crate::halftone::{self, Steering};
use crate::logo::{self, LogoShape};
use crate::matrix::{QrMatrix, misdecode_protection};
use crate::palette::Token;
use crate::scanbox::{AlignmentShape, EyeShape, RingShape};
use crate::scene;
use crate::style::{Border, Effect, Frame, ModuleShape, Style, TargetSize, Texture};
use crate::version::BlockLayout;

/// The OKLab lightness difference below which a code isn't rendered. At
/// or above it, codes decode from clean renders (`freeqrent-verify`).
pub const CONTRAST_FLOOR: f32 = 0.40;
/// The share of a block's correctable codewords that cleared modules may
/// use up (SPEC §7.3), leaving the rest for print and camera damage.
pub const DAMAGE_BUDGET: f32 = 0.60;
/// [`DAMAGE_BUDGET`] for a code drawn into a picture (SPEC §7.7): its
/// sub-cells cost decoders that sample off-center a few codewords too.
/// Measured (W20261026092854): with a logo at 0.5, rqrr missed 3 of 216
/// clean renders between 4 and 12 px/module; at 0.4, none.
pub const HALFTONE_DAMAGE_BUDGET: f32 = 0.40;

/// [`DAMAGE_BUDGET`] at ECC H. H's large share of error correction lets a
/// logo cover much more of the code, and the largest ones started to hide
/// it from ZXing under blur and noise. Measured (W20261026171519): the
/// largest logo each code afforded, a striped dark mark on square and
/// circle plates, framed and not, square and dot modules, light and dark:
/// at 60%, 10 of 80 codes from v16 to v20 at H weren't found on the
/// degraded render (logos 0.22–0.35 wide); at 45%, none from v5 to v40.
pub const DAMAGE_BUDGET_H: f32 = 0.45;

/// The damage budget that applies to `style` on `qr`: the smaller of the
/// picture's and ECC H's, where they apply.
pub(crate) fn damage_budget(style: &Style, qr: &QrMatrix) -> f32 {
    let level = if qr.ecc() == Ecc::H {
        DAMAGE_BUDGET_H
    } else {
        DAMAGE_BUDGET
    };
    #[cfg(feature = "halftone")]
    if style.halftone.is_some() {
        return level.min(HALFTONE_DAMAGE_BUDGET);
    }
    let _ = style;
    level
}

/// Whether `style`'s logo must leave every alignment pattern uncovered: on
/// a picture (SPEC §7.3, §7.7), unless the alignment patterns are outlined
/// and the timing dotted, as in art mode. rqrr fits its grid to the
/// bottom-right alignment pattern and refines it by scoring every pattern,
/// covered ones too, and on a halftone that fit drifted across the lower
/// half of the code. Measured (W20261026151955), on the gallery's
/// halftone cards with a logo at 0.12 on two wallpapers and the sample
/// sunset, light and dark, at every size from 4 to 14 px/module: with
/// the plate over an alignment pattern, 13 of 18 missed rqrr at 6, 7 or
/// 12 px; with Outline alone, the 9 dark ones still missed at 7; with
/// Outline and dotted timing, and on art mode, none did; nor did any code
/// whose plate left the patterns uncovered.
fn spares_alignment(style: &Style) -> bool {
    #[cfg(feature = "halftone")]
    if style.halftone.is_some() && style.logo.is_some() {
        let art = style.alignment.emphasis == crate::scanbox::Emphasis::Outline
            && style.timing == crate::scanbox::TimingStyle::Dotted;
        return !art;
    }
    let _ = style;
    false
}

/// The OKLab lightness difference below which `logo.frame` isn't rendered
/// (SPEC §6.1). The frame sits on the logo's plate, among modules the
/// budget already counts as lost, so it needs none of the read tokens'
/// contrast. Measured (W20261026160831) on the largest logo each code
/// affords, every frame and plate shape, square and dot modules, light and
/// dark twins: frames from 0.25 to 0.66 scan exactly alike (the only
/// misses, at ECC H on v16–17, happen at 0.66 too), and frames at 0.13
/// and 0.05 missed the degraded render more often on dark twins.
pub const FRAME_FLOOR: f32 = 0.25;

/// The difference below which a code renders with a warning: under blur
/// and sensor noise, ZXing starts missing codes below about 0.6, the
/// finder eyes first.
pub const CONTRAST_WARNING: f32 = 0.60;

/// The lightness targets every measured lens shape scanned at (SPEC §7.9):
/// dark glass modules at most 0.25, light ones at least 0.90, in OKLab L.
pub const GLASS_SAFE_TARGETS: (f32, f32) = (0.25, 0.90);
/// How far glass targets may sit inside [`GLASS_SAFE_TARGETS`] (the larger
/// of the two gaps) before the code is refused as
/// [`RenderError::GlassTargetsTooWeak`].
pub const GLASS_TARGET_MARGIN: f32 = 0.02;
/// The least finder contrast (plate minus finder glass, OKLab L) at which
/// ZXing read every measured glass code; below it, or with the plate under
/// [`GLASS_PLATE_FLOOR`], a glass code is refused as
/// [`RenderError::GlassFinderContrast`].
pub const GLASS_FINDER_FLOOR: f32 = 0.58;
/// The least plate lightness (OKLab L) at which ZXing read every measured
/// glass code: at 0.88 and 0.90 the degraded render lost some, whatever the
/// finder's contrast.
pub const GLASS_PLATE_FLOOR: f32 = 0.92;
/// Below this finder contrast (the default's, 0.62) a glass code renders
/// with [`Warning::LowContrast`]: its degraded render was the marginal one.
pub const GLASS_FINDER_WARNING: f32 = 0.62;
/// The largest version whose glass codes passed the full bar (ZXing and
/// rqrr) in every measured case at the default resolution. Above it, or
/// below [`GLASS_RESOLUTION`], a glass code warns
/// [`Warning::GlassNeedsZxing`].
pub const GLASS_FULL_BAR_VERSION: u8 = 4;
/// The same for vector glass ([`GlassRender::Vector`](crate::GlassRender::Vector)),
/// which has no resolution: every measured code from v2 to v11 passed the
/// full bar, and v12 lost rqrr on one of 14.
pub const GLASS_VECTOR_FULL_BAR_VERSION: u8 = 11;
/// Vector glass's full bar ([`GLASS_VECTOR_FULL_BAR_VERSION`]) was measured
/// at lenses this wide or wider; narrower lenses warn
/// [`Warning::GlassNeedsZxing`] above [`GLASS_VECTOR_NARROW_FULL_BAR_VERSION`]
/// (lenses of 0.7 and 0.8 lost rqrr at v8, held at v3 and v5).
pub const GLASS_VECTOR_FULL_BAR_LENS: f32 = 0.9;
/// See [`GLASS_VECTOR_FULL_BAR_LENS`].
pub const GLASS_VECTOR_NARROW_FULL_BAR_VERSION: u8 = 5;
/// The narrowest vector glass lens: at 0.6 ZXing missed a code (the flat
/// disc at v8) and at 0.5 three, so `Glass.lens` is held to 0.7–1.0 in
/// vector mode ([`RenderError::OptionOutOfRange`]).
pub const GLASS_VECTOR_MIN_LENS: f32 = 0.7;
/// The most plate texture at which every measured glass code held the full
/// bar (W20261124091813, the three showcase photos × square, Liquid with
/// rounded finders, 0.9 dots and v8): at 0.4 rqrr lost the samurai at
/// 4 px per module, at 1 also the windmill; ZXing read every render at
/// every texture, the degraded one included. Above it a glass code warns
/// [`Warning::GlassNeedsZxing`].
pub const GLASS_TEXTURE_FULL_BAR: f32 = 0.2;
/// The smallest dots baked glass was measured to scan with
/// (W20261124091813): 0.9 read every picture at its bar; at 0.85 the disc
/// lost the degraded render once, at 0.75–0.8 once or twice.
pub const GLASS_DOTS_MIN: f32 = 0.9;
/// The least margin (OKLab L) the weakest tenth of data modules may land
/// from the dark/light midpoint once the glass is composited
/// ([`RenderError::GlassLowReadability`]): the safe targets' half-gap less
/// [`GLASS_TARGET_MARGIN`], what the targets were measured to hold (SPEC
/// §7.9). Measured (W20261124091813, 7 pictures × v3/5/8 at M, baked and
/// vector): every default code is at 0.325; at opacity 0.9 ZXing missed 1
/// of 42 on the degraded render, at 0.7 9, at 0.5 30, at 0.3 all.
pub const GLASS_READABILITY: f32 = 0.305;
/// The default baked resolution of glass, in pixels per module
/// ([`Glass::resolution`](crate::Glass::resolution)).
pub const GLASS_RESOLUTION: u8 = 16;

/// The most baked pixels per module glass takes
/// ([`Glass::resolution`](crate::Glass::resolution)): enough for a 4096 px
/// export of a code up to about 43 modules across, frame included.
pub(crate) const GLASS_RESOLUTION_MAX: u8 = 96;
/// The glass layer's size (a baked PNG, or vector glass's elements) above
/// which a code warns [`Warning::LargeOutput`].
pub const LARGE_OUTPUT_BYTES: usize = 1_000_000;

/// How strictly [`Style::check`](crate::Style::check) holds a style to
/// what was measured to scan (SPEC §9; W20261124091813). Every error is
/// either a *readability* error (a measured risk: the code can be drawn,
/// some scanners may miss it) or a *hard* one (the code can't be drawn as
/// asked, or certainly won't decode).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Checks {
    /// Refuses every error, readability and hard alike: the guarantee
    /// that a rendered code scans.
    Strict,
    /// The default. Readability errors render, each as a warning
    /// ([`Warning::Unchecked`], or the same-named glass warning), and
    /// [`ScanReport::scannable`] is false. Hard errors are refused.
    #[default]
    Lenient,
    /// Nothing is refused. Every error becomes a warning and the code
    /// renders as well as it can: options outside their range are clamped
    /// into it and options that can't be combined are left out (SPEC §9
    /// lists what each error draws).
    Off,
}

/// Options whose range is a scan measurement on one side, with the hard
/// bounds past which the value stops making sense (SPEC §9): inside the
/// hard bounds, an out-of-range value is a readability error, drawn as
/// given under [`Checks::Lenient`]; past them, or NaN, it is a hard error
/// and [`Checks::Off`] clamps it to the hard bounds. Every other range is
/// physical: out of it is a hard error, clamped to the range.
const MEASURED: &[(&str, f32, f32)] = &[
    ("Square.gap", 0.0, 0.45),
    ("Dots.size", 0.05, 1.0),
    ("Effect::Glow radius", 0.05, 1.0),
    ("Effect::Glow strength", 0.0, 1.0),
    ("Effect::Emboss depth", 0.05, 0.5),
    ("Effect::Sketch roughness", 0.02, 0.5),
    ("Texture::Grain amount", 0.02, 0.5),
    ("Texture::Scanlines opacity", 0.05, 1.0),
    ("Texture::Ink fade", 0.05, 1.0),
    ("Texture::Scanlines opacity + Texture::Ink fade", 0.0, 2.0),
    ("Glass.lens", 0.1, 1.0),
    ("Dots.size on baked glass", 0.05, 1.0),
];

/// Errors as they are found, each marked hard or not (readability).
#[derive(Default)]
pub(crate) struct Found {
    list: Vec<(RenderError, bool)>,
}

impl Found {
    /// A readability error: a measured risk.
    fn soft(&mut self, e: RenderError) {
        self.list.push((e, false));
    }

    /// A hard error: can't be drawn as asked, or certainly won't decode.
    fn hard(&mut self, e: RenderError) {
        self.list.push((e, true));
    }

    /// Holds `value` to `min`–`max` (NaN fails). Out of range, it is a
    /// hard error and clamped, unless [`MEASURED`] lists the option and the
    /// value is within its hard bounds: then a readability error, and kept.
    fn range(&mut self, option: &'static str, value: &mut f32, min: f32, max: f32) {
        if *value >= min && *value <= max {
            return;
        }
        let e = RenderError::OptionOutOfRange {
            option,
            value: *value,
            min,
            max,
        };
        let hard = MEASURED.iter().find(|m| m.0 == option);
        match hard {
            Some(&(_, lo, hi)) if *value >= lo && *value <= hi => self.soft(e),
            Some(&(_, lo, hi)) => {
                self.hard(e);
                *value = if value.is_nan() {
                    min
                } else {
                    value.clamp(lo, hi)
                };
            }
            None => {
                self.hard(e);
                *value = if value.is_nan() {
                    min
                } else {
                    value.clamp(min, max)
                };
            }
        }
    }

    /// [`Found::range`] for whole numbers; their ranges are all physical.
    fn range_int<T: Copy + Into<f64>>(
        &mut self,
        option: &'static str,
        value: &mut T,
        min: T,
        max: T,
    ) {
        let (v, lo, hi) = ((*value).into(), min.into(), max.into());
        if v >= lo && v <= hi {
            return;
        }
        self.hard(RenderError::OptionOutOfRange {
            option,
            value: v as f32,
            min: lo as f32,
            max: hi as f32,
        });
        *value = if v < lo { min } else { max };
    }
}

/// A reason a code isn't rendered.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum RenderError {
    /// A painted token is too close to the background in lightness.
    ContrastTooLow {
        /// The token.
        token: Token,
        /// Its OKLab lightness difference from `bg`.
        delta_l: f32,
    },
    /// A painted token is lighter than the background and inverted codes
    /// weren't allowed (`Style::allow_inverted`).
    InvertedColors {
        /// The token.
        token: Token,
    },
    /// The quiet zone is narrower than 2 modules.
    QuietZoneTooSmall {
        /// The requested width.
        modules: u8,
    },
    /// A numeric option is outside its range (or NaN).
    OptionOutOfRange {
        /// Which option, e.g. `"Dots.size"`.
        option: &'static str,
        /// The value given.
        value: f32,
        /// Smallest allowed.
        min: f32,
        /// Largest allowed.
        max: f32,
    },
    /// Modules the style leaves out (alignment `Outline`) damage more of a
    /// Reed–Solomon block than [`DAMAGE_BUDGET`] allows. Raise the ECC
    /// level or the version.
    EccBudgetExceeded {
        /// The worst block's damaged codewords.
        damaged: u16,
        /// What that block may lose within the budget.
        allowed: u16,
    },
    /// The logo covers more codewords than [`DAMAGE_BUDGET`] allows, or
    /// reaches a finder, timing, format or version module or the
    /// bottom-right alignment pattern. Raise the ECC level or the version
    /// (`QrBuilder::encode_text_for` does both), or shrink the logo.
    LogoTooLarge {
        /// The largest scale, in hundredths, that fits this code.
        max_scale: f32,
    },
    /// On a picture, the logo covers an alignment pattern, which some
    /// decoders then misread the code around (SPEC §7.3). Outline the
    /// alignment patterns and dot the timing, as
    /// [`Style::art`](crate::Style::art) does, or use a version whose
    /// patterns clear the logo (`QrBuilder::encode_text_for` finds one
    /// when its range allows).
    LogoOverAlignment,
    /// [`Idle::LogoTurn`](crate::Idle::LogoTurn) turns the logo's frame,
    /// and only a round one stays inside the plate while it turns: the
    /// frame must be [`LogoFrame::Orbit`](crate::LogoFrame::Orbit) or sit
    /// on a [`LogoShape::Circle`] plate.
    TurnNeedsRoundFrame,
    /// An intro replayed with [`Trigger::Every`](crate::Trigger::Every)
    /// must end by half the period, so the finished code shows at least
    /// half the time. Lengthen the period or shorten the intro.
    ReplayTooSoon {
        /// The period.
        every_ms: u32,
        /// When the intro ends: its delay and duration.
        intro_ms: u32,
    },
    /// A shadow ([`Style::shadow`](crate::Style::shadow)) needs a frame
    /// and an opaque background: without them it would show through the
    /// quiet zone and between the modules.
    ShadowNeedsBackground,
    /// Custom shapes ([`Glyph`](crate::Glyph)) can't be drawn on a
    /// picture (SPEC §7.7), whose modules are sub-cells.
    GlyphOnPicture,
    /// Glass ink embeds the picture's file as the photo, and none was
    /// given ([`Halftone::source`](crate::Halftone::source)).
    GlassNeedsSource,
    /// The glass halo ([`Glass::halo`](crate::Glass::halo)) is narrower
    /// than a module: without it, the photo in the quiet zone hid most
    /// codes from ZXing (SPEC §7.9). [`Style::allow_unscannable`](crate::Style::allow_unscannable)
    /// renders it anyway, with the same-named warning.
    GlassHaloTooNarrow {
        /// The halo, in modules.
        halo: u8,
    },
    /// The finder glass is too close to its plate in lightness
    /// ([`Glass::finders`](crate::Glass::finders)), below
    /// [`GLASS_FINDER_FLOOR`], or the plate is darker than
    /// [`GLASS_PLATE_FLOOR`]. Downgraded by
    /// [`Style::allow_unscannable`](crate::Style::allow_unscannable).
    GlassFinderContrast {
        /// Plate minus finder glass, OKLab L.
        delta_l: f32,
        /// The plate's lightness.
        plate: f32,
    },
    /// The glass's lightness targets ([`Glass::targets`](crate::Glass::targets))
    /// sit further inside [`GLASS_SAFE_TARGETS`](crate::GLASS_SAFE_TARGETS)
    /// than [`GLASS_TARGET_MARGIN`](crate::GLASS_TARGET_MARGIN). Downgraded
    /// by [`Style::allow_unscannable`](crate::Style::allow_unscannable).
    GlassTargetsTooWeak {
        /// The dark target.
        dark: f32,
        /// The light target.
        light: f32,
    },
    /// The composited glass leaves too little of the code (SPEC §7.9,
    /// W20261124091813): the weakest tenth of the data modules land nearer
    /// the dark/light midpoint than [`GLASS_READABILITY`], or the finders'
    /// composited contrast is under [`GLASS_FINDER_FLOOR`]. Opacity below
    /// about 0.95 and weak targets cause it. A readability error: rendered
    /// with a warning unless [`Checks::Strict`].
    GlassLowReadability {
        /// The share of data modules clearly readable (at least 0.10 from
        /// the midpoint), 0–1.
        share: f32,
        /// The weakest tenth's margin from the midpoint, OKLab L.
        margin: f32,
        /// The finders' composited contrast, OKLab L.
        finder: f32,
    },
    /// An option glass ink can't be combined with (SPEC §7.9 lists them),
    /// e.g. `"Frame::Circle"` or `"Style.intro"`.
    GlassUnsupported {
        /// The option.
        option: &'static str,
    },
    /// The output sink returned an error.
    Write,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::ContrastTooLow { token, delta_l } => {
                let floor = if *token == Token::LogoFrame {
                    FRAME_FLOOR
                } else {
                    CONTRAST_FLOOR
                };
                write!(
                    f,
                    "{token} has contrast {delta_l:.2} against bg; at least {floor} is needed"
                )
            }
            RenderError::InvertedColors { token } => write!(
                f,
                "{token} is lighter than bg; allow inverted colors to render it"
            ),
            RenderError::QuietZoneTooSmall { modules } => {
                write!(f, "quiet zone of {modules} modules; at least 2 are needed")
            }
            RenderError::OptionOutOfRange {
                option,
                value,
                min,
                max,
            } => write!(f, "{option} is {value}; it must be in {min}–{max}"),
            RenderError::EccBudgetExceeded { damaged, allowed } => write!(
                f,
                "cleared modules damage {damaged} codewords of one block; the budget allows {allowed}"
            ),
            RenderError::LogoOverAlignment => f.write_str(
                "the logo covers an alignment pattern of the picture; outline them and dot the timing, or use a version that clears it",
            ),
            RenderError::TurnNeedsRoundFrame => {
                f.write_str("the logo-turn idle needs an orbit frame or a frame on a circle plate")
            }
            RenderError::ReplayTooSoon { every_ms, intro_ms } => write!(
                f,
                "the intro ends at {intro_ms} ms; replaying it every {every_ms} ms needs a period of at least {} ms",
                2 * intro_ms
            ),
            RenderError::LogoTooLarge { max_scale } => write!(
                f,
                "the logo is too large for this code; at most scale {max_scale:.2} fits"
            ),
            RenderError::ShadowNeedsBackground => {
                f.write_str("a shadow needs a frame and an opaque background")
            }
            RenderError::GlyphOnPicture => {
                f.write_str("custom shapes can't be drawn on a picture code")
            }
            RenderError::GlassNeedsSource => f.write_str(
                "glass ink embeds the picture's file as the photo; give it with Halftone::source",
            ),
            RenderError::GlassHaloTooNarrow { halo } => write!(
                f,
                "a glass halo of {halo} modules; at least 1 is needed for scanners to find the code"
            ),
            RenderError::GlassFinderContrast { delta_l, plate } => write!(
                f,
                "the glass finders have contrast {delta_l:.2} on a plate of lightness {plate:.2}; at least {GLASS_FINDER_FLOOR} on {GLASS_PLATE_FLOOR} is needed"
            ),
            RenderError::GlassTargetsTooWeak { dark, light } => write!(
                f,
                "glass targets of {dark}/{light} are too weak; dark at most {:.2} and light at least {:.2} are needed",
                GLASS_SAFE_TARGETS.0 + GLASS_TARGET_MARGIN,
                GLASS_SAFE_TARGETS.1 - GLASS_TARGET_MARGIN
            ),
            RenderError::GlassUnsupported { option } => {
                write!(f, "{option} can't be combined with glass ink")
            }
            RenderError::GlassLowReadability {
                share,
                margin,
                finder,
            } => write!(
                f,
                "the glass leaves {:.0}% of the modules clearly readable (the weakest tenth {margin:.2} from the midpoint; {GLASS_READABILITY} measured to scan) and finder contrast {finder:.2} ({GLASS_FINDER_FLOOR})",
                share * 100.0
            ),
            RenderError::Write => f.write_str("the output sink failed"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RenderError {}

/// A risk that doesn't stop rendering.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Warning {
    /// Contrast is above the floor but below [`CONTRAST_WARNING`].
    LowContrast {
        /// The token.
        token: Token,
        /// Its OKLab lightness difference from `bg`.
        delta_l: f32,
    },
    /// Light-on-dark colors (allowed); some older scanners fail on them.
    Inverted {
        /// The token.
        token: Token,
    },
    /// A quiet zone of 2 or 3 modules; most modern scanners cope.
    NarrowQuietZone {
        /// The width.
        modules: u8,
    },
    /// Dots smaller than 0.75 of a module are harder to read.
    SmallDots {
        /// The dot diameter.
        size: f32,
    },
    /// The background isn't opaque, so contrast depends on what's behind
    /// the code; it was judged over white.
    TranslucentBackground,
    /// At the target size (`Style::target_size`), modules are smaller than
    /// 3 device pixels.
    SmallOnScreen {
        /// Device pixels per module.
        px_per_module: f32,
    },
    /// At the target size, modules are smaller than 0.4 mm in print.
    SmallInPrint {
        /// Millimeters per module.
        mm_per_module: f32,
    },
    /// A finder or alignment ring isn't square. ZXing-family scanners
    /// (most phones) read it, but decoders that fit the ring's square
    /// corners to find perspective, like quirc and rqrr, lose it: in the
    /// verify suite even a 0.25-module radius made rqrr miss renders.
    RoundedFinder,
    /// The intro runs longer than [`LONG_INTRO_MS`]; people may try to
    /// scan before it ends.
    LongIntro {
        /// Its length.
        ms: u32,
    },
    /// During a palette shift or an idle shimmer, a token comes closer to
    /// the background than [`CONTRAST_FLOOR`] for a moment (a shift between
    /// palettes of opposite polarity always does). The finished code is
    /// unaffected.
    MotionContrast {
        /// The token.
        token: Token,
        /// The lowest OKLab lightness difference on the way.
        delta_l: f32,
    },
    /// More elements animate than [`ANIMATED_ELEMENTS`], which slows
    /// browsers. Use fewer stagger groups.
    ManyAnimatedElements {
        /// How many.
        count: usize,
    },
    /// The data modules are a custom shape ([`Style::module_glyph`](crate::Style::module_glyph)).
    /// Scanners read each module's center, and the library can't see how
    /// much of it the picture fills: verify the code
    /// (`freeqrent_verify::verify_style`, or the CLI's `--verify`).
    CustomModules,
    /// A finder's ring or eye is a custom shape. Scanners find a code by
    /// the finders' 1:1:3:1:1 profile, which is now up to the picture:
    /// verify the code. Like [`Warning::RoundedFinder`], quirc-style
    /// decoders may lose it.
    CustomFinder,
    /// A custom shape keeps its own colors, whose contrast isn't checked.
    GlyphColors,
    /// [`RenderError::GlassHaloTooNarrow`], rendered anyway
    /// ([`Style::allow_unscannable`](crate::Style::allow_unscannable)).
    GlassHaloTooNarrow {
        /// The halo, in modules.
        halo: u8,
    },
    /// [`RenderError::GlassFinderContrast`], rendered anyway.
    GlassFinderContrast {
        /// Plate minus finder glass, OKLab L.
        delta_l: f32,
        /// The plate's lightness.
        plate: f32,
    },
    /// [`RenderError::GlassTargetsTooWeak`], rendered anyway.
    GlassTargetsTooWeak {
        /// The dark target.
        dark: f32,
        /// The light target.
        light: f32,
    },
    /// A glass code above [`GLASS_FULL_BAR_VERSION`](crate::GLASS_FULL_BAR_VERSION)
    /// (or baked below the default resolution). ZXing-family scanners (most
    /// phones) read it; quirc-style decoders such as rqrr lost some in the
    /// verify suite (SPEC §7.9).
    GlassNeedsZxing {
        /// The code's version.
        version: u8,
    },
    /// The baked glass layer is estimated past [`LARGE_OUTPUT_BYTES`](crate::LARGE_OUTPUT_BYTES).
    /// A lower [`Glass::resolution`](crate::Glass::resolution) shrinks it.
    LargeOutput {
        /// The estimated size of the layer's PNG.
        bytes: usize,
    },
    /// An error rendered anyway, under [`Checks::Lenient`] (readability
    /// errors) or [`Checks::Off`] (every error), with the code drawn as
    /// SPEC §9 says for that error. [`ScanReport::scannable`] is false.
    /// The three glass limits keep their own warnings
    /// ([`Warning::GlassHaloTooNarrow`] and the two after it).
    Unchecked(RenderError),
}

/// Intros longer than this raise [`Warning::LongIntro`] (SPEC §8.1).
pub const LONG_INTRO_MS: u32 = 2000;
/// The animated element budget (SPEC §8.4).
pub const ANIMATED_ELEMENTS: usize = 64;

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Warning::LowContrast { token, delta_l } => write!(
                f,
                "{token} has contrast {delta_l:.2} against bg; {CONTRAST_WARNING} or more is safer"
            ),
            Warning::Inverted { token } => {
                write!(
                    f,
                    "{token} is lighter than bg; some scanners can't read inverted codes"
                )
            }
            Warning::NarrowQuietZone { modules } => {
                write!(
                    f,
                    "quiet zone of {modules} modules; the standard asks for 4"
                )
            }
            Warning::SmallDots { size } => {
                write!(
                    f,
                    "dots of {size} modules; 0.75 or more scans more reliably"
                )
            }
            Warning::SmallOnScreen { px_per_module } => write!(
                f,
                "{px_per_module:.1} px per module at the target size; 3 or more scans reliably"
            ),
            Warning::SmallInPrint { mm_per_module } => write!(
                f,
                "{mm_per_module:.2} mm per module at the target size; 0.4 or more scans reliably"
            ),
            Warning::RoundedFinder => f.write_str(
                "rounded scan-box rings need a ZXing-family scanner; quirc-style decoders can't locate them",
            ),
            Warning::LongIntro { ms } => write!(
                f,
                "the intro runs {ms} ms; {LONG_INTRO_MS} ms or less lets people scan sooner"
            ),
            Warning::MotionContrast { token, delta_l } => write!(
                f,
                "{token} dips to contrast {delta_l:.2} while it animates; the code may not scan for that moment"
            ),
            Warning::ManyAnimatedElements { count } => write!(
                f,
                "{count} elements animate; {ANIMATED_ELEMENTS} or fewer keeps browsers smooth"
            ),
            Warning::TranslucentBackground => f.write_str(
                "bg is translucent; contrast was judged over white and depends on the page",
            ),
            Warning::CustomModules => f.write_str(
                "the modules are a custom shape, which can't be checked; verify that the code scans",
            ),
            Warning::CustomFinder => f.write_str(
                "a finder is a custom shape, which can't be checked; verify that the code scans",
            ),
            Warning::GlyphColors => f.write_str(
                "a custom shape keeps its own colors, whose contrast isn't checked",
            ),
            Warning::GlassHaloTooNarrow { halo } => write!(
                f,
                "a glass halo of {halo} modules, rendered anyway; scanners may not find the code"
            ),
            Warning::GlassFinderContrast { delta_l, plate } => write!(
                f,
                "the glass finders have contrast {delta_l:.2} on a plate of lightness {plate:.2}, rendered anyway; scanners may not find the code"
            ),
            Warning::GlassTargetsTooWeak { dark, light } => write!(
                f,
                "glass targets of {dark}/{light}, rendered anyway; modules may read wrong"
            ),
            Warning::GlassNeedsZxing { version } => write!(
                f,
                "glass at version {version} needs a ZXing-family scanner; quirc-style decoders may not read it"
            ),
            Warning::LargeOutput { bytes } => write!(
                f,
                "the glass layer is about {} KB; a lower glass resolution (baked) or a smaller code makes it smaller",
                bytes.div_ceil(1000)
            ),
            Warning::Unchecked(e) => write!(f, "{e}; rendered anyway, so it may not scan"),
        }
    }
}

/// The result of [`Style::check`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ScanReport {
    /// Problems that stop rendering.
    pub errors: Vec<RenderError>,
    /// Risks worth knowing about.
    pub warnings: Vec<Warning>,
    /// The lowest contrast of any painted token against `bg` (not the
    /// logo's frame, which has [`FRAME_FLOOR`]).
    pub min_contrast: f32,
    /// How much of the error-correction budget cleared modules use, in the
    /// worst block: damaged codewords over correctable ones (0 when nothing
    /// is cleared). Above [`DAMAGE_BUDGET`] the code isn't rendered.
    pub ecc_budget_used: f32,
    /// The scale the logo is drawn at, when there is one: its own, or what
    /// [`LogoSize::Largest`](crate::LogoSize::Largest) grew it to.
    pub logo_scale: Option<f32>,
    /// How well the modules follow the style's picture, when it has one
    /// (SPEC §7.8).
    #[cfg(feature = "halftone")]
    pub steering: Option<Steering>,
    /// False when an error was rendered anyway, as a warning
    /// ([`Checks::Lenient`] or [`Checks::Off`]): the code renders, outside
    /// the guarantee that it scans (SPEC §9).
    pub scannable: bool,
}

impl ScanReport {
    /// True when the code would render.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// The strongest glow and the deepest emboss (SPEC §7.5).
pub const GLOW_MAX: f32 = 0.25;
/// The deepest emboss (SPEC §7.5).
pub const EMBOSS_MAX: f32 = 0.12;
/// The roughest sketch (SPEC §7.5).
pub const SKETCH_MAX: f32 = 0.15;

/// Checks `style` against `qr` at its [`Checks`] level.
pub(crate) fn check(style: &Style, qr: &QrMatrix) -> ScanReport {
    resolve(style, qr).0
}

/// Every error `style` has on `qr`, whatever its [`Checks`] level: what
/// fitting a code to a style (`QrBuilder::encode_text_for`) steers clear of.
pub(crate) fn raw_errors(style: &Style, qr: &QrMatrix) -> Vec<RenderError> {
    let mut s = style.clone();
    let mut found = Found::default();
    let mut warnings = Vec::new();
    examine(&mut s, qr, &mut found, &mut warnings);
    found.list.into_iter().map(|(e, _)| e).collect()
}

/// The report at `style`'s [`Checks`] level, and the style to draw: the
/// same style, except under [`Checks::Off`] with hard errors, where options
/// outside their range are clamped and options that can't be combined are
/// left out (SPEC §9).
pub(crate) fn resolve(style: &Style, qr: &QrMatrix) -> (ScanReport, Style) {
    let mut s = style.clone();
    let mut found = Found::default();
    let mut warnings = Vec::new();
    let mut report = examine(&mut s, qr, &mut found, &mut warnings);
    let mut errors = Vec::new();
    for (e, hard) in found.list {
        let refuse = match style.checks {
            Checks::Strict => true,
            Checks::Lenient => hard,
            Checks::Off => false,
        };
        if refuse {
            errors.push(e);
        } else {
            report.scannable = false;
            warnings.push(match e {
                RenderError::GlassHaloTooNarrow { halo } => Warning::GlassHaloTooNarrow { halo },
                RenderError::GlassFinderContrast { delta_l, plate } => {
                    Warning::GlassFinderContrast { delta_l, plate }
                }
                RenderError::GlassTargetsTooWeak { dark, light } => {
                    Warning::GlassTargetsTooWeak { dark, light }
                }
                e => Warning::Unchecked(e),
            });
        }
    }
    report.errors = errors;
    report.warnings = warnings;
    (report, s)
}

/// Every check, on a copy of the style that range checks clamp and
/// conflicts trim (used only under [`Checks::Off`]). Errors go to `found`;
/// the report's `errors` and `warnings` are filled in by [`resolve`].
fn examine(
    s: &mut Style,
    qr: &QrMatrix,
    found: &mut Found,
    warnings: &mut Vec<Warning>,
) -> ScanReport {
    match &mut s.modules {
        ModuleShape::Square { gap } => found.range("Square.gap", gap, 0.0, 0.25),
        ModuleShape::Rounded { radius } => found.range("Rounded.radius", radius, 0.0, 0.5),
        ModuleShape::Dots { size } => {
            found.range("Dots.size", size, 0.6, 1.0);
            if (0.6..0.75).contains(size) {
                warnings.push(Warning::SmallDots { size: *size });
            }
        }
        ModuleShape::Liquid { smoothness } => {
            found.range("Liquid.smoothness", smoothness, 0.0, 1.0)
        }
        ModuleShape::Bars { radius, .. } => found.range("Bars.radius", radius, 0.0, 0.5),
    }

    match s.quiet_zone {
        0 | 1 => found.soft(RenderError::QuietZoneTooSmall {
            modules: s.quiet_zone,
        }),
        2 | 3 => warnings.push(Warning::NarrowQuietZone {
            modules: s.quiet_zone,
        }),
        _ => {}
    }

    let quiet = s.quiet_zone as f32;
    match &mut s.frame {
        Frame::RoundedSquare(r) => found.range("Frame::RoundedSquare radius", r, 0.0, quiet),
        Frame::Circle { filler_density, .. } => {
            found.range("Frame::Circle filler_density", filler_density, 0.0, 1.0)
        }
        Frame::Square | Frame::None => {}
    }
    match &mut s.border {
        Some(Border::Solid { width }) => found.range("Border::Solid width", width, 0.1, 2.0),
        Some(Border::Double { width }) => found.range("Border::Double width", width, 0.3, 3.0),
        _ => {}
    }
    match &mut s.effect {
        Some(Effect::Glow { radius, strength }) => {
            found.range("Effect::Glow radius", radius, 0.05, 0.5);
            found.range("Effect::Glow strength", strength, 0.0, GLOW_MAX);
        }
        Some(Effect::Emboss { depth }) => {
            found.range("Effect::Emboss depth", depth, 0.05, EMBOSS_MAX)
        }
        Some(Effect::Sketch { roughness }) => {
            found.range("Effect::Sketch roughness", roughness, 0.02, SKETCH_MAX)
        }
        _ => {}
    }
    if let Some(c) = &mut s.caption {
        found.range("Caption.size", &mut c.size, 1.0, 4.0);
        found.range_int("Caption.tracking", &mut c.tracking, 0, 50);
    }
    for texture in &mut s.textures {
        match texture {
            Texture::Grain { amount } => found.range("Texture::Grain amount", amount, 0.02, 0.1),
            Texture::Scanlines { opacity } => {
                found.range("Texture::Scanlines opacity", opacity, 0.05, 0.3)
            }
            Texture::Ink { fade } => found.range("Texture::Ink fade", fade, 0.05, 0.3),
        }
    }
    // Measured: lines over faded modules lose codes when both are strong.
    if let (Some(lines), Some(fade)) = (s.scanlines(), s.ink_fade())
        && lines >= 0.05
        && fade >= 0.05
    {
        let mut both = lines + fade;
        found.range(
            "Texture::Scanlines opacity + Texture::Ink fade",
            &mut both,
            0.0,
            0.4,
        );
    }
    if let Some(offset) = &mut s.shadow {
        found.range("Style.shadow offset", offset, 0.25, 3.0);
        if s.frame == Frame::None || s.palette.background().alpha() < 1.0 {
            found.hard(RenderError::ShadowNeedsBackground);
            s.shadow = None;
        }
    }
    if let Some(logo) = &mut s.logo {
        found.range("Logo.scale", &mut logo.scale, 0.0, 0.5);
        found.range("Logo.padding", &mut logo.padding, 0.0, 4.0);
        if let LogoShape::Rounded(r) = &mut logo.shape {
            found.range("LogoShape::Rounded radius", r, 0.0, f32::MAX);
        }
        if let Some(width) = &mut logo.frame_width {
            found.range("Logo.frame_width", width, 0.1, 2.5);
        }
        if let Some(moon) = &mut logo.moon {
            found.range("Logo.moon_size", moon, 0.4, 4.0);
        }
    }

    #[cfg(feature = "halftone")]
    check_halftone(s, found, warnings);
    #[cfg(feature = "halftone")]
    let glass_contrast = check_glass(s, qr, found, warnings);
    #[cfg(not(feature = "halftone"))]
    let glass_contrast: Option<f32> = None;
    #[cfg(feature = "animate")]
    check_intro(s, qr, found, warnings);
    check_scan_boxes(s, found, warnings);
    check_target(s, qr, found, warnings);
    let ecc_budget_used = budget(s, qr, found);

    let style = &*s;
    let palette = &style.palette;
    if palette.background().is_translucent() {
        warnings.push(Warning::TranslucentBackground);
    }
    let mut min_contrast = glass_contrast.unwrap_or(f32::INFINITY);
    // Filler and the frame ring are decoration outside the quiet zone, so
    // they may be as faint as the design wants.
    let read = scene::layers(style, qr)
        .into_iter()
        .filter(|l| !matches!(l.token, Token::Filler | Token::Frame));
    for layer in read {
        let token = layer.token;
        let contrast = if layer.fade < 1.0 {
            palette.contrast_faded(token, layer.fade)
        } else {
            palette.contrast(token)
        };
        min_contrast = min_contrast.min(contrast.delta_l);
        if contrast.delta_l < CONTRAST_FLOOR {
            found.soft(RenderError::ContrastTooLow {
                token,
                delta_l: contrast.delta_l,
            });
        } else if contrast.delta_l < CONTRAST_WARNING {
            warnings.push(Warning::LowContrast {
                token,
                delta_l: contrast.delta_l,
            });
        }
        if contrast.inverted {
            if style.allow_inverted {
                warnings.push(Warning::Inverted { token });
            } else {
                found.soft(RenderError::InvertedColors { token });
            }
        }
    }

    // The logo's frame sits on its plate and isn't read: a floor of its own
    // and no warning band, in either polarity.
    if style.logo.as_ref().is_some_and(|l| l.frame.is_some()) {
        let contrast = palette.contrast(Token::LogoFrame);
        if contrast.delta_l < FRAME_FLOOR {
            found.soft(RenderError::ContrastTooLow {
                token: Token::LogoFrame,
                delta_l: contrast.delta_l,
            });
        }
    }

    ScanReport {
        errors: Vec::new(),
        warnings: Vec::new(),
        min_contrast,
        ecc_budget_used,
        logo_scale: style.logo.as_ref().map(|_| logo_scale(style, qr)),
        #[cfg(feature = "halftone")]
        steering: steering(style, qr),
        scannable: true,
    }
}

/// Steering numbers for a style with a picture: the free bits, and how
/// many data and ECC modules match the picture's dithered light and dark.
#[cfg(feature = "halftone")]
fn steering(style: &Style, qr: &QrMatrix) -> Option<Steering> {
    let halftone = style.halftone.as_ref()?;
    let n = qr.size() as usize;
    let targets = halftone::module_targets(
        halftone,
        n,
        crate::glass::span(style, n),
        style.palette.background(),
        style.inverted_ink(),
    );
    let (mut code, mut matched) = (0, 0);
    for (x, y, dark, role) in qr.modules() {
        if !role.is_function() && role != crate::ModuleRole::Remainder {
            code += 1;
            matched += usize::from(targets[y as usize * n + x as usize] == dark);
        }
    }
    let report = qr.capacity_report();
    let free = report
        .data_bits_available
        .saturating_sub(report.data_bits_used + 4);
    Some(Steering {
        steerable_modules: if halftone.steer { free } else { 0 },
        code_modules: code,
        matched_modules: matched,
    })
}

/// A picture's options. In color, the gap between the dark and light
/// bands is the code's contrast, judged like a token's (reported as `fg`).
#[cfg(feature = "halftone")]
fn check_halftone(s: &mut Style, found: &mut Found, warnings: &mut Vec<Warning>) {
    use crate::halftone::{Clarity, Ink};
    let Some(h) = &mut s.halftone else {
        return;
    };
    if let Clarity::Uniform(c) = &mut h.clarity {
        found.range("Clarity::Uniform", c, 0.0, 1.0);
    }
    if let Ink::Color { .. } = h.ink {
        found.range("Halftone.bands dark", &mut h.bands.0, 0.0, 1.0);
        found.range("Halftone.bands light", &mut h.bands.1, 0.0, 1.0);
        let gap = h.bands.1 - h.bands.0;
        if gap < CONTRAST_FLOOR {
            found.soft(RenderError::ContrastTooLow {
                token: Token::Fg,
                delta_l: gap,
            });
        } else if gap < CONTRAST_WARNING {
            warnings.push(Warning::LowContrast {
                token: Token::Fg,
                delta_l: gap,
            });
        }
    }
}

/// Glass ink (SPEC §7.9): its options' ranges, what it can't be combined
/// with (hard errors, left out under [`Checks::Off`]), and the measured
/// limits (readability errors). Returns the finder contrast, the code's
/// own.
#[cfg(feature = "halftone")]
fn check_glass(
    s: &mut Style,
    qr: &QrMatrix,
    found: &mut Found,
    warnings: &mut Vec<Warning>,
) -> Option<f32> {
    let quiet = s.quiet_zone;
    let h = s.halftone.as_mut()?;
    let has_source = h.source.is_some();
    let clarity = h.clarity != crate::halftone::Clarity::default();
    let g = h.glass_mut()?;
    if !has_source {
        found.hard(RenderError::GlassNeedsSource);
    }
    found.range("Glass.border", &mut g.border, 0.0, 0.25);
    found.range_int("Glass.halo", &mut g.halo, 0, quiet);
    let min_lens = if g.is_vector() {
        GLASS_VECTOR_MIN_LENS
    } else {
        0.5
    };
    found.range("Glass.lens", &mut g.lens, min_lens, 1.0);
    let lens = g.lens.min(1.0);
    found.range("Glass.core", &mut g.core, 0.3_f32.min(lens), lens);
    found.range("Glass.targets dark", &mut g.targets.0, 0.0, 1.0);
    found.range("Glass.targets light", &mut g.targets.1, 0.0, 1.0);
    found.range("Glass.union", &mut g.union, 0.2, 0.8);
    found.range("Glass.frost", &mut g.frost, 0.0, 0.3);
    found.range("Glass.finders dark", &mut g.finders.0, 0.0, 1.0);
    found.range("Glass.finders plate", &mut g.finders.1, 0.0, 1.0);
    found.range_int(
        "Glass.resolution",
        &mut g.resolution,
        8,
        GLASS_RESOLUTION_MAX,
    );
    found.range("Glass.texture", &mut g.texture, 0.0, 1.0);
    found.range("Glass.bevel", &mut g.bevel, 0.05, 0.45);
    found.range("Glass.ior", &mut g.ior, 1.0, 2.0);
    found.range("Glass.dome", &mut g.dome, 0.0, 1.0);
    found.range("Glass.opacity", &mut g.opacity, 0.1, 1.0);
    let g = *g;

    // What glass doesn't draw: hard errors, left out under `Checks::Off`.
    let mut unsupported = |yes: bool, option: &'static str| {
        if yes {
            found.hard(RenderError::GlassUnsupported { option });
        }
        yes
    };
    if unsupported(matches!(s.frame, Frame::Circle { .. }), "Frame::Circle") {
        s.frame = Frame::Square;
    }
    // Module, finder, alignment and timing shapes are cut from the code's
    // own geometry (W20261124091813).
    // Data colors tint the glass with `GlassColors::Palette` (W20261124091813).
    if unsupported(!s.textures.is_empty(), "Style.textures") {
        s.textures.clear();
    }
    // Effects filter the glass, not the photo (W20261123231733). Sketch
    // holds on both renders and every effect on vector glass; over the
    // baked layer a glow lost the bar (dusk v11, from strength 0.05) and
    // an emboss lost 11–14 of 40 noise seeds on the rings at v7 at every
    // depth, so those two stay refused there (SPEC §7.9).
    if !g.is_vector()
        && (unsupported(
            matches!(s.effect, Some(Effect::Glow { .. })),
            "Effect::Glow on baked glass",
        ) || unsupported(
            matches!(s.effect, Some(Effect::Emboss { .. })),
            "Effect::Emboss on baked glass",
        ))
    {
        s.effect = None;
    }
    if unsupported(clarity, "Halftone.clarity")
        && let Some(h) = &mut s.halftone
    {
        h.clarity = crate::halftone::Clarity::default();
    }
    // Intros, palette shifts and the other idles move the photo and glass
    // whole (SPEC §8.3); these two would need the modules: the eyes to
    // pulse, and the read colors to check the band against.
    #[cfg(feature = "animate")]
    for idle in [crate::Idle::FinderPulse, crate::Idle::Shimmer] {
        let option = if idle == crate::Idle::Shimmer {
            "Idle::Shimmer"
        } else {
            "Idle::FinderPulse"
        };
        if unsupported(s.idle.contains(&idle), option) {
            s.idle.retain(|&i| i != idle);
        }
    }

    // Dots on baked glass held the bar at 0.9 and lost the degraded disc
    // at 0.85 and below (SPEC §7.9).
    if let ModuleShape::Dots { size } = s.modules
        && !g.is_vector()
    {
        let mut size = size;
        found.range("Dots.size on baked glass", &mut size, GLASS_DOTS_MIN, 1.0);
    }

    // Measured limits: readability errors.
    if g.halo < 1 {
        found.soft(RenderError::GlassHaloTooNarrow { halo: g.halo });
    }
    let (plate, contrast) = (g.finders.1, g.finders.1 - g.finders.0);
    // A hair of slack, so the measured pair (0.92 − 0.30) counts as 0.62.
    let below = |v: f32, floor: f32| v < floor - 1e-4;
    // Whether an option's own limit already explains a weak code.
    let mut explained = false;
    if below(contrast, GLASS_FINDER_FLOOR) || below(plate, GLASS_PLATE_FLOOR) {
        explained = true;
        found.soft(RenderError::GlassFinderContrast {
            delta_l: contrast,
            plate,
        });
    } else if below(contrast, GLASS_FINDER_WARNING) {
        warnings.push(Warning::LowContrast {
            token: Token::FinderOuter,
            delta_l: contrast,
        });
    }
    let (dark, light) = g.targets;
    let weak = (dark - GLASS_SAFE_TARGETS.0).max(GLASS_SAFE_TARGETS.1 - light);
    if weak > GLASS_TARGET_MARGIN + 1e-4 {
        explained = true;
        found.soft(RenderError::GlassTargetsTooWeak { dark, light });
    }
    // What the composited glass leaves: opacity, mostly.
    if !explained
        && let Some(read) = crate::glass::readability(s, qr)
        && (read.margin < GLASS_READABILITY - 1e-3 || read.finder < GLASS_FINDER_FLOOR - 1e-3)
    {
        found.soft(RenderError::GlassLowReadability {
            share: read.share,
            margin: read.margin,
            finder: read.finder,
        });
    }
    let version = qr.version().number();
    let zxing = if g.is_vector() {
        let narrow = g.lens < GLASS_VECTOR_FULL_BAR_LENS - 1e-4;
        version
            > if narrow {
                GLASS_VECTOR_NARROW_FULL_BAR_VERSION
            } else {
                GLASS_VECTOR_FULL_BAR_VERSION
            }
    } else {
        version > GLASS_FULL_BAR_VERSION || g.resolution < GLASS_RESOLUTION
    } || g.texture > GLASS_TEXTURE_FULL_BAR + 1e-4;
    if zxing {
        warnings.push(Warning::GlassNeedsZxing { version });
    }
    let bytes = crate::glass::estimated_bytes(s, &g, qr);
    if bytes > LARGE_OUTPUT_BYTES {
        warnings.push(Warning::LargeOutput { bytes });
    }
    Some(contrast)
}

#[cfg(feature = "animate")]
fn check_intro(s: &mut Style, qr: &QrMatrix, found: &mut Found, warnings: &mut Vec<Warning>) {
    if let Some(ms) = &mut s.theme_transition {
        found.range_int("Style.theme_transition", ms, 0, 10_000);
    }
    for (token, delta_l) in crate::animate::motion_contrast(s, qr) {
        if delta_l < CONTRAST_FLOOR {
            warnings.push(Warning::MotionContrast { token, delta_l });
        }
    }
    if s.idle.contains(&crate::Idle::LogoTurn) {
        let round = s.logo.as_ref().is_some_and(|l| match l.frame {
            Some(crate::LogoFrame::Orbit) => true,
            Some(_) => l.shape == LogoShape::Circle,
            None => false,
        });
        if !round {
            found.hard(RenderError::TurnNeedsRoundFrame);
            s.idle.retain(|&i| i != crate::Idle::LogoTurn);
        }
    }
    let Some(intro) = &mut s.intro else {
        return;
    };
    found.range_int("Intro.duration_ms", &mut intro.duration_ms, 100, 10_000);
    found.range_int("Intro.delay_ms", &mut intro.delay_ms, 0, 10_000);
    found.range_int("Intro.groups", &mut intro.groups, 2, 16);
    if let Some(crate::Ease::Custom(c)) = &mut intro.ease {
        found.range("CubicBezier.x1", &mut c.x1, 0.0, 1.0);
        found.range("CubicBezier.x2", &mut c.x2, 0.0, 1.0);
        found.range("CubicBezier.y1", &mut c.y1, -10.0, 10.0);
        found.range("CubicBezier.y2", &mut c.y2, -10.0, 10.0);
    }
    if let crate::Trigger::Every(every_ms) = &mut intro.trigger {
        found.range_int("Trigger::Every", every_ms, 1000, 60_000);
        let every_ms = *every_ms;
        let intro_ms = intro.delay_ms.saturating_add(intro.duration_ms);
        if intro.entrance().is_some() && every_ms < intro_ms.saturating_mul(2) {
            // Replays as asked under `Checks::Off`, overlapping.
            found.hard(RenderError::ReplayTooSoon { every_ms, intro_ms });
        }
    }
    if intro.duration_ms > LONG_INTRO_MS {
        warnings.push(Warning::LongIntro {
            ms: intro.duration_ms,
        });
    }
    let count = crate::animate::element_count(s, qr);
    if count > ANIMATED_ELEMENTS {
        warnings.push(Warning::ManyAnimatedElements { count });
    }
}

/// Screen and print minimums from SPEC §7.8.
const MIN_PX_PER_MODULE: f32 = 3.0;
const MIN_MM_PER_MODULE: f32 = 0.4;

fn check_target(s: &mut Style, qr: &QrMatrix, found: &mut Found, warnings: &mut Vec<Warning>) {
    let modules = scene::canvas(s, qr).side as f32;
    match &mut s.target {
        None => {}
        Some(TargetSize::Pixels(px)) => {
            let px_per_module = *px as f32 / modules;
            if px_per_module < MIN_PX_PER_MODULE {
                warnings.push(Warning::SmallOnScreen { px_per_module });
            }
        }
        Some(TargetSize::Millimeters(mm)) => {
            found.range("TargetSize::Millimeters", mm, f32::MIN_POSITIVE, f32::MAX);
            let mm_per_module = *mm / modules;
            if mm_per_module < MIN_MM_PER_MODULE {
                warnings.push(Warning::SmallInPrint { mm_per_module });
            }
        }
    }
}

fn check_scan_boxes(s: &mut Style, found: &mut Found, warnings: &mut Vec<Warning>) {
    let mut rings = Vec::new();
    for corner in scene::CORNERS {
        let (mut ring, mut eye) = s.finders.at(corner);
        // A custom ring replaces the shape; its own warning says so.
        if s.glyphs.get(GlyphPart::Ring, Some(corner)).is_none() {
            rings.push(ring);
        }
        if let RingShape::Rounded(r) = &mut ring {
            found.range("FinderStyle.outer radius", r, 0.0, 2.5);
        }
        if let EyeShape::Rounded(r) = &mut eye {
            found.range("FinderStyle.eye radius", r, 0.0, 1.5);
        }
        if (ring, eye) != s.finders.at(corner) {
            s.finders = s.finders.corner(corner, ring, eye);
        }
    }
    if let AlignmentShape::Own { outer, eye } = &mut s.alignment.shape {
        rings.push(*outer);
        if let RingShape::Rounded(r) = outer {
            found.range("AlignmentShape.outer radius", r, 0.0, 2.5);
        }
        if let EyeShape::Rounded(r) = eye {
            found.range("AlignmentShape.eye radius", r, 0.0, 0.5);
        }
    }
    if rings.iter().any(|&r| r != RingShape::Square) {
        warnings.push(Warning::RoundedFinder);
    }
    check_glyphs(s, found, warnings);
}

/// Custom shapes (SPEC §7.1, §7.2): the library can't see inside them, so
/// they always warn.
fn check_glyphs(s: &mut Style, found: &mut Found, warnings: &mut Vec<Warning>) {
    if !s.glyphs.any() {
        return;
    }
    #[cfg(feature = "halftone")]
    if s.halftone.is_some() {
        found.hard(RenderError::GlyphOnPicture);
        s.glyphs = crate::glyph::Glyphs::default();
        return;
    }
    let _ = &found;
    let g = &s.glyphs;
    if g.modules.is_some() {
        warnings.push(Warning::CustomModules);
    }
    if g.any_finder() {
        warnings.push(Warning::CustomFinder);
    }
    let all = core::iter::once(&g.modules).chain(&g.ring).chain(&g.eye);
    if all.flatten().any(|glyph| glyph.keep_colors) {
        warnings.push(Warning::GlyphColors);
    }
}

/// What each Reed–Solomon block of a code may lose.
struct Budget {
    /// Correctable codewords per block (after misdecode protection).
    correctable: u16,
    /// Of those, what the damage budget lets cleared modules take.
    allowed: u16,
    /// For each module, its codeword and block.
    map: Vec<Option<(u16, u16)>>,
    blocks: usize,
    codewords: usize,
}

impl Budget {
    fn new(qr: &QrMatrix, share: f32) -> Budget {
        let layout = BlockLayout::new(qr.version(), qr.ecc());
        let correctable =
            ((layout.ecc_per_block - misdecode_protection(qr.version(), qr.ecc())) / 2) as u16;
        Budget {
            correctable,
            allowed: (correctable as f32 * share) as u16,
            map: qr.codeword_map(),
            blocks: layout.num_blocks,
            codewords: layout.total_codewords,
        }
    }

    /// The worst block's damaged codewords. A codeword is damaged when one
    /// of its `cleared` modules was dark (a cleared module reads light),
    /// or when any of its modules is `covered` (a logo may read either way).
    fn worst(&self, qr: &QrMatrix, cleared: &Grid, covered: Option<&Grid>) -> u16 {
        let mut hit = alloc::vec![false; self.codewords];
        let mut per_block = alloc::vec![0u16; self.blocks];
        for (x, y, dark, _) in qr.modules() {
            let (xi, yi) = (x as i64, y as i64);
            let damaged = (dark && cleared.get(xi, yi)) || covered.is_some_and(|c| c.get(xi, yi));
            if !damaged {
                continue;
            }
            if let Some((codeword, block)) = self.map[y as usize * qr.size() as usize + x as usize]
                && !core::mem::replace(&mut hit[codeword as usize], true)
            {
                per_block[block as usize] += 1;
            }
        }
        per_block.iter().copied().max().unwrap_or(0)
    }
}

/// Spends the error-correction budget (SPEC §7.2 Outline, §7.3 logo) and
/// returns the worst block's share of its correctable codewords. Over the
/// budget is a readability error while the damage stays within what
/// Reed–Solomon corrects, and hard past it (the code can't decode); either
/// way the code is drawn as asked when it renders.
fn budget(style: &Style, qr: &QrMatrix, found: &mut Found) -> f32 {
    let cleared = scene::cleared(style, qr);
    if cleared.is_empty() && style.logo.is_none() {
        return 0.0;
    }
    let budget = Budget::new(qr, damage_budget(style, qr));
    let share = |damaged: u16| damaged as f32 / budget.correctable as f32;
    let mut push = |e: RenderError, damaged: u16| {
        if damaged > budget.correctable {
            found.hard(e);
        } else {
            found.soft(e);
        }
    };
    let outline = budget.worst(qr, &cleared, None);
    if outline > budget.allowed {
        push(
            RenderError::EccBudgetExceeded {
                damaged: outline,
                allowed: budget.allowed,
            },
            outline,
        );
        return share(outline);
    }
    let Some(logo) = &style.logo else {
        return share(outline);
    };
    let fits = |scale: f32| {
        let cover = logo::cover(logo, qr, scale);
        let worst = budget.worst(qr, &cleared, Some(&cover.modules));
        (cover.valid && worst <= budget.allowed, worst)
    };
    let scale = logo_scale(style, qr);
    let (ok, worst) = fits(scale);
    if ok && spares_alignment(style) && !logo::cover(logo, qr, scale).alignment.is_empty() {
        push(RenderError::LogoOverAlignment, 0);
    }
    if !ok {
        // Coverage grows with scale, so search the hundredths below it.
        let (mut lo, mut hi) = (0u32, (logo.scale * 100.0) as u32);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if fits(mid as f32 / 100.0).0 {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        push(
            RenderError::LogoTooLarge {
                max_scale: lo as f32 / 100.0,
            },
            worst,
        );
    }
    share(worst)
}

/// The scale `style`'s logo is drawn at on `qr`: its own, or with
/// [`LogoSize::Largest`](crate::LogoSize::Largest) the largest hundredth
/// up to 0.5 that the budget affords, when its own fits.
pub(crate) fn logo_scale(style: &Style, qr: &QrMatrix) -> f32 {
    let Some(logo) = &style.logo else {
        return 0.0;
    };
    if !logo.grow {
        return logo.scale;
    }
    let cleared = scene::cleared(style, qr);
    let budget = Budget::new(qr, damage_budget(style, qr));
    let spare = spares_alignment(style);
    let fits = |scale: f32| {
        let cover = logo::cover(logo, qr, scale);
        cover.valid
            && (!spare || cover.alignment.is_empty())
            && budget.worst(qr, &cleared, Some(&cover.modules)) <= budget.allowed
    };
    let mut lo = crate::math::ceil(logo.scale as f64 * 100.0 - 1e-3) as u32;
    if !fits(logo.scale) || lo > 50 || !fits(lo as f32 / 100.0) {
        return logo.scale;
    }
    let mut hi = 50;
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(mid as f32 / 100.0) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo as f32 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanbox::{AlignmentStyle, Emphasis, FinderStyle};
    use crate::{Ecc, QrCode, Version};

    fn qr(version: u8, ecc: Ecc) -> QrMatrix {
        QrCode::builder()
            .ecc(ecc)
            .boost_ecc(false)
            .version(Version::new(version).unwrap())
            .encode_text("HELLO")
            .unwrap()
    }

    fn outline() -> Style {
        Style::new().alignment(AlignmentStyle {
            emphasis: Emphasis::Outline,
            ..AlignmentStyle::default()
        })
    }

    /// Every error at every level (SPEC §9): `Strict` refuses it,
    /// `Lenient` renders a readability error with its warning and refuses a
    /// hard one, `Off` renders both, to a whole SVG.
    #[cfg(all(feature = "halftone", feature = "animate"))]
    #[test]
    fn every_error_at_every_level() {
        use crate::{
            Glass, Glyph, Halftone, Idle, Intro, Logo, Palette, Preset, RasterFormat, Rgba8Image,
            Trigger,
        };
        let code = qr(5, Ecc::M);
        let image = Rgba8Image::filled(8, 8, [90, 90, 90, 255]).unwrap();
        let photo = || {
            Halftone::new(image.clone())
                .ink(crate::Ink::Glass(Glass::new()))
                .source(crate::png::encode_rgba(&image), RasterFormat::Png)
                .unwrap()
        };
        let logo = || Logo::svg(r#"<svg viewBox="0 0 1 1"/>"#).unwrap();
        let readability: Vec<(&str, Style)> = alloc::vec![
            (
                "contrast",
                Style::new().palette(Palette::from_tokens([("fg", "#bbb")]).unwrap()),
            ),
            (
                "inverted",
                Style::new()
                    .palette(Palette::from_tokens([("fg", "#fff"), ("bg", "#000")]).unwrap()),
            ),
            ("quiet zone", Style::new().quiet_zone(1)),
            ("quiet zone 0", Style::new().quiet_zone(0)),
            (
                "measured range",
                Style::new().modules(ModuleShape::Dots { size: 0.4 })
            ),
            ("glass halo", Style::new().halftone(photo()).quiet_zone(4)),
        ];
        let hard: Vec<(&str, Style)> = alloc::vec![
            (
                "physical range",
                Style::new().modules(ModuleShape::Rounded { radius: 0.9 }),
            ),
            (
                "NaN",
                Style::new().modules(ModuleShape::Dots { size: f32::NAN }),
            ),
            ("shadow", Style::new().shadow(1.0).frame(Frame::None)),
            (
                "turn",
                Style::new().logo(logo().scale(0.1)).idle(Idle::LogoTurn),
            ),
            (
                "replay",
                Style::new().intro(
                    Intro::new(Preset::Ripple)
                        .duration_ms(1400)
                        .trigger(Trigger::Every(2000)),
                ),
            ),
            (
                "glyph on picture",
                Style::new()
                    .halftone(Halftone::new(image.clone()))
                    .module_glyph(
                        Glyph::svg(
                            r#"<svg viewBox="0 0 1 1"><circle cx=".5" cy=".5" r=".5"/></svg>"#
                        )
                        .unwrap(),
                    ),
            ),
            (
                "glass source",
                Style::new()
                    .halftone(Halftone::new(image.clone()).ink(crate::Ink::Glass(Glass::new()))),
            ),
            (
                "glass unsupported",
                Style::new().halftone(photo()).frame(Frame::Circle {
                    filler_density: 0.5,
                    ring: crate::FrameRing::None,
                }),
            ),
        ];
        let glass_halo = |s: Style| match s.halftone.as_ref().and_then(|h| h.glass()) {
            Some(_) => s.clone().halftone({
                let mut h = s.halftone.clone().unwrap();
                *h.glass_mut().unwrap() = Glass::new().halo(0);
                h
            }),
            None => s,
        };
        let whole = |name: &str, svg: &str| {
            assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{name}");
            // Numbers outside embedded files are finite.
            let text: alloc::string::String = svg
                .split("data:")
                .enumerate()
                .map(|(i, part)| {
                    if i == 0 {
                        part
                    } else {
                        part.split_once('"').map_or("", |p| p.1)
                    }
                })
                .collect();
            let bare_inf = text.match_indices("inf").any(|(i, _)| {
                let b = text.as_bytes();
                !b.get(i + 3).is_some_and(u8::is_ascii_alphabetic)
                    && !(i > 0 && b[i - 1].is_ascii_alphabetic())
            });
            assert!(!text.contains("NaN") && !bare_inf, "{name}");
        };
        for (name, style) in readability {
            let style = if name == "glass halo" {
                glass_halo(style)
            } else {
                style
            };
            let strict = style.clone().checks(Checks::Strict).check(&code);
            assert!(!strict.ok() && strict.scannable, "{name}: {strict:?}");
            for level in [Checks::Lenient, Checks::Off] {
                let s = style.clone().checks(level);
                let report = s.check(&code);
                assert!(
                    report.ok() && !report.scannable,
                    "{name} {level:?}: {report:?}"
                );
                assert!(report.warnings.len() >= strict.errors.len(), "{name}");
                whole(name, &s.render(&code).unwrap());
            }
        }
        for (name, style) in hard {
            for level in [Checks::Strict, Checks::Lenient] {
                let report = style.clone().checks(level).check(&code);
                assert!(!report.ok(), "{name} {level:?}: {report:?}");
                assert!(style.clone().checks(level).render(&code).is_err(), "{name}");
            }
            let off = style.clone().checks(Checks::Off);
            let report = off.check(&code);
            assert!(report.ok() && !report.scannable, "{name}: {report:?}");
            assert!(
                report
                    .warnings
                    .iter()
                    .any(|w| matches!(w, Warning::Unchecked(_))),
                "{name}: {report:?}"
            );
            whole(name, &off.render(&code).unwrap());
            off.plan(&code).unwrap();
        }
        // The damage budgets: refused strictly, drawn as asked with checks off.
        for (name, style, v, ecc) in [
            ("outline", outline(), 2, Ecc::L),
            ("logo", Style::new().logo(logo().scale(0.5)), 2, Ecc::L),
        ] {
            let code = qr(v, ecc);
            assert!(
                !style.clone().checks(Checks::Strict).check(&code).ok(),
                "{name}"
            );
            let off = style.checks(Checks::Off);
            assert!(!off.check(&code).scannable, "{name}");
            whole(name, &off.render(&code).unwrap());
        }
        // Fitting a code to a style still steers clear of what it would
        // warn about: the logo bumps the code whatever the level.
        let big = Style::new().logo(logo().scale(0.3));
        let fitted = |s: &Style| {
            QrCode::builder()
                .ecc(Ecc::L)
                .boost_ecc(false)
                .encode_text_for("HELLO", s)
                .unwrap()
                .version()
        };
        assert_eq!(fitted(&big), fitted(&big.clone().checks(Checks::Off)));
    }

    #[cfg(feature = "halftone")]
    #[test]
    fn scanlines_and_ink_share_a_limit() {
        let qr = crate::QrCode::encode("HELLO", crate::Ecc::M).unwrap();
        let both = |lines, fade, checks| {
            Style::new()
                .texture(Texture::Scanlines { opacity: lines })
                .texture(Texture::Ink { fade })
                .checks(checks)
                .check(&qr)
                .ok()
        };
        assert!(both(0.25, 0.15, Checks::Strict));
        assert!(!both(0.3, 0.3, Checks::Strict));
        // A measured limit: drawn as given by default.
        assert!(both(0.3, 0.3, Checks::Lenient));
        assert!(
            Style::new()
                .texture(Texture::Ink { fade: 0.3 })
                .check(&qr)
                .ok()
        );
    }

    #[test]
    fn effects_keep_to_their_measured_limits() {
        let qr = crate::QrCode::encode("HELLO", crate::Ecc::M).unwrap();
        let glow = |radius, strength| {
            Style::new()
                .effect(Effect::Glow { radius, strength })
                .checks(Checks::Strict)
        };
        assert!(glow(0.5, GLOW_MAX).check(&qr).ok());
        assert!(!glow(0.3, 0.4).check(&qr).ok());
        assert!(!glow(0.6, 0.1).check(&qr).ok());
        let emboss = |depth| {
            Style::new()
                .effect(Effect::Emboss { depth })
                .checks(Checks::Strict)
        };
        assert!(emboss(EMBOSS_MAX).check(&qr).ok());
        assert!(!emboss(0.15).check(&qr).ok());
        // Measured limits: drawn as given by default, with a warning.
        let lenient = glow(0.3, 0.4).checks(Checks::Lenient).check(&qr);
        assert!(lenient.ok() && !lenient.scannable, "{lenient:?}");
        assert!(matches!(
            lenient.warnings[..],
            [Warning::Unchecked(RenderError::OptionOutOfRange {
                option: "Effect::Glow strength",
                ..
            })]
        ));
        // Past the hard bound, refused unless checks are off, which clamps.
        let wild = glow(0.3, 3.0).checks(Checks::Lenient);
        assert!(!wild.check(&qr).ok());
        let off = wild.checks(Checks::Off);
        assert!(off.check(&qr).ok());
        assert_eq!(
            off.render(&qr).unwrap(),
            glow(0.3, 1.0).checks(Checks::Off).render(&qr).unwrap()
        );
    }

    #[test]
    fn shadows_need_an_opaque_framed_background() {
        let qr = crate::QrCode::encode("HELLO", crate::Ecc::M).unwrap();
        assert!(Style::new().shadow(1.0).check(&qr).ok());
        for style in [
            Style::new().shadow(1.0).frame(Frame::None),
            Style::new()
                .shadow(1.0)
                .palette(crate::Palette::from_tokens([("bg", "transparent")]).unwrap()),
        ] {
            assert!(
                style
                    .check(&qr)
                    .errors
                    .contains(&RenderError::ShadowNeedsBackground)
            );
        }
        let wide = Style::new()
            .border(Border::Solid { width: 2.5 })
            .shadow(4.0);
        assert_eq!(wide.check(&qr).errors.len(), 2);
    }

    #[test]
    fn halftone_logos_spare_alignment_patterns() {
        // Version 7 has a pattern in the middle, under any logo; version 6
        // has none there. Art mode outlines the patterns and dots the
        // timing, so its logo may cover one.
        use crate::{Halftone, Logo, Rgba8Image};
        let picture = Halftone::new(Rgba8Image::filled(8, 8, [90, 90, 90, 255]).unwrap());
        let logo = Logo::svg(r#"<svg viewBox="0 0 1 1"/>"#).unwrap().scale(0.1);
        let halftone = Style::new()
            .halftone(picture.clone())
            .logo(logo.clone())
            .checks(Checks::Strict);
        let report = halftone.check(&qr(7, Ecc::H));
        assert!(
            matches!(report.errors[..], [RenderError::LogoOverAlignment]),
            "{report:?}"
        );
        assert!(halftone.check(&qr(6, Ecc::H)).ok());
        let art = Style::art(picture).logo(logo);
        let steered = QrCode::builder()
            .ecc(Ecc::H)
            .version(Version::new(7).unwrap())
            .encode_text_for("HELLO", &art)
            .unwrap();
        let report = art.check(&steered);
        assert!(report.ok(), "{report:?}");
    }

    #[test]
    fn logo_frames_have_a_floor_of_their_own() {
        use crate::{Color, Logo, LogoFrame};
        let logo = Logo::svg(r#"<svg viewBox="0 0 1 1"/>"#)
            .unwrap()
            .scale(0.1)
            .frame(LogoFrame::Ring);
        let framed = |l: f32| {
            let mut palette = crate::Palette::new();
            let bg = palette.background().to_oklab()[0];
            let color = Color::oklch(bg - l, 0.0, 0.0);
            palette.set(Token::LogoFrame, color).unwrap();
            Style::new()
                .palette(palette)
                .logo(logo.clone())
                .checks(Checks::Strict)
                .check(&qr(7, Ecc::M))
        };
        // Between the frame's floor and the read tokens' warning: clean,
        // and not the code's contrast.
        let report = framed(0.3);
        assert!(report.ok() && report.warnings.is_empty(), "{report:?}");
        assert!(report.min_contrast > CONTRAST_WARNING);
        let report = framed(FRAME_FLOOR - 0.02);
        assert!(
            matches!(
                report.errors[..],
                [RenderError::ContrastTooLow {
                    token: Token::LogoFrame,
                    ..
                }]
            ),
            "{report:?}"
        );
        let message = alloc::format!("{}", report.errors[0]);
        assert!(message.contains("at least 0.25"), "{message}");
    }

    #[test]
    fn nothing_cleared_means_no_budget_used() {
        assert_eq!(Style::new().check(&qr(7, Ecc::M)).ecc_budget_used, 0.0);
        // Version 1 has no alignment patterns to outline.
        assert_eq!(outline().check(&qr(1, Ecc::M)).ecc_budget_used, 0.0);
    }

    #[test]
    fn outline_spends_the_budget() {
        let report = outline().check(&qr(5, Ecc::M));
        assert!(report.ok(), "{report:?}");
        assert!(report.ecc_budget_used > 0.0 && report.ecc_budget_used <= DAMAGE_BUDGET);
        // A single low-ECC block can't afford it.
        let report = outline().check(&qr(2, Ecc::L));
        assert!(
            matches!(report.errors[..], [RenderError::EccBudgetExceeded { .. }]),
            "{report:?}"
        );
        assert!(report.ecc_budget_used > DAMAGE_BUDGET);
    }

    #[test]
    fn target_size_warns_below_the_minimums() {
        let code = qr(5, Ecc::M); // 37 modules + 8 of quiet zone = 45
        let warned = |size| {
            Style::new()
                .target_size(size)
                .check(&code)
                .warnings
                .into_iter()
                .any(|w| {
                    matches!(
                        w,
                        Warning::SmallOnScreen { .. } | Warning::SmallInPrint { .. }
                    )
                })
        };
        assert!(!warned(TargetSize::Pixels(135)));
        assert!(warned(TargetSize::Pixels(134)));
        assert!(!warned(TargetSize::Millimeters(18.0)));
        assert!(warned(TargetSize::Millimeters(17.9)));
        let report = Style::new()
            .target_size(TargetSize::Millimeters(f32::NAN))
            .check(&code);
        assert!(!report.ok());
    }

    #[test]
    fn scan_box_options_are_range_checked() {
        let style = Style::new().finders(FinderStyle::new(
            RingShape::Rounded(4.0),
            EyeShape::Rounded(-1.0),
        ));
        let report = style.check(&qr(1, Ecc::M));
        assert_eq!(report.errors.len(), 6, "{report:?}"); // ring and eye at each corner
        let style = Style::new().alignment(AlignmentStyle {
            shape: AlignmentShape::Own {
                outer: RingShape::Rounded(2.5),
                eye: EyeShape::Rounded(0.6),
            },
            emphasis: Emphasis::Accent,
        });
        let report = style.check(&qr(5, Ecc::M));
        assert!(matches!(
            report.errors[..],
            [RenderError::OptionOutOfRange { .. }]
        ));
        assert!(report.warnings.contains(&Warning::RoundedFinder));
        assert!(
            !Style::new()
                .check(&qr(5, Ecc::M))
                .warnings
                .contains(&Warning::RoundedFinder)
        );
    }
}
