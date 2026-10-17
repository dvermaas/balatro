//! `random.Random` (MT19937) with CPython 3.14's seeding and sampling algorithms.
//! (Modules/_randommodule.c, Lib/random.py)

use sha2::{Digest, Sha512};
use std::collections::HashSet;

const N: usize = 624;
const M: usize = 397;

pub struct Random {
    mt: [u32; N],
    index: usize,
}

impl Random {
    fn init_genrand(s: u32) -> Random {
        let mut mt = [0u32; N];
        mt[0] = s;
        for i in 1..N {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        Random { mt, index: N }
    }

    fn init_by_array(key: &[u32]) -> Random {
        let mut r = Random::init_genrand(19650218);
        let mut i = 1usize;
        let mut j = 0usize;
        let mut k = N.max(key.len());
        while k > 0 {
            r.mt[i] = (r.mt[i] ^ (r.mt[i - 1] ^ (r.mt[i - 1] >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                r.mt[0] = r.mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            r.mt[i] = (r.mt[i] ^ (r.mt[i - 1] ^ (r.mt[i - 1] >> 30)).wrapping_mul(1566083941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                r.mt[0] = r.mt[N - 1];
                i = 1;
            }
            k -= 1;
        }
        r.mt[0] = 0x80000000;
        r.index = N;
        r
    }

    fn genrand_u32(&mut self) -> u32 {
        if self.index >= N {
            const UPPER: u32 = 0x80000000;
            const LOWER: u32 = 0x7fffffff;
            let mag = |y: u32| if y & 1 == 1 { 0x9908b0dfu32 } else { 0 };
            for kk in 0..N - M {
                let y = (self.mt[kk] & UPPER) | (self.mt[kk + 1] & LOWER);
                self.mt[kk] = self.mt[kk + M] ^ (y >> 1) ^ mag(y);
            }
            for kk in N - M..N - 1 {
                let y = (self.mt[kk] & UPPER) | (self.mt[kk + 1] & LOWER);
                self.mt[kk] = self.mt[kk + M - N] ^ (y >> 1) ^ mag(y);
            }
            let y = (self.mt[N - 1] & UPPER) | (self.mt[0] & LOWER);
            self.mt[N - 1] = self.mt[M - 1] ^ (y >> 1) ^ mag(y);
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c5680;
        y ^= (y << 15) & 0xefc60000;
        y ^= y >> 18;
        y
    }

    /// `random.Random(n)` for an int seed (init_by_array over |n|'s 32-bit words).
    pub fn new_int(seed: i64) -> Self {
        let a = seed.unsigned_abs();
        let key: Vec<u32> = if a >> 32 != 0 {
            vec![a as u32, (a >> 32) as u32]
        } else {
            vec![a as u32]
        };
        Random::init_by_array(&key)
    }

    /// `random.Random(s)` for a str seed (version 2: s.encode() + sha512(s.encode()) as a big int).
    pub fn new_str(seed: &str) -> Self {
        let mut bytes = seed.as_bytes().to_vec();
        let digest = Sha512::digest(seed.as_bytes());
        bytes.extend_from_slice(&digest);
        // big-endian bytes -> little-endian 32-bit words, high zero words dropped
        bytes.reverse();
        let mut key: Vec<u32> = bytes
            .chunks(4)
            .map(|c| {
                let mut w = [0u8; 4];
                w[..c.len()].copy_from_slice(c);
                u32::from_le_bytes(w)
            })
            .collect();
        while key.len() > 1 && *key.last().unwrap() == 0 {
            key.pop();
        }
        Random::init_by_array(&key)
    }

    /// `random()`: 53-bit float in [0, 1).
    pub fn random(&mut self) -> f64 {
        let a = (self.genrand_u32() >> 5) as f64;
        let b = (self.genrand_u32() >> 6) as f64;
        (a * 67108864.0 + b) * (1.0 / 9007199254740992.0)
    }

    /// `getrandbits(k)` for k <= 64.
    pub fn getrandbits(&mut self, k: u32) -> u64 {
        assert!(k <= 64, "getrandbits: k > 64 not supported");
        if k == 0 {
            return 0;
        }
        if k <= 32 {
            return (self.genrand_u32() >> (32 - k)) as u64;
        }
        let lo = self.genrand_u32() as u64;
        let mut hi = self.genrand_u32();
        let rem = k - 32;
        if rem < 32 {
            hi >>= 32 - rem;
        }
        lo | ((hi as u64) << 32)
    }

    /// `_randbelow(n)` (getrandbits rejection sampling), the core of randrange/choice/sample/shuffle.
    pub fn randbelow(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        let k = 64 - n.leading_zeros();
        let mut r = self.getrandbits(k);
        while r >= n {
            r = self.getrandbits(k);
        }
        r
    }

    /// `randrange(start, stop)`.
    pub fn randrange(&mut self, start: i64, stop: i64) -> i64 {
        let width = stop as i128 - start as i128;
        assert!(width > 0, "empty range in randrange({}, {})", start, stop);
        (start as i128 + self.randbelow(width as u64) as i128) as i64
    }

    /// `choice(seq)`: the index chosen.
    pub fn choice_index(&mut self, len: usize) -> usize {
        assert!(len > 0, "Cannot choose from an empty sequence");
        self.randbelow(len as u64) as usize
    }

    /// `shuffle(x)` in place.
    pub fn shuffle<T>(&mut self, x: &mut [T]) {
        for i in (1..x.len()).rev() {
            let j = self.randbelow(i as u64 + 1) as usize;
            x.swap(i, j);
        }
    }

    /// `sample(range(n), k)`: the sampled indices in CPython's order (it switches between the pool
    /// and the selected-set algorithm by n and k). `sample(population, k)` = these indices mapped.
    pub fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        assert!(k <= n, "Sample larger than population or is negative");
        let mut result = Vec::with_capacity(k);
        let mut setsize: f64 = 21.0;
        if k > 5 {
            // 4 ** _ceil(_log(k * 3, 4))
            let e = (((k * 3) as f64).ln() / 4f64.ln()).ceil();
            setsize += 4f64.powf(e);
        }
        if (n as f64) <= setsize {
            let mut pool: Vec<usize> = (0..n).collect();
            for i in 0..k {
                let j = self.randbelow((n - i) as u64) as usize;
                result.push(pool[j]);
                pool[j] = pool[n - i - 1];
            }
        } else {
            let mut selected: HashSet<usize> = HashSet::new();
            for _ in 0..k {
                let mut j = self.randbelow(n as u64) as usize;
                while selected.contains(&j) {
                    j = self.randbelow(n as u64) as usize;
                }
                selected.insert(j);
                result.push(j);
            }
        }
        result
    }
}
