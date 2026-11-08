//! The player's states, the events it takes and the commands it sends (core spec, section 4, States and Events and
//! transitions, and section 5, Player events and transitions).

use super::Anchor;
use crate::time::Instant;
use std::time::Duration;

/// Where the player of a session is (core spec, section 4, States).
///
/// `E` identifies an entry of the session's queue, `S` a playable source the resolver gave. `duration` is the media's
/// duration when the resolver knew it, kept from the resolution so that positions are clamped to it. The waiting states
/// keep the timer that guards them, and every state with an entry but `Stopped` keeps its [`Renewal`] (section 5).
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
        /// The timer guarding the wait.
        timer: Timer,
        /// Whether the entry may still get a new source after an expiry.
        renewal: Renewal,
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
        /// The timer guarding the wait.
        timer: Timer,
        /// Whether the entry may still get a new source after an expiry.
        renewal: Renewal,
    },
    /// The platform player plays, from its last report.
    Playing {
        /// The entry playing.
        entry: E,
        /// The platform player's last report.
        anchor: Anchor,
        /// The media's duration, when known.
        duration: Option<Duration>,
        /// Whether the entry may still get a new source after an expiry.
        renewal: Renewal,
    },
    /// Paused at `position`.
    Paused {
        /// The entry paused.
        entry: E,
        /// Where it is paused.
        position: Duration,
        /// The media's duration, when known.
        duration: Option<Duration>,
        /// Whether the entry may still get a new source after an expiry.
        renewal: Renewal,
    },
    /// Waiting for the platform player to play from `position`: buffering, or after a resume or a seek.
    Buffering {
        /// The entry waiting.
        entry: E,
        /// Where it plays from.
        position: Duration,
        /// The media's duration, when known.
        duration: Option<Duration>,
        /// The timer guarding the wait.
        timer: Timer,
        /// Whether the entry may still get a new source after an expiry.
        renewal: Renewal,
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

    /// Whether the platform player holds a source in this state, which a new entry, a stop or a failure must release.
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

    /// The timer the state waits with, in a waiting state.
    pub fn timer(&self) -> Option<Timer> {
        match self {
            Self::Resolving { timer, .. }
            | Self::Loading { timer, .. }
            | Self::Buffering { timer, .. } => Some(*timer),
            _ => None,
        }
    }

    /// The state's renewal, in a state with an entry but `Stopped`.
    pub fn renewal(&self) -> Option<Renewal> {
        match self {
            Self::Resolving { renewal, .. }
            | Self::Loading { renewal, .. }
            | Self::Playing { renewal, .. }
            | Self::Paused { renewal, .. }
            | Self::Buffering { renewal, .. } => Some(*renewal),
            Self::Idle | Self::Stopped { .. } => None,
        }
    }
}

/// Whether an entry may still get a new source after its source expires (core spec, section 5, Player events and
/// transitions).
///
/// A user's `Start`, or `Resume` from `Stopped`, makes it [`Renewal::Available`]; an expiry spends it. So between two
/// user intents, an entry gets at most one new source after an expiry, and a source that keeps expiring never loops.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Renewal {
    /// An expiry resolves the entry again.
    Available,
    /// An expiry fails the entry.
    Spent,
}

/// The number of one [`Command::Resolve`], which the edges give back in its [`Event::Resolved`] or
/// [`Event::ResolveFailed`], so that the answer to an earlier request for the same entry is told from the awaited one
/// (core spec, section 4, Events and transitions).
///
/// Only the player makes requests; the edges copy them and never make one up.
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::player::{Command, Deadlines, Event, Player};
/// use vehicoule_core::time::Instant;
///
/// let second = Duration::from_secs(1);
/// let mut player: Player<u32, &str> = Player::new(Deadlines::new(second, second, second));
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

/// The number of one timer armed by [`Command::ArmTimer`], which the runner gives back in its [`Event::TimerFired`], so
/// that a timer guarding a state the player has left is told from the one it waits with (core spec, section 5, Player
/// events and transitions).
///
/// Only the player arms timers; the runner copies their numbers and never makes one up.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timer(pub(super) u64);

