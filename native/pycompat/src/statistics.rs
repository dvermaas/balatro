//! The `statistics` functions the bot uses. CPython computes these exactly (Fractions) and rounds
//! once, so a naive f64 sum is NOT identical. The result type follows Python: `mean` of ints is an
//! int when exact (e.g. mean([1, 3]) == 2), else a float.
//!
//! Exact arithmetic uses the small `math::Big` integer: every float is `m * 2**e`, so all inputs
//! share the denominator `2**K` and sums / sums of squares are exact big integers (no overflow
//! bound needed). Ints are the e = 0 case.

use crate::math::Big;

/// A Python number as `statistics` returns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PyNum {
    Int(i64),
    Float(f64),
}

impl PyNum {
    pub fn f64(self) -> f64 {
        match self {
            PyNum::Int(i) => i as f64,
            PyNum::Float(f) => f,
        }
    }
}

/// x = mant * 2**exp exactly (mant signed).
fn decode(x: f64) -> (i128, i32) {
    let bits = x.to_bits();
    let neg = (bits >> 63) != 0;
    let ex = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    let (m, e) = if ex == 0 { (frac, -1074) } else { (frac | (1 << 52), ex - 1075) };
    (if neg { -(m as i128) } else { m as i128 }, e)
}

/// Exact sums over values `mant * 2**exp`, scaled to the common exponent `-k` (k >= 0).
struct Exact {
    k: usize,
    pos: Big,
    neg: Big,
    sxx: Big,
    count: u64,
}

fn exact(vals: &[(i128, i32)]) -> Exact {
    let emin = vals.iter().map(|v| v.1).min().unwrap_or(0).min(0);
    let k = (-emin) as usize;
    let (mut pos, mut neg, mut sxx) = (Big::zero(), Big::zero(), Big::zero());
    for &(m, e) in vals {
        let sh = (e - emin) as usize;
        let mag = Big::from_u128(m.unsigned_abs());
        let t = mag.shl(sh);
        if m < 0 {
            neg = neg.add(&t);
        } else {
            pos = pos.add(&t);
        }
        sxx = sxx.add(&mag.mul(&mag).shl(2 * sh));
    }
    Exact { k, pos, neg, sxx, count: vals.len() as u64 }
}

fn mean_vals(vals: &[(i128, i32)]) -> f64 {
    assert!(!vals.is_empty(), "mean requires at least one data point");
    let ex = exact(vals);
    let den = Big::from_u64(ex.count).shl(ex.k);
    if ex.pos.cmp(&ex.neg) == std::cmp::Ordering::Less {
        -(ex.neg.sub(&ex.pos).true_div(&den))
    } else {
        ex.pos.sub(&ex.neg).true_div(&den)
    }
}

// statistics.py _integer_sqrt_of_frac_rto
fn integer_sqrt_of_frac_rto(n: &Big, m: &Big) -> Big {
    let a = n.divrem(m).0.isqrt();
    if a.mul(&a).mul(m) != *n {
        // a | 1
        let mut a = a;
        if a.l.is_empty() {
            a.l.push(1);
        } else {
            a.l[0] |= 1;
        }
        a
    } else {
        a
    }
}

// statistics.py _float_sqrt_of_frac
fn float_sqrt_of_frac(n: &Big, m: &Big) -> f64 {
    let q: i64 = (n.bit_length() as i64 - m.bit_length() as i64 - 109).div_euclid(2);
    if q >= 0 {
        let numerator = integer_sqrt_of_frac_rto(n, &m.shl(2 * q as usize)).shl(q as usize);
        numerator.true_div(&Big::from_u64(1))
    } else {
        let numerator = integer_sqrt_of_frac_rto(&n.shl((-2 * q) as usize), m);
        numerator.true_div(&Big::from_u64(1).shl((-q) as usize))
    }
}

fn stdev_vals(vals: &[(i128, i32)]) -> f64 {
    assert!(vals.len() >= 2, "stdev requires at least two data points");
    let ex = exact(vals);
    let c = Big::from_u64(ex.count);
    // sx = (pos - neg) / 2**k; only sx**2 is needed
    let sx_mag = if ex.pos.cmp(&ex.neg) == std::cmp::Ordering::Less {
        ex.neg.sub(&ex.pos)
    } else {
        ex.pos.sub(&ex.neg)
    };
    // ssd = (count*sxx - sx*sx) / count ; mss = ssd / (n - 1)
    let num = c.mul(&ex.sxx).sub(&sx_mag.mul(&sx_mag));
    let den = c.mul(&Big::from_u64(ex.count - 1)).shl(2 * ex.k);
    // Fraction normalisation
    let (n, m) = if num.is_zero() {
        (Big::zero(), Big::from_u64(1))
    } else {
        let g = num.gcd(&den);
        (num.divrem(&g).0, den.divrem(&g).0)
    };
    float_sqrt_of_frac(&n, &m)
}

/// `statistics.mean` over ints.
pub fn mean_int(xs: &[i64]) -> PyNum {
    assert!(!xs.is_empty(), "mean requires at least one data point");
    let total: i128 = xs.iter().map(|&x| x as i128).sum();
    let n = xs.len() as i128;
    if total % n == 0 {
        return PyNum::Int((total / n) as i64);
    }
    let den = Big::from_u64(n as u64);
    let mag = Big::from_u128(total.unsigned_abs());
    let f = mag.true_div(&den);
    PyNum::Float(if total < 0 { -f } else { f })
}

/// `statistics.mean` over floats (exact sum of the binary values, one rounding).
pub fn mean_f64(xs: &[f64]) -> f64 {
    if xs.iter().any(|x| !x.is_finite()) {
        // inf / nan: CPython's partials[None] path; sum of the non-finite values
        let s: f64 = xs.iter().filter(|x| !x.is_finite()).sum();
        return s / xs.len() as f64;
    }
    let vals: Vec<(i128, i32)> = xs.iter().map(|&x| decode(x)).collect();
    mean_vals(&vals)
}

/// `statistics.stdev` over ints (sample SD; CPython 3.14's exact sum of squares and correctly
/// rounded square root).
pub fn stdev_int(xs: &[i64]) -> f64 {
    let vals: Vec<(i128, i32)> = xs.iter().map(|&x| (x as i128, 0)).collect();
    stdev_vals(&vals)
}

/// `statistics.stdev` over floats.
pub fn stdev_f64(xs: &[f64]) -> f64 {
    let vals: Vec<(i128, i32)> = xs.iter().map(|&x| decode(x)).collect();
    stdev_vals(&vals)
}

/// `statistics.median` over ints (an int for odd n; for even n `(a + b) / 2`, a float).
pub fn median_int(xs: &[i64]) -> PyNum {
    let mut d = xs.to_vec();
    d.sort();
    let n = d.len();
    assert!(n > 0, "no median for empty data");
    if n % 2 == 1 {
        PyNum::Int(d[n / 2])
    } else {
        let s = d[n / 2 - 1] as i128 + d[n / 2] as i128;
        let f = Big::from_u128(s.unsigned_abs()).true_div(&Big::from_u64(2));
        PyNum::Float(if s < 0 { -f } else { f })
    }
}

/// `statistics.median` over floats.
pub fn median_f64(xs: &[f64]) -> f64 {
    let mut d = xs.to_vec();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = d.len();
    assert!(n > 0, "no median for empty data");
    if n % 2 == 1 {
        d[n / 2]
    } else {
        (d[n / 2 - 1] + d[n / 2]) / 2.0
    }
}
