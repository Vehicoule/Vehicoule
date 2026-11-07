//! Vehicoule's core.
//!
//! The core owns every piece of app state: the queue, the player model, the library. It is a set of synchronous state
//! machines: events come in, commands and state snapshots go out. It does no I/O and draws nothing; the network, the
//! disk and the platform players live at the edges and talk to it through those events and commands
//! ([ADR 0003](https://github.com/Vehicoule/Vehicoule/blob/main/docs/adr/0003-core-architecture.md)).
//!
//! What it must do is written in the core spec (`docs/core-playback-spec.md`); every invariant marked \[Kani\] there is
//! a proof harness here.

pub mod player;
pub mod queue;
pub mod time;
