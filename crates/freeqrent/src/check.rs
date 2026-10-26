//! Static scannability rules (SPEC §9, first row): what `Style::check`
//! reports and what makes `Style::render` refuse.

use alloc::vec::Vec;
use core::fmt;

use crate::matrix::QrMatrix;
use crate::palette::Token;
use crate::scene;
use crate::style::{ModuleShape, Style};

/// The OKLab lightness difference below which a code isn't rendered. At
/// or above it, codes decode from clean renders (`freeqrent-verify`).
pub const CONTRAST_FLOOR: f32 = 0.40;
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
}

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

    let palette = &style.palette;
    if palette.background().is_translucent() {
        warnings.push(Warning::TranslucentBackground);
    }
    let mut min_contrast = f32::INFINITY;
    for layer in scene::layers(style, qr) {
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
    }
}
