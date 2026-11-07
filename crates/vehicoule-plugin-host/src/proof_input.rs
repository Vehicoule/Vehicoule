//! Inputs for the Kani proofs of the host's checks (ADR 0014).

/// Any bytes, at most `N` of them. The checks are proved on bytes: building a `str` from symbolic bytes makes Kani
/// validate UTF-8, which it cannot do in reasonable time, and proving every byte sequence covers every string.
pub(crate) struct AnyBytes<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> AnyBytes<N> {
    pub(crate) fn new() -> Self {
        let bytes: [u8; N] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= N);
        Self { bytes, len }
    }

    pub(crate) fn get(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
