//! The deterministic generator behind shuffle.
//!
//! The core never draws randomness itself (ADR 0003): the seed comes in with the event that turns shuffle on, and the
//! same seed always gives the same order. `SplitMix64` is ten lines with no dependency, and its quality is ample for
//! ordering a queue.

/// `SplitMix64`, as published by Sebastiano Vigna (<https://prng.di.unimi.it/splitmix.c>, read on 2026-11-06), from
/// Steele, Lea and Flood, "Fast splittable pseudorandom number generators" (OOPSLA 2014). The three constants and the
/// shifts are that algorithm's, not tuning values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[cfg_attr(
        kani,
        expect(dead_code, reason = "under Kani, `below` draws any value instead")
    )]
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..bound`, or `None` when `bound` is zero. The modulo bias is below `bound / 2^64`, invisible for a
    /// queue.
    /// Under Kani, `kani_rng.rs` replaces this function.
    #[cfg(not(kani))]
    pub(crate) fn below(&mut self, bound: usize) -> Option<usize> {
        let bound = u64::try_from(bound).ok()?;
        let value = self.next_u64().checked_rem(bound)?;
        usize::try_from(value).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first outputs for seed 1234567 (an arbitrary seed), computed by a separate Python transcription of
    /// `splitmix.c`: catches any change to a constant, a shift or the order of operations.
    #[test]
    fn matches_the_reference_sequence() {
        let mut rng = SplitMix64::new(1_234_567);
        let got: Vec<u64> = (0..5).map(|_| rng.next_u64()).collect();
        assert_eq!(
            got,
            [
                6_457_827_717_110_365_317,
                3_203_168_211_198_807_973,
                9_817_491_932_198_370_423,
                4_593_380_528_125_082_431,
                16_408_922_859_458_223_821,
            ]
        );
    }

    #[test]
    fn below_stays_in_range_and_refuses_zero() {
        let mut rng = SplitMix64::new(7);
        assert_eq!(rng.below(0), None);
        for bound in 1..50 {
            let value = rng.below(bound);
            assert!(value.is_some_and(|v| v < bound));
        }
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut rng = SplitMix64::new(99);
        for _ in 0..10 {
            assert_eq!(rng.below(1), Some(0));
        }
    }
}
