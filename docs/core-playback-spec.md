# Core spec: playback sessions, queue and player model

- Status: Proposed, waiting for the user
- Scope: the first increment of `vehicoule-core`. Library, plugins, sync and UI come in their own specs.
- Rules: [ADR 0003](adr/0003-core-architecture.md) (state machines, no I/O), [ADR 0004](adr/0004-verification.md)
  (Kani, proptest, mutants).

This spec says what the core must do and what it must never do. Every "must" and "never" below becomes a test, and
every invariant marked **[Kani]** becomes a proof harness. A behavior that is not written here is not specified: the
code does not invent it, it comes back here first.

## 1. Shape

The core is a value with one entry point:

```rust
fn handle(&mut self, now: Instant, event: Event) -> Commands
```

- `Instant` is the core's own monotonic timestamp type, given by the runner. It is not `std::time::Instant`, which a
  test or a Kani proof cannot build at a chosen value. The core never reads a clock.
- `Event` is everything that can happen: a user intent, a report from the platform player, a result from the
  resolver (plugins), a timer firing.
- `Commands` is the ordered list of things the edges must do: resolve a media, load a source, play, pause, seek, stop,
  arm a timer, persist state, publish a snapshot.
- After every `handle`, the shell can read a `Snapshot`: an immutable, versioned view of the state. Its version only
  grows, by one per change, and does not change when nothing changed.

Given the same initial state and the same sequence of `(now, event)`, the core produces the same commands and
snapshots, on every platform. That is what makes the black box replay and the Kani proofs possible.

## 2. Domains and sessions

- Content belongs to a domain: `Music`, `Video`, `Books`. The enum is `#[non_exhaustive]`; queue and player types are
  generic and know nothing of music.
- There are two playback sessions, one for audio (`Music`) and one for `Video`. They are independent: each has its own
  queue and player model, and they can play at the same time. Books never create a session.
- The platform shows one session in its media controls: the one started last. The core says which, in the snapshot.
- A sleep timer is shared: when it fires, every session stops (the fade is the edge's job, its duration is not set
  here).

## 3. Queue

### State

- A queue is an ordered list of **entries**. An entry has its own `EntryId`, unique in the queue for the queue's whole
  life, and the `MediaId` it plays. The same media can appear twice, as two entries.
- The queue remembers its **source**: what filled it (an album, a playlist, an artist, a search, the user's manual
  adds). Entries added by hand after the source keep a mark saying so.
- `current`: the entry being played, or none.
- `repeat`: `Off`, `All`, `One`.
- `shuffle`: off, or on with the order it plays in. Turning shuffle off goes back to the source order, with `current`
  unchanged. The shuffle order comes from a seed given in the event, never from a random source inside the core.

### Operations

`Replace { entries, start }`, `PlayNext { entries }`, `Append { entries }`, `Remove { entry }`, `Move { entry, to }`,
`Clear`, `SetRepeat`, `SetShuffle { seed }`, `Next`, `Previous`, `JumpTo { entry }`.

### Invariants

1. **[Kani]** `current`, when set, is an entry of the queue.
2. **[Kani]** Entry ids are unique.
3. **[Kani]** With shuffle on, the play order is a permutation of the entries: every entry exactly once.
4. **[Kani]** With shuffle off, the play order is the queue order.
5. **[Kani]** No operation loses or duplicates an entry except `Remove`, `Clear` and `Replace`, which say which.
6. Removing the current entry moves `current` to the entry that would have played next; if there is none, the
   session stops.
7. A search never fills the queue with other results of that search unless the user asked for it (the first
   version filled it with covers).
8. `Next` with `repeat = One` goes to the next entry; `repeat = One` only replays when a track ends by itself.

## 4. Player model

### States

```
Idle
Resolving { entry }                  waiting for the resolver to give a playable source
Loading   { entry, source }          the platform player is opening it
Playing   { entry, anchor }
Paused    { entry, position }
Buffering { entry, position }
Stopped   { entry, position }        after an explicit stop; resumable
```

Errors are not a resting state: a failure moves the session on (section 5) and is reported in the snapshot.

### Position

- The position is never polled. The platform player reports `(instant, position, rate)` when something changes
  (start, pause, seek, rate change, buffering, a discontinuity). The core keeps that **anchor** and computes the position
  at any instant as `position + (now - instant) × rate`, clamped to `[0, duration]`.
- **[Kani]** The computed position is never negative and never beyond the known duration.
- **[Kani]** Between two reports while `Playing` at rate 1, the computed position never goes backwards.

### Ownership

- The core is the only owner of the playback state. The platform player never decides to pause, skip or retry by
  itself: it reports, the core commands. Media buttons, headphone unplug and audio focus loss arrive as events.
- **[Kani]** After `Stop`, the core sends no `Play`, `Load` or `Resolve` for that session until a new user intent.

## 5. Failures and time limits

- A failed resolution or load marks the entry as failed **for the current pass** and moves to the next playable
  entry, with a notice in the snapshot.
- **[Kani]** A failed entry is not tried again in the same pass. A pass ends when the queue wraps (repeat `All`) or when
  the user acts on that entry.
- **[Kani]** Automatic skips in a row never exceed the number of entries: when every entry has failed, the session
  stops and says so. No endless loop.
- A source that expires while playing (the platform reports an expired or forbidden source) gets **one** new
  resolution, then playback resumes at the same position. A second expiry of the same entry in a row is a failure.
- Every wait has a deadline: `Resolving`, `Loading` and `Buffering` arm a timer when entered. When it fires before the
  awaited report, the entry fails as above. The deadline values are not set here (section 7).
- **[Kani]** A timer that fires after the state it guarded has been left does nothing.
- Retries, timeouts and fallbacks are never stacked: there is exactly one rule per failure, above.

## 6. Persistence and the black box

- The queue (entries, source, `current`, position, repeat, shuffle) is persisted through a `Persist` command on every
  change, never on a timer. At launch the runner gives it back in one event, and the session restores in `Paused`.
- Every event and command goes into a bounded ring log owned by the runner. A test can load a log and replay it to
  get the exact same snapshots.

## 7. Open questions for the user

1. **Previous**: restart the track when it has played more than a threshold, else go to the previous entry? Media3's
   default threshold is to be checked in its source before it is proposed as the value.
2. **Preloading**: when to resolve the next entry ahead of the end (the first version used about 20 s, without a
   measurement). It drives the < 250 ms click-to-sound target for a preloaded track.
3. **Deadlines** for resolving, loading and buffering: to be set from a measurement on the real resolver and players,
   not guessed.
4. **Radio at the end of the queue** (on by default in the first version's notes): in this increment, or with the
   plugins?
5. **Offline**: skip entries that are not downloaded and say so, as the first version's notes say?

## 8. Not in this increment

The library and its storage, the plugin host and resolver routing, downloads, lyrics, volume normalization, sync, the
UI. Each gets its own spec, written the same way.
