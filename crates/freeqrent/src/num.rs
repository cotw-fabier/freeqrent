//! Short decimal output shared by colors and path data: a fixed number of
//! decimals at most, trailing zeros trimmed, and no leading zero (`.5`,
//! `-.25`). Values are rounded to integers first, so formatting is exact.

use core::fmt::{self, Write};

/// Writes `value / 10^decimals` in the short form.
pub(crate) fn write_scaled<W: Write + ?Sized>(w: &mut W, value: i64, decimals: u32) -> fmt::Result {
    if value < 0 {
        w.write_char('-')?;
    }
    let value = value.unsigned_abs();
    let scale = 10u64.pow(decimals);
    let (int, mut frac) = (value / scale, value % scale);
    if int != 0 || frac == 0 {
        write!(w, "{int}")?;
    }
    if frac != 0 {
        let mut digits = decimals;
        while frac % 10 == 0 {
            frac /= 10;
            digits -= 1;
        }
        write!(w, ".{frac:0width$}", width = digits as usize)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    fn scaled(value: i64, decimals: u32) -> String {
        let mut s = String::new();
        write_scaled(&mut s, value, decimals).unwrap();
        s
    }

    #[test]
    fn short_forms() {
        assert_eq!(scaled(0, 3), "0");
        assert_eq!(scaled(500, 3), ".5");
        assert_eq!(scaled(-250, 3), "-.25");
        assert_eq!(scaled(12_000, 3), "12");
        assert_eq!(scaled(12_005, 3), "12.005");
        assert_eq!(scaled(-3, 3), "-.003");
        assert_eq!(scaled(7, 0), "7");
    }
}
