//! freeQRent: styled and animated SVG QR codes, with zero dependencies.
//!
//! This release contains the encoder (SPEC §3): versions 1–40, ECC L/M/Q/H,
//! numeric/alphanumeric/byte/Kanji/ECI segments with minimal-length
//! segmentation, mask selection, and a
//! [`QrMatrix`] that records the role of every module; and CSS color
//! parsing with OKLab contrast math for the color tokens (SPEC §6).
//!
//! ```
//! use freeqrent::{Ecc, ModuleRole, QrCode};
//!
//! let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
//! let data_modules = qr.modules().filter(|&(_, _, _, role)| role == ModuleRole::Data).count();
//! assert!(data_modules > 0);
//! ```

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod bits;
mod code;
mod color;
mod error;
mod gf;
#[cfg(feature = "kanji")]
mod kanji_table;
mod math;
mod matrix;
mod num;
mod optimize;
mod palette;
mod segment;
mod tables;
mod version;

pub use code::{QrBuilder, QrCode};
pub use color::{Color, ColorError};
pub use error::EncodeError;
pub use matrix::{CapacityReport, Corner, FinderRing, Mask, ModuleRole, QrMatrix};
pub use palette::{
    Contrast, Gradient, GradientKind, GradientStop, Paint, Palette, PaletteError, Token,
};
pub use segment::{ECI_UTF8, EciPolicy, Mode, Segment};
pub use version::{Ecc, Version};