impl Timer {
    /// The timer after this one. The counter would take 2^64 timers to come back to a number still armed.
    pub(super) const fn following(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// How long the player waits in each waiting state before the entry fails (core spec, section 5).
///
/// The core is given them in its configuration and never writes them; their real values are set from measurements.
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::player::Deadlines;
///
/// let deadlines = Deadlines::new(Duration::from_secs(10), Duration::from_secs(15), Duration::from_secs(20));
/// assert_eq!(deadlines.loading(), Duration::from_secs(15));
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Deadlines {
    resolving: Duration,
    loading: Duration,
    buffering: Duration,
}

impl Deadlines {
    /// The deadlines of `Resolving`, `Loading` and `Buffering`.
    #[must_use]
    pub const fn new(resolving: Duration, loading: Duration, buffering: Duration) -> Self {
        Self {
            resolving,
            loading,
            buffering,
        }
    }

    /// How long the player waits for the resolver.
    #[must_use]
    pub const fn resolving(&self) -> Duration {
        self.resolving
    }

    /// How long the player waits for the platform player to open a source.
    #[must_use]
    pub const fn loading(&self) -> Duration {
        self.loading
    }

    /// How long the player waits for the platform player to play after buffering, a resume or a seek.
    #[must_use]
    pub const fn buffering(&self) -> Duration {
        self.buffering
    }
}

/// Why an entry failed, for the snapshot's notice (core spec, section 5, Player events and transitions).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Failure {
    /// The resolver could not give a source.
    Resolution,
    /// The platform player could not open or play the source.
    Playback,
    /// The source expired again after its renewal.
    Expiry,
    /// A wait went past its deadline.
    Deadline,
}

/// Something that happened, for the player of one session (core spec, section 4, Events and transitions, and section
/// 5, Player events and transitions).
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
    /// The resolver could not answer `request`.
    ResolveFailed {
        /// The request that failed, as the [`Command::Resolve`] carried it.
        request: Request,
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
    /// The platform player cannot open or play the source of `entry`.
    Failed {
        /// The entry whose source failed.
        entry: E,
    },
    /// The platform player reports the source of `entry` expired or forbidden.
    Expired {
        /// The entry whose source expired.
        entry: E,
    },
    /// The timer armed with `timer` fired.
    TimerFired {
        /// The timer, as the [`Command::ArmTimer`] carried it.
        timer: Timer,
    },
}

/// Something the edges, the runner or the session must do (core spec, section 4, Events and transitions, and section
/// 5, Player events and transitions).
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
    /// Open `source` in the platform player and play it from `from` as soon as it can.
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
    /// For the runner: give [`Event::TimerFired`] with `timer` once `after` has passed.
    ArmTimer {
        /// The number of this timer.
        timer: Timer,
        /// How long until it fires.
        after: Duration,
    },
    /// For the session: `entry` failed, and the session moves on (core spec, section 5).
    Fail {
        /// The entry that failed.
        entry: E,
        /// Why.
        failure: Failure,
    },
}

/// The commands of one event, in the order they must run: at most three.
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::player::{Command, Deadlines, Event, Player};
/// use vehicoule_core::time::Instant;
///
/// let second = Duration::from_secs(1);
/// let mut player: Player<u32, &str> = Player::new(Deadlines::new(second, second, second));
/// let commands = player.handle(Instant::default(), Event::Start { entry: 7 });
/// assert!(matches!(
///     commands.iter().collect::<Vec<_>>()[..],
///     [Command::Resolve { entry: 7, .. }, Command::ArmTimer { .. }]
/// ));
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commands<E, S> {
    first: Option<Command<E, S>>,
    second: Option<Command<E, S>>,
    third: Option<Command<E, S>>,
}

impl<E, S> Commands<E, S> {
    const fn none() -> Self {
        Self {
            first: None,
            second: None,
            third: None,
        }
    }

    const fn one(command: Command<E, S>) -> Self {
        Self {
            first: Some(command),
            second: None,
            third: None,
        }
    }

    const fn two(first: Command<E, S>, second: Command<E, S>) -> Self {
        Self {
            first: Some(first),
            second: Some(second),
            third: None,
        }
    }

    /// These commands, then `last`. Every row sends at most two commands before the timer it arms, so `last` never
    /// replaces one.
    fn then(self, last: Command<E, S>) -> Self {
        match (self.first, self.second) {
            (None, _) => Self::one(last),
            (Some(first), None) => Self::two(first, last),
            (Some(first), Some(second)) => Self {
                first: Some(first),
                second: Some(second),
                third: Some(last),
            },
        }
    }

