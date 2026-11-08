//! The player's states, the events it takes and the commands it sends (core spec, section 4, States and Events and
//! transitions).

use super::Anchor;
use crate::time::Instant;
use std::time::Duration;

/// Where the player of a session is (core spec, section 4, States).
///
/// `E` identifies an entry of the session's queue, `S` a playable source the resolver gave. `duration` is the media's
/// duration when the resolver knew it, kept from the resolution so that positions are clamped to it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum State<E, S> {
    /// Nothing to play.
    #[default]
    Idle,
    /// Waiting for the resolver to give a playable source for `entry`.
    Resolving {
        /// The entry being resolved.
        entry: E,
        /// The position playback starts at once loaded.
        from: Duration,
        /// The request whose answer the player waits for.
        request: Request,
    },
    /// The platform player is opening `source`.
    Loading {
        /// The entry being loaded.
        entry: E,
        /// The source being opened.
        source: S,
        /// The position playback starts at.
        from: Duration,
        /// The media's duration, when known.
        duration: Option<Duration>,
    },
    /// The platform player plays, from its last report.
    Playing {
        /// The entry playing.
        entry: E,
        /// The platform player's last report.
        anchor: Anchor,
        /// The media's duration, when known.
        duration: Option<Duration>,
    },
    /// Paused at `position`.
    Paused {
        /// The entry paused.
        entry: E,
        /// Where it is paused.
        position: Duration,
        /// The media's duration, when known.
        duration: Option<Duration>,
    },
    /// Waiting for the platform player to play from `position`: buffering, or after a resume or a seek.
    Buffering {
        /// The entry waiting.
        entry: E,
        /// Where it plays from.
        position: Duration,
        /// The media's duration, when known.
        duration: Option<Duration>,
    },
    /// After an explicit stop; resumes from a new resolution.
    Stopped {
        /// The entry stopped.
        entry: E,
        /// Where it resumes.
        position: Duration,
    },
}

impl<E: Copy, S> State<E, S> {
    /// The entry of this state, if any.
    pub fn entry(&self) -> Option<E> {
        match self {
            Self::Idle => None,
            Self::Resolving { entry, .. }
            | Self::Loading { entry, .. }
            | Self::Playing { entry, .. }
            | Self::Paused { entry, .. }
            | Self::Buffering { entry, .. }
            | Self::Stopped { entry, .. } => Some(*entry),
        }
    }

    /// Whether the platform player holds a source in this state, which a new entry or a stop must release.
    fn has_source(&self) -> bool {
        matches!(
            self,
            Self::Loading { .. }
                | Self::Playing { .. }
                | Self::Paused { .. }
                | Self::Buffering { .. }
        )
    }

    /// The position at `now`: computed from the anchor while playing, the kept position otherwise, and none when
    /// idle.
    pub fn position_at(&self, now: Instant) -> Option<Duration> {
        match self {
            Self::Idle => None,
            Self::Resolving { from, .. } | Self::Loading { from, .. } => Some(*from),
            Self::Playing {
                anchor, duration, ..
            } => Some(anchor.position_at(now, *duration)),
            Self::Paused { position, .. }
            | Self::Buffering { position, .. }
            | Self::Stopped { position, .. } => Some(*position),
        }
    }
}

/// The number of one [`Command::Resolve`], which the edges give back in its [`Event::Resolved`], so that the answer to an
/// earlier request for the same entry is told from the awaited one (core spec, section 4, Events and transitions).
///
/// Only the player makes requests; the edges copy them and never make one up.
///
/// ```
/// use vehicoule_core::player::{Command, Event, Player};
/// use vehicoule_core::time::Instant;
///
/// let mut player: Player<u32, &str> = Player::new();
/// let mut start = || match player.handle(Instant::default(), Event::Start { entry: 7 }).into_iter().next() {
///     Some(Command::Resolve { request, .. }) => Some(request),
///     _ => None,
/// };
/// // Starting the same entry twice makes two requests: only the second one's answer will be loaded.
/// assert_ne!(start(), start());
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Request(pub(super) u64);

