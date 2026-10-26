//! Raw pixel buffers (SPEC §6.4): what palette extraction reads. Decoding
//! PNG or JPEG into one lives in the `freeqrent-image` crate, so the core
//! stays dependency-free.

use alloc::vec::Vec;
use core::fmt;

/// An 8-bit RGBA image: rows top to bottom, four bytes per pixel, sRGB,
/// alpha not premultiplied.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rgba8Image {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

/// Why a pixel buffer was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageError {
    /// The width or height is zero.
    Empty,
    /// The buffer isn't `width × height × 4` bytes long.
    LengthMismatch {
        /// `width × height × 4`.
        expected: usize,
        /// The length given.
        actual: usize,
    },
    /// `width × height × 4` doesn't fit in memory on this platform.
    TooLarge,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::Empty => f.write_str("the image has no pixels"),
            ImageError::LengthMismatch { expected, actual } => {
                write!(f, "expected {expected} bytes of RGBA, got {actual}")
            }
            ImageError::TooLarge => f.write_str("the image is too large"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ImageError {}

impl Rgba8Image {
    /// Wraps `data`, which must hold exactly `width × height` RGBA pixels.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Result<Rgba8Image, ImageError> {
        if width == 0 || height == 0 {
            return Err(ImageError::Empty);
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(ImageError::TooLarge)?;
        if data.len() != expected {
            return Err(ImageError::LengthMismatch {
                expected,
                actual: data.len(),
            });
        }
        Ok(Rgba8Image {
            width,
            height,
            data,
        })
    }

    /// An image of one color.
    pub fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Result<Rgba8Image, ImageError> {
        let n = (width as usize)
            .checked_mul(height as usize)
            .ok_or(ImageError::TooLarge)?;
        let mut data = Vec::new();
        data.try_reserve_exact(n.checked_mul(4).ok_or(ImageError::TooLarge)?)
            .map_err(|_| ImageError::TooLarge)?;
        for _ in 0..n {
            data.extend_from_slice(&rgba);
        }
        Rgba8Image::new(width, height, data)
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGBA bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The RGBA bytes, by value.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// The pixel at `(x, y)`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some([
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ])
    }

    /// Sets the pixel at `(x, y)`; does nothing outside the image.
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x < self.width && y < self.height {
            let i = (y as usize * self.width as usize + x as usize) * 4;
            self.data[i..i + 4].copy_from_slice(&rgba);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn lengths_are_checked() {
        assert_eq!(Rgba8Image::new(0, 3, vec![]), Err(ImageError::Empty));
        assert_eq!(
            Rgba8Image::new(2, 2, vec![0; 15]),
            Err(ImageError::LengthMismatch {
                expected: 16,
                actual: 15
            })
        );
        let mut img = Rgba8Image::filled(2, 2, [1, 2, 3, 4]).unwrap();
        img.set_pixel(1, 1, [9, 9, 9, 9]);
        assert_eq!(img.pixel(0, 1), Some([1, 2, 3, 4]));
        assert_eq!(img.pixel(1, 1), Some([9, 9, 9, 9]));
        assert_eq!(img.pixel(2, 0), None);
    }
}
