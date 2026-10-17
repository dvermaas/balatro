//! `math.fsum`, `math.comb`, plus a small unsigned big-integer (`Big`) used by `statistics` and
//! `fmt::pyround` for exact rational arithmetic (no bigint crate needed).

use std::cmp::Ordering;

/// `math.fsum(xs)`: CPython's exact (Shewchuk / msum) summation, correctly rounded.
/// (Modules/mathmodule.c math_fsum)
pub fn fsum(xs: &[f64]) -> f64 {
    let mut p: Vec<f64> = Vec::new();
    let mut special_sum = 0.0f64;
    let mut inf_sum = 0.0f64;
    for &xv in xs {
        let xsave = xv;
        let mut x = xv;
        let mut i = 0usize;
        for j in 0..p.len() {
            let mut y = p[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let yr = hi - x;
            let lo = y - yr;
            if lo != 0.0 {
                p[i] = lo;
                i += 1;
            }
            x = hi;
        }
        p.truncate(i);
        if x != 0.0 {
            if !x.is_finite() {
                // intermediate overflow would raise OverflowError in Python; not reproduced
                if xsave.is_infinite() {
                    inf_sum += xsave;
                }
                special_sum += xsave;
                p.clear();
            } else {
                p.push(x);
            }
        }
    }
    if special_sum != 0.0 {
        if inf_sum.is_nan() {
            return f64::NAN;
        }
        return special_sum;
    }
    let mut n = p.len();
    let mut hi = 0.0f64;
    if n > 0 {
        n -= 1;
        hi = p[n];
        let mut lo = 0.0f64;
        while n > 0 {
            let x = hi;
            n -= 1;
            let y = p[n];
            hi = x + y;
            let yr = hi - x;
            lo = y - yr;
            if lo != 0.0 {
                break;
            }
        }
        // Make half-even rounding work across multiple partials.
        if n > 0 && ((lo < 0.0 && p[n - 1] < 0.0) || (lo > 0.0 && p[n - 1] > 0.0)) {
            let y = lo * 2.0;
            let x = hi + y;
            let yr = x - hi;
            if y == yr {
                hi = x;
            }
        }
    }
    hi
}

/// `math.comb(n, k)`, exact. Panics on overflow of u128.
pub fn comb(n: u64, k: u64) -> u128 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut r: u128 = 1;
    for i in 1..=k as u128 {
        r = r
            .checked_mul(n as u128 - k as u128 + i)
            .expect("comb overflow")
            / i;
    }
    r
}

/// Unsigned big integer, little-endian u32 limbs, no trailing zero limbs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Big {
    pub l: Vec<u32>,
}

