//! freeQRent: styled and animated SVG QR codes, with zero dependencies.
//!
//! The encoder (SPEC §3): versions 1–40, ECC L/M/Q/H,
//! numeric/alphanumeric/byte/Kanji/ECI segments with minimal-length
//! segmentation, mask selection, and a [`QrMatrix`] that records the role
//! of every module. The styling layer (SPEC §4–§7): a [`Style`] with module
//! shapes, color tokens with OKLab contrast checks, and a streaming SVG
//! writer.
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

#![no_std]
#![forbid(unsafe_code)]

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
mod image;
#[cfg(feature = "kanji")]
mod kanji_table;
mod logo;
mod math;
mod matrix;
mod num;
mod optimize;
mod palette;
mod rng;
mod sanitize;
mod scanbox;
mod scene;
mod segment;
mod style;
mod svg;
mod tables;
mod version;

#[cfg(feature = "animate")]
pub use animate::{
    AnimationPlan, CubicBezier, Ease, Fallback, Idle, Intro, Keyframe, PlanLayer, Preset, Property,
    Reveal, RevealShape, StaggerOrder, Target, Track, Trigger, Value,
};
pub use check::{
    ANIMATED_ELEMENTS, CONTRAST_FLOOR, CONTRAST_WARNING, DAMAGE_BUDGET, LONG_INTRO_MS, RenderError,
    ScanReport, Warning,
};
pub use code::{QrBuilder, QrCode};
pub use color::{Color, ColorError};
pub use error::EncodeError;
#[cfg(feature = "palette-extract")]
pub use extract::{
    Choice, EXTRACT_CONTRAST, ExtractError, Origin, PaletteOpts, PaletteReport, Swatch,
};
pub use image::{ImageError, Rgba8Image};
pub use logo::{Logo, LogoError, LogoShape, RasterFormat};
pub use matrix::{CapacityReport, Corner, FinderRing, Mask, ModuleRole, QrMatrix};
pub use palette::{
    Contrast, Gradient, GradientKind, GradientStop, Paint, Palette, PaletteError, Token,
};
pub use scanbox::{
    AlignmentShape, AlignmentStyle, Emphasis, EyeShape, FinderStyle, RingShape, TimingStyle,
};
pub use scene::LayerRole;
pub use segment::{ECI_UTF8, EciPolicy, Mode, Segment};
pub use style::{Axis, Frame, FrameRing, ModuleShape, Style, TargetSize};
pub use version::{Ecc, Version};