    /// Whether any command is `wanted`. Looks at each slot, without the chained iterator of [`Commands::iter`], which
    /// costs more on every event (ADR 0016).
    fn any(&self, wanted: impl Fn(&Command<E, S>) -> bool) -> bool {
        self.first.as_ref().is_some_and(&wanted)
            || self.second.as_ref().is_some_and(&wanted)
            || self.third.as_ref().is_some_and(&wanted)
    }

    /// The commands, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Command<E, S>> {
        self.first
            .iter()
            .chain(self.second.iter())
            .chain(self.third.iter())
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
        std::iter::Chain<
            std::option::IntoIter<Command<E, S>>,
            std::option::IntoIter<Command<E, S>>,
        >,
        std::option::IntoIter<Command<E, S>>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.first.into_iter().chain(self.second).chain(self.third)
    }
}

/// The player model of one session: the only owner of its playback state (core spec, section 4, Ownership).
///
/// ```
/// use std::time::Duration;
/// use vehicoule_core::player::{Anchor, Command, Deadlines, Event, Player, Rate, Renewal, State};
/// use vehicoule_core::time::Instant;
///
/// let at = |secs| Instant::from_origin(Duration::from_secs(secs));
/// let second = Duration::from_secs(1);
/// let mut player: Player<u32, &str> = Player::new(Deadlines::new(second, second, second));
/// let Some(Command::Resolve { request, .. }) = player.handle(at(0), Event::Start { entry: 1 }).into_iter().next() else {
///     unreachable!()
/// };
/// player.handle(at(1), Event::Resolved { request, source: "https://…", duration: None });
/// player.handle(at(2), Event::Playing { entry: 1, anchor: Anchor::new(at(2), Duration::ZERO, Rate::NORMAL) });
///
/// let commands = player.handle(at(12), Event::Pause);
/// assert_eq!(commands.iter().collect::<Vec<_>>(), [&Command::Pause]);
/// assert_eq!(player.state().position_at(at(30)), Some(Duration::from_secs(10)));
/// assert!(matches!(player.state(), State::Paused { renewal: Renewal::Available, .. }));
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player<E, S> {
    // All visible to the module's tests and proofs, which start the player in any state.
    pub(super) state: State<E, S>,
    /// The number of the next request.
    pub(super) next: Request,
    /// The number of the next timer.
    pub(super) next_timer: Timer,
    /// How long each waiting state waits.
    pub(super) deadlines: Deadlines,
}

impl<E: Copy + PartialEq, S: Clone> Player<E, S> {
    /// An idle player, whose waiting states wait as long as `deadlines` say.
    #[must_use]
    pub const fn new(deadlines: Deadlines) -> Self {
        Self {
            state: State::Idle,
            next: Request(0),
            next_timer: Timer(0),
            deadlines,
        }
    }

    /// Where the player is.
    pub fn state(&self) -> &State<E, S> {
        &self.state
    }

    /// Takes `event`, which happened at `now`, and returns the commands for the edges, the runner and the session.
    /// Moves only as the core spec's tables say (section 4, Events and transitions, and section 5, Player events and
    /// transitions): any other event changes nothing and returns no command.
    ///
    /// `Ended` and every failure leave the player `Idle`; the session then advances its queue and gives the next entry
    /// with `Start`.
    pub fn handle(&mut self, now: Instant, event: Event<E, S>) -> Commands<E, S> {
        let state = std::mem::take(&mut self.state);
        let next = Next {
            request: self.next,
            timer: self.next_timer,
            deadlines: self.deadlines,
        };
        let (state, commands) = step(state, now, event, next);
        if commands.any(|command| matches!(command, Command::Resolve { .. })) {
            self.next = self.next.following();
        }
        if commands.any(|command| matches!(command, Command::ArmTimer { .. })) {
            self.next_timer = self.next_timer.following();
        }
        self.state = state;
        commands
    }
}

/// What a row needs to make a new request or arm a new timer.
#[derive(Clone, Copy)]
struct Next {
    /// The number of a new request.
    request: Request,
    /// The number of a new timer.
    timer: Timer,
    /// How long each waiting state waits.
    deadlines: Deadlines,
}

