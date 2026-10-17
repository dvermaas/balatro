//! `random.Random` (MT19937) with CPython 3.14's seeding and sampling algorithms.

pub struct Random {
    // MT19937 state
}

impl Random {
    /// `random.Random(n)` for an int seed (init_by_array over |n|'s 32-bit words).
    pub fn new_int(seed: i64) -> Self {
        let _ = seed;
        todo!("pycompat::random::Random::new_int")
    }

    /// `random.Random(s)` for a str seed (version 2: s.encode() + sha512(s.encode()) as a big int).
    pub fn new_str(seed: &str) -> Self {
        let _ = seed;
        todo!("pycompat::random::Random::new_str")
    }

    /// `random()`: 53-bit float in [0, 1).
    pub fn random(&mut self) -> f64 {
        todo!("pycompat::random::Random::random")
    }

    /// `getrandbits(k)` for k <= 64.
    pub fn getrandbits(&mut self, k: u32) -> u64 {
        let _ = k;
        todo!("pycompat::random::Random::getrandbits")
    }

    /// `_randbelow(n)` (getrandbits rejection sampling), the core of randrange/choice/sample/shuffle.
    pub fn randbelow(&mut self, n: u64) -> u64 {
        let _ = n;
        todo!("pycompat::random::Random::randbelow")
    }

    /// `randrange(start, stop)`.
    pub fn randrange(&mut self, start: i64, stop: i64) -> i64 {
        let _ = (start, stop);
        todo!("pycompat::random::Random::randrange")
    }

    /// `choice(seq)`: the index chosen.
    pub fn choice_index(&mut self, len: usize) -> usize {
        let _ = len;
        todo!("pycompat::random::Random::choice_index")
    }

    /// `shuffle(x)` in place.
    pub fn shuffle<T>(&mut self, x: &mut [T]) {
        let _ = x;
        todo!("pycompat::random::Random::shuffle")
    }

    /// `sample(range(n), k)`: the sampled indices in CPython's order (it switches between the pool
    /// and the selected-set algorithm by n and k). `sample(population, k)` = these indices mapped.
    pub fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        let _ = (n, k);
        todo!("pycompat::random::Random::sample_indices")
    }
}
