//! The player model of one session (core spec, section 4:
//! <https://github.com/Vehicoule/Vehicoule/blob/main/docs/core-playback-spec.md>).
//!
//! A [`Player`] moves between the [`State`]s of the spec on [`Event`]s, and answers each with the [`Command`]s the
//! edges must run, as the spec's table of transitions says.
//!
//! The position is never polled. The platform player reports where it is when something changes, and the core keeps
//! that report as an [`Anchor`], from which it computes the position at any instant.

mod machine;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;

pub use machine::{
    Command, Commands, Deadlines, Event, Failure, Player, Renewal, Request, State, Timer,
};

use crate::time::Instant;
use std::time::Duration;

/// Thousandths of normal speed in normal speed.
const NORMAL_THOUSANDTHS: u32 = 1000;

/// How fast the media advances, in thousandths of normal speed: 1000 is normal speed, 0 does not advance.
///
/// Thousandths rather than a float, so that the position is computed exactly and the same on every platform (core
/// spec, section 1). The edge converts the platform player's rate when it reports it.
///
/// ```
/// use vehicoule_core::player::Rate;
///
/// assert_eq!(Rate::NORMAL.thousandths(), 1000);
/// assert_eq!(Rate::from_thousandths(1500).thousandths(), 1500);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rate(u32);

impl Rate {
    /// Normal speed.
    pub const NORMAL: Self = Self(NORMAL_THOUSANDTHS);

    /// A rate of `thousandths` thousandths of normal speed.
    #[must_use]
    pub const fn from_thousandths(thousandths: u32) -> Self {
        Self(thousandths)
    }

    /// The rate in thousandths of normal speed.
    #[must_use]
    pub const fn thousandths(self) -> u32 {
        self.0
    }
}

impl Default for Rate {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// What the platform player last reported: at `instant`, the media was at `position` and advancing at `rate`.
///
/// The player reports when something changes (start, pause, seek, rate change, buffering, a discontinuity), never on
/// a timer (core spec, section 4).
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::player::{Anchor, Rate};
/// use vehicoule_core::time::Instant;
///
/// let reported = Instant::from_origin(Duration::from_secs(100));
/// let anchor = Anchor::new(reported, Duration::from_secs(30), Rate::NORMAL);
///
/// let now = Instant::from_origin(Duration::from_secs(105));
/// assert_eq!(anchor.position_at(now, Some(Duration::from_secs(240))), Duration::from_secs(35));
/// // Never beyond the media's duration.
/// assert_eq!(anchor.position_at(now, Some(Duration::from_secs(32))), Duration::from_secs(32));
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    instant: Instant,
    position: Duration,
    rate: Rate,
}

impl Anchor {
    /// A report that at `instant`, the media was at `position` and advancing at `rate`.
    #[must_use]
    pub const fn new(instant: Instant, position: Duration, rate: Rate) -> Self {
        Self {
            instant,
            position,
            rate,
        }
    }

    /// When the player reported.
    #[must_use]
    pub const fn instant(&self) -> Instant {
        self.instant
    }

    /// Where the media was when the player reported.
    #[must_use]
    pub const fn position(&self) -> Duration {
        self.position
    }

    /// How fast the media was advancing when the player reported.
    #[must_use]
    pub const fn rate(&self) -> Rate {
        self.rate
    }

    /// The position at `now`: the reported position, plus the time since the report at the reported rate, and never
    /// beyond `duration` when the media's duration is known.
    ///
    /// An instant before the report gives the reported position: the position does not go back in time. A position
    /// too far to represent is [`Duration::MAX`], then clamped to `duration`.
    #[must_use]
    pub fn position_at(&self, now: Instant, duration: Option<Duration>) -> Duration {
        let elapsed = now.saturating_since(self.instant);
        // Normal speed, the common case, adds the time since the report as it is. `scale` gives the same result,
        // but with no multiplication or division Kani can prove that the position never goes backwards at rate 1
        // (core spec, section 4) for any instants.
        let advanced = if self.rate == Rate::NORMAL {
            elapsed
        } else {
            scale(elapsed, self.rate)
        };
        let position = self.position.saturating_add(advanced);
        duration.map_or(position, |duration| position.min(duration))
    }
}

/// `elapsed × rate`, rounded down to the nanosecond, or [`Duration::MAX`] when that is too far to represent.
///
/// `elapsed` is split into a thousand times `whole` plus `rest`, under a microsecond, so that only a result too far to
/// represent saturates: multiplying `elapsed` by the thousandths first would overflow a thousand times too early.
fn scale(elapsed: Duration, rate: Rate) -> Duration {
    let whole = elapsed
        .checked_div(NORMAL_THOUSANDTHS)
        .unwrap_or(Duration::ZERO);
    let rest = elapsed.saturating_sub(whole.saturating_mul(NORMAL_THOUSANDTHS));
    let scaled_rest = rest
        .saturating_mul(rate.0)
        .checked_div(NORMAL_THOUSANDTHS)
        .unwrap_or(Duration::ZERO);
    whole.saturating_mul(rate.0).saturating_add(scaled_rest)
}
