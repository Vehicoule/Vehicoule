//! Kani proofs of the position invariants (core spec, section 4, Position).
//!
//! The position is a `Duration`, so it is never negative by its type. `the_position_is_never_beyond_the_known_duration`
//! holds for any report, any rate, any instant and any duration, and shows that computing the position never panics.
//! `the_position_never_goes_backwards` is the spec's invariant at rate 1, for any report, any instants and any
//! duration. At other rates, the position is scaled by a multiplication and a division, and the same proof did not
//! finish in 25 minutes locally; the property test `the_position_never_goes_backwards_between_two_reports` checks
//! those rates instead.

use super::*;

fn any_duration() -> Duration {
    let nanos: u32 = kani::any();
    kani::assume(nanos < 1_000_000_000);
    Duration::new(kani::any(), nanos)
}

fn any_instant() -> Instant {
    Instant::from_origin(any_duration())
}

fn any_anchor() -> Anchor {
    Anchor::new(
        any_instant(),
        any_duration(),
        Rate::from_thousandths(kani::any()),
    )
}

/// The computed position is never beyond the known duration.
#[kani::proof]
fn the_position_is_never_beyond_the_known_duration() {
    let anchor = any_anchor();
    let duration = any_duration();
    assert!(anchor.position_at(any_instant(), Some(duration)) <= duration);
}

/// Between two reports while playing at rate 1, the computed position never goes backwards, whether the duration is
/// known or not.
#[kani::proof]
fn the_position_never_goes_backwards() {
    let anchor = Anchor::new(any_instant(), any_duration(), Rate::NORMAL);
    let duration = if kani::any() {
        Some(any_duration())
    } else {
        None
    };
    let earlier = any_instant();
    let later = any_instant();
    kani::assume(earlier <= later);
    assert!(anchor.position_at(earlier, duration) <= anchor.position_at(later, duration));
}