/// `commands`, then the arming of `next`'s timer for `after`.
fn armed<E, S>(commands: Commands<E, S>, next: Next, after: Duration) -> Commands<E, S> {
    commands.then(Command::ArmTimer {
        timer: next.timer,
        after,
    })
}

/// One transition of the core spec's tables, or the unchanged state with no command. Each event has its own function,
/// holding the rows of the tables for that event. A new request or timer, when one is made, takes `next`'s number.
fn step<E: Copy + PartialEq, S: Clone>(
    state: State<E, S>,
    now: Instant,
    event: Event<E, S>,
    next: Next,
) -> (State<E, S>, Commands<E, S>) {
    match event {
        Event::Start { entry } => start(&state, entry, next),
        Event::Pause => pause(state, now),
        Event::Resume => resume(state, next),
        Event::Stop => stop(state, now),
        Event::Seek { position } => seek(state, position, next),
        Event::Resolved {
            request,
            source,
            duration,
        } => resolved(state, request, source, duration, next),
        Event::ResolveFailed { request } => resolve_failed(state, request),
        Event::Playing { entry, anchor } => playing(state, entry, anchor),
        Event::Paused { entry, position } => paused(state, entry, position),
        Event::Buffering { entry, position } => buffering(state, entry, position, next),
        Event::Ended { entry } => ended(state, entry),
        Event::Failed { entry } => failed(state, entry),
        Event::Expired { entry } => expired(state, entry, now, next),
        Event::TimerFired { timer } => timer_fired(state, timer),
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

/// Resolving `entry` from `from`, with `next`'s request and timer: the state and its commands, after a `Stop` when
/// `release` says a source must be released.
fn resolving<E: Copy, S>(
    entry: E,
    from: Duration,
    renewal: Renewal,
    release: bool,
    next: Next,
) -> Step<E, S> {
    let resolve = Command::Resolve {
        entry,
        request: next.request,
    };
    let commands = if release {
        Commands::two(Command::Stop, resolve)
    } else {
        Commands::one(resolve)
    };
    (
        State::Resolving {
            entry,
            from,
            request: next.request,
            timer: next.timer,
            renewal,
        },
        armed(commands, next, next.deadlines.resolving),
    )
}

/// Buffering `entry` at `position`, with `next`'s timer, after `commands`.
fn buffering_at<E, S>(
    entry: E,
    position: Duration,
    duration: Option<Duration>,
    renewal: Renewal,
    commands: Commands<E, S>,
    next: Next,
) -> Step<E, S> {
    (
        State::Buffering {
            entry,
            position,
            duration,
            timer: next.timer,
            renewal,
        },
        armed(commands, next, next.deadlines.buffering),
    )
}

/// Any state: resolve the new entry from its start, releasing the source the platform player holds. A user's start is
/// a new try of the entry, so its renewal is available.
fn start<E: Copy, S>(state: &State<E, S>, entry: E, next: Next) -> Step<E, S> {
    resolving(
        entry,
        Duration::ZERO,
        Renewal::Available,
        state.has_source(),
        next,
    )
}

/// `Playing` pauses where the anchor is now; `Buffering` where it waited.
fn pause<E: Copy, S>(state: State<E, S>, now: Instant) -> Step<E, S> {
    match state {
        State::Playing {
            entry,
            anchor,
            duration,
            renewal,
        } => (
            State::Paused {
                entry,
                position: anchor.position_at(now, duration),
                duration,
                renewal,
            },
            Commands::one(Command::Pause),
        ),
        State::Buffering {
            entry,
            position,
            duration,
            renewal,
            ..
        } => (
            State::Paused {
                entry,
                position,
                duration,
                renewal,
            },
            Commands::one(Command::Pause),
        ),
        state => unchanged(state),
    }
}

/// `Paused` waits for the platform to play; `Stopped` resolves its entry again, as a new try.
fn resume<E: Copy, S>(state: State<E, S>, next: Next) -> Step<E, S> {
    match state {
        State::Paused {
            entry,
            position,
            duration,
            renewal,
        } => buffering_at(
            entry,
            position,
            duration,
            renewal,
            Commands::one(Command::Play),
            next,
        ),
        State::Stopped { entry, position } => {
            resolving(entry, position, Renewal::Available, false, next)
        }
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
fn seek<E: Copy, S>(state: State<E, S>, position: Duration, next: Next) -> Step<E, S> {
    match state {
        State::Playing {
            entry,
            duration,
            renewal,
            ..
        }
        | State::Buffering {
            entry,
            duration,
            renewal,
            ..
        } => {
            let position = within(position, duration);
            buffering_at(
                entry,
                position,
                duration,
                renewal,
                Commands::one(Command::Seek { position }),
                next,
            )
        }
        State::Paused {
            entry,
            duration,
            renewal,
            ..
        } => {
            let position = within(position, duration);
            (
                State::Paused {
                    entry,
                    position,
                    duration,
                    renewal,
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
    next: Next,
) -> Step<E, S> {
    match state {
        State::Resolving {
            entry,
            from,
            request,
            renewal,
            ..
        } if request == answered => {
            let from = within(from, duration);
            (
                State::Loading {
                    entry,
                    source: source.clone(),
                    from,
                    duration,
                    timer: next.timer,
                    renewal,
                },
                armed(
                    Commands::one(Command::Load { source, from }),
                    next,
                    next.deadlines.loading,
                ),
            )
        }
        state => unchanged(state),
    }
}

/// `Resolving` fails its entry when the request it waits for fails.
fn resolve_failed<E: Copy, S>(state: State<E, S>, failed: Request) -> Step<E, S> {
    match state {
        State::Resolving { entry, request, .. } if request == failed => (
            State::Idle,
            Commands::one(Command::Fail {
                entry,
                failure: Failure::Resolution,
            }),
        ),
        state => unchanged(state),
    }
}

/// `Loading`, `Playing` and `Buffering` the same entry play from the new anchor.
fn playing<E: Copy + PartialEq, S>(state: State<E, S>, playing: E, anchor: Anchor) -> Step<E, S> {
    match state {
        State::Loading {
            entry,
            duration,
            renewal,
            ..
        }
        | State::Playing {
            entry,
            duration,
            renewal,
            ..
        }
        | State::Buffering {
            entry,
            duration,
            renewal,
            ..
        } if entry == playing => (
            State::Playing {
                entry,
                anchor,
                duration,
                renewal,
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
            entry,
            duration,
            renewal,
            ..
        } if entry == paused => (
            State::Paused {
                entry,
                position: within(position, duration),
                duration,
                renewal,
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
    next: Next,
) -> Step<E, S> {
    match state {
        State::Playing {
            entry,
            duration,
            renewal,
            ..
        } if entry == buffering => buffering_at(
            entry,
            within(position, duration),
            duration,
            renewal,
            Commands::none(),
            next,
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

/// The entry fails: release the source the platform player holds, then tell the session.
fn fail<E: Copy, S>(state: &State<E, S>, entry: E, failure: Failure) -> Step<E, S> {
    let fail = Command::Fail { entry, failure };
    let commands = if state.has_source() {
        Commands::two(Command::Stop, fail)
    } else {
        Commands::one(fail)
    };
    (State::Idle, commands)
}

/// A state holding a source of the same entry fails it.
fn failed<E: Copy + PartialEq, S>(state: State<E, S>, failed: E) -> Step<E, S> {
    if state.has_source() && state.entry() == Some(failed) {
        fail(&state, failed, Failure::Playback)
    } else {
        unchanged(state)
    }
}

/// A state holding a source of the same entry resolves it again from where it is, once between two user intents; a
/// second expiry fails it.
fn expired<E: Copy + PartialEq, S>(
    state: State<E, S>,
    expired: E,
    now: Instant,
    next: Next,
) -> Step<E, S> {
    if !state.has_source() || state.entry() != Some(expired) {
        return unchanged(state);
    }
    match (state.renewal(), state.position_at(now)) {
        (Some(Renewal::Available), Some(position)) => {
            resolving(expired, position, Renewal::Spent, true, next)
        }
        _ => fail(&state, expired, Failure::Expiry),
    }
}

/// A waiting state fails its entry when the timer it waits with fires.
fn timer_fired<E: Copy, S>(state: State<E, S>, fired: Timer) -> Step<E, S> {
    match (state.entry(), state.timer()) {
        (Some(entry), Some(timer)) if timer == fired => fail(&state, entry, Failure::Deadline),
        _ => unchanged(state),
    }
}
