# Core spec: playback sessions, queue and player model

- Status: Accepted (the user, 2026-11-06)
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
`Clear`, `SetRepeat`, `SetShuffle { seed }`, `SetRadio`, `Next`, `Previous`, `JumpTo { entry }`.

- `Previous`: when the position is at most 3 s, go to the previous entry (restart the current one if it is the first);
  beyond 3 s, restart the current entry. The 3 s is Media3's `C.DEFAULT_MAX_SEEK_TO_PREVIOUS_POSITION_MS = 3_000`
  (`libraries/common/src/main/java/androidx/media3/common/C.java`, branch `release` of `androidx/media`, read on
  2026-11-06), with the same comparison: at most the threshold goes back.

### Radio

- `radio` is on or off, per session, on by default for music.
- When radio is on and the last entry of the queue becomes current, the core sends one `RequestContinuation` seeded
  with that entry's media. The answer appends its entries at the end, marked as radio; a failed or empty answer is
  reported and the queue ends normally.
- **[Kani]** A session never has more than one continuation request outstanding, and never sends one with radio off.
- Which media follow is the plugins' job; the core only decides when to ask and where the answer goes.

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

### Preloading

- When an entry starts playing, the core resolves the entry that would play next. When the next entry changes (an
  edit, shuffle, repeat), it resolves the new one. No time before the end is involved.
- A preloaded source that has expired when it is used follows the expiry rule of section 5.

### Offline

- Each entry knows whether its media is downloaded. Connectivity comes in as an event.
- Offline, entries that are not downloaded are skipped without being marked failed, with a notice in the snapshot.
  They are played normally again once the network is back.
- Offline with no downloaded entry left ahead, the session waits in `WaitingForNetwork` on the next entry and starts
  it when the network comes back. It never stops only because the next entry is not downloaded.

## 4. Player model

### States

```
Idle
Resolving { entry, from, request, timer, renewal }            waiting for the resolver to give a playable source
Loading   { entry, source, from, duration, timer, renewal }   the platform player is opening it, to play from `from`
Playing   { entry, anchor, duration, renewal }
Paused    { entry, position, duration, renewal }
Buffering { entry, position, duration, timer, renewal }
Stopped   { entry, position }                                 after an explicit stop; resumable
WaitingForNetwork { entry }          offline, nothing downloaded ahead (section 3, Offline)
```

Errors are not a resting state: a failure moves the session on (section 5) and is reported in the snapshot.

`from` is the position playback starts at: zero for a new entry, the kept position when a stopped entry resumes.
`duration` is the media's duration when the resolver knew it, kept from the resolution while the source is loaded, so
that positions are clamped to it (Position, below). `request` identifies the `Resolve` the player waits for (Events and
transitions, below). `timer` and `renewal` come from section 5: the timer a waiting state is guarded by, and whether
the entry may still get a new source after an expiry.

### Events and transitions

Every event comes with `now`. The player of a session changes state only as the table below says; any other event in
any other state changes nothing and sends no command. Section 5 adds failures and deadlines, section 3 the offline
state `WaitingForNetwork`; both come with their own pull requests.

Events:

- From the user, through the shell or the session's queue: `Start { entry }` (the queue's current entry changed),
  `Pause`, `Resume`, `Stop`, `Seek { position }`.
- From the resolver: `Resolved { request, source, duration }`, the answer to the `Resolve` that carried `request`,
  where `duration` is the media's duration when known.
- From the platform player, each naming the entry it is about: `Playing { entry, anchor }` (it plays from the anchor:
  start, resume, seek done, rate change, end of buffering, discontinuity), `Paused { entry, position }`,
  `Buffering { entry, position }`, `Ended { entry }`.

A late event changes nothing: a report about another entry than the player's, or a resolution of another request than
the one the player waits for. The request, not the entry, says which resolution is awaited, because one entry can be
resolved several times in a row: stopped while resolving and resumed, or started again while resolving. An answer to
the earlier request must not be loaded in place of the new one. A resolution cannot be cancelled and no command
separates the two requests, so the player numbers its requests; the edges give the number back unchanged. The numbers
are those of a 64-bit counter, which would take 2^64 requests to come back to a number still in flight.

Reports need no number. The edges run the commands in order and give the events in order, and give no report about a
source after running the `Stop` that released it, nor after reporting `Ended` for it. Every way out of a loaded source
is one of the two (the table below), so a report from a source left behind comes before the resolution that follows,
while the player is `Stopped`, `Idle` or `Resolving`, where reports change nothing.