impl Request {
    /// The request after this one. The counter would take 2^64 requests to come back to a number still in flight.
    pub(super) const fn following(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Something that happened, for the player of one session (core spec, section 4, Events and transitions).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<E, S> {
    /// The user, or the session's queue, made `entry` the current entry.
    Start {
        /// The new current entry.
        entry: E,
    },
    /// The user asked to pause.
    Pause,
    /// The user asked to resume.
    Resume,
    /// The user asked to stop.
    Stop,
    /// The user asked to go to `position`.
    Seek {
        /// Where to go.
        position: Duration,
    },
    /// The resolver answered `request` with a playable source.
    Resolved {
        /// The request answered, as the [`Command::Resolve`] carried it.
        request: Request,
        /// Its playable source.
        source: S,
        /// The media's duration, when known.
        duration: Option<Duration>,
    },
    /// The platform player plays `entry` from `anchor`: start, resume, seek done, rate change, end of buffering,
    /// discontinuity.
    Playing {
        /// The entry it plays.
        entry: E,
        /// Where and how fast, and when.
        anchor: Anchor,
    },
    /// The platform player paused `entry` at `position`.
    Paused {
        /// The entry it paused.
        entry: E,
        /// Where.
        position: Duration,
    },
    /// The platform player buffers `entry` at `position`.
    Buffering {
        /// The entry it buffers.
        entry: E,
        /// Where.
        position: Duration,
    },
    /// The platform player reached the end of `entry`.
    Ended {
        /// The entry that ended.
        entry: E,
    },
}

/// Something the edges must do (core spec, section 4, Events and transitions).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command<E, S> {
    /// Ask the resolver for a playable source for `entry`, and give `request` back with the answer.
    Resolve {
        /// The entry to resolve.
        entry: E,
        /// The number of this request.
        request: Request,
    },
    /// Open `source` in the platform player, to play from `from`.
    Load {
        /// The source to open.
        source: S,
        /// Where to start.
        from: Duration,
    },
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Go to `position`.
    Seek {
        /// Where to go.
        position: Duration,
    },
    /// Stop and release the source.
    Stop,
}

/// The commands of one event, in the order the edges must run them: at most two.
///
/// ```
/// use vehicoule_core::player::{Command, Event, Player};
/// use vehicoule_core::time::Instant;
///
/// let mut player: Player<u32, &str> = Player::new();
/// let commands = player.handle(Instant::default(), Event::Start { entry: 7 });
/// assert!(matches!(commands.iter().collect::<Vec<_>>()[..], [Command::Resolve { entry: 7, .. }]));
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commands<E, S> {
    first: Option<Command<E, S>>,
    second: Option<Command<E, S>>,
}

impl<E, S> Commands<E, S> {
    const fn none() -> Self {
        Self {
            first: None,
            second: None,
        }
    }

    const fn one(command: Command<E, S>) -> Self {
        Self {
            first: Some(command),
            second: None,
        }
    }

    const fn two(first: Command<E, S>, second: Command<E, S>) -> Self {
        Self {
            first: Some(first),
            second: Some(second),
        }
    }

    /// The commands, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Command<E, S>> {
        self.first.iter().chain(self.second.iter())
    }

    /// Whether there is no command.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.first.is_none()
    }
}

impl<E, S> IntoIterator for Commands<E, S> {
    type Item = Command<E, S>;
    type IntoIter = std::iter::Chain<
        std::option::IntoIter<Command<E, S>>,
        std::option::IntoIter<Command<E, S>>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.first.into_iter().chain(self.second)
    }
}

/// The player model of one session: the only owner of its playback state (core spec, section 4, Ownership).
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::player::{Anchor, Command, Event, Player, Rate, State};
/// use vehicoule_core::time::Instant;
///
/// let at = |secs| Instant::from_origin(Duration::from_secs(secs));
/// let mut player: Player<u32, &str> = Player::new();
/// let Some(Command::Resolve { request, .. }) = player.handle(at(0), Event::Start { entry: 1 }).into_iter().next() else {
///     unreachable!()
/// };
/// player.handle(at(1), Event::Resolved { request, source: "https://…", duration: None });
/// player.handle(at(2), Event::Playing { entry: 1, anchor: Anchor::new(at(2), Duration::ZERO, Rate::NORMAL) });
///
/// let commands = player.handle(at(12), Event::Pause);
/// assert_eq!(commands.iter().collect::<Vec<_>>(), [&Command::Pause]);
/// assert_eq!(player.state(), &State::Paused { entry: 1, position: Duration::from_secs(10), duration: None });
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player<E, S> {
    // Both visible to the module's tests and proofs, which start the player in any state.
    pub(super) state: State<E, S>,
    /// The number of the next request.
    pub(super) next: Request,
}

