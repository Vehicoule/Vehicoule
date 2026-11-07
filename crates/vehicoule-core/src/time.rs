//! The core's time (core spec, section 1: <https://github.com/Vehicoule/Vehicoule/blob/main/docs/core-playback-spec.md>).
//!
//! The core never reads a clock: every event comes with the instant the runner read on its monotonic clock. This is
//! the core's own type, not `std::time::Instant`, which a test or a Kani proof cannot build at a chosen value.

use std::time::Duration;

/// A moment on the runner's monotonic clock: the time since an origin the runner chose.
///
/// Only the difference between two instants of the same runner means something.
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::time::Instant;
///
/// let start = Instant::from_origin(Duration::from_secs(10));
/// let later = Instant::from_origin(Duration::from_secs(12));
/// assert_eq!(later.saturating_since(start), Duration::from_secs(2));
/// assert_eq!(start.saturating_since(later), Duration::ZERO);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant(Duration);

impl Instant {
    /// The instant `since_origin` after the runner's origin.
    #[must_use]
    pub const fn from_origin(since_origin: Duration) -> Self {
        Self(since_origin)
    }

    /// The time since the runner's origin.
    #[must_use]
    pub const fn since_origin(self) -> Duration {
        self.0
    }

    /// The time from `earlier` to `self`, or zero when `earlier` is not earlier.
    #[must_use]
    pub fn saturating_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}
