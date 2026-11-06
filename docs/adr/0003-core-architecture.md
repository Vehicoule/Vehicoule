# 0003. Build the core as synchronous state machines, in one process, with I/O at the edges

- Status: Proposed
- Date: 2026-11-06
- Deciders: the user

## Context

In the first version, playback state was shared between Dart (two mpv instances) and Rust, and failures were hidden
behind stacked safety nets (15 s and 30 s timeouts, silence detection, reloads) instead of being fixed. The queue did
not follow its source list, a failed track was retried on every loop, and resume, next and previous could load forever
after a stop.

The rewrite's principles, kept by the user: one owner per state; the core decides everything, pause included; the shell
displays and forwards gestures; a small boundary (commands in, versioned state snapshots out), never hundreds of
generated functions; separate shells for mobile and desktop on the same core.

Two ideas from the first version's notes were discussed on 2026-11-06: the core as a separate process (D1) and core
modules as Wasm components (D2). The cost found then: a separate process adds IPC, memory and a second lifecycle;
Wasm modules copy data on every call and run interpreted on iOS. Wasm stays for content plugins.

## Options

1. **Synchronous state machines in one process.** Each domain (queue, player, library, later sync) is a pure state
   machine: `handle(event) -> commands`, with no I/O, no clock and no thread. A runner on its own thread feeds events
   (gestures, player reports, plugin results, ticks of time) and executes commands at the edges (platform player,
   network, disk, shell). One crate per domain. Deterministic, so every behavior is a test and the critical machines
   can be model-checked with Kani.
2. **Core as a separate process** (D1). Survives a UI crash; costs IPC, serialization, a second lifecycle, and differs
   per platform (Android service, desktop daemon, same process on iOS).
3. **Core modules as Wasm components** (D2). Isolation per module; costs copies at every call, Wasmtime in every path,
   interpretation on iOS.

## Decision

Recommended: option 1. Also recommended with it:

- The playback position is computed from `(instant, position, rate)` reported by the player, never polled.
- Every event and command passes through a bounded ring log (the "black box"), which a test can replay to reproduce a
  bug exactly.
- Core types are generic over the content domain; music is the first domain, video and books add to it.

## Consequences

- `vehicoule-core` and the domain crates depend on no runtime, no NSH and no platform API; they build and test on any
  host, including in Kani.
- The edges (platform players, plugin host, storage, NSH shells) are separate crates that the core never imports.
- An Android foreground service still hosts the process for background playback; that is a property of the Android
  shell, not of the core.
- A crash of the UI takes the core with it; the black box and persisted state make the restart exact.