The resolution of the next entry for preloading (section 3, Preloading) is not given to the player: the session keeps
it for when that entry starts, and comes with the preloading pull request.

Commands: `Resolve { entry, request }`, `Load { source, from }`, `Play`, `Pause`, `Seek { position }`, `Stop`.
`Load` opens the source and plays it from `from` as soon as it can: no `Play` follows it, and the platform reports
`Playing` once it plays.

| State | Event | Next state | Commands |
|---|---|---|---|
| any | `Start { e }` | `Resolving { e, 0, r }` | `Stop` when a source was loaded (`Loading`, `Playing`, `Paused`, `Buffering`), then `Resolve { e, r }` |
| `Resolving { e, from, r }` | `Resolved { r, s, d }` | `Loading { e, s, min(from, d), d }` | `Load { s, min(from, d) }` |
| `Loading { e, .. }` | `Playing { e, a }` | `Playing { e, a, d }` | |
| `Playing { e, .. }` or `Buffering { e, .. }` | `Playing { e, a }` | `Playing { e, a, d }` | |
| `Playing { e, a, d }` | `Pause` | `Paused { e, a at now, d }` | `Pause` |
| `Buffering { e, p, d }` | `Pause` | `Paused { e, p, d }` | `Pause` |
| `Paused { e, p, d }` | `Resume` | `Buffering { e, p, d }` | `Play` |
| `Paused { e, .. }` | `Paused { e, p }` | `Paused { e, min(p, d), d }` | |
| `Playing { e, .. }` | `Buffering { e, p }` | `Buffering { e, min(p, d), d }` | |
| `Playing { e, .. }` or `Buffering { e, .. }` | `Seek { p }` | `Buffering { e, min(p, d), d }` | `Seek { min(p, d) }` |
| `Paused { e, .. }` | `Seek { p }` | `Paused { e, min(p, d), d }` | `Seek { min(p, d) }` |
| `Stopped { e, .. }` | `Seek { p }` | `Stopped { e, p }` | |
| `Resolving`, `Loading`, `Playing`, `Paused` or `Buffering`, on `e` | `Stop` | `Stopped { e, position }` | `Stop` when a source was loaded |
| `Stopped { e, p }` | `Resume` | `Resolving { e, p, r }` | `Resolve { e, r }` |
| `Playing { e, .. }` or `Buffering { e, .. }` | `Ended { e }` | see below | |

- `r` is a new request number, different from every request still in flight. `d` is the duration the state had,
  carried along. `a at now` is the anchor's position at `now`, clamped to `d` (Position, below). `min(p, d)` is `p`
  clamped to `d`, and `p` itself when the duration is unknown: the player never sends or shows a position beyond the
  known duration. `Stopped` and `Resolving` know no duration, so a position kept there is clamped by `Resolved`.
  `position` in `Stop` is the position of the state left: `a at now` from `Playing`, the kept position from `Paused`
  and `Buffering`, `from` from `Resolving` and `Loading`. `Stop` in `WaitingForNetwork`, which keeps no position, comes
  with that state's pull request.
- `Buffering` is also where the player waits after `Resume` and `Seek`, until the platform reports `Playing`: section 5
  gives it its deadline.
- `Ended { e }`: the session advances its queue as an entry that ended by itself (`Advance::Finished`). The player
  gets `Start` for the new current entry, or goes `Idle`, with no command, when the queue has ended.
- `Pause` and `Seek` before the platform first reports `Playing` (in `Resolving` and `Loading`) change nothing: the
  shell shows them unavailable in those states, from the snapshot.
- A stopped entry resumes from a new resolution: its source may have expired while it was stopped.
- The rows above carry `renewal` along, and arm a timer on every move into a waiting state, as section 5 says.

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
- Every wait on an edge has a deadline: `Resolving`, `Loading` and `Buffering` arm a timer when entered. When it fires
  before the awaited report, the entry fails as above. `WaitingForNetwork` is not a wait on an edge but on the user's
  connectivity, and has no deadline: it ends when the network comes back or when the user acts (stop, another
  entry). The three deadlines are given to the core in its configuration, never written in it; tests use arbitrary
  values. Their real values are set from measurements (section 7).
- **[Kani]** A timer that fires after the state it guarded has been left does nothing.
- Retries, timeouts and fallbacks are never stacked: there is exactly one rule per failure, above.

### Player events and transitions

The player of section 4 takes these events and sends these commands too. Its table still holds; the table below adds
rows to it, and any other pair still changes nothing.

Events:

- From the resolver: `ResolveFailed { request }`, the `Resolve` that carried `request` failed.
- From the platform player, each naming the entry it is about: `Failed { entry }`, it cannot open or play the source;
  `Expired { entry }`, it reports the source expired or forbidden.
