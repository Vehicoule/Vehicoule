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

// The player's ownership (core spec, section 4, Ownership). Entries and sources are bytes; every value is symbolic.

fn any_option_duration() -> Option<Duration> {
    if kani::any() {
        Some(any_duration())
    } else {
        None
    }
}

fn any_renewal() -> Renewal {
    if kani::any() {
        Renewal::Available
    } else {
        Renewal::Spent
    }
}

fn any_state() -> State<u8, u8> {
    let entry: u8 = kani::any();
    match kani::any::<u8>() % 7 {
        0 => State::Idle,
        1 => State::Resolving {
            entry,
            from: any_duration(),
            request: Request(kani::any()),
            timer: Timer(kani::any()),
            renewal: any_renewal(),
        },
        2 => State::Loading {
            entry,
            source: kani::any(),
            from: any_duration(),
            duration: any_option_duration(),
            timer: Timer(kani::any()),
            renewal: any_renewal(),
        },
        3 => State::Playing {
            entry,
            anchor: any_anchor(),
            duration: any_option_duration(),
            renewal: any_renewal(),
        },
        4 => State::Paused {
            entry,
            position: any_duration(),
            duration: any_option_duration(),
            renewal: any_renewal(),
        },
        5 => State::Buffering {
            entry,
            position: any_duration(),
            duration: any_option_duration(),
            timer: Timer(kani::any()),
            renewal: any_renewal(),
        },
        _ => State::Stopped {
            entry,
            position: any_duration(),
        },
    }
}

/// Any event but `Start` and `Resume`, the user intents that end the guarantee after a stop.
fn any_event_but_start_or_resume() -> Event<u8, u8> {
    let entry: u8 = kani::any();
    match kani::any::<u8>() % 12 {
        0 => Event::Pause,
        1 => Event::Stop,
        2 => Event::Seek {
            position: any_duration(),
        },
        3 => Event::Resolved {
            request: Request(kani::any()),
            source: kani::any(),
            duration: any_option_duration(),
        },
        4 => Event::Playing {
            entry,
            anchor: any_anchor(),
        },
        5 => Event::Paused {
            entry,
            position: any_duration(),
        },
        6 => Event::Buffering {
            entry,
            position: any_duration(),
        },
        7 => Event::Ended { entry },
        8 => Event::ResolveFailed {
            request: Request(kani::any()),
        },
        9 => Event::Failed { entry },
        10 => Event::Expired { entry },
        _ => Event::TimerFired {
            timer: Timer(kani::any()),
        },
    }
}

/// A player in `state`, with any numbers for its next request and timer, and any deadlines.
fn any_player(state: State<u8, u8>) -> Player<u8, u8> {
    Player {
        state,
        next: Request(kani::any()),
        next_timer: Timer(kani::any()),
        deadlines: Deadlines::new(any_duration(), any_duration(), any_duration()),
    }
}

fn plays_loads_or_resolves(commands: &Commands<u8, u8>) -> bool {
    commands.iter().any(|command| {
        matches!(
            command,
            Command::Play | Command::Load { .. } | Command::Resolve { .. }
        )
    })
}

/// `Stop` from any state leaves the player stopped, or idle when it had no entry, and sends nothing but `Stop`.
#[kani::proof]
fn stop_leaves_the_player_stopped_or_idle() {
    let mut player = any_player(any_state());
    let entry = player.state.entry();
    let commands = player.handle(any_instant(), Event::Stop);
    let expected = match entry {
        Some(entry) => matches!(player.state, State::Stopped { entry: e, .. } if e == entry),
        None => matches!(player.state, State::Idle),
    };
    assert!(expected);
    assert!(
        commands
            .iter()
            .all(|command| matches!(command, Command::Stop))
    );
}

/// Once stopped or idle, any event but a user's `Start` or `Resume` keeps the player stopped or idle on the same
/// entry, and sends no `Play`, `Load` or `Resolve`. With the proof above, by induction on the events: after `Stop`, the
/// core sends none of them until a new user intent, however many events come.
#[kani::proof]
fn after_stop_nothing_plays_loads_or_resolves_until_a_user_intent() {
    let state = any_state();
    kani::assume(matches!(state, State::Stopped { .. } | State::Idle));
    let entry = state.entry();
    let mut player = any_player(state);
    let commands = player.handle(any_instant(), any_event_but_start_or_resume());
    assert!(matches!(player.state, State::Stopped { .. } | State::Idle));
    assert!(player.state.entry() == entry);
    assert!(!plays_loads_or_resolves(&commands));
}

/// A report, a failure or an expiry about another entry than the player's, an answer or a failure of another request
/// than the one it waits for, and a timer it does not wait with, change nothing and send nothing.
#[kani::proof]
fn a_late_event_changes_nothing() {
    let state = any_state();
    let mut player = any_player(state);
    let before = player.clone();
    let event = any_event_but_start_or_resume();
    let late = match event {
        Event::Resolved { request, .. } | Event::ResolveFailed { request } => !matches!(
            player.state,
            State::Resolving { request: awaited, .. } if awaited == request
        ),
        Event::Playing { entry, .. }
        | Event::Paused { entry, .. }
        | Event::Buffering { entry, .. }
        | Event::Ended { entry }
        | Event::Failed { entry }
        | Event::Expired { entry } => player.state.entry() != Some(entry),
        Event::TimerFired { timer } => player.state.timer() != Some(timer),
        _ => return,
    };
    kani::assume(late);
    let commands = player.handle(any_instant(), event);
    assert!(player == before);
    assert!(commands.is_empty());
}

