//! Static scannability rules (SPEC §9, first row): what `Style::check`
//! reports and what makes `Style::render` refuse.

use alloc::vec::Vec;
use core::fmt;

use crate::geometry::Grid;
use crate::logo::{self, LogoShape};
use crate::matrix::{QrMatrix, misdecode_protection};
use crate::palette::Token;
use crate::scanbox::{AlignmentShape, EyeShape, RingShape};
use crate::scene;
use crate::style::{Frame, ModuleShape, Style, TargetSize};
use crate::version::BlockLayout;

/// The OKLab lightness difference below which a code isn't rendered. At
/// or above it, codes decode from clean renders (`freeqrent-verify`).
pub const CONTRAST_FLOOR: f32 = 0.40;
/// The share of a block's correctable codewords that cleared modules may
/// use up (SPEC §7.3), leaving the rest for print and camera damage.
pub const DAMAGE_BUDGET: f32 = 0.60;

/// The difference below which a code renders with a warning: under blur
/// and sensor noise, ZXing starts missing codes below about 0.6, the
/// finder eyes first.
pub const CONTRAST_WARNING: f32 = 0.60;

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
    /// The output sink returned an error.
    Write,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::ContrastTooLow { token, delta_l } => write!(
                f,
                "{token} has contrast {delta_l:.2} against bg; at least {CONTRAST_FLOOR} is needed"
            ),
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
            RenderError::LogoTooLarge { max_scale } => write!(
                f,
                "the logo is too large for this code; at most scale {max_scale:.2} fits"
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
        }
    }
}

/// The result of [`Style::check`].
#[derive(Clone, Debug, PartialEq)]
pub struct ScanReport {
    /// Problems that stop rendering.
    pub errors: Vec<RenderError>,
    /// Risks worth knowing about.
    pub warnings: Vec<Warning>,
    /// The lowest contrast of any painted token against `bg`.
    pub min_contrast: f32,
    /// How much of the error-correction budget cleared modules use, in the
    /// worst block: damaged codewords over correctable ones (0 when nothing
    /// is cleared). Above [`DAMAGE_BUDGET`] the code isn't rendered.
    pub ecc_budget_used: f32,
}

