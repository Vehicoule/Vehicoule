//! Under Kani, shuffle draws any value: the queue proofs then cover every order a shuffle can produce, not only the
//! orders of the seeds Kani picks, and avoid solving 64-bit multiplications. The generator itself is checked against
//! its reference outputs by the tests in `rng.rs`.
//!
//! Kept in its own file so that mutation testing, which runs without Kani, can leave it out (`.cargo/mutants.toml`).

use super::rng::SplitMix64;

impl SplitMix64 {
    /// Any value in `0..bound`, or `None` when `bound` is zero.
    pub(crate) fn below(&mut self, bound: usize) -> Option<usize> {
        let value: usize = kani::any();
        kani::assume(value < bound);
        (bound > 0).then_some(value)
    }
}