/// Any event, `Start` and `Resume` included.
fn any_event() -> Event<u8, u8> {
    match kani::any::<u8>() % 3 {
        0 => Event::Start { entry: kani::any() },
        1 => Event::Resume,
        _ => any_event_but_start_or_resume(),
    }
}

/// From any state, an event that sends a `Resolve` numbers it with the player's next request, waits for that request,
/// and moves the next request on; an event that sends none keeps the next request. So no two requests a player makes in
/// a row of fewer than 2^64 share a number.
#[kani::proof]
fn each_request_takes_the_next_number() {
    let mut player = any_player(any_state());
    let next = player.next;
    let commands = player.handle(any_instant(), any_event());
    let sent = commands.iter().find_map(|command| match command {
        Command::Resolve { request, .. } => Some(*request),
        _ => None,
    });
    match sent {
        Some(request) => {
            assert!(request == next);
            assert!(
                matches!(player.state, State::Resolving { request: awaited, .. } if awaited == next)
            );
            assert!(player.next == next.following());
        }
        None => {
            assert!(player.next == next);
        }
    }
}

/// From any state, an event that arms a timer numbers it with the player's next timer, waits with it, and moves the
/// next timer on; an event that arms none keeps the next timer, and leaves the player waiting with the timer it waited
/// with before, if any. So the player waits with the last timer it armed, and no two timers share a number.
#[kani::proof]
fn each_wait_takes_the_next_timer() {
    let mut player = any_player(any_state());
    let next = player.next_timer;
    let waited = player.state.timer();
    let commands = player.handle(any_instant(), any_event());
    let armed = commands.iter().find_map(|command| match command {
        Command::ArmTimer { timer, .. } => Some(*timer),
        _ => None,
    });
    match armed {
        Some(timer) => {
            assert!(timer == next);
            assert!(player.state.timer() == Some(next));
            assert!(player.next_timer == next.following());
        }
        None => {
            assert!(player.next_timer == next);
            if let Some(timer) = player.state.timer() {
                assert!(waited == Some(timer));
            }
        }
    }
}

/// From a state whose renewal is spent, any event but a user's `Start` or `Resume` sends no `Resolve`, and leaves the
/// renewal spent or the player without one (`Idle`, `Stopped`). With `after_stop_nothing_plays_loads_or_resolves_until_a_user_intent`,
/// by induction on the events: between two user intents, an entry gets at most one new resolution after an expiry.
#[kani::proof]
fn a_spent_renewal_stays_spent_until_a_user_intent() {
    let state = any_state();
    kani::assume(state.renewal() == Some(Renewal::Spent));
    let mut player = any_player(state);
    let commands = player.handle(any_instant(), any_event_but_start_or_resume());
    assert!(matches!(
        player.state.renewal(),
        Some(Renewal::Spent) | None
    ));
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, Command::Resolve { .. }))
    );
}

// Positions within the known duration (core spec, section 4, Events and transitions).

/// The duration `state` keeps, if any.
fn duration_of(state: &State<u8, u8>) -> Option<Duration> {
    match state {
        State::Loading { duration, .. }
        | State::Playing { duration, .. }
        | State::Paused { duration, .. }
        | State::Buffering { duration, .. } => *duration,
        _ => None,
    }
}

/// Whether the position `state` keeps is within the duration it knows. A playing position is computed, and clamped
/// when computed (`the_position_is_never_beyond_the_known_duration`).
fn within_its_duration(state: &State<u8, u8>) -> bool {
    match state {
        State::Loading {
            from: position,
            duration: Some(duration),
            ..
        }
        | State::Paused {
            position,
            duration: Some(duration),
            ..
        }
        | State::Buffering {
            position,
            duration: Some(duration),
            ..
        } => position <= duration,
        _ => true,
    }
}

/// From a state whose position is within its known duration, any event keeps it within the duration it then knows, and
/// sends no position beyond it. With `the_position_is_never_beyond_the_known_duration`, by induction on the events: the
/// player never sends or shows a position beyond the known duration. An event sends at most three commands, so four
/// unwindings of the loop over them suffice, which Kani checks.
#[kani::proof]
#[kani::unwind(4)]
fn positions_stay_within_the_known_duration() {
    let state = any_state();
    kani::assume(within_its_duration(&state));
    let mut player = any_player(state);
    let commands = player.handle(any_instant(), any_event());
    assert!(within_its_duration(&player.state));
    if let Some(duration) = duration_of(&player.state) {
        for command in commands.iter() {
            if let Command::Load { from: position, .. } | Command::Seek { position } = command {
                assert!(*position <= duration);
            }
        }
    }
}