impl Big {
    pub fn zero() -> Big {
        Big { l: vec![] }
    }
    pub fn from_u128(mut v: u128) -> Big {
        let mut l = vec![];
        while v != 0 {
            l.push(v as u32);
            v >>= 32;
        }
        Big { l }
    }
    pub fn from_u64(v: u64) -> Big {
        Big::from_u128(v as u128)
    }
    fn norm(mut self) -> Big {
        while self.l.last() == Some(&0) {
            self.l.pop();
        }
        self
    }
    pub fn is_zero(&self) -> bool {
        self.l.is_empty()
    }
    pub fn bit_length(&self) -> usize {
        match self.l.last() {
            None => 0,
            Some(&t) => (self.l.len() - 1) * 32 + (32 - t.leading_zeros() as usize),
        }
    }
    pub fn bit(&self, i: usize) -> bool {
        let w = i / 32;
        w < self.l.len() && (self.l[w] >> (i % 32)) & 1 == 1
    }
    /// True if any of the low `k` bits is set.
    pub fn low_bits_nonzero(&self, k: usize) -> bool {
        let full = k / 32;
        for w in 0..full.min(self.l.len()) {
            if self.l[w] != 0 {
                return true;
            }
        }
        let r = k % 32;
        r != 0 && full < self.l.len() && (self.l[full] & ((1u32 << r) - 1)) != 0
    }
    pub fn shl(&self, n: usize) -> Big {
        if self.is_zero() {
            return Big::zero();
        }
        let (w, b) = (n / 32, n % 32);
        let mut l = vec![0u32; w];
        if b == 0 {
            l.extend_from_slice(&self.l);
        } else {
            let mut carry = 0u32;
            for &x in &self.l {
                l.push((x << b) | carry);
                carry = x >> (32 - b);
            }
            if carry != 0 {
                l.push(carry);
            }
        }
        Big { l }
    }
    pub fn shr(&self, n: usize) -> Big {
        let (w, b) = (n / 32, n % 32);
        if w >= self.l.len() {
            return Big::zero();
        }
        let src = &self.l[w..];
        let mut l = Vec::with_capacity(src.len());
        for i in 0..src.len() {
            let lo = src[i] >> b;
            let hi = if b != 0 && i + 1 < src.len() { src[i + 1] << (32 - b) } else { 0 };
            l.push(lo | hi);
        }
        Big { l }.norm()
    }
    pub fn add(&self, o: &Big) -> Big {
        let n = self.l.len().max(o.l.len());
        let mut l = Vec::with_capacity(n + 1);
        let mut c = 0u64;
        for i in 0..n {
            let s = *self.l.get(i).unwrap_or(&0) as u64 + *o.l.get(i).unwrap_or(&0) as u64 + c;
            l.push(s as u32);
            c = s >> 32;
        }
        if c != 0 {
            l.push(c as u32);
        }
        Big { l }
    }
    /// self - o, requires self >= o.
    pub fn sub(&self, o: &Big) -> Big {
        let mut l = Vec::with_capacity(self.l.len());
        let mut borrow = 0i64;
        for i in 0..self.l.len() {
            let mut d = self.l[i] as i64 - *o.l.get(i).unwrap_or(&0) as i64 - borrow;
            if d < 0 {
                d += 1 << 32;
                borrow = 1;
            } else {
                borrow = 0;
            }
            l.push(d as u32);
        }
        assert!(borrow == 0, "Big::sub underflow");
        Big { l }.norm()
    }
    pub fn mul(&self, o: &Big) -> Big {
        if self.is_zero() || o.is_zero() {
            return Big::zero();
        }
        let mut l = vec![0u32; self.l.len() + o.l.len()];
        for (i, &a) in self.l.iter().enumerate() {
            let mut c = 0u64;
            for (j, &b) in o.l.iter().enumerate() {
                let t = l[i + j] as u64 + a as u64 * b as u64 + c;
                l[i + j] = t as u32;
                c = t >> 32;
            }
            l[i + o.l.len()] = c as u32;
        }
        Big { l }.norm()
    }
    pub fn cmp(&self, o: &Big) -> Ordering {
        if self.l.len() != o.l.len() {
            return self.l.len().cmp(&o.l.len());
        }
        for i in (0..self.l.len()).rev() {
            if self.l[i] != o.l[i] {
                return self.l[i].cmp(&o.l[i]);
            }
        }
        Ordering::Equal
    }
    /// (quotient, remainder) by binary long division.
    pub fn divrem(&self, d: &Big) -> (Big, Big) {
        assert!(!d.is_zero(), "division by zero");
        if self.cmp(d) == Ordering::Less {
            return (Big::zero(), self.clone());
        }
        let nb = self.bit_length();
        let mut q = vec![0u32; self.l.len()];
        let db = d.bit_length();
        // the top db-1 bits of self are < d: start the remainder there, loop over the rest
        let mut r = self.shr(nb - (db - 1));
        for i in (0..=(nb - db)).rev() {
            r = r.shl(1);
            if self.bit(i) {
                if r.l.is_empty() {
                    r.l.push(1);
                } else {
                    r.l[0] |= 1;
                }
            }
            if r.cmp(d) != Ordering::Less {
                r = r.sub(d);
                q[i / 32] |= 1 << (i % 32);
            }
        }
        (Big { l: q }.norm(), r)
    }
    pub fn isqrt(&self) -> Big {
        if self.is_zero() {
            return Big::zero();
        }
        let mut x = Big::from_u64(1).shl((self.bit_length() + 1) / 2);
        loop {
            let y = x.add(&self.divrem(&x).0).shr(1);
            if y.cmp(&x) != Ordering::Less {
                return x;
            }
            x = y;
        }
    }
    pub fn gcd(&self, o: &Big) -> Big {
        let (mut a, mut b) = (self.clone(), o.clone());
        while !b.is_zero() {
            let r = a.divrem(&b).1;
            a = b;
            b = r;
        }
        a
    }
    /// Low 64 bits.
    pub fn low_u64(&self) -> u64 {
        let a = *self.l.first().unwrap_or(&0) as u64;
        let b = *self.l.get(1).unwrap_or(&0) as u64;
        a | (b << 32)
    }
    pub fn to_decimal(&self) -> String {
        if self.is_zero() {
            return "0".into();
        }
        let mut parts = vec![];
        let mut cur = self.clone();
        let ten9 = Big::from_u64(1_000_000_000);
        while !cur.is_zero() {
            let (q, r) = cur.divrem(&ten9);
            parts.push(r.low_u64());
            cur = q;
        }
        let mut s = format!("{}", parts.pop().unwrap());
        while let Some(p) = parts.pop() {
            s.push_str(&format!("{:09}", p));
        }
        s
    }
    /// `a / b` as Python int true division: correctly rounded (half-even), subnormals handled.
    /// Overflow gives infinity (Python raises OverflowError).
    pub fn true_div(&self, b: &Big) -> f64 {
        assert!(!b.is_zero(), "division by zero");
        if self.is_zero() {
            return 0.0;
        }
        let ab = self.bit_length() as i64;
        let bb = b.bit_length() as i64;
        let s = 56 + bb - ab + 1;
        let (q, rem_nonzero) = if s >= 0 {
            let (q, r) = self.shl(s as usize).divrem(b);
            (q, !r.is_zero())
        } else {
            let (q, r) = self.divrem(&b.shl((-s) as usize));
            (q, !r.is_zero())
        };
        let ql = q.bit_length() as i64;
        let e = ql - 1 - s;
        let lsb_exp = (e - 52).max(-1074);
        let drop = lsb_exp + s;
        debug_assert!(drop >= 1);
        let drop = drop as usize;
        let round_bit = q.bit(drop - 1);
        let sticky = rem_nonzero || q.low_bits_nonzero(drop - 1);
        let mut m = q.shr(drop).low_u64();
        if round_bit && (sticky || m & 1 == 1) {
            m += 1;
        }
        let scale = if lsb_exp > 1023 {
            f64::INFINITY
        } else if lsb_exp >= -1022 {
            f64::from_bits(((lsb_exp + 1023) as u64) << 52)
        } else {
            f64::from_bits(1u64 << (lsb_exp + 1074))
        };
        m as f64 * scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basics() {
        let a = Big::from_u128(123456789012345678901234567890u128);
        let b = Big::from_u128(987654321);
        let (q, r) = a.divrem(&b);
        assert_eq!(q.mul(&b).add(&r), a);
        assert_eq!(Big::from_u64(1).true_div(&Big::from_u64(3)), 1.0 / 3.0);
        assert_eq!(Big::from_u64(10).isqrt().low_u64(), 3);
    }
}
