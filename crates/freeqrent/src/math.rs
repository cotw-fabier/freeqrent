//! The float functions `core` doesn't provide on stable: written on `f64`
//! from range reduction and short series, so every platform computes the
//! same values. Accuracy is about 1e-15 relative, far beyond what color
//! math and 3-decimal SVG coordinates need.

use core::f64::consts::{FRAC_PI_2, LN_2, PI, SQRT_2};

/// Largest integer ≤ `x`, for `|x| < 2^52`.
pub(crate) fn floor(x: f64) -> f64 {
    let t = x as i64 as f64;
    if t > x { t - 1.0 } else { t }
}

/// Smallest integer ≥ `x`, for `|x| < 2^52`.
pub(crate) fn ceil(x: f64) -> f64 {
    -floor(-x)
}

/// Nearest integer, halves away from zero, for `|x| < 2^52`.
pub(crate) fn round(x: f64) -> f64 {
    if x < 0.0 {
        -floor(-x + 0.5)
    } else {
        floor(x + 0.5)
    }
}

/// `x · 2^k` for normal results.
fn scale_by_pow2(x: f64, k: i64) -> f64 {
    let k = k.clamp(-1022, 1023);
    x * f64::from_bits(((k + 1023) as u64) << 52)
}

/// Natural logarithm; `x` must be positive and finite.
pub(crate) fn ln(x: f64) -> f64 {
    debug_assert!(x > 0.0 && x.is_finite());
    let mut x = x;
    let mut k: i64 = 0;
    if x < f64::MIN_POSITIVE {
        x *= 18_014_398_509_481_984.0; // 2^54
        k -= 54;
    }
    let bits = x.to_bits();
    k += ((bits >> 52) & 0x7ff) as i64 - 1023;
    // Mantissa in [1, 2), then shifted into [√½, √2) so the series is short.
    let mut m = f64::from_bits((bits & ((1 << 52) - 1)) | (1023 << 52));
    if m > SQRT_2 {
        m /= 2.0;
        k += 1;
    }
    // ln m = 2·atanh(s), s = (m − 1)/(m + 1), |s| < 0.172.
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let mut term = s;
    let mut sum = 0.0;
    let mut n = 1.0;
    while n < 40.0 {
        sum += term / n;
        term *= s2;
        n += 2.0;
    }
    2.0 * sum + k as f64 * LN_2
}

/// `e^x`, saturating far outside the representable range.
pub(crate) fn exp(x: f64) -> f64 {
    if x > 709.0 {
        return f64::INFINITY;
    }
    if x < -745.0 {
        return 0.0;
    }
    let k = round(x / LN_2);
    let r = x - k * LN_2;
    // |r| ≤ ln2/2, so 20 Taylor terms are plenty.
    let mut term = 1.0;
    let mut sum = 1.0;
    for n in 1..20 {
        term *= r / n as f64;
        sum += term;
    }
    // Split the scaling so results near the subnormal edge stay accurate.
    let k = k as i64;
    let half = k / 2;
    scale_by_pow2(scale_by_pow2(sum, half), k - half)
}

/// `x^y` for `x ≥ 0`.
pub(crate) fn powf(x: f64, y: f64) -> f64 {
    debug_assert!(x >= 0.0);
    if x == 0.0 {
        return if y == 0.0 { 1.0 } else { 0.0 };
    }
    exp(y * ln(x))
}

/// Real cube root, defined for negative `x` too.
pub(crate) fn cbrt(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let a = x.abs();
    let mut r = exp(ln(a) / 3.0);
    // One Newton step polishes the last bits.
    r -= (r * r * r - a) / (3.0 * r * r);
    if x < 0.0 { -r } else { r }
}

/// `(sin x, cos x)` for finite `x`.
pub(crate) fn sin_cos(x: f64) -> (f64, f64) {
    // Reduce to r in [−π/4, π/4] plus a quadrant.
    let q = round(x / FRAC_PI_2);
    let r = x - q * FRAC_PI_2;
    let r2 = r * r;
    let (mut s, mut c) = (r, 1.0);
    let (mut ts, mut tc) = (r, 1.0);
    let mut n = 1.0;
    while n < 24.0 {
        ts *= -r2 / ((n + 1.0) * (n + 2.0));
        tc *= -r2 / (n * (n + 1.0));
        s += ts;
        c += tc;
        n += 2.0;
    }
    match (q as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// Square root of `x ≥ 0`.
#[cfg_attr(not(feature = "animate"), allow(dead_code))]
pub(crate) fn sqrt(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut r = exp(ln(x) / 2.0);
    r = (r + x / r) / 2.0;
    r
}

/// `atan(y / x)` in the right quadrant, in `(−π, π]`.
#[cfg_attr(not(feature = "animate"), allow(dead_code))]
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    // atan of |y/x| or its reciprocal, so the argument is at most 1.
    let (a, b) = (y.abs(), x.abs());
    let swap = a > b;
    let t = if swap { b / a } else { a / b };
    let mut r = atan_unit(t);
    if swap {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 { -r } else { r }
}

/// `atan t` for `0 ≤ t ≤ 1`: shifted by π/6 below tan(π/12), then a
/// short series.
#[cfg_attr(not(feature = "animate"), allow(dead_code))]
fn atan_unit(t: f64) -> f64 {
    const TAN_PI_12: f64 = 0.267_949_192_431_122_7;
    const SQRT_3: f64 = 1.732_050_807_568_877_2;
    let (base, t) = if t > TAN_PI_12 {
        (PI / 6.0, (t * SQRT_3 - 1.0) / (t + SQRT_3))
    } else {
        (0.0, t)
    };
    let t2 = t * t;
    let (mut sum, mut term) = (0.0, t);
    let mut n = 1.0;
    while n < 40.0 {
        sum += term / n;
        term *= -t2;
        n += 2.0;
    }
    base + sum
}

/// Degrees to radians.
pub(crate) fn radians(degrees: f64) -> f64 {
    degrees * (PI / 180.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-13 * b.abs().max(1.0)
    }

    #[cfg(feature = "std")]
    #[test]
    fn matches_std() {
        for i in 1..2000 {
            let x = i as f64 * 0.0137;
            assert!(close(ln(x), x.ln()), "ln {x}");
            assert!(close(exp(x - 10.0), (x - 10.0).exp()), "exp {x}");
            assert!(close(powf(x, 2.4), x.powf(2.4)), "pow {x}");
            assert!(close(cbrt(x - 13.0), (x - 13.0).cbrt()), "cbrt {x}");
            let a = x * 7.0 - 50.0;
            let (s, c) = sin_cos(a);
            assert!(close(s, a.sin()) && close(c, a.cos()), "sin_cos {a}");
            assert!(close(sqrt(x), x.sqrt()), "sqrt {x}");
            let (y, z) = (x * 3.0 - 20.0, 13.0 - x * 2.0);
            assert!((atan2(y, z) - y.atan2(z)).abs() < 1e-14, "atan2 {y} {z}");
            assert_eq!(floor(a), a.floor());
            assert_eq!(round(a), a.round());
        }
        assert_eq!(floor(-2.0), -2.0);
        assert_eq!(round(-2.5), -3.0);
        assert!(close(ln(1e-310), 1e-310f64.ln()));
    }
}
