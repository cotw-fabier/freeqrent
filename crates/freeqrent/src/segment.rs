use alloc::vec::Vec;

use crate::bits::BitBuffer;
use crate::error::EncodeError;
use crate::version::Version;

/// Characters allowed in alphanumeric mode, in code order.
const ALPHANUMERIC: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// ECI designator for UTF-8.
pub const ECI_UTF8: u32 = 26;

/// How a segment's characters are packed into bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Digits `0-9`, 10 bits per 3 digits.
    Numeric,
    /// `0-9`, `A-Z`, space and `$%*+-./:`, 11 bits per 2 characters.
    Alphanumeric,
    /// Arbitrary bytes, 8 bits each.
    Byte,
    /// Extended Channel Interpretation header (no characters).
    Eci,
}

impl Mode {
    const fn indicator(self) -> u32 {
        match self {
            Mode::Numeric => 0b0001,
            Mode::Alphanumeric => 0b0010,
            Mode::Byte => 0b0100,
            Mode::Eci => 0b0111,
        }
    }

    /// Width of the character count field.
    pub(crate) const fn char_count_bits(self, version: Version) -> u8 {
        let band = version.count_band();
        let widths: [u8; 3] = match self {
            Mode::Numeric => [10, 12, 14],
            Mode::Alphanumeric => [9, 11, 13],
            Mode::Byte => [8, 16, 16],
            Mode::Eci => [0, 0, 0],
        };
        widths[band]
    }
}

/// How text input chooses an ECI header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EciPolicy {
    /// ASCII text gets no header; any other text gets ECI 26 (UTF-8).
    #[default]
    Auto,
    /// Never write an ECI header. Text is still encoded as UTF-8 bytes,
    /// which most phone readers detect on their own.
    Never,
    /// Always write ECI 26, even for ASCII.
    Always,
}

impl EciPolicy {
    /// Whether `text` gets an ECI 26 header under this policy.
    pub fn applies_to(self, text: &str) -> bool {
        match self {
            EciPolicy::Auto => !text.is_ascii(),
            EciPolicy::Never => false,
            EciPolicy::Always => true,
        }
    }
}

/// One run of data in a single mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    mode: Mode,
    num_chars: usize,
    data: BitBuffer,
}

impl Segment {
    /// A numeric segment. Fails on anything but ASCII digits.
    pub fn numeric(digits: &str) -> Result<Segment, EncodeError> {
        if let Some(index) = digits.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(EncodeError::InvalidCharacter {
                mode: Mode::Numeric,
                index,
            });
        }
        let mut data = BitBuffer::new();
        for chunk in digits.as_bytes().chunks(3) {
            let value = chunk
                .iter()
                .fold(0u32, |acc, &b| acc * 10 + (b - b'0') as u32);
            data.push(value, chunk.len() as u8 * 3 + 1);
        }
        Ok(Segment {
            mode: Mode::Numeric,
            num_chars: digits.len(),
            data,
        })
    }

    /// An alphanumeric segment. Fails on characters outside the 45-character set.
    pub fn alphanumeric(text: &str) -> Result<Segment, EncodeError> {
        let mut codes = Vec::with_capacity(text.len());
        for (index, b) in text.bytes().enumerate() {
            match alphanumeric_code(b) {
                Some(code) => codes.push(code),
                None => {
                    return Err(EncodeError::InvalidCharacter {
                        mode: Mode::Alphanumeric,
                        index,
                    });
                }
            }
        }
        let mut data = BitBuffer::new();
        for pair in codes.chunks(2) {
            match *pair {
                [a, b] => data.push(a as u32 * 45 + b as u32, 11),
                [a] => data.push(a as u32, 6),
                _ => unreachable!(),
            }
        }
        Ok(Segment {
            mode: Mode::Alphanumeric,
            num_chars: text.len(),
            data,
        })
    }

    /// A byte segment.
    pub fn bytes(bytes: &[u8]) -> Segment {
        let mut data = BitBuffer::new();
        for &b in bytes {
            data.push(b as u32, 8);
        }
        Segment {
            mode: Mode::Byte,
            num_chars: bytes.len(),
            data,
        }
    }

    /// An ECI header that sets how following byte segments are interpreted.
    pub fn eci(designator: u32) -> Result<Segment, EncodeError> {
        let mut data = BitBuffer::new();
        match designator {
            0..=127 => data.push(designator, 8),
            128..=16_383 => data.push(0b10 << 14 | designator, 16),
            16_384..=999_999 => data.push(0b110 << 21 | designator, 24),
            _ => return Err(EncodeError::InvalidEci(designator)),
        }
        Ok(Segment {
            mode: Mode::Eci,
            num_chars: 0,
            data,
        })
    }

    /// The segment's mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Characters (bytes, digits…) in the segment; 0 for ECI.
    pub fn num_chars(&self) -> usize {
        self.num_chars
    }

    /// Bits this segment takes at `version`, or `None` if its character
    /// count doesn't fit the count field.
    pub(crate) fn encoded_bits(&self, version: Version) -> Option<usize> {
        let count_bits = self.mode.char_count_bits(version);
        if self.mode != Mode::Eci && self.num_chars >= 1 << count_bits {
            return None;
        }
        Some(4 + count_bits as usize + self.data.len())
    }

    pub(crate) fn write(&self, out: &mut BitBuffer, version: Version) {
        out.push(self.mode.indicator(), 4);
        let count_bits = self.mode.char_count_bits(version);
        if count_bits > 0 {
            out.push(self.num_chars as u32, count_bits);
        }
        out.extend(&self.data);
    }
}

