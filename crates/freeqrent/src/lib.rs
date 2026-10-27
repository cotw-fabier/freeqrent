//! freeQRent: styled and animated SVG QR codes, with zero dependencies.
//!
//! The crate encodes text into a [`QrMatrix`], and a [`Style`] renders it
//! as SVG: module shapes, finder and alignment styles, color tokens,
//! round frames, logos, pictures and intro animations. Every style is
//! checked against an error-correction and contrast budget before it
//! renders, and the crate refuses what it can't verify to scan.
//!
//! The core is `no_std` + `alloc`, has no dependencies and no `unsafe`,
//! and its output is deterministic: the same input gives byte-identical
//! SVG. References like "SPEC §7.3" point into
//! [SPEC.md](https://github.com/freeqrent/freeqrent/blob/main/SPEC.md),
//! where each limit's measurement is recorded.
//!
//! # Encode and render
//!
//! ```
//! use freeqrent::{Ecc, ModuleShape, QrCode, Style};
//!
//! let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
//! let style = Style::new().modules(ModuleShape::Rounded { radius: 0.4 });
//! assert!(style.check(&qr).ok());
//! let svg = style.render(&qr).unwrap();
//! assert!(svg.starts_with("<svg "));
//! ```
//!
//! [`QrCode::builder`] pins versions, sets the ECC level and mask, and
//! encodes text, bytes or hand-built [`Segment`]s. [`Style::render_to`]
//! streams into any `fmt::Write`, and `Style::render_io` (feature `std`)
//! into an `io::Write`.
//!
//! # Presets
//!
//! [`Style::preset`] is a quick start: a named style ([`Style::presets`]
//! lists them) to change with the builder methods like any other.
//!
//! ```
//! use freeqrent::{Ecc, QrCode, Style};
//!
//! let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
//! let svg = Style::preset("ink").unwrap().render(&qr).unwrap();
//! assert!(svg.contains("#f7f3ec"));
//! ```
//!
//! # Scannability
//!
//! [`Style::check`] returns a [`ScanReport`]: hard [`RenderError`]s
//! (contrast below [`CONTRAST_FLOOR`], a logo past the error-correction
//! budget, a missing quiet zone) that make `render` refuse, and soft
//! [`Warning`]s. The `freeqrent-verify` crate rasterizes a render and
//! decodes it with ZXing and rqrr, held to the bar the report implies.
//!
//! # Colors
//!
//! A [`Palette`] maps [`Token`]s (`bg`, `fg`, `finder.eye`, …) to CSS
//! colors or gradients. Tokens left out fall back along a chain, and the
//! SVG exposes each as a CSS custom property (`--qr-fg`), so a host page
//! can re-theme a code without re-rendering it.
//!
//! ```
//! use freeqrent::{Ecc, Palette, QrCode, Style};
//!
//! let palette = Palette::from_tokens([
//!     ("bg", "#f7f3ec"),
//!     ("fg", "oklch(32% .08 265)"),
//!     ("finder.eye", "oklch(35% .15 30)"),
//! ])
//! .unwrap();
//! let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
//! let svg = Style::new().palette(palette).render(&qr).unwrap();
//! assert!(svg.contains("--qr-fg"));
//! ```
//!
//! With `palette-extract`, `Palette::from_image` (raw RGBA pixels; the
//! `freeqrent-image` crate decodes PNG and JPEG), `Palette::from_css` and
//! `Palette::from_html` derive a palette that passes the contrast rules
//! from a brand's own colors. The crate never fetches anything.
//!
//! # Logos and frames
//!
//! A [`Logo`] (sanitized SVG, or PNG/JPEG/GIF/WebP bytes) sits in the
//! middle on a plate. Encode *for* the style, and the code grows until the
//! logo fits its error-correction budget:
//!
//! ```
//! use freeqrent::{Ecc, Frame, FrameRing, Logo, LogoShape, QrCode, Style};
//!
//! let mark = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
//!     <circle cx="5" cy="5" r="4" fill="#a3302a"/></svg>"##;
//! let style = Style::new()
//!     .frame(Frame::Circle { filler_density: 0.5, ring: FrameRing::Solid })
//!     .logo(Logo::svg(mark).unwrap().scale(0.22).shape(LogoShape::Circle));
//! let qr = QrCode::builder()
//!     .ecc(Ecc::Q)
//!     .encode_text_for("https://example.com", &style)
//!     .unwrap();
//! assert!(style.render(&qr).is_ok());
//! ```
//!
//! # Pictures and art mode
//!
//! With `halftone`, a [`Halftone`] draws a picture into the code's
//! modules, and the encoder chooses the free data bits so the modules
//! follow it. `Style::art` is the large-image setup, with crisp tracking
//! boxes on an alignment grid ([`TrackingGrid`]).
//!
//! # Animation
//!
//! With `animate`, an [`Intro`] plays into the finished code as CSS
//! keyframes; renderers that ignore animation, and viewers who prefer
//! reduced motion, see the static code. `Style::plan` exports the same
//! motion as data ([`plan`]) for players outside CSS.
//!
//! ```
//! # #[cfg(feature = "animate")] {
//! use freeqrent::{Ecc, Intro, Preset, QrCode, Style};
//!
//! let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
//! let style = Style::new().intro(Intro::new(Preset::Settle).duration_ms(900));
//! let svg = style.render(&qr).unwrap();
//! assert!(svg.contains("@keyframes"));
//! # }
//! ```
//!
//! # Features
//!
//! | Feature | Default | Enables |
//! |---|---|---|
//! | `std` | yes | `std::error::Error` impls, `Style::render_io` |
//! | `animate` | yes | Intros, idle loops, palette transitions, `AnimationPlan` |
//! | `palette-extract` | yes | Palettes from pixels and CSS/HTML text |
//! | `halftone` | yes | Pictures in codes and art mode |
//! | `kanji` | no | Kanji mode (a ~27 KB JIS X 0208 table) |
//!
//! All are additive and dependency-free. The MSRV is Rust 1.92.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
// Docs link to feature-gated items, which resolve only with those features
// on; docs.rs builds with all of them.
#![cfg_attr(
    not(all(feature = "animate", feature = "palette-extract", feature = "halftone")),
    allow(rustdoc::broken_intra_doc_links)
)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "animate")]
mod animate;
mod bits;
mod check;
mod code;
mod color;
mod error;
#[cfg(feature = "palette-extract")]
mod extract;
mod geometry;
mod gf;
#[cfg(feature = "halftone")]
mod halftone;
mod image;
#[cfg(feature = "kanji")]
mod kanji_table;
mod logo;
mod math;
mod matrix;
mod num;
mod optimize;
mod palette;
mod preset;
mod rng;
mod sanitize;
mod scanbox;
mod scene;
mod segment;
#[cfg(feature = "palette-extract")]
mod site;
mod sort;
#[cfg(feature = "halftone")]
mod steer;
mod style;
mod svg;
mod tables;
mod version;

