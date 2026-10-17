//! The `statistics` functions the bot uses. CPython computes these exactly (Fractions) and rounds
//! once, so a naive f64 sum is NOT identical. The result type follows Python: `mean` of ints is an
//! int when exact (e.g. mean([1, 3]) == 2), else a float.

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

/// `statistics.mean` over ints.
pub fn mean_int(xs: &[i64]) -> PyNum {
    let _ = xs;
    todo!("pycompat::statistics::mean_int")
}

/// `statistics.mean` over floats (exact sum of the binary values, one rounding).
pub fn mean_f64(xs: &[f64]) -> f64 {
    let _ = xs;
    todo!("pycompat::statistics::mean_f64")
}

/// `statistics.stdev` over ints (sample SD; CPython 3.14's exact sum of squares and correctly
/// rounded square root).
pub fn stdev_int(xs: &[i64]) -> f64 {
    let _ = xs;
    todo!("pycompat::statistics::stdev_int")
}

/// `statistics.stdev` over floats.
pub fn stdev_f64(xs: &[f64]) -> f64 {
    let _ = xs;
    todo!("pycompat::statistics::stdev_f64")
}

/// `statistics.median` over ints (an int for odd n; for even n `(a + b) / 2`, a float).
pub fn median_int(xs: &[i64]) -> PyNum {
    let _ = xs;
    todo!("pycompat::statistics::median_int")
}

/// `statistics.median` over floats.
pub fn median_f64(xs: &[f64]) -> f64 {
    let _ = xs;
    todo!("pycompat::statistics::median_f64")
}
