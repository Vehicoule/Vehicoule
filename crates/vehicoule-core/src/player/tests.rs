use super::*;
use proptest::prelude::*;

fn at(millis: u64) -> Instant {
    Instant::from_origin(Duration::from_millis(millis))
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn the_position_advances_with_the_time_since_the_report_at_normal_speed() {
    let anchor = Anchor::new(at(1_000), ms(30_000), Rate::NORMAL);
    assert_eq!(anchor.position_at(at(1_000), None), ms(30_000));
    assert_eq!(anchor.position_at(at(3_500), None), ms(32_500));
}

#[test]
fn the_position_advances_at_the_reported_rate() {
    let fast = Anchor::new(at(0), ms(10_000), Rate::from_thousandths(1_500));
    assert_eq!(fast.position_at(at(2_000), None), ms(13_000));
    let slow = Anchor::new(at(0), ms(10_000), Rate::from_thousandths(250));
    assert_eq!(slow.position_at(at(2_000), None), ms(10_500));
}

#[test]
fn at_rate_zero_the_position_stays_where_it_was_reported() {
    let anchor = Anchor::new(at(0), ms(42_000), Rate::from_thousandths(0));
    assert_eq!(anchor.position_at(at(60_000), None), ms(42_000));
}

#[test]
fn the_rate_keeps_sub_millisecond_precision() {
    // 1 ms at 1.001x is 1.001 ms: the thousandth is not rounded away.
    let anchor = Anchor::new(at(0), Duration::ZERO, Rate::from_thousandths(1_001));
    assert_eq!(
        anchor.position_at(at(1), None),
        Duration::from_micros(1_001)
    );
}

#[test]
fn an_instant_before_the_report_gives_the_reported_position() {
    let anchor = Anchor::new(at(5_000), ms(20_000), Rate::NORMAL);
    assert_eq!(anchor.position_at(at(4_000), None), ms(20_000));
    assert_eq!(anchor.position_at(at(0), None), ms(20_000));
}

#[test]
fn the_position_never_goes_beyond_the_known_duration() {
    let anchor = Anchor::new(at(0), ms(170_000), Rate::NORMAL);
    let duration = Some(ms(180_000));
    assert_eq!(anchor.position_at(at(9_999), duration), ms(179_999));
    assert_eq!(anchor.position_at(at(10_000), duration), ms(180_000));
    assert_eq!(anchor.position_at(at(60_000), duration), ms(180_000));
}

#[test]
fn a_reported_position_beyond_the_duration_is_clamped_to_it() {
    let anchor = Anchor::new(at(0), ms(200_000), Rate::NORMAL);
    assert_eq!(anchor.position_at(at(0), Some(ms(180_000))), ms(180_000));
}

#[test]
fn without_a_known_duration_the_position_is_not_clamped() {
    let anchor = Anchor::new(at(0), ms(200_000), Rate::NORMAL);
    assert_eq!(anchor.position_at(at(1_000), None), ms(201_000));
}

#[test]
fn a_position_too_far_to_represent_saturates_instead_of_wrapping() {
    let far = Instant::from_origin(Duration::MAX);
    let anchor = Anchor::new(at(0), ms(1), Rate::from_thousandths(u32::MAX));
    assert_eq!(anchor.position_at(far, None), Duration::MAX);
    assert_eq!(anchor.position_at(far, Some(ms(5_000))), ms(5_000));
    let near_the_end = Anchor::new(at(0), Duration::MAX, Rate::NORMAL);
    assert_eq!(near_the_end.position_at(at(1_000), None), Duration::MAX);
}

#[test]
fn at_normal_speed_the_position_is_exact_where_scaling_would_overflow() {
    // A thousand times this many seconds does not fit in a `Duration`; at normal speed nothing is scaled.
    let long = Duration::from_secs(u64::MAX / 2);
    let anchor = Anchor::new(at(0), Duration::ZERO, Rate::NORMAL);
    assert_eq!(anchor.position_at(Instant::from_origin(long), None), long);
    let barely_faster = Anchor::new(at(0), Duration::ZERO, Rate::from_thousandths(1_001));
    assert_eq!(
        barely_faster.position_at(Instant::from_origin(long), None),
        Duration::MAX
    );
}

#[test]
fn an_anchor_gives_back_what_was_reported() {
    let anchor = Anchor::new(at(7), ms(8), Rate::from_thousandths(9));
    assert_eq!(anchor.instant(), at(7));
    assert_eq!(anchor.position(), ms(8));
    assert_eq!(anchor.rate(), Rate::from_thousandths(9));
}

#[test]
fn the_default_rate_is_normal_speed() {
    assert_eq!(Rate::default(), Rate::NORMAL);
    assert_eq!(Rate::NORMAL.thousandths(), 1_000);
}

#[test]
fn instants_measure_the_time_between_them_and_never_a_negative_one() {
    assert_eq!(at(1_500).since_origin(), ms(1_500));
    assert_eq!(at(1_500).saturating_since(at(500)), ms(1_000));
    assert_eq!(at(500).saturating_since(at(1_500)), Duration::ZERO);
    assert_eq!(Instant::default(), at(0));
}

fn any_duration() -> impl Strategy<Value = Duration> {
    (any::<u64>(), 0..1_000_000_000_u32).prop_map(|(secs, nanos)| Duration::new(secs, nanos))
}

proptest! {
    #[test]
    fn the_position_never_goes_backwards_between_two_reports(
        reported in any_duration(),
        position in any_duration(),
        rate in any::<u32>(),
        first in any_duration(),
        gap in any_duration(),
        duration in proptest::option::of(any_duration()),
    ) {
        let anchor = Anchor::new(Instant::from_origin(reported), position, Rate::from_thousandths(rate));
        let later = first.saturating_add(gap);
        prop_assert!(
            anchor.position_at(Instant::from_origin(first), duration)
                <= anchor.position_at(Instant::from_origin(later), duration)
        );
    }

    #[test]
    fn the_position_is_never_beyond_the_known_duration(
        reported in any_duration(),
        position in any_duration(),
        rate in any::<u32>(),
        now in any_duration(),
        duration in any_duration(),
    ) {
        let anchor = Anchor::new(Instant::from_origin(reported), position, Rate::from_thousandths(rate));
        prop_assert!(anchor.position_at(Instant::from_origin(now), Some(duration)) <= duration);
    }

    #[test]
    fn at_normal_speed_the_position_advances_by_exactly_the_time_elapsed(
        reported in 0..u64::MAX / 4,
        position in 0..u64::MAX / 4,
        elapsed in 0..u64::MAX / 4,
    ) {
        let anchor = Anchor::new(Instant::from_origin(Duration::from_nanos(reported)), Duration::from_nanos(position), Rate::NORMAL);
        let now = Instant::from_origin(Duration::from_nanos(reported.checked_add(elapsed).unwrap()));
        prop_assert_eq!(anchor.position_at(now, None), Duration::from_nanos(position.checked_add(elapsed).unwrap()));
    }
}
