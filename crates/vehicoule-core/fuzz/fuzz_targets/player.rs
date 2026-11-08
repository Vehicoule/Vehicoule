//! Runs any sequence of events on a player: never a panic, and after every event the player keeps the rules of the
//! core spec (sections 4 and 5) that hold whatever the sequence.
//!
//! The bytes are read as events one after another until they run out, at instants that only move forward. Entries are
//! small numbers, so reports often name the player's entry and often another one. A resolution, or a failed one,
//! answers one of the requests the player has made so far, often the awaited one and often an earlier one; a timer that
//! fires is one of the timers the player has armed so far, likewise.

#![no_main]

use libfuzzer_sys::arbitrary::{Result, Unstructured};
use libfuzzer_sys::fuzz_target;
use std::time::Duration;
use vehicoule_core::player::{
    Anchor, Command, Deadlines, Event, Player, Rate, Renewal, Request, State, Timer,
};
use vehicoule_core::time::Instant;

fn duration(u: &mut Unstructured<'_>) -> Result<Duration> {
    Ok(Duration::from_millis(u.int_in_range(0..=600_000)?))
}

/// The player's deadlines, short enough that the instants often pass them.
const DEADLINES: Deadlines = Deadlines::new(
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(30),
);

fn event(
    u: &mut Unstructured<'_>,
    now: Instant,
    requests: &[Request],
    timers: &[Timer],
) -> Result<Event<u8, u8>> {
    let entry = u.int_in_range(0..=3)?;
    Ok(match u.int_in_range(0..=13_u8)? {
        0 => Event::Start { entry },
        1 => Event::Pause,
        2 => Event::Resume,
        3 => Event::Stop,
        4 => Event::Seek {
            position: duration(u)?,
        },
        // Before the first request, there is nothing to answer.
        5 if requests.is_empty() => Event::Ended { entry },
        5 => Event::Resolved {
            request: *u.choose(requests)?,
            source: u.arbitrary()?,
            duration: if u.arbitrary()? {
                Some(duration(u)?)
            } else {
                None
            },
        },
        6 => Event::Playing {
            entry,
            anchor: Anchor::new(
                now,
                duration(u)?,
                Rate::from_thousandths(u.int_in_range(0..=4_000)?),
            ),
        },
        7 => Event::Paused {
            entry,
            position: duration(u)?,
        },
        8 => Event::Buffering {
            entry,
            position: duration(u)?,
        },
        // Before the first request, there is nothing to fail.
        9 if requests.is_empty() => Event::Failed { entry },
        9 => Event::ResolveFailed {
            request: *u.choose(requests)?,
        },
        10 => Event::Failed { entry },
        11 => Event::Expired { entry },
        // Before the first timer, there is nothing to fire.
        12 if timers.is_empty() => Event::Expired { entry },
        12 => Event::TimerFired {
            timer: *u.choose(timers)?,
        },
        _ => Event::Ended { entry },
    })
}

/// Whether `event` is a report, a failure or an expiry about another entry than the player's, an answer or a failure of
/// another request than the one it waits for, or a timer it does not wait with.
fn is_late(event: &Event<u8, u8>, state: &State<u8, u8>) -> bool {
    match event {
        Event::Resolved { request, .. } | Event::ResolveFailed { request } => !matches!(
            state,
            State::Resolving { request: awaited, .. } if awaited == request
        ),
        Event::Playing { entry, .. }
        | Event::Paused { entry, .. }
        | Event::Buffering { entry, .. }
        | Event::Ended { entry }
        | Event::Failed { entry }
        | Event::Expired { entry } => state.entry() != Some(*entry),
        Event::TimerFired { timer } => state.timer() != Some(*timer),
        _ => false,
    }
}

/// The deadline of the waiting state `state`, if it is one.
fn deadline_of(state: &State<u8, u8>) -> Option<Duration> {
    match state {
        State::Resolving { .. } => Some(DEADLINES.resolving()),
        State::Loading { .. } => Some(DEADLINES.loading()),
        State::Buffering { .. } => Some(DEADLINES.buffering()),
        _ => None,
    }
}