impl ScanReport {
    /// True when the code would render.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

fn in_range(errors: &mut Vec<RenderError>, option: &'static str, value: f32, min: f32, max: f32) {
    // Written so that NaN fails.
    if !(value >= min && value <= max) {
        errors.push(RenderError::OptionOutOfRange {
            option,
            value,
            min,
            max,
        });
    }
}

pub(crate) fn check(style: &Style, qr: &QrMatrix) -> ScanReport {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    match style.modules {
        ModuleShape::Square { gap } => in_range(&mut errors, "Square.gap", gap, 0.0, 0.25),
        ModuleShape::Rounded { radius } => {
            in_range(&mut errors, "Rounded.radius", radius, 0.0, 0.5)
        }
        ModuleShape::Dots { size } => {
            in_range(&mut errors, "Dots.size", size, 0.6, 1.0);
            if (0.6..0.75).contains(&size) {
                warnings.push(Warning::SmallDots { size });
            }
        }
        ModuleShape::Liquid { smoothness } => {
            in_range(&mut errors, "Liquid.smoothness", smoothness, 0.0, 1.0)
        }
        ModuleShape::Bars { radius, .. } => in_range(&mut errors, "Bars.radius", radius, 0.0, 0.5),
    }

    match style.quiet_zone {
        0 | 1 => errors.push(RenderError::QuietZoneTooSmall {
            modules: style.quiet_zone,
        }),
        2 | 3 => warnings.push(Warning::NarrowQuietZone {
            modules: style.quiet_zone,
        }),
        _ => {}
    }

    match style.frame {
        Frame::RoundedSquare(r) => in_range(
            &mut errors,
            "Frame::RoundedSquare radius",
            r,
            0.0,
            style.quiet_zone as f32,
        ),
        Frame::Circle { filler_density, .. } => in_range(
            &mut errors,
            "Frame::Circle filler_density",
            filler_density,
            0.0,
            1.0,
        ),
        Frame::Square | Frame::None => {}
    }
    if let Some(logo) = &style.logo {
        in_range(&mut errors, "Logo.scale", logo.scale, 0.0, 0.5);
        in_range(&mut errors, "Logo.padding", logo.padding, 0.0, 4.0);
        if let LogoShape::Rounded(r) = logo.shape {
            in_range(&mut errors, "LogoShape::Rounded radius", r, 0.0, f32::MAX);
        }
    }

    #[cfg(feature = "animate")]
    check_intro(style, qr, &mut errors, &mut warnings);
    check_scan_boxes(style, &mut errors, &mut warnings);
    check_target(style, qr, &mut errors, &mut warnings);
    let ecc_budget_used = budget(style, qr, &mut errors);

    let palette = &style.palette;
    if palette.background().is_translucent() {
        warnings.push(Warning::TranslucentBackground);
    }
    let mut min_contrast = f32::INFINITY;
    // Filler and the frame ring are decoration outside the quiet zone, so
    // they may be as faint as the design wants.
    let read = scene::layers(style, qr)
        .into_iter()
        .filter(|l| !matches!(l.token, Token::Filler | Token::Frame));
    for layer in read {
        let token = layer.token;
        let contrast = palette.contrast(token);
        min_contrast = min_contrast.min(contrast.delta_l);
        if contrast.delta_l < CONTRAST_FLOOR {
            errors.push(RenderError::ContrastTooLow {
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
                errors.push(RenderError::InvertedColors { token });
            }
        }
    }

    ScanReport {
        errors,
        warnings,
        min_contrast,
        ecc_budget_used,
    }
}

#[cfg(feature = "animate")]
fn check_intro(
    style: &Style,
    qr: &QrMatrix,
    errors: &mut Vec<RenderError>,
    warnings: &mut Vec<Warning>,
) {
    if let Some(ms) = style.theme_transition {
        in_range(errors, "Style.theme_transition", ms as f32, 0.0, 10_000.0);
    }
    for (token, delta_l) in crate::animate::motion_contrast(style, qr) {
        if delta_l < CONTRAST_FLOOR {
            warnings.push(Warning::MotionContrast { token, delta_l });
        }
    }
    let Some(intro) = &style.intro else {
        return;
    };
    in_range(
        errors,
        "Intro.duration_ms",
        intro.duration_ms as f32,
        100.0,
        10_000.0,
    );
    in_range(
        errors,
        "Intro.delay_ms",
        intro.delay_ms as f32,
        0.0,
        10_000.0,
    );
    in_range(errors, "Intro.groups", intro.groups as f32, 2.0, 16.0);
    if let Some(crate::Ease::Custom(c)) = intro.ease {
        in_range(errors, "CubicBezier.x1", c.x1, 0.0, 1.0);
        in_range(errors, "CubicBezier.x2", c.x2, 0.0, 1.0);
        in_range(errors, "CubicBezier.y1", c.y1, -10.0, 10.0);
        in_range(errors, "CubicBezier.y2", c.y2, -10.0, 10.0);
    }
    if intro.duration_ms > LONG_INTRO_MS {
        warnings.push(Warning::LongIntro {
            ms: intro.duration_ms,
        });
    }
    let count = crate::animate::element_count(style, qr);
    if count > ANIMATED_ELEMENTS {
        warnings.push(Warning::ManyAnimatedElements { count });
    }
}

/// Screen and print minimums from SPEC §7.8.
const MIN_PX_PER_MODULE: f32 = 3.0;
const MIN_MM_PER_MODULE: f32 = 0.4;

fn check_target(
    style: &Style,
    qr: &QrMatrix,
    errors: &mut Vec<RenderError>,
    warnings: &mut Vec<Warning>,
) {
    let modules = scene::canvas(style, qr).side as f32;
    match style.target {
        None => {}
        Some(TargetSize::Pixels(px)) => {
            let px_per_module = px as f32 / modules;
            if px_per_module < MIN_PX_PER_MODULE {
                warnings.push(Warning::SmallOnScreen { px_per_module });
            }
        }
        Some(TargetSize::Millimeters(mm)) => {
            in_range(
                errors,
                "TargetSize::Millimeters",
                mm,
                f32::MIN_POSITIVE,
                f32::MAX,
            );
            let mm_per_module = mm / modules;
            if mm_per_module < MIN_MM_PER_MODULE {
                warnings.push(Warning::SmallInPrint { mm_per_module });
            }
        }
    }
}

fn check_scan_boxes(style: &Style, errors: &mut Vec<RenderError>, warnings: &mut Vec<Warning>) {
    let mut rings = Vec::new();
    for corner in scene::CORNERS {
        let (ring, eye) = style.finders.at(corner);
        rings.push(ring);
        if let RingShape::Rounded(r) = ring {
            in_range(errors, "FinderStyle.outer radius", r, 0.0, 2.5);
        }
        if let EyeShape::Rounded(r) = eye {
            in_range(errors, "FinderStyle.eye radius", r, 0.0, 1.5);
        }
    }
    if let AlignmentShape::Own { outer, eye } = style.alignment.shape {
        rings.push(outer);
        if let RingShape::Rounded(r) = outer {
            in_range(errors, "AlignmentShape.outer radius", r, 0.0, 2.5);
        }
        if let EyeShape::Rounded(r) = eye {
            in_range(errors, "AlignmentShape.eye radius", r, 0.0, 0.5);
        }
    }
    if rings.iter().any(|&r| r != RingShape::Square) {
        warnings.push(Warning::RoundedFinder);
    }
}

/// What each Reed–Solomon block of a code may lose.
struct Budget {
    /// Correctable codewords per block (after misdecode protection).
    correctable: u16,
    /// Of those, what [`DAMAGE_BUDGET`] lets cleared modules take.
    allowed: u16,
    /// For each module, its codeword and block.
    map: Vec<Option<(u16, u16)>>,
    blocks: usize,
    codewords: usize,
}

impl Budget {
    fn new(qr: &QrMatrix) -> Budget {
        let layout = BlockLayout::new(qr.version(), qr.ecc());
        let correctable =
            ((layout.ecc_per_block - misdecode_protection(qr.version(), qr.ecc())) / 2) as u16;
        Budget {
            correctable,
            allowed: (correctable as f32 * DAMAGE_BUDGET) as u16,
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
/// returns the worst block's share of its correctable codewords.
fn budget(style: &Style, qr: &QrMatrix, errors: &mut Vec<RenderError>) -> f32 {
    let cleared = scene::cleared(style, qr);
    if cleared.is_empty() && style.logo.is_none() {
        return 0.0;
    }
    let budget = Budget::new(qr);
    let share = |damaged: u16| damaged as f32 / budget.correctable as f32;
    let outline = budget.worst(qr, &cleared, None);
    if outline > budget.allowed {
        errors.push(RenderError::EccBudgetExceeded {
            damaged: outline,
            allowed: budget.allowed,
        });
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
    let (ok, worst) = fits(logo.scale);
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
        errors.push(RenderError::LogoTooLarge {
            max_scale: lo as f32 / 100.0,
        });
    }
    share(worst)
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
