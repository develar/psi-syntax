//! Python's `random.Random`, draw for draw: the Mersenne Twister (MT19937) seeded as `random.seed(a)` seeds it
//! (version 2: a string as the big-endian integer of its UTF-8 bytes followed by their SHA-512, an integer by its
//! 32-bit words), with the methods the mutation generators draw with, each as CPython 3.12+ computes it
//! (`Lib/random.py`, `Modules/_randommodule.c`). The corpora of psi-syntax's oracles were first made by Python scripts
//! with string seeds (`java-mut`, `java-lex`); with this generator their Rust ports write the same files, so the
//! cached oracle answers for them stay valid.
use sha2::Digest as _;

const N: usize = 624;
const M: usize = 397;

pub struct PyRandom {
    mt: [u32; N],
    index: usize,
}

impl PyRandom {
    /// `random.Random(seed)` for a string seed.
    pub fn from_str_seed(seed: &str) -> Self {
        let mut bytes = seed.as_bytes().to_vec();
        bytes.extend_from_slice(&sha2::Sha512::digest(seed.as_bytes()));
        Self::from_big_endian(&bytes)
    }

    /// `random.Random(seed)` for an integer seed (its absolute value, as Python takes it).
    pub fn from_int_seed(seed: i64) -> Self {
        Self::from_big_endian(&seed.unsigned_abs().to_be_bytes())
    }

    /// `init_by_array` with the 32-bit words of the non-negative integer whose big-endian bytes are `bytes`, least
    /// significant first, as `random_seed` converts an int (`_PyLong_AsByteArray`, little endian); zero is one word 0.
    fn from_big_endian(bytes: &[u8]) -> Self {
        let significant = &bytes[bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len())..];
        let mut little: Vec<u8> = significant.iter().rev().copied().collect();
        while !little.len().is_multiple_of(4) {
            little.push(0);
        }
        let mut key: Vec<u32> = little.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        if key.is_empty() {
            key.push(0);
        }
        let mut r = PyRandom { mt: [0; N], index: N };
        r.init_by_array(&key);
        r
    }

    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..N {
            self.mt[i] = 1812433253u32.wrapping_mul(self.mt[i - 1] ^ (self.mt[i - 1] >> 30)).wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            let prev = self.mt[i - 1] ^ (self.mt[i - 1] >> 30);
            self.mt[i] = (self.mt[i] ^ prev.wrapping_mul(1664525)).wrapping_add(key[j]).wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let prev = self.mt[i - 1] ^ (self.mt[i - 1] >> 30);
            self.mt[i] = (self.mt[i] ^ prev.wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
        }
        self.mt[0] = 0x8000_0000;
    }

    /// `genrand_uint32`.
    pub fn next_u32(&mut self) -> u32 {
        const UPPER: u32 = 0x8000_0000;
        const LOWER: u32 = 0x7fff_ffff;
        let mag = |y: u32| if y & 1 == 0 { 0 } else { 0x9908_b0df };
        if self.index >= N {
            for k in 0..N {
                let y = (self.mt[k] & UPPER) | (self.mt[(k + 1) % N] & LOWER);
                self.mt[k] = self.mt[(k + M) % N] ^ (y >> 1) ^ mag(y);
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `getrandbits(k)` for `k <= 64`: whole words least significant first, the last one's low bits dropped.
    pub fn getrandbits(&mut self, k: u32) -> u64 {
        assert!(k <= 64, "getrandbits({k}): at most 64 bits");
        if k == 0 {
            return 0;
        }
        if k <= 32 {
            return u64::from(self.next_u32() >> (32 - k));
        }
        let low = u64::from(self.next_u32());
        low | (u64::from(self.next_u32() >> (64 - k)) << 32)
    }

    /// `_randbelow(n)`: `getrandbits(n.bit_length())` until it is below `n`. `n > 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0)");
        let k = u64::BITS - n.leading_zeros();
        loop {
            let r = self.getrandbits(k);
            if r < n {
                return r;
            }
        }
    }

    /// An index below `n`, as `below`.
    pub fn index_below(&mut self, n: usize) -> usize {
        self.below(n as u64) as usize
    }

    /// `randrange(start, stop)`, `start < stop`.
    pub fn randrange(&mut self, start: i64, stop: i64) -> i64 {
        assert!(start < stop, "randrange({start}, {stop}): empty range");
        start + self.below(stop.abs_diff(start)) as i64
    }

    /// `randint(a, b)`: `a..=b`.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        self.randrange(a, b + 1)
    }

    /// `random()`: 53 bits from two words, in `[0, 1)`.
    pub fn random(&mut self) -> f64 {
        let a = f64::from(self.next_u32() >> 5);
        let b = f64::from(self.next_u32() >> 6);
        (a * 67108864.0 + b) * (1.0 / 9007199254740992.0)
    }

    /// `choice(items)`, items not empty.
    pub fn choice<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.index_below(items.len())]
    }

    /// `shuffle(items)` in place.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index_below(i + 1);
            items.swap(i, j);
        }
    }

    /// The indices `sample(range(n), k)` picks, in its order: a pool when a list of `n` is smaller than a set of
    /// `k` would be, else rejection against the picked set, as `Random.sample` chooses between them.
    pub fn sample(&mut self, n: usize, k: usize) -> Vec<usize> {
        assert!(k <= n, "sample larger than population");
        let mut setsize = 21usize;
        if k > 5 {
            setsize += 4usize.pow(((k * 3) as f64).log(4.0).ceil() as u32);
        }
        if n <= setsize {
            let mut pool: Vec<usize> = (0..n).collect();
            (0..k)
                .map(|i| {
                    let j = self.index_below(n - i);
                    let picked = pool[j];
                    pool[j] = pool[n - i - 1];
                    picked
                })
                .collect()
        } else {
            let mut selected = std::collections::HashSet::new();
            (0..k)
                .map(|_| loop {
                    let j = self.index_below(n);
                    if selected.insert(j) {
                        break j;
                    }
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The values CPython 3.14 prints for the same calls.
    #[test]
    fn draws_equal_cpythons() {
        let mut r = PyRandom::from_str_seed("java-lex");
        assert_eq!([r.next_u32(), r.next_u32(), r.next_u32()], [2095084774, 2788659671, 3412548949]);
        assert_eq!(r.random(), 0.2092462179396064);
        assert_eq!(r.randint(1, 4), 1);
        assert_eq!(r.randint(0, 100000), 52209);
        assert_eq!(r.getrandbits(40), 532944338211);

        let mut r = PyRandom::from_int_seed(1);
        assert_eq!([r.next_u32(), r.next_u32(), r.next_u32()], [577090037, 2444712010, 3639700191]);
        assert_eq!(r.random(), 0.8022650611681835);
        assert_eq!(r.randrange(-3, 4), -3);
        assert_eq!(PyRandom::from_int_seed(0).next_u32(), 3626764237);

        let mut r = PyRandom::from_str_seed("java-mut:parser-full/x.java:delete-token");
        assert_eq!(r.sample(19, 4), [16, 8, 14, 9]);
        assert_eq!(r.sample(100, 2), [93, 86]);
        assert_eq!(r.sample(3, 2), [1, 2]);
        let mut x: Vec<u32> = (0..10).collect();
        r.shuffle(&mut x);
        assert_eq!(x, [5, 9, 3, 4, 2, 8, 7, 6, 0, 1]);
        assert_eq!(r.sample(1000, 7), [464, 280, 743, 735, 546, 320, 127]);
    }
}
