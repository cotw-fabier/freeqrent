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

mod bits;
mod check;
mod code;
mod color;
mod error;
mod geometry;
mod gf;
#[cfg(feature = "kanji")]
mod kanji_table;
mod math;
mod matrix;
mod num;
mod optimize;
mod palette;
mod rng;
mod scanbox;
mod scene;
mod segment;
mod style;
mod svg;
mod tables;
mod version;

pub use check::{
    CONTRAST_FLOOR, CONTRAST_WARNING, DAMAGE_BUDGET, RenderError, ScanReport, Warning,
};
pub use code::{QrBuilder, QrCode};
pub use color::{Color, ColorError};
pub use error::EncodeError;
pub use matrix::{CapacityReport, Corner, FinderRing, Mask, ModuleRole, QrMatrix};
pub use palette::{
    Contrast, Gradient, GradientKind, GradientStop, Paint, Palette, PaletteError, Token,
};
pub use scanbox::{
    AlignmentShape, AlignmentStyle, Emphasis, EyeShape, FinderStyle, RingShape, TimingStyle,
};
pub use segment::{ECI_UTF8, EciPolicy, Mode, Segment};
pub use style::{Axis, Frame, FrameRing, ModuleShape, Style, TargetSize};
pub use version::{Ecc, Version};
