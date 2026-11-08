//! Instruction counts of the player's transitions (ADR 0016).
//!
//! Each benchmark brings a player to a state through its events in a setup that is not counted, then runs one event
//! and returns the player and the commands, so that the compiler keeps the work. Each one fails when its event runs
//! more instructions than its ceiling.

use gungraun::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use std::hint::black_box;
use std::time::Duration;
use vehicoule_core::player::{
    Anchor, Command, Commands, Deadlines, Event, Player, Rate, Request, Timer,
};
use vehicoule_core::time::Instant;

type BenchPlayer = Player<u32, u32>;

const ENTRY: u32 = 1;
const SOURCE: u32 = 7;
const DEADLINES: Deadlines = Deadlines::new(
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(30),
);

fn at(secs: u64) -> Instant {
    Instant::from_origin(Duration::from_secs(secs))
}

/// A player resolving [`ENTRY`], and the request it waits for.
fn resolving() -> (BenchPlayer, Request) {
    let mut player = BenchPlayer::new(DEADLINES);
    let request = player
        .handle(at(0), Event::Start { entry: ENTRY })
        .into_iter()
        .find_map(|command| match command {
            Command::Resolve { request, .. } => Some(request),
            _ => None,
        })
        .expect("start sends a request");
    (player, request)
}

fn playing() -> BenchPlayer {
    let (mut player, request) = resolving();
    player.handle(
        at(1),
        Event::Resolved {
            request,
            source: SOURCE,
            duration: Some(Duration::from_secs(240)),
        },
    );
    player.handle(
        at(2),
        Event::Playing {
            entry: ENTRY,
            anchor: Anchor::new(at(2), Duration::ZERO, Rate::from_thousandths(1_250)),
        },
    );
    player
}

/// A player buffering [`ENTRY`], and the timer it waits with.
fn buffering() -> (BenchPlayer, Timer) {
    let mut player = playing();
    let timer = player
        .handle(
            at(20),
            Event::Buffering {
                entry: ENTRY,
                position: Duration::from_secs(22),
            },
        )
        .into_iter()
        .find_map(|command| match command {
            Command::ArmTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .expect("buffering arms a timer");
    (player, timer)
}

fn stopped() -> BenchPlayer {
    let mut player = playing();
    player.handle(at(30), Event::Stop);
    player
}

/// The least margin over a recorded count, in instructions (ADR 0016).
const LEAST_MARGIN: u64 = 100;

/// The configuration of a benchmark whose event ran `measured` instructions when its ceiling was set: it fails above
/// 10 % more, or [`LEAST_MARGIN`] more when that is larger (ADR 0016). `measured` comes from the CI's `bench` job, and
/// raising it is the user's decision.
fn measured(measured: u64) -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    let ceiling = measured + (measured / 10).max(LEAST_MARGIN);
    config.tool(Callgrind::default().hard_limits([(EventKind::Ir, ceiling)]));
    config
}

/// Runs `event` at `now` on `player`.
fn run(
    mut player: BenchPlayer,
    now: Instant,
    event: Event<u32, u32>,
) -> (BenchPlayer, Commands<u32, u32>) {
    let commands = player.handle(black_box(now), black_box(event));
    (player, commands)
}

#[library_benchmark]
#[bench::playing(setup = playing, config = measured(135))]
fn start(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(40), Event::Start { entry: 2 })
}

#[library_benchmark]
#[bench::resolving(setup = resolving, config = measured(169))]
fn resolved((player, request): (BenchPlayer, Request)) -> (BenchPlayer, Commands<u32, u32>) {
    let event = Event::Resolved {
        request,
        source: SOURCE,
        duration: Some(Duration::from_secs(240)),
    };
    run(player, at(1), event)
}

#[library_benchmark]
#[bench::playing(setup = playing, config = measured(138))]
fn report(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    let anchor = Anchor::new(at(20), Duration::from_secs(18), Rate::NORMAL);
    run(
        player,
        at(20),
        Event::Playing {
            entry: ENTRY,
            anchor,
        },
    )
}

#[library_benchmark]
#[bench::playing(setup = playing, config = measured(299))]
fn pause(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(30), Event::Pause)
}

#[library_benchmark]
#[bench::playing(setup = playing, config = measured(324))]
fn stop(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(30), Event::Stop)
}

#[library_benchmark]
#[bench::stopped(setup = stopped, config = measured(126))]
fn resume(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(40), Event::Resume)
}

#[library_benchmark]
#[bench::playing(setup = playing, config = measured(123))]
fn late_report(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(30), Event::Ended { entry: 2 })
}

#[library_benchmark]
#[bench::playing(setup = playing, config = measured(456))]
fn expired(player: BenchPlayer) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(30), Event::Expired { entry: ENTRY })
}

#[library_benchmark]
#[bench::buffering(setup = buffering, config = measured(227))]
fn deadline((player, timer): (BenchPlayer, Timer)) -> (BenchPlayer, Commands<u32, u32>) {
    run(player, at(50), Event::TimerFired { timer })
}

library_benchmark_group!(
    name = player,
    benchmarks = [
        start,
        resolved,
        report,
        pause,
        stop,
        resume,
        late_report,
        expired,
        deadline
    ]
);

main!(library_benchmark_groups = player);
