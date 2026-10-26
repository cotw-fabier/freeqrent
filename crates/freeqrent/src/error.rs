use core::fmt;

use crate::segment::Mode;
use crate::version::{Ecc, Version};

/// Why encoding failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// The data doesn't fit in the largest allowed version.
    DataTooLong {
        /// Bits the segments need at `max_version`.
        bits_needed: usize,
        /// Data bits available at `max_version` and the requested ECC level.
        bits_available: usize,
        /// The largest version that was tried.
        max_version: Version,
        /// The ECC level that was tried.
        ecc: Ecc,
    },
    /// `min_version` is greater than `max_version`.
    InvalidVersionRange {
        /// The requested minimum.
        min: Version,
        /// The requested maximum.
        max: Version,
    },
    /// A character can't be represented in the segment's mode.
    InvalidCharacter {
        /// The segment's mode.
        mode: Mode,
        /// Byte offset of the offending character in the input.
        index: usize,
    },
    /// An ECI designator above 999 999.
    InvalidEci(u32),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataTooLong {
                bits_needed,
                bits_available,
                max_version,
                ecc,
            } => write!(
                f,
                "data needs {bits_needed} bits but version {} at ECC {ecc:?} holds {bits_available}",
                max_version.number()
            ),
            Self::InvalidVersionRange { min, max } => write!(
                f,
                "min version {} is greater than max version {}",
                min.number(),
                max.number()
            ),
            Self::InvalidCharacter { mode, index } => {
                write!(f, "character at byte {index} is not valid in {mode:?} mode")
            }
            Self::InvalidEci(value) => write!(f, "ECI designator {value} is out of range"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EncodeError {}