fn run(u: &mut Unstructured<'_>) -> Result<()> {
    let mut player: Player<u8, u8> = Player::new(DEADLINES);
    let mut now = Instant::default();
    let mut stopped = false;
    let mut last_started = None;
    let mut requests = Vec::new();
    let mut timers = Vec::new();
    let mut renewals_since_intent = 0_u8;
    while !u.is_empty() {
        now = Instant::from_origin(now.since_origin().saturating_add(duration(u)?));
        let event = event(u, now, &requests, &timers)?;
        let before = player.clone();
        let late = is_late(&event, before.state());
        let intent = matches!(event, Event::Start { .. } | Event::Resume);
        let renews = matches!(event, Event::Start { .. })
            || matches!(
                (&event, before.state()),
                (Event::Resume, State::Stopped { .. })
            );
        let is_stop = matches!(event, Event::Stop);
        let is_expiry = matches!(event, Event::Expired { .. });
        let pause_or_stop = matches!(event, Event::Pause | Event::Stop);
        if let Event::Start { entry } = event {
            last_started = Some(entry);
        }
        let commands: Vec<_> = player.handle(now, event).into_iter().collect();

        // Every request has a new number, and the player waits for the last one it made.
        let request = commands.iter().find_map(|command| match command {
            Command::Resolve { request, .. } => Some(*request),
            _ => None,
        });
        if let Some(request) = request {
            assert!(!requests.contains(&request));
            requests.push(request);
            assert!(
                matches!(player.state(), State::Resolving { request: awaited, .. } if *awaited == request)
            );
        }
        // Every timer has a new number, lasts the deadline of the state it waits in, and the player waits with the
        // last one it armed; without a new one, it waits with the one it waited with before, if any.
        let armed = commands.iter().find_map(|command| match command {
            Command::ArmTimer { timer, after } => Some((*timer, *after)),
            _ => None,
        });
        match armed {
            Some((timer, after)) => {
                assert!(!timers.contains(&timer));
                timers.push(timer);
                assert_eq!(player.state().timer(), Some(timer));
                assert_eq!(Some(after), deadline_of(player.state()));
            }
            None => {
                if let Some(timer) = player.state().timer() {
                    assert_eq!(before.state().timer(), Some(timer));
                }
            }
        }
        // A waiting state always waits with a timer, and no other state does.
        assert_eq!(
            player.state().timer().is_some(),
            deadline_of(player.state()).is_some()
        );

        // At most three commands: a `Stop` comes first, an `ArmTimer` or a `Fail` last, and a `Fail` leaves the
        // player idle.
        assert!(commands.len() <= 3);
        for (index, command) in commands.iter().enumerate() {
            match command {
                Command::Stop => assert_eq!(index, 0),
                Command::ArmTimer { .. } | Command::Fail { .. } => {
                    assert_eq!(index + 1, commands.len());
                }
                _ => {}
            }
        }
        if matches!(commands.last(), Some(Command::Fail { .. })) {
            assert!(matches!(player.state(), State::Idle));
        }
        // Between two user intents that renew, an entry gets at most one new resolution after an expiry.
        if renews {
            renewals_since_intent = 0;
        }
        if is_expiry && request.is_some() {
            renewals_since_intent = renewals_since_intent.saturating_add(1);
            assert_eq!(player.state().renewal(), Some(Renewal::Spent));
        }
        assert!(renewals_since_intent <= 1);
        // A late event changes nothing.
        if late {
            assert_eq!(player, before);
            assert!(commands.is_empty());
        }
        // After a stop, nothing plays, loads or resolves until a user starts or resumes.
        if intent {
            stopped = false;
        }
        if stopped {
            assert!(!commands.iter().any(|c| matches!(
                c,
                Command::Play
                    | Command::Load { .. }
                    | Command::Resolve { .. }
                    | Command::ArmTimer { .. }
            )));
        }
        if is_stop {
            stopped = true;
        }
        // The entry is always the last one started: only `Start` changes it.
        if let Some(entry) = player.state().entry() {
            assert_eq!(Some(entry), last_started);
        }
        // No position kept or sent is beyond the known duration.
        let duration = match player.state() {
            State::Loading { duration, .. }
            | State::Playing { duration, .. }
            | State::Paused { duration, .. }
            | State::Buffering { duration, .. } => *duration,
            _ => None,
        };
        if let Some(duration) = duration {
            assert!(player.state().position_at(now) <= Some(duration));
            for command in &commands {
                if let Command::Load { from: position, .. } | Command::Seek { position } = command {
                    assert!(*position <= duration);
                }
            }
        }
        // Pausing and stopping keep the position the player was at, at that instant.
        if pause_or_stop {
            assert_eq!(
                player.state().position_at(now),
                before.state().position_at(now)
            );
        }
    }
    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = run(&mut Unstructured::new(data));
});
