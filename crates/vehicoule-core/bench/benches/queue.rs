//! Instruction counts of the queue's operations on a large queue, unshuffled and shuffled (ADR 0016).
//!
//! Each benchmark builds its queue in a setup that is not counted, then runs one operation and returns what it
//! touched, so that the compiler keeps the work. Each one fails when its operation runs more instructions than its
//! ceiling.

use gungraun::{
    Callgrind, EventKind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use std::hint::black_box;
use vehicoule_core::queue::{Advance, EntryId, Origin, Queue, QueueSource};

/// Entries in the benchmarked queue: a whole large library queued at once. Chosen for the benchmarks; the core spec
/// sets no limit on the queue's length.
const LARGE: u32 = 10_000;

/// Entries added by `play_next` and `append`: the most the queue's fuzz target adds at once.
const ADDED: u32 = 16;

/// The seed of every shuffle, so that every run counts the same order.
const SEED: u64 = 1;

/// An empty queue, shuffled when `shuffled` is true.
fn empty(shuffled: bool) -> Queue<u32> {
    let mut queue = Queue::new();
    if shuffled {
        queue.shuffle(SEED);
    }
    queue
}

/// A queue of [`LARGE`] entries with the middle one current, shuffled when `shuffled` is true.
fn large(shuffled: bool) -> Queue<u32> {
    let mut queue = empty(shuffled);
    queue
        .replace(QueueSource::Selection, 0..LARGE, Some(LARGE as usize / 2))
        .expect("the middle is a position of the media");
    queue
}

/// The queue of [`large`], and the id of its current entry.
fn large_and_current(shuffled: bool) -> (Queue<u32>, EntryId) {
    let queue = large(shuffled);
    let current = queue.current().expect("the middle entry is current").id();
    (queue, current)
}

/// The queue of [`large`], and the id of the last entry of its source order: the farthest from the start of every
/// search through the entries.
fn large_and_last(shuffled: bool) -> (Queue<u32>, EntryId) {
    let queue = large(shuffled);
    let last = queue.entries().last().expect("the queue has entries").id();
    (queue, last)
}

/// The configuration of a benchmark whose operation ran `measured` instructions when its ceiling was set: it fails
/// above 10 % more (ADR 0016). `measured` comes from the CI's `bench` job, and raising it is the user's decision.
fn measured(measured: u64) -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(Callgrind::default().hard_limits([(EventKind::Ir, measured + measured / 10)]));
    config
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = empty, config = measured(140_526))]
#[bench::shuffled(args = (true), setup = empty, config = measured(400_823))]
fn replace(mut queue: Queue<u32>) -> Queue<u32> {
    let replaced = queue.replace(QueueSource::Selection, black_box(0..LARGE), Some(0));
    black_box(replaced).expect("0 is a position of the media");
    queue
}

#[library_benchmark]
#[bench::large(args = (false), setup = large, config = measured(330_349))]
fn shuffle(mut queue: Queue<u32>) -> Queue<u32> {
    queue.shuffle(black_box(SEED));
    queue
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large, config = measured(30_061))]
#[bench::shuffled(args = (true), setup = large, config = measured(67))]
fn next(mut queue: Queue<u32>) -> (Queue<u32>, Option<EntryId>) {
    let next = queue.next(black_box(Advance::Requested));
    (queue, next)
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large, config = measured(30_055))]
#[bench::shuffled(args = (true), setup = large, config = measured(42))]
fn previous(mut queue: Queue<u32>) -> (Queue<u32>, Option<EntryId>) {
    let previous = queue.previous();
    (queue, previous)
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large_and_last, config = measured(50_047))]
#[bench::shuffled(args = (true), setup = large_and_last, config = measured(50_047))]
fn jump_to((mut queue, last): (Queue<u32>, EntryId)) -> Queue<u32> {
    black_box(queue.jump_to(black_box(last))).expect("the last entry is in the queue");
    queue
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large_and_current, config = measured(140_091))]
#[bench::shuffled(args = (true), setup = large_and_current, config = measured(190_124))]
fn remove_current((mut queue, current): (Queue<u32>, EntryId)) -> Queue<u32> {
    black_box(queue.remove(black_box(current))).expect("the current entry is in the queue");
    queue
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large_and_last, config = measured(170_065))]
#[bench::shuffled(args = (true), setup = large_and_last, config = measured(106_462))]
fn move_last_to_front((mut queue, last): (Queue<u32>, EntryId)) -> Queue<u32> {
    black_box(queue.move_to(black_box(last), 0)).expect("the last entry is in the queue");
    queue
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large, config = measured(152_166))]
#[bench::shuffled(args = (true), setup = large, config = measured(353_795))]
fn play_next(mut queue: Queue<u32>) -> Queue<u32> {
    black_box(queue.play_next(black_box(0..ADDED))).expect("the ids do not run out");
    queue
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large, config = measured(945))]
#[bench::shuffled(args = (true), setup = large, config = measured(81_709))]
fn append(mut queue: Queue<u32>) -> Queue<u32> {
    black_box(queue.append(black_box(0..ADDED), Origin::Manual)).expect("the ids do not run out");
    queue
}

#[library_benchmark]
#[bench::unshuffled(args = (false), setup = large, config = measured(28_037))]
// Shuffled, `play_order` looks each id of the shuffled order up among the entries, one by one: its count grows with
// the square of the queue's length.
#[bench::shuffled(args = (true), setup = large, config = measured(250_199_586))]
fn play_order(queue: Queue<u32>) -> usize {
    black_box(queue.play_order()).len()
}

library_benchmark_group!(
    name = queue,
    benchmarks = [
        replace,
        shuffle,
        next,
        previous,
        jump_to,
        remove_current,
        move_last_to_front,
        play_next,
        append,
        play_order
    ]
);

main!(library_benchmark_groups = queue);