#[cfg(feature = "animate")]
pub use animate::{
    AnimationPlan, CubicBezier, Ease, Idle, Intro, LogoIntro, LogoTiming, Preset, StaggerOrder,
    Trigger,
};

/// The parts of an [`AnimationPlan`] (SPEC §8.5): what a player outside
/// CSS, such as the Flutter widget, reads to replay an intro.
#[cfg(feature = "animate")]
pub mod plan {
    pub use crate::animate::{
        AnimationPlan, Fallback, Keyframe, PlanLayer, PlanOutline, Property, Reveal, RevealShape,
        Target, Track, Value,
    };
}
pub use check::{
    ANIMATED_ELEMENTS, CONTRAST_FLOOR, CONTRAST_WARNING, DAMAGE_BUDGET, DAMAGE_BUDGET_H,
    FRAME_FLOOR, HALFTONE_DAMAGE_BUDGET, LONG_INTRO_MS, RenderError, ScanReport, Warning,
};
pub use code::{QrBuilder, QrCode};
pub use color::{Color, ColorError};
pub use error::EncodeError;
#[cfg(feature = "palette-extract")]
pub use extract::{
    Choice, EXTRACT_CONTRAST, ExtractError, INVERTED_CONTRAST, Origin, PaletteOpts, PaletteReport,
    Swatch,
};
#[cfg(feature = "halftone")]
pub use halftone::{Clarity, DEFAULT_BANDS, DEFAULT_CLARITY, Fit, Halftone, Ink, Steering};
pub use image::{ImageError, Rgba8Image};
pub use logo::{
    DEFAULT_MOON, FRAME_REFERENCE, Logo, LogoError, LogoFrame, LogoShape, LogoSize, RasterFormat,
};
pub use matrix::{CapacityReport, Corner, FinderRing, Mask, ModuleRole, QrMatrix};
pub use palette::{
    Contrast, Gradient, GradientKind, GradientStop, Paint, Palette, PaletteError, Token,
};
pub use scanbox::{
    AlignmentShape, AlignmentStyle, Emphasis, EyeShape, FinderStyle, RingShape, TimingStyle,
};
pub use scene::LayerRole;
pub use segment::{ECI_UTF8, EciPolicy, Mode, Segment};
pub use style::{Axis, Border, Frame, FrameRing, ModuleShape, Style, TargetSize};
pub use version::{Ecc, TrackingGrid, Version};

// The README's examples run as doctests.
#[cfg(all(
    doctest,
    feature = "std",
    feature = "animate",
    feature = "palette-extract"
))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
