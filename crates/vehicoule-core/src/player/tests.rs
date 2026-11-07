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
fn a_scaled_position_that_fits_does_not_saturate() {
    // A thousand times the elapsed time overflows a `Duration`, but 1.001 times it fits.
    let long = Duration::from_secs(u64::MAX / 2);
    let expected = reference_scale(long, 1_001);
    assert!(expected > long && expected < Duration::MAX);
    let barely_faster = Anchor::new(at(0), Duration::ZERO, Rate::from_thousandths(1_001));
    assert_eq!(
        barely_faster.position_at(Instant::from_origin(long), None),
        expected
    );
    let normal = Anchor::new(at(0), Duration::ZERO, Rate::NORMAL);
    assert_eq!(normal.position_at(Instant::from_origin(long), None), long);
}

/// `elapsed × thousandths / 1000` in nanoseconds on `u128`, where nothing overflows, saturated to `Duration::MAX`.
fn reference_scale(elapsed: Duration, thousandths: u32) -> Duration {
    let nanos = elapsed
        .as_nanos()
        .checked_mul(u128::from(thousandths))
        .unwrap()
        / 1_000;
    if nanos > Duration::MAX.as_nanos() {
        return Duration::MAX;
    }
    let secs = u64::try_from(nanos / 1_000_000_000).unwrap();
    let subsec = u32::try_from(nanos % 1_000_000_000).unwrap();
    Duration::new(secs, subsec)
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
    fn the_time_since_the_report_is_scaled_exactly_or_saturates_only_beyond_the_maximum(
        elapsed in any_duration(),
        rate in any::<u32>(),
    ) {
        let anchor = Anchor::new(Instant::default(), Duration::ZERO, Rate::from_thousandths(rate));
        prop_assert_eq!(anchor.position_at(Instant::from_origin(elapsed), None), reference_scale(elapsed, rate));
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

// The player's transitions (core spec, section 4, Events and transitions). Entries are numbers, sources strings.

type TestPlayer = Player<u8, &'static str>;
type TestState = State<u8, &'static str>;
type TestEvent = Event<u8, &'static str>;
type TestCommand = Command<u8, &'static str>;

const SOURCE: &str = "source";
const NONE: [TestCommand; 0] = [];
/// The request the states built below wait for when resolving.
const AWAITED: Request = Request(3);
/// The number of the next request of a player built below.
const NEXT: Request = Request(4);

fn commands(player: &mut TestPlayer, now: u64, event: TestEvent) -> Vec<TestCommand> {
    player.handle(at(now), event).into_iter().collect()
}

fn player_in(state: TestState) -> TestPlayer {
    let mut player = TestPlayer::new();
    player.state = state;
    player.next = NEXT;
    player
}

fn playing(entry: u8, reported: u64, position: u64) -> TestState {
    State::Playing {
        entry,
        anchor: Anchor::new(at(reported), ms(position), Rate::NORMAL),
        duration: Some(ms(100_000)),
    }
}

fn paused(entry: u8, position: u64) -> TestState {
    State::Paused {
        entry,
        position: ms(position),
        duration: Some(ms(100_000)),
    }
}

fn buffering(entry: u8, position: u64) -> TestState {
    State::Buffering {
        entry,
        position: ms(position),
        duration: Some(ms(100_000)),
    }
}

fn loading(entry: u8, from: u64) -> TestState {
    State::Loading {
        entry,
        source: SOURCE,
        from: ms(from),
        duration: Some(ms(100_000)),
    }
}

fn resolving(entry: u8, from: u64) -> TestState {
    resolving_for(entry, from, AWAITED)
}

fn resolving_for(entry: u8, from: u64, request: Request) -> TestState {
    State::Resolving {
        entry,
        from: ms(from),
        request,
    }
}

fn stopped(entry: u8, position: u64) -> TestState {
    State::Stopped {
        entry,
        position: ms(position),
    }
}

/// One state of each kind, all on entry 1.
fn every_state() -> Vec<TestState> {
    vec![
        State::Idle,
        resolving(1, 0),
        loading(1, 0),
        playing(1, 0, 0),
        paused(1, 5_000),
        buffering(1, 5_000),
        stopped(1, 5_000),
    ]
}

#[test]
fn a_new_player_is_idle() {
    let player = TestPlayer::new();
    assert_eq!(player.state(), &State::Idle);
    assert_eq!(player.state().entry(), None);
    assert_eq!(player.state().position_at(at(0)), None);
}

#[test]
fn start_resolves_the_entry_from_zero_and_releases_a_loaded_source() {
    for state in every_state() {
        let had_source = matches!(
            state,
            State::Loading { .. }
                | State::Playing { .. }
                | State::Paused { .. }
                | State::Buffering { .. }
        );
        let mut player = player_in(state.clone());
        let sent = commands(&mut player, 9_000, Event::Start { entry: 2 });
        let resolve = Command::Resolve {
            entry: 2,
            request: NEXT,
        };
        let expected = if had_source {
            vec![Command::Stop, resolve]
        } else {
            vec![resolve]
        };
        assert_eq!(sent, expected, "from {state:?}");
        assert_eq!(player.state(), &resolving_for(2, 0, NEXT), "from {state:?}");
    }
}

#[test]
fn the_answer_to_the_awaited_request_loads_its_source_from_where_it_starts() {
    let mut player = player_in(resolving(1, 4_000));
    let sent = commands(
        &mut player,
        0,
        Event::Resolved {
            request: AWAITED,
            source: SOURCE,
            duration: Some(ms(100_000)),
        },
    );
    assert_eq!(
        sent,
        [Command::Load {
            source: SOURCE,
            from: ms(4_000)
        }]
    );
    assert_eq!(player.state(), &loading(1, 4_000));
}

#[test]
fn the_platform_playing_moves_loading_playing_and_buffering_to_playing_on_the_new_anchor() {
    let anchor = Anchor::new(at(3_000), ms(1_000), Rate::NORMAL);
    for state in [loading(1, 0), playing(1, 0, 0), buffering(1, 500)] {
        let mut player = player_in(state.clone());
        assert_eq!(
            commands(&mut player, 3_000, Event::Playing { entry: 1, anchor }),
            NONE
        );
        assert_eq!(
            player.state(),
            &State::Playing {
                entry: 1,
                anchor,
                duration: Some(ms(100_000))
            },
            "from {state:?}"
        );
    }
}

#[test]
fn pause_while_playing_keeps_the_position_at_that_instant() {
    let mut player = player_in(playing(1, 1_000, 2_000));
    assert_eq!(commands(&mut player, 4_500, Event::Pause), [Command::Pause]);
    assert_eq!(player.state(), &paused(1, 5_500));
}

#[test]
fn pause_keeps_the_position_within_the_duration() {
    let mut player = player_in(playing(1, 0, 99_000));
    commands(&mut player, 60_000, Event::Pause);
    assert_eq!(player.state(), &paused(1, 100_000));
}

#[test]
fn pause_while_buffering_keeps_the_position_it_waited_at() {
    let mut player = player_in(buffering(1, 7_000));
    assert_eq!(commands(&mut player, 0, Event::Pause), [Command::Pause]);
    assert_eq!(player.state(), &paused(1, 7_000));
}

#[test]
fn resume_while_paused_plays_and_waits_for_the_platform() {
    let mut player = player_in(paused(1, 7_000));
    assert_eq!(commands(&mut player, 0, Event::Resume), [Command::Play]);
    assert_eq!(player.state(), &buffering(1, 7_000));
}

#[test]
fn the_platform_paused_updates_the_paused_position() {
    let mut player = player_in(paused(1, 7_000));
    assert_eq!(
        commands(
            &mut player,
            0,
            Event::Paused {
                entry: 1,
                position: ms(7_040)
            }
        ),
        NONE
    );
    assert_eq!(player.state(), &paused(1, 7_040));
}

#[test]
fn the_platform_buffering_while_playing_waits_at_its_position() {
    let mut player = player_in(playing(1, 0, 0));
    let event = Event::Buffering {
        entry: 1,
        position: ms(12_000),
    };
    assert_eq!(commands(&mut player, 12_000, event), NONE);
    assert_eq!(player.state(), &buffering(1, 12_000));
}

#[test]
fn seek_while_playing_or_buffering_waits_for_the_platform_at_the_new_position() {
    for state in [playing(1, 0, 0), buffering(1, 500)] {
        let mut player = player_in(state.clone());
        let sent = commands(
            &mut player,
            0,
            Event::Seek {
                position: ms(30_000),
            },
        );
        assert_eq!(
            sent,
            [Command::Seek {
                position: ms(30_000)
            }],
            "from {state:?}"
        );
        assert_eq!(player.state(), &buffering(1, 30_000), "from {state:?}");
    }
}

#[test]
fn seek_while_paused_stays_paused_at_the_new_position() {
    let mut player = player_in(paused(1, 7_000));
    let sent = commands(
        &mut player,
        0,
        Event::Seek {
            position: ms(30_000),
        },
    );
    assert_eq!(
        sent,
        [Command::Seek {
            position: ms(30_000)
        }]
    );
    assert_eq!(player.state(), &paused(1, 30_000));
}

#[test]
fn seek_while_stopped_moves_where_it_resumes_and_sends_nothing() {
    let mut player = player_in(stopped(1, 7_000));
    assert_eq!(
        commands(
            &mut player,
            0,
            Event::Seek {
                position: ms(30_000)
            }
        ),
        NONE
    );
    assert_eq!(player.state(), &stopped(1, 30_000));
}

#[test]
fn stop_keeps_the_position_of_the_state_it_leaves_and_releases_a_loaded_source() {
    let cases = [
        (resolving(1, 4_000), 4_000, false),
        (loading(1, 4_000), 4_000, true),
        (playing(1, 1_000, 2_000), 5_500, true),
        (paused(1, 7_000), 7_000, true),
        (buffering(1, 7_000), 7_000, true),
    ];
    for (state, position, had_source) in cases {
        let mut player = player_in(state.clone());
        let sent = commands(&mut player, 4_500, Event::Stop);
        let expected = if had_source {
            vec![Command::Stop]
        } else {
            vec![]
        };
        assert_eq!(sent, expected, "from {state:?}");
        assert_eq!(player.state(), &stopped(1, position), "from {state:?}");
    }
}

#[test]
fn stop_changes_nothing_when_idle_or_already_stopped() {
    for state in [State::Idle, stopped(1, 7_000)] {
        let mut player = player_in(state.clone());
        assert_eq!(commands(&mut player, 0, Event::Stop), NONE);
        assert_eq!(player.state(), &state);
    }
}

#[test]
fn resume_while_stopped_resolves_the_entry_again_from_where_it_stopped() {
    let mut player = player_in(stopped(1, 7_000));
    assert_eq!(
        commands(&mut player, 0, Event::Resume),
        [Command::Resolve {
            entry: 1,
            request: NEXT
        }]
    );
    assert_eq!(player.state(), &resolving_for(1, 7_000, NEXT));
}

/// The request of the `Resolve` among `commands`.
fn request_of(commands: &[TestCommand]) -> Option<Request> {
    commands.iter().find_map(|command| match command {
        Command::Resolve { request, .. } => Some(*request),
        _ => None,
    })
}

fn answer(request: Request, source: &'static str) -> TestEvent {
    Event::Resolved {
        request,
        source,
        duration: None,
    }
}

#[test]
fn the_answer_to_a_request_made_before_a_stop_is_late_once_resumed() {
    let mut player = TestPlayer::new();
    let first = request_of(&commands(&mut player, 0, Event::Start { entry: 1 })).unwrap();
    commands(&mut player, 100, Event::Stop);
    let second = request_of(&commands(&mut player, 200, Event::Resume)).unwrap();
    assert_ne!(first, second);

    let before = player.clone();
    assert_eq!(commands(&mut player, 300, answer(first, "expired")), NONE);
    assert_eq!(player, before);

    assert_eq!(
        commands(&mut player, 400, answer(second, "fresh")),
        [Command::Load {
            source: "fresh",
            from: Duration::ZERO
        }]
    );
}

#[test]
fn the_answer_to_an_earlier_start_of_the_same_entry_is_late() {
    let mut player = TestPlayer::new();
    let first = request_of(&commands(&mut player, 0, Event::Start { entry: 1 })).unwrap();
    let second = request_of(&commands(&mut player, 100, Event::Start { entry: 1 })).unwrap();
    assert_ne!(first, second);

    let before = player.clone();
    assert_eq!(commands(&mut player, 200, answer(first, "earlier")), NONE);
    assert_eq!(player, before);
    assert_eq!(
        commands(&mut player, 300, answer(second, "awaited")),
        [Command::Load {
            source: "awaited",
            from: Duration::ZERO
        }]
    );
}

#[test]
fn the_request_counter_wraps_instead_of_overflowing() {
    let mut player = TestPlayer::new();
    player.next = Request(u64::MAX);
    assert_eq!(
        request_of(&commands(&mut player, 0, Event::Start { entry: 1 })),
        Some(Request(u64::MAX))
    );
    assert_eq!(player.next, Request(0));
}

#[test]
fn the_end_of_the_entry_while_playing_or_buffering_goes_idle_with_no_command() {
    for state in [playing(1, 0, 0), buffering(1, 500)] {
        let mut player = player_in(state.clone());
        assert_eq!(commands(&mut player, 0, Event::Ended { entry: 1 }), NONE);
        assert_eq!(player.state(), &State::Idle, "from {state:?}");
    }
}

/// Every event but `Start` and `Stop`, about entry `entry` or answering `request` where it names one.
fn events_about(entry: u8, request: Request) -> Vec<TestEvent> {
    vec![
        Event::Pause,
        Event::Resume,
        Event::Seek { position: ms(1) },
        Event::Resolved {
            request,
            source: SOURCE,
            duration: None,
        },
        Event::Playing {
            entry,
            anchor: Anchor::new(at(0), ms(1), Rate::NORMAL),
        },
        Event::Paused {
            entry,
            position: ms(1),
        },
        Event::Buffering {
            entry,
            position: ms(1),
        },
        Event::Ended { entry },
    ]
}

/// The (state kind, event kind) pairs of the spec's table, besides `Start` and `Stop`.
fn in_the_table(state: &TestState, event: &TestEvent) -> bool {
    matches!(
        (state, event),
        (State::Resolving { .. }, Event::Resolved { .. })
            | (
                State::Loading { .. } | State::Playing { .. } | State::Buffering { .. },
                Event::Playing { .. }
            )
            | (
                State::Playing { .. } | State::Buffering { .. },
                Event::Pause | Event::Seek { .. } | Event::Ended { .. }
            )
            | (
                State::Paused { .. },
                Event::Resume | Event::Paused { .. } | Event::Seek { .. }
            )
            | (State::Playing { .. }, Event::Buffering { .. })
            | (State::Stopped { .. }, Event::Seek { .. } | Event::Resume)
    )
}

#[test]
fn an_event_outside_the_table_changes_nothing_and_sends_nothing() {
    for state in every_state() {
        for event in events_about(1, AWAITED) {
            if in_the_table(&state, &event) {
                continue;
            }
            let mut player = player_in(state.clone());
            assert_eq!(
                commands(&mut player, 50_000, event.clone()),
                NONE,
                "{event:?} in {state:?}"
            );
            assert_eq!(player.state(), &state, "{event:?} in {state:?}");
        }
    }
}

#[test]
fn a_report_about_another_entry_or_an_answer_to_another_request_is_late_and_changes_nothing() {
    for state in every_state() {
        for event in events_about(2, NEXT) {
            if !matches!(
                event,
                Event::Resolved { .. }
                    | Event::Playing { .. }
                    | Event::Paused { .. }
                    | Event::Buffering { .. }
                    | Event::Ended { .. }
            ) {
                continue;
            }
            let mut player = player_in(state.clone());
            assert_eq!(
                commands(&mut player, 50_000, event.clone()),
                NONE,
                "{event:?} in {state:?}"
            );
            assert_eq!(player.state(), &state, "{event:?} in {state:?}");
        }
    }
}

#[test]
fn the_position_of_each_state() {
    assert_eq!(resolving(1, 4_000).position_at(at(9_000)), Some(ms(4_000)));
    assert_eq!(loading(1, 4_000).position_at(at(9_000)), Some(ms(4_000)));
    assert_eq!(
        playing(1, 1_000, 2_000).position_at(at(9_000)),
        Some(ms(10_000))
    );
    assert_eq!(paused(1, 7_000).position_at(at(9_000)), Some(ms(7_000)));
    assert_eq!(buffering(1, 7_000).position_at(at(9_000)), Some(ms(7_000)));
    assert_eq!(stopped(1, 7_000).position_at(at(9_000)), Some(ms(7_000)));
}

#[test]
fn commands_come_in_order_and_say_when_there_are_none() {
    let mut player = player_in(playing(1, 0, 0));
    let sent = player.handle(at(0), Event::Start { entry: 2 });
    assert!(!sent.is_empty());
    assert_eq!(
        sent.iter().collect::<Vec<_>>(),
        [
            &Command::Stop,
            &Command::Resolve {
                entry: 2,
                request: NEXT
            }
        ]
    );
    assert!(player.handle(at(0), Event::Resume).is_empty());
}

fn any_event() -> impl Strategy<Value = TestEvent> {
    let entry = 0..3_u8;
    let position = (0..200_000_u64).prop_map(ms);
    prop_oneof![
        entry.clone().prop_map(|entry| Event::Start { entry }),
        Just(Event::Pause),
        Just(Event::Resume),
        Just(Event::Stop),
        position
            .clone()
            .prop_map(|position| Event::Seek { position }),
        (0..4_u64, proptest::option::of(position.clone())).prop_map(|(request, duration)| {
            Event::Resolved {
                request: Request(request),
                source: SOURCE,
                duration,
            }
        }),
        (entry.clone(), position.clone(), 0..3_000_u32).prop_map(|(entry, position, rate)| {
            Event::Playing {
                entry,
                anchor: Anchor::new(at(0), position, Rate::from_thousandths(rate)),
            }
        }),
        (entry.clone(), position.clone())
            .prop_map(|(entry, position)| Event::Paused { entry, position }),
        (entry.clone(), position)
            .prop_map(|(entry, position)| Event::Buffering { entry, position }),
        entry.prop_map(|entry| Event::Ended { entry }),
    ]
}

proptest! {
    #[test]
    fn after_stop_nothing_plays_loads_or_resolves_until_a_user_starts_or_resumes(
        events in prop::collection::vec(any_event(), 0..64),
    ) {
        let mut player = TestPlayer::new();
        let mut stopped = false;
        for (now, event) in (0_u64..).step_by(100).zip(events) {
            let intent = matches!(event, Event::Start { .. } | Event::Resume);
            let is_stop = matches!(event, Event::Stop);
            let sent: Vec<_> = player.handle(at(now), event).into_iter().collect();
            if intent {
                stopped = false;
            }
            if stopped {
                let forbidden = sent
                    .iter()
                    .any(|c| matches!(c, Command::Play | Command::Load { .. } | Command::Resolve { .. }));
                prop_assert!(!forbidden, "sent {:?} after a stop", sent);
            }
            if is_stop {
                stopped = true;
            }
        }
    }

    #[test]
    fn the_entry_is_the_last_one_started_until_it_ends(
        events in prop::collection::vec(any_event(), 0..64),
    ) {
        let mut player = TestPlayer::new();
        let mut last_started = None;
        for (now, event) in (0_u64..).step_by(100).zip(events) {
            if let Event::Start { entry } = event {
                last_started = Some(entry);
            }
            player.handle(at(now), event);
            if let Some(entry) = player.state().entry() {
                prop_assert_eq!(Some(entry), last_started);
            }
        }
    }

    #[test]
    fn every_request_has_a_new_number(
        events in prop::collection::vec(any_event(), 0..64),
    ) {
        let mut player = TestPlayer::new();
        let mut seen = std::collections::HashSet::new();
        for (now, event) in (0_u64..).step_by(100).zip(events) {
            let sent: Vec<_> = player.handle(at(now), event).into_iter().collect();
            if let Some(request) = request_of(&sent) {
                prop_assert!(seen.insert(request), "{:?} sent twice", request);
            }
        }
    }
}

// Positions stay within the known duration (core spec, section 4, Events and transitions). The states built above
// know a duration of 100 s.

#[test]
fn a_seek_beyond_the_duration_goes_to_the_end() {
    let beyond = Event::Seek {
        position: ms(150_000),
    };
    let to_the_end = [Command::Seek {
        position: ms(100_000),
    }];
    for state in [playing(1, 0, 0), buffering(1, 500)] {
        let mut player = player_in(state.clone());
        assert_eq!(commands(&mut player, 0, beyond.clone()), to_the_end);
        assert_eq!(player.state(), &buffering(1, 100_000), "from {state:?}");
    }
    let mut player = player_in(paused(1, 7_000));
    assert_eq!(commands(&mut player, 0, beyond), to_the_end);
    assert_eq!(player.state(), &paused(1, 100_000));
}

#[test]
fn a_seek_with_no_known_duration_goes_where_asked() {
    let mut player = player_in(State::Paused {
        entry: 1,
        position: ms(7_000),
        duration: None,
    });
    let sent = commands(
        &mut player,
        0,
        Event::Seek {
            position: ms(150_000),
        },
    );
    assert_eq!(
        sent,
        [Command::Seek {
            position: ms(150_000)
        }]
    );
    assert_eq!(player.state().position_at(at(0)), Some(ms(150_000)));
}

#[test]
fn a_reported_position_beyond_the_duration_is_kept_at_the_end() {
    let mut player = player_in(paused(1, 7_000));
    let event = Event::Paused {
        entry: 1,
        position: ms(150_000),
    };
    commands(&mut player, 0, event);
    assert_eq!(player.state(), &paused(1, 100_000));

    let mut player = player_in(playing(1, 0, 0));
    let event = Event::Buffering {
        entry: 1,
        position: ms(150_000),
    };
    commands(&mut player, 0, event);
    assert_eq!(player.state(), &buffering(1, 100_000));
}

#[test]
fn a_stopped_position_beyond_the_new_duration_loads_from_the_end() {
    let mut player = player_in(resolving(1, 150_000));
    let sent = commands(
        &mut player,
        0,
        Event::Resolved {
            request: AWAITED,
            source: SOURCE,
            duration: Some(ms(100_000)),
        },
    );
    assert_eq!(
        sent,
        [Command::Load {
            source: SOURCE,
            from: ms(100_000)
        }]
    );
    assert_eq!(player.state(), &loading(1, 100_000));
}

/// The state's duration, when it keeps one.
fn duration_of(state: &TestState) -> Option<Duration> {
    match state {
        State::Loading { duration, .. }
        | State::Playing { duration, .. }
        | State::Paused { duration, .. }
        | State::Buffering { duration, .. } => *duration,
        _ => None,
    }
}

proptest! {
    #[test]
    fn no_position_kept_or_sent_is_beyond_the_known_duration(
        events in prop::collection::vec(any_event(), 0..64),
    ) {
        let mut player = TestPlayer::new();
        for (now, event) in (0_u64..).step_by(100).zip(events) {
            let sent: Vec<_> = player.handle(at(now), event).into_iter().collect();
            if let Some(duration) = duration_of(player.state()) {
                let position = player.state().position_at(at(now));
                prop_assert!(position <= Some(duration), "{:?} beyond {:?}", position, duration);
                for command in &sent {
                    if let Command::Load { from: position, .. } | Command::Seek { position } = command {
                        prop_assert!(*position <= duration, "{:?} beyond {:?}", command, duration);
                    }
                }
            }
        }
    }
}