- From the runner: `TimerFired { timer }`, the timer armed with that number fired.

Commands:

- `ArmTimer { timer, after }`, for the runner: give `TimerFired { timer }` once `after` has passed. There is no command
  to disarm a timer: one that fires once the player no longer waits with it is late and changes nothing.
- `Fail { entry, failure }`, for the session and not for the edges: `entry` failed, and the session moves on as the
  rules above say. `failure` is `Resolution` (`ResolveFailed`), `Playback` (`Failed`), `Expiry` (a second expiry) or
  `Deadline` (`TimerFired`), so that the snapshot's notice can say which.

Timers. `Resolving`, `Loading` and `Buffering` wait with a timer. Every move into one of them arms a new one, also from
the same state (a second seek while buffering): the row's commands end with `ArmTimer { t, deadline }`, where `t` is a
new timer number, as `r` is for requests, and `deadline` is the configured deadline of the state entered. A move out of
a waiting state needs nothing more: the timer it waited with becomes late.

Renewal. Every state with an entry but `Stopped` carries `renewal`, `Available` or `Spent`. `Start`, and `Resume` from
`Stopped`, give `Available`: a user's intent is a new try of the entry. Other rows carry it along. An expiry spends it,
so an entry gets at most one new source after an expiry between two user intents, whatever plays in between.

| State | Event | Next state | Commands |
|---|---|---|---|
| `Resolving { e, r, .. }` | `ResolveFailed { r }` | `Idle` | `Fail { e, Resolution }` |
| `Loading`, `Playing`, `Paused` or `Buffering`, on `e` | `Failed { e }` | `Idle` | `Stop`, then `Fail { e, Playback }` |
| `Loading`, `Playing`, `Paused` or `Buffering`, on `e`, renewal `Available` | `Expired { e }` | `Resolving { e, position, r, t, Spent }` | `Stop`, `Resolve { e, r }`, `ArmTimer { t, resolving }` |
| `Loading`, `Playing`, `Paused` or `Buffering`, on `e`, renewal `Spent` | `Expired { e }` | `Idle` | `Stop`, then `Fail { e, Expiry }` |
| `Resolving`, `Loading` or `Buffering` on `e`, waiting with `t` | `TimerFired { t }` | `Idle` | `Stop` when a source was loaded, then `Fail { e, Deadline }` |

- `position` in `Expired` is the position of the state left, as for `Stop` (section 4). It is where playback resumes
  once the new source is loaded.
- A failure, an expiry or a timer about another entry, another request or another timer than the player's is late and
  changes nothing. `Stopped` and `Idle` wait for nothing, so every timer there is late; and the edges give no report
  about a released source (section 4), so no failure or expiry reaches `Stopped` or `Idle` from the source stopped.
- An expiry is not a user intent but sends `Resolve`: it never happens in `Stopped` or `Idle`, which hold no source, so
  the ownership invariant of section 4 still holds.
- An event sends at most three commands: `Stop`, `Resolve` and `ArmTimer`, for `Start` from a loaded source and for a
  first expiry.
- **[Kani]** The player arms a timer on every move into a waiting state, and waits with the last timer it armed.
- **[Kani]** A timer, a failure or an expiry that is late changes nothing and sends nothing.
- **[Kani]** Between two user intents, an entry gets at most one new resolution after an expiry.

## 6. Persistence and the black box

- The queue (entries, source, `current`, position, repeat, shuffle) is persisted through a `Persist` command on every
  change, never on a timer. At launch the runner gives it back in one event, and the session restores in `Paused`.
- Every event and command goes into a bounded ring log owned by the runner. A test can load a log and replay it to
  get the exact same snapshots.

## 7. Decisions on the open questions

Answered by the user on 2026-11-06:

1. **Previous**: Media3's threshold and comparison, 3 s (section 3).
2. **Preloading**: resolve the next entry as soon as the current one starts playing (section 3).
3. **Deadlines**: configuration of the core. Once the real resolver and players exist, the black box records every
   resolution, load and buffering duration, in release builds on the Xiaomi 14 and the Mac; each deadline is then set
   above the worst measured case, with a margin the user chooses, in an ADR.
4. **Radio**: in the core now, the candidates from plugins later (section 3).
5. **Offline**: skip entries that are not downloaded, never stop for it (section 3).

## 8. Not in this increment

The library and its storage, the plugin host and resolver routing, downloads, lyrics, volume normalization, sync, the
UI. Each gets its own spec, written the same way.
