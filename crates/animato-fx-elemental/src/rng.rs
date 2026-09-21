//! Deterministic RNG (splitmix64) for spike placement.
//!
//! The original sandbox uses `Math.random()` at spawn time. A seekable
//! pipeline must reproduce the exact same cast for a given seed, so placement
//! randomness comes from this tiny embedded generator instead of the system
//! RNG. No `rand` dependency; `no_std`-compatible.

/// Minimal deterministic 64-bit generator with `f32` range helpers.
#[derive(Clone, Debug)]
pub struct FxRng(u64);

impl FxRng {
    /// Seed the generator. Any `u64` is valid; the seed is mixed so `0` works.
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
        let bits = (self.next_u64() >> 40) as u32;
        (bits as f32) / (1u32 << 24) as f32
    }

    /// Uniform `f32` in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = FxRng::new(7);
        let mut b = FxRng::new(7);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_stays_in_bounds() {
        let mut rng = FxRng::new(1);
        for _ in 0..256 {
            let v = rng.range(-1.0, 1.0);
            assert!((-1.0..1.0).contains(&v));
        }
    }
}
