//! freeQRent: styled and animated SVG QR codes, with zero dependencies.
//!
//! This release contains the encoder (SPEC §3): versions 1–40, ECC L/M/Q/H,
//! numeric/alphanumeric/byte/ECI segments, mask selection, and a
//! [`QrMatrix`] that records the role of every module.
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
mod error;
mod gf;
mod matrix;
mod segment;
mod tables;
mod version;

pub use code::{QrBuilder, QrCode};
pub use error::EncodeError;
pub use matrix::{CapacityReport, Corner, FinderRing, Mask, ModuleRole, QrMatrix};
pub use segment::{ECI_UTF8, EciPolicy, Mode, Segment};
pub use version::{Ecc, Version};
