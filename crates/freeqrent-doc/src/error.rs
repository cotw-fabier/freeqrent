//! Structured errors (SPEC §10.1): `{ code, message, detail }`, the same in
//! every language. `code` is a stable snake_case name, `message` is for
//! people, and `detail` carries the error's fields.

use std::fmt;

use freeqrent::{EncodeError, ExtractError, ImageError, LogoError, RenderError, Warning};

use crate::json::{self, ObjectWriter};

/// An error from any boundary function.
#[derive(Clone, Debug, PartialEq)]
pub struct DocError {
    /// A stable snake_case name, e.g. `contrast_too_low` (SPEC §10.1 lists
    /// them).
    pub code: &'static str,
    /// What went wrong, in English.
    pub message: String,
    /// The error's fields as a JSON object, e.g. `{"token":"fg","delta_l":0.2}`.
    pub detail: String,
}

impl DocError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>, detail: Detail) -> DocError {
        DocError {
            code,
            message: message.into(),
            detail: detail.finish(),
        }
    }

    /// A problem with the document at `path` (dotted keys and `[index]`).
    pub(crate) fn invalid(path: &str, reason: impl Into<String>) -> DocError {
        let reason = reason.into();
        let message = if path.is_empty() {
            reason.clone()
        } else {
            format!("{path}: {reason}")
        };
        let mut d = Detail::new();
        d.str("path", path);
        d.str("reason", &reason);
        DocError::new("invalid_style", message, d)
    }

    /// A bad argument to a boundary function (a null pointer, text that
    /// isn't UTF-8).
    pub fn invalid_argument(reason: impl Into<String>) -> DocError {
        let reason = reason.into();
        let mut d = Detail::new();
        d.str("reason", &reason);
        DocError::new("invalid_argument", reason, d)
    }

    /// The error as `{"code":…,"message":…,"detail":{…}}`.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        let mut o = ObjectWriter::new(&mut out);
        o.str("code", self.code);
        o.str("message", &self.message);
        o.raw("detail", &self.detail);
        o.end();
        out
    }
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DocError {}

/// A `detail` object under construction.
pub(crate) struct Detail(String, bool);

impl Detail {
    pub(crate) fn new() -> Detail {
        Detail(String::from("{"), true)
    }

    fn key(&mut self, key: &str) -> &mut String {
        if !self.1 {
            self.0.push(',');
        }
        self.1 = false;
        json::write_str(&mut self.0, key);
        self.0.push(':');
        &mut self.0
    }

    pub(crate) fn str(&mut self, key: &str, value: &str) {
        json::write_str(self.key(key), value);
    }

    pub(crate) fn f32(&mut self, key: &str, value: f32) {
        json::write_f32(self.key(key), value);
    }

    pub(crate) fn int(&mut self, key: &str, value: impl fmt::Display) {
        let out = self.key(key);
        out.push_str(&value.to_string());
    }

    pub(crate) fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

/// The ECC level's letter.
pub(crate) fn ecc_name(ecc: freeqrent::Ecc) -> &'static str {
    match ecc {
        freeqrent::Ecc::L => "L",
        freeqrent::Ecc::M => "M",
        freeqrent::Ecc::Q => "Q",
        freeqrent::Ecc::H => "H",
    }
}

fn mode_name(mode: freeqrent::Mode) -> &'static str {
    match mode {
        freeqrent::Mode::Numeric => "numeric",
        freeqrent::Mode::Alphanumeric => "alphanumeric",
        freeqrent::Mode::Byte => "byte",
        freeqrent::Mode::Kanji => "kanji",
        freeqrent::Mode::Eci => "eci",
        _ => "other",
    }
}

impl From<EncodeError> for DocError {
    fn from(e: EncodeError) -> DocError {
        let mut d = Detail::new();
        let code = match &e {
            EncodeError::DataTooLong {
                bits_needed,
                bits_available,
                max_version,
                ecc,
            } => {
                d.int("bits_needed", bits_needed);
                d.int("bits_available", bits_available);
                d.int("max_version", max_version.number());
                d.str("ecc", ecc_name(*ecc));
                "data_too_long"
            }
            EncodeError::InvalidVersionRange { min, max } => {
                d.int("min", min.number());
                d.int("max", max.number());
                "invalid_version_range"
            }
            EncodeError::InvalidCharacter { mode, index } => {
                d.str("mode", mode_name(*mode));
                d.int("index", index);
                "invalid_character"
            }
            EncodeError::InvalidEci(n) => {
                d.int("eci", n);
                "invalid_eci"
            }
            _ => "encode_error",
        };
        DocError::new(code, e.to_string(), d)
    }
}