impl<E, S> Default for Player<E, S> {
    fn default() -> Self {
        Self {
            state: State::Idle,
            next: Request(0),
        }
    }
}

impl<E: Copy + PartialEq, S: Clone> Player<E, S> {
    /// An idle player.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Where the player is.
    pub fn state(&self) -> &State<E, S> {
        &self.state
    }

    /// Takes `event`, which happened at `now`, and returns the commands for the edges. Moves only as the core spec's
    /// table says (section 4, Events and transitions): any other event changes nothing and returns no command.
    ///
    /// `Ended` leaves the player `Idle`; the session then advances its queue and gives the next entry with `Start`.
    pub fn handle(&mut self, now: Instant, event: Event<E, S>) -> Commands<E, S> {
        let state = std::mem::take(&mut self.state);
        let (state, commands) = step(state, now, event, self.next);
        if commands
            .iter()
            .any(|command| matches!(command, Command::Resolve { .. }))
        {
            self.next = self.next.following();
        }
        self.state = state;
        commands
    }
}

/// One transition of the core spec's table, or the unchanged state with no command. Each event has its own function,
/// holding the rows of the table for that event. A new request, when one is sent, is numbered `next`.
fn step<E: Copy + PartialEq, S: Clone>(
    state: State<E, S>,
    now: Instant,
    event: Event<E, S>,
    next: Request,
) -> (State<E, S>, Commands<E, S>) {
    match event {
        Event::Start { entry } => start(&state, entry, next),
        Event::Pause => pause(state, now),
        Event::Resume => resume(state, next),
        Event::Stop => stop(state, now),
        Event::Seek { position } => seek(state, position),
        Event::Resolved {
            request,
            source,
            duration,
        } => resolved(state, request, source, duration),
        Event::Playing { entry, anchor } => playing(state, entry, anchor),
        Event::Paused { entry, position } => paused(state, entry, position),
        Event::Buffering { entry, position } => buffering(state, entry, position),
        Event::Ended { entry } => ended(state, entry),
    }
}

type Step<E, S> = (State<E, S>, Commands<E, S>);

fn unchanged<E, S>(state: State<E, S>) -> Step<E, S> {
    (state, Commands::none())
}

/// `position`, but never beyond `duration` when it is known: the player never sends or shows a position beyond the
/// media's duration (core spec, section 4, Events and transitions).
fn within(position: Duration, duration: Option<Duration>) -> Duration {
    duration.map_or(position, |duration| position.min(duration))
}

/// Any state: resolve the new entry from its start, releasing the source the platform player holds.
fn start<E: Copy, S>(state: &State<E, S>, entry: E, request: Request) -> Step<E, S> {
    let resolve = Command::Resolve { entry, request };
    let commands = if state.has_source() {
        Commands::two(Command::Stop, resolve)
    } else {
        Commands::one(resolve)
    };
    (
        State::Resolving {
            entry,
            from: Duration::ZERO,
            request,
        },
        commands,
    )
}

/// `Playing` pauses where the anchor is now; `Buffering` where it waited.
fn pause<E: Copy, S>(state: State<E, S>, now: Instant) -> Step<E, S> {
    match state {
        State::Playing {
            entry,
            anchor,
            duration,
        } => (
            State::Paused {
                entry,
                position: anchor.position_at(now, duration),
                duration,
            },
            Commands::one(Command::Pause),
        ),
        State::Buffering {
            entry,
            position,
            duration,
        } => (
            State::Paused {
                entry,
                position,
                duration,
            },
            Commands::one(Command::Pause),
        ),
        state => unchanged(state),
    }
}

