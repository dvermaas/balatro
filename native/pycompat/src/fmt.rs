//! Python float formatting and rounding.

/// `str(x)` / `repr(x)` / `f"{x}"` for a Python float: shortest round-trip digits, always with a
/// decimal point or exponent ("3.0", "0.1", "1e-05", "1e+16", "inf", "nan", "-0.0").
pub fn pyfloat(x: f64) -> String {
    let _ = x;
    todo!("pycompat::fmt::pyfloat")
}

/// `f"{x:.{prec}f}"` / `format(x, f".{prec}f")`: correctly rounded from the exact binary value,
/// ties to even (CPython's dtoa mode 3). Handles "-0.00" like Python.
pub fn fixed(x: f64, prec: usize) -> String {
    let _ = (x, prec);
    todo!("pycompat::fmt::fixed")
}

/// `f"{x:+.{prec}f}"`: like `fixed` with an explicit sign ("+0.00", "-1.50").
pub fn signed(x: f64, prec: usize) -> String {
    let _ = (x, prec);
    todo!("pycompat::fmt::signed")
}

/// `round(x, ndigits)` for a float with ndigits given: CPython's correctly rounded algorithm
/// (round-half-even on the exact binary value, via dtoa), returning a float.
pub fn pyround(x: f64, ndigits: i32) -> f64 {
    let _ = (x, ndigits);
    todo!("pycompat::fmt::pyround")
}

/// `round(x)` with no ndigits: nearest integer, ties to even, returned as an int.
pub fn pyround_int(x: f64) -> i64 {
    let _ = x;
    todo!("pycompat::fmt::pyround_int")
}
