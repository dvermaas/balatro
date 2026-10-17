//! Python float formatting and rounding.

use crate::math::Big;
use std::cmp::Ordering;

/// `str(x)` / `repr(x)` / `f"{x}"` for a Python float: shortest round-trip digits, always with a
/// decimal point or exponent ("3.0", "0.1", "1e-05", "1e+16", "inf", "nan", "-0.0").
pub fn pyfloat(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf".into() } else { "inf".into() };
    }
    // Rust's `{:e}` gives the shortest round-trip digits, like Python's repr (dtoa mode 0).
    let mut s = format!("{:e}", x.abs());
    // Several shortest decimals can round-trip; Python (dtoa mode 0) returns the one closest to
    // the exact value, Rust's shortest algorithm does not always. The correctly rounded decimal of
    // the same length is the closest; use it when it round-trips.
    let nd0 = s.split_once('e').unwrap().0.chars().filter(|c| *c != '.').count();
    if nd0 >= 2 {
        let alt = format!("{:.*e}", nd0 - 1, x.abs());
        if alt.parse::<f64>().unwrap() == x.abs() {
            s = alt;
        }
    }
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let nd = digits.len() as i32;
    let decpt = exp + 1; // position of the decimal point relative to the digits
    let body = if decpt > 16 || decpt < -3 {
        // exponent form
        let mut m = String::new();
        m.push_str(&digits[..1]);
        if nd > 1 {
            m.push('.');
            m.push_str(&digits[1..]);
        }
        let sgn = if exp < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", m, sgn, exp.abs())
    } else if decpt <= 0 {
        format!("0.{}{}", "0".repeat((-decpt) as usize), digits)
    } else if decpt >= nd {
        format!("{}{}.0", digits, "0".repeat((decpt - nd) as usize))
    } else {
        format!("{}.{}", &digits[..decpt as usize], &digits[decpt as usize..])
    };
    format!("{}{}", sign, body)
}

/// `f"{x:.{prec}f}"` / `format(x, f".{prec}f")`: correctly rounded from the exact binary value,
/// ties to even (CPython's dtoa mode 3). Handles "-0.00" like Python.
pub fn fixed(x: f64, prec: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf".into() } else { "inf".into() };
    }
    format!("{:.*}", prec, x)
}

/// `f"{x:+.{prec}f}"`: like `fixed` with an explicit sign ("+0.00", "-1.50").
pub fn signed(x: f64, prec: usize) -> String {
    let s = fixed(x, prec);
    if s.starts_with('-') || x.is_nan() {
        s
    } else {
        format!("+{}", s)
    }
}

/// `round(x, ndigits)` for a float with ndigits given: CPython's correctly rounded algorithm
/// (round-half-even on the exact binary value, via dtoa), returning a float.
/// (Objects/floatobject.c float___round___impl / double_round)
pub fn pyround(x: f64, ndigits: i32) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    if ndigits > 323 {
        return x;
    }
    if ndigits < -308 {
        return 0.0 * x;
    }
    if ndigits >= 0 {
        let s = format!("{:.*}", ndigits as usize, x);
        return s.parse::<f64>().unwrap();
    }
    // negative ndigits: round to a multiple of 10**k, exactly
    let k = (-ndigits) as usize;
    let bits = x.to_bits();
    let neg = x < 0.0;
    let ex = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (m, e) = if ex == 0 { (frac, -1074) } else { (frac | (1 << 52), ex - 1075) };
    let mut q10 = Big::from_u64(1);
    let ten = Big::from_u64(10);
    for _ in 0..k {
        q10 = q10.mul(&ten);
    }
    let (num, den) = if e >= 0 {
        (Big::from_u64(m).shl(e as usize), q10.clone())
    } else {
        (Big::from_u64(m), q10.shl((-e) as usize))
    };
    let (mut q, r) = num.divrem(&den);
    let twice = r.shl(1);
    match twice.cmp(&den) {
        Ordering::Greater => q = q.add(&Big::from_u64(1)),
        Ordering::Equal => {
            if q.bit(0) {
                q = q.add(&Big::from_u64(1));
            }
        }
        Ordering::Less => {}
    }
    let v = q.mul(&q10);
    let f = v.to_decimal().parse::<f64>().unwrap();
    if neg {
        -f
    } else {
        f
    }
}

/// `round(x)` with no ndigits: nearest integer, ties to even, returned as an int.
pub fn pyround_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}