/// `Paused` waits for the platform to play; `Stopped` resolves its entry again.
fn resume<E: Copy, S>(state: State<E, S>, request: Request) -> Step<E, S> {
    match state {
        State::Paused {
            entry,
            position,
            duration,
        } => (
            State::Buffering {
                entry,
                position,
                duration,
            },
            Commands::one(Command::Play),
        ),
        State::Stopped { entry, position } => (
            State::Resolving {
                entry,
                from: position,
                request,
            },
            Commands::one(Command::Resolve { entry, request }),
        ),
        state => unchanged(state),
    }
}

/// Any state with an entry but `Stopped` stops where it is, releasing the source the platform player holds.
fn stop<E: Copy, S>(state: State<E, S>, now: Instant) -> Step<E, S> {
    if matches!(state, State::Stopped { .. }) {
        return unchanged(state);
    }
    match (state.entry(), state.position_at(now)) {
        (Some(entry), Some(position)) => {
            let commands = if state.has_source() {
                Commands::one(Command::Stop)
            } else {
                Commands::none()
            };
            (State::Stopped { entry, position }, commands)
        }
        _ => unchanged(state),
    }
}

/// `Playing` and `Buffering` wait for the platform at the new position; `Paused` and `Stopped` keep it. The position
/// is clamped to the duration where it is known.
fn seek<E: Copy, S>(state: State<E, S>, position: Duration) -> Step<E, S> {
    match state {
        State::Playing {
            entry, duration, ..
        }
        | State::Buffering {
            entry, duration, ..
        } => {
            let position = within(position, duration);
            (
                State::Buffering {
                    entry,
                    position,
                    duration,
                },
                Commands::one(Command::Seek { position }),
            )
        }
        State::Paused {
            entry, duration, ..
        } => {
            let position = within(position, duration);
            (
                State::Paused {
                    entry,
                    position,
                    duration,
                },
                Commands::one(Command::Seek { position }),
            )
        }
        State::Stopped { entry, .. } => (State::Stopped { entry, position }, Commands::none()),
        state => unchanged(state),
    }
}

/// `Resolving` loads the source that answers the request it waits for, from where it starts within the duration.
fn resolved<E: Copy, S: Clone>(
    state: State<E, S>,
    answered: Request,
    source: S,
    duration: Option<Duration>,
) -> Step<E, S> {
    match state {
        State::Resolving {
            entry,
            from,
            request,
        } if request == answered => {
            let from = within(from, duration);
            (
                State::Loading {
                    entry,
                    source: source.clone(),
                    from,
                    duration,
                },
                Commands::one(Command::Load { source, from }),
            )
        }
        state => unchanged(state),
    }
}

/// `Loading`, `Playing` and `Buffering` the same entry play from the new anchor.
fn playing<E: Copy + PartialEq, S>(state: State<E, S>, playing: E, anchor: Anchor) -> Step<E, S> {
    match state {
        State::Loading {
            entry, duration, ..
        }
        | State::Playing {
            entry, duration, ..
        }
        | State::Buffering {
            entry, duration, ..
        } if entry == playing => (
            State::Playing {
                entry,
                anchor,
                duration,
            },
            Commands::none(),
        ),
        state => unchanged(state),
    }
}

/// `Paused` on the same entry takes the reported position, within the duration.
fn paused<E: Copy + PartialEq, S>(state: State<E, S>, paused: E, position: Duration) -> Step<E, S> {
    match state {
        State::Paused {
            entry, duration, ..
        } if entry == paused => (
            State::Paused {
                entry,
                position: within(position, duration),
                duration,
            },
            Commands::none(),
        ),
        state => unchanged(state),
    }
}

/// `Playing` the same entry waits at the reported position, within the duration.
fn buffering<E: Copy + PartialEq, S>(
    state: State<E, S>,
    buffering: E,
    position: Duration,
) -> Step<E, S> {
    match state {
        State::Playing {
            entry, duration, ..
        } if entry == buffering => (
            State::Buffering {
                entry,
                position: within(position, duration),
                duration,
            },
            Commands::none(),
        ),
        state => unchanged(state),
    }
}

/// `Playing` and `Buffering` the same entry go idle, for the session to start the next entry.
fn ended<E: Copy + PartialEq, S>(state: State<E, S>, ended: E) -> Step<E, S> {
    match state {
        State::Playing { entry, .. } | State::Buffering { entry, .. } if entry == ended => {
            (State::Idle, Commands::none())
        }
        state => unchanged(state),
    }
}