/// Total bits for all segments at `version`, or `None` if any count overflows.
pub(crate) fn total_bits(segments: &[Segment], version: Version) -> Option<usize> {
    segments
        .iter()
        .try_fold(0usize, |acc, s| Some(acc + s.encoded_bits(version)?))
}

pub(crate) fn is_alphanumeric(b: u8) -> bool {
    alphanumeric_code(b).is_some()
}

fn alphanumeric_code(b: u8) -> Option<u8> {
    ALPHANUMERIC.iter().position(|&c| c == b).map(|i| i as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits_of(segment: &Segment) -> alloc::string::String {
        let mut out = BitBuffer::new();
        segment.write(&mut out, Version::MIN);
        let mut s = alloc::string::String::new();
        let mut padded = out.clone();
        while !padded.len().is_multiple_of(8) {
            padded.push(0, 1);
        }
        for byte in padded.to_bytes() {
            s.push_str(&alloc::format!("{byte:08b}"));
        }
        s.truncate(out.len());
        s
    }

    #[test]
    fn numeric_iso_example() {
        // ISO 18004 7.4.3: "01234567" -> 0001 0000001000 0000001100 0101011001 1000011
        let s = Segment::numeric("01234567").unwrap();
        assert_eq!(bits_of(&s), "00010000001000000000110001010110011000011");
    }

    #[test]
    fn alphanumeric_iso_example() {
        // ISO 18004 7.4.4: "AC-42" -> 0010 000000101 00111001110 11100111001 000010
        let s = Segment::alphanumeric("AC-42").unwrap();
        assert_eq!(bits_of(&s), "00100000001010011100111011100111001000010");
    }

    #[test]
    fn eci_designator_widths() {
        assert_eq!(Segment::eci(26).unwrap().data.len(), 8);
        assert_eq!(Segment::eci(9000).unwrap().data.len(), 16);
        assert_eq!(Segment::eci(999_999).unwrap().data.len(), 24);
        assert_eq!(
            Segment::eci(1_000_000),
            Err(EncodeError::InvalidEci(1_000_000))
        );
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(
            Segment::numeric("12a"),
            Err(EncodeError::InvalidCharacter {
                mode: Mode::Numeric,
                index: 2
            })
        );
        assert!(Segment::alphanumeric("abc").is_err());
    }

    #[test]
    fn eci_policy() {
        assert!(!EciPolicy::Auto.applies_to("plain"));
        assert!(EciPolicy::Auto.applies_to("café"));
        assert!(!EciPolicy::Never.applies_to("café"));
        assert!(EciPolicy::Always.applies_to("plain"));
    }

    #[test]
    fn count_overflow_is_detected() {
        let big = alloc::vec![0u8; 256];
        assert_eq!(Segment::bytes(&big).encoded_bits(Version::MIN), None);
        assert!(
            Segment::bytes(&big)
                .encoded_bits(Version::new(10).unwrap())
                .is_some()
        );
    }
}
