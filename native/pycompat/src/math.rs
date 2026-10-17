//! `math.fsum`, `math.comb`.

/// `math.fsum(xs)`: CPython's exact (Shewchuk / msum) summation, correctly rounded.
pub fn fsum(xs: &[f64]) -> f64 {
    let _ = xs;
    todo!("pycompat::math::fsum")
}

/// `math.comb(n, k)`, exact. Panics on overflow of u128.
pub fn comb(n: u64, k: u64) -> u128 {
    let _ = (n, k);
    todo!("pycompat::math::comb")
}