/// A render error's code and detail; the report uses it for each entry.
pub(crate) fn render_error_parts(e: &RenderError) -> (&'static str, Detail) {
    let mut d = Detail::new();
    let code = match e {
        RenderError::ContrastTooLow { token, delta_l } => {
            d.str("token", token.name());
            d.f32("delta_l", *delta_l);
            "contrast_too_low"
        }
        RenderError::InvertedColors { token } => {
            d.str("token", token.name());
            "inverted_colors"
        }
        RenderError::QuietZoneTooSmall { modules } => {
            d.int("modules", modules);
            "quiet_zone_too_small"
        }
        RenderError::OptionOutOfRange {
            option,
            value,
            min,
            max,
        } => {
            d.str("option", option);
            d.f32("value", *value);
            d.f32("min", *min);
            d.f32("max", *max);
            "option_out_of_range"
        }
        RenderError::EccBudgetExceeded { damaged, allowed } => {
            d.int("damaged", damaged);
            d.int("allowed", allowed);
            "ecc_budget_exceeded"
        }
        RenderError::LogoTooLarge { max_scale } => {
            d.f32("max_scale", *max_scale);
            "logo_too_large"
        }
        RenderError::LogoOverAlignment => "logo_over_alignment",
        RenderError::TurnNeedsRoundFrame => "turn_needs_round_frame",
        RenderError::ReplayTooSoon { every_ms, intro_ms } => {
            d.int("every_ms", every_ms);
            d.int("intro_ms", intro_ms);
            "replay_too_soon"
        }
        RenderError::Write => "write",
        _ => "render_error",
    };
    (code, d)
}

impl From<RenderError> for DocError {
    fn from(e: RenderError) -> DocError {
        let (code, d) = render_error_parts(&e);
        DocError::new(code, e.to_string(), d)
    }
}

/// A warning's code and detail, for the report.
pub(crate) fn warning_parts(w: &Warning) -> (&'static str, Detail) {
    let mut d = Detail::new();
    let code = match w {
        Warning::LowContrast { token, delta_l } => {
            d.str("token", token.name());
            d.f32("delta_l", *delta_l);
            "low_contrast"
        }
        Warning::Inverted { token } => {
            d.str("token", token.name());
            "inverted"
        }
        Warning::NarrowQuietZone { modules } => {
            d.int("modules", modules);
            "narrow_quiet_zone"
        }
        Warning::SmallDots { size } => {
            d.f32("size", *size);
            "small_dots"
        }
        Warning::TranslucentBackground => "translucent_background",
        Warning::SmallOnScreen { px_per_module } => {
            d.f32("px_per_module", *px_per_module);
            "small_on_screen"
        }
        Warning::SmallInPrint { mm_per_module } => {
            d.f32("mm_per_module", *mm_per_module);
            "small_in_print"
        }
        Warning::RoundedFinder => "rounded_finder",
        Warning::LongIntro { ms } => {
            d.int("ms", ms);
            "long_intro"
        }
        Warning::MotionContrast { token, delta_l } => {
            d.str("token", token.name());
            d.f32("delta_l", *delta_l);
            "motion_contrast"
        }
        Warning::ManyAnimatedElements { count } => {
            d.int("count", count);
            "many_animated_elements"
        }
        _ => "warning",
    };
    (code, d)
}

impl From<ExtractError> for DocError {
    fn from(e: ExtractError) -> DocError {
        let code = match e {
            ExtractError::NoColors => "no_colors",
            _ => "extract_error",
        };
        DocError::new(code, e.to_string(), Detail::new())
    }
}

/// An image that doesn't hold together, at `path` (empty for an argument).
pub(crate) fn image_error(path: &str, e: ImageError) -> DocError {
    let mut d = Detail::new();
    d.str("path", path);
    match e {
        ImageError::LengthMismatch { expected, actual } => {
            d.int("expected", expected);
            d.int("actual", actual);
        }
        ImageError::Empty | ImageError::TooLarge => {}
        _ => {}
    }
    let message = if path.is_empty() {
        e.to_string()
    } else {
        format!("{path}: {e}")
    };
    DocError::new("invalid_image", message, d)
}

/// A logo that can't be read, at `path`.
pub(crate) fn logo_error(path: &str, e: LogoError) -> DocError {
    let mut d = Detail::new();
    d.str("path", path);
    let reason = match e {
        LogoError::NotSvg => "not_svg",
        LogoError::NoSize => "no_size",
        LogoError::NotImage(_) => "not_image",
        _ => "logo_error",
    };
    d.str("reason", reason);
    DocError::new("invalid_logo", format!("{path}: {e}"), d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_form() {
        let e = DocError::invalid("intro.duration_ms", "expected a number, got a string");
        assert_eq!(
            e.to_json(),
            r#"{"code":"invalid_style","message":"intro.duration_ms: expected a number, got a string","detail":{"path":"intro.duration_ms","reason":"expected a number, got a string"}}"#
        );
        let e = DocError::from(RenderError::ContrastTooLow {
            token: freeqrent::Token::Fg,
            delta_l: 0.25,
        });
        assert_eq!(e.code, "contrast_too_low");
        assert_eq!(e.detail, r#"{"token":"fg","delta_l":0.25}"#);
    }
}
