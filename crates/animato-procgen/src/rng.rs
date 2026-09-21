//! Deterministic RNG (splitmix64-based) for sampling routines.
//!
//! `animato-procgen` deliberately avoids a `rand` dependency: procedural
//! placement must be reproducible for seekable playback, and a tiny embedded
//! generator keeps the crate dependency-free (MIT/Apache-only tree).

/// Minimal deterministic 64-bit generator with `f32` range helpers.
#[derive(Clone, Debug)]
pub struct SmallRng(u64);

impl SmallRng {
    /// Seed the generator. Any `u64` is valid; `0` is mixed to a nonzero state.
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }

    /// Next `u64` via splitmix64.
    pub fn next_u64(&mut self) -> u64 {
        let mut z = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.0 = z;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform `f32` in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        // Take the top 24 bits into an f32 mantissa.
        let bits = (self.next_u64() >> 40) as u32;
        (bits as f32) / (1u32 << 24) as f32
    }

    /// Uniform `f32` in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }

    /// Uniform index in `[0, n)`. Returns `0` when `n == 0`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = SmallRng::new(42);
        let mut b = SmallRng::new(42);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn f32_in_unit_range() {
        let mut rng = SmallRng::new(7);
        for _ in 0..256 {
            let v = rng.next_f32();
            assert!(v >= 0.0 && v < 1.0, "out of range: {v}");
        }
    }
}
