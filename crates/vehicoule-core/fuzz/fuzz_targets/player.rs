//! Runs any sequence of events on a player: never a panic, and after every event the player keeps the rules of the
//! core spec (section 4) that hold whatever the sequence.
//!
//! The bytes are read as events one after another until they run out, at instants that only move forward. Entries are
//! small numbers, so reports often name the player's entry and often another one. A resolution answers one of the
//! requests the player has made so far, often the awaited one and often an earlier one.

#![no_main]

use libfuzzer_sys::arbitrary::{Result, Unstructured};
use libfuzzer_sys::fuzz_target;
use std::time::Duration;
use vehicoule_core::player::{Anchor, Command, Event, Player, Rate, Request, State};
use vehicoule_core::time::Instant;

fn duration(u: &mut Unstructured<'_>) -> Result<Duration> {
    Ok(Duration::from_millis(u.int_in_range(0..=600_000)?))
}

fn event(u: &mut Unstructured<'_>, now: Instant, requests: &[Request]) -> Result<Event<u8, u8>> {
    let entry = u.int_in_range(0..=3)?;
    Ok(match u.int_in_range(0..=9_u8)? {
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
        _ => Event::Ended { entry },
    })
}

/// Whether `event` is a report about another entry than the player's, or an answer to another request than the one it
/// waits for.
fn is_late(event: &Event<u8, u8>, state: &State<u8, u8>) -> bool {
    match event {
        Event::Resolved { request, .. } => !matches!(
            state,
            State::Resolving { request: awaited, .. } if awaited == request
        ),
        Event::Playing { entry, .. }
        | Event::Paused { entry, .. }
        | Event::Buffering { entry, .. }
        | Event::Ended { entry } => state.entry() != Some(*entry),
        _ => false,
    }
}

fn run(u: &mut Unstructured<'_>) -> Result<()> {
    let mut player: Player<u8, u8> = Player::new();
    let mut now = Instant::default();
    let mut stopped = false;
    let mut last_started = None;
    let mut requests = Vec::new();
    while !u.is_empty() {
        now = Instant::from_origin(now.since_origin().saturating_add(duration(u)?));
        let event = event(u, now, &requests)?;
        let before = player.clone();
        let late = is_late(&event, before.state());
        let intent = matches!(event, Event::Start { .. } | Event::Resume);
        let is_stop = matches!(event, Event::Stop);
        let pause_or_stop = matches!(event, Event::Pause | Event::Stop);
        if let Event::Start { entry } = event {
            last_started = Some(entry);
        }
        let commands: Vec<_> = player.handle(now, event).into_iter().collect();

        // Every request has a new number, and the player waits for the last one it made.
        if let Some(Command::Resolve { request, .. }) = commands.last() {
            assert!(!requests.contains(request));
            requests.push(*request);
            assert!(
                matches!(player.state(), State::Resolving { request: awaited, .. } if awaited == request)
            );
        }

        // At most two commands, and two only for a new entry replacing a loaded source.
        assert!(commands.len() <= 2);
        if commands.len() == 2 {
            assert!(matches!(commands[0], Command::Stop));
            assert!(matches!(commands[1], Command::Resolve { .. }));
        }
        // A late report or resolution changes nothing.
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
                Command::Play | Command::Load { .. } | Command::Resolve { .. }
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
