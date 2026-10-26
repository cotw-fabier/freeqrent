//! Reed–Solomon over GF(2^8) with the QR polynomial x^8 + x^4 + x^3 + x^2 + 1.

use alloc::vec;
use alloc::vec::Vec;

/// Multiplies two field elements (Russian peasant, no tables).
pub(crate) const fn mul(x: u8, y: u8) -> u8 {
    let mut z: u32 = 0;
    let mut i = 7;
    loop {
        z = (z << 1) ^ ((z >> 7) * 0x11D);
        z ^= ((y as u32 >> i) & 1) * x as u32;
        if i == 0 {
            break;
        }
        i -= 1;
    }
    z as u8
}

/// Coefficients of the generator polynomial of the given degree, highest
/// power first with the leading 1 dropped.
pub(crate) fn generator(degree: usize) -> Vec<u8> {
    let mut result = vec![0u8; degree];
    result[degree - 1] = 1;
    let mut root: u8 = 1;
    for _ in 0..degree {
        // Multiply by (x - root).
        for j in 0..degree {
            result[j] = mul(result[j], root);
            if j + 1 < degree {
                result[j] ^= result[j + 1];
            }
        }
        root = mul(root, 0x02);
    }
    result
}

/// The ECC codewords for `data`: the remainder of `data * x^n` divided by
/// the generator.
pub(crate) fn remainder(data: &[u8], generator: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; generator.len()];
    for &b in data {
        let factor = b ^ result[0];
        result.rotate_left(1);
        let last = result.len() - 1;
        result[last] = 0;
        for (r, &g) in result.iter_mut().zip(generator) {
            *r ^= mul(g, factor);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplication_basics() {
        assert_eq!(mul(0, 0x53), 0);
        assert_eq!(mul(1, 0x53), 0x53);
        assert_eq!(mul(0x02, 0x80), 0x1D);
        for a in 0..=255u8 {
            for b in [1u8, 2, 3, 0x8E, 0xFF] {
                assert_eq!(mul(a, b), mul(b, a));
            }
        }
    }

    #[test]
    fn iso_annex_example() {
        // ISO 18004 Annex I: "01234567" at 1-M.
        let data = [
            0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11,
            0xEC, 0x11,
        ];
        let ecc = remainder(&data, &generator(10));
        assert_eq!(
            ecc,
            [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55]
        );
    }
}
