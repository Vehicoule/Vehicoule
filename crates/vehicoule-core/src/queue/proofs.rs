//! Kani proofs of the queue invariants (core spec, section 3, invariants 1 to 5).
//!
//! Two kinds of proofs:
//!
//! - **Position logic, for any length.** Every move through the play order goes through `position_forward`,
//!   `position_back` and `position_after`, and every shuffle through `fisher_yates`. Their proofs take any length and
//!   any position, so a move can never point outside the queue (invariant 1) and a shuffled order is always a
//!   permutation (invariant 3), whatever the queue's size.
//! - **Whole operations, inductively, on small queues.** Starting from *any* queue that satisfies the invariants, the
//!   operation keeps them. Kani's cost grows fast with each `Vec` insertion, removal or collection: on 2 and 3 entries,
//!   `replace`, `play_next`, `append`, `remove`, `move_to`, `shuffle` and navigation ran out of the 16 GB of memory or
//!   past 150 s each (measured on 2026-11-06, Kani 0.68.0, a 4-core 16 GB Linux container, one run each). Only
//!   `clear` and `unshuffle` verify (22 s and 38 s on 3 entries). The other operations are checked as whole
//!   operations by the property tests in `tests.rs` (long random sequences, every invariant after every step) and by
//!   mutation testing.
//!
//! Kani also proves that none of this code panics on these inputs.

use super::*;

fn any_repeat() -> Repeat {
    match kani::any::<u8>() % 3 {
        0 => Repeat::Off,
        1 => Repeat::All,
        _ => Repeat::One,
    }
}

/// The orders of three entries, by index, so the starting queues have at most three entries: every extra entry
/// multiplies the cases, and the operations treat positions alike beyond the first, the last and the current one,
/// which three entries already separate. Taking the indices below the length, in this order, gives every order of a
/// shorter queue too.
const ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

/// Any queue of `len` entries that satisfies the invariants: any media, current entry, repeat mode, next id, and
/// shuffle off or any shuffled order.
fn any_valid_queue(len: usize) -> Queue<u8> {
    let entries: Vec<Entry<u8>> = [EntryId(0), EntryId(1), EntryId(2)][..len]
        .iter()
        .map(|id| Entry {
            id: *id,
            media: kani::any(),
            origin: Origin::Source,
        })
        .collect();
    let next_id: u64 = kani::any();
    kani::assume(next_id >= 3);
    let current = if kani::any() {
        let index: usize = kani::any();
        kani::assume(index < len);
        Some(EntryId(index as u64))
    } else {
        None
    };
    let shuffle = if kani::any() {
        let which: usize = kani::any();
        kani::assume(which < ORDERS.len());
        let order = ORDERS[which]
            .iter()
            .filter(|i| **i < len)
            .map(|i| EntryId(*i as u64))
            .collect();
        Shuffle::On {
            order,
            rng: SplitMix64::new(kani::any()),
        }
    } else {
        Shuffle::Off
    };
    Queue {
        entries,
        source: QueueSource::Selection,
        current,
        repeat: any_repeat(),
        shuffle,
        next_id,
    }
}

fn ids(queue: &Queue<u8>) -> Vec<EntryId> {
    queue.entries().iter().map(Entry::id).collect()
}

/// Invariants 1 to 3 after `clear`, from any valid queue.
fn clear(len: usize) {
    let mut queue = any_valid_queue(len);
    queue.clear();
    assert!(queue.invariants_hold());
    assert!(queue.is_empty() && queue.current().is_none());
}

/// Invariant 4: with shuffle off, the play order is the queue order; unshuffling keeps every entry and the current
/// one.
fn unshuffle(len: usize) {
    let mut queue = any_valid_queue(len);
    let before = queue.clone();
    queue.unshuffle();
    assert!(queue.invariants_hold());
    assert!(queue.current == before.current);
    let play: Vec<EntryId> = queue.play_order().iter().map(|e| e.id()).collect();
    assert!(play == ids(&queue));
    assert!(ids(&queue) == ids(&before));
}

// Position logic, for any length.

#[kani::proof]
fn forward_stays_in_the_queue() {
    let (len, position, wrap): (usize, usize, bool) = (kani::any(), kani::any(), kani::any());
    kani::assume(position < len);
    match position_forward(len, position, wrap) {
        Some(next) => {
            assert!(next < len);
            assert!(next == position + 1 || (wrap && next == 0 && position + 1 == len));
        }
        None => {
            assert!(!wrap && position + 1 == len);
        }
    }
}

#[kani::proof]
fn back_stays_in_the_queue() {
    let (len, position, wrap): (usize, usize, bool) = (kani::any(), kani::any(), kani::any());
    kani::assume(position < len);
    match position_back(len, position, wrap) {
        Some(before) => {
            assert!(before < len);
            assert!(before + 1 == position || (wrap && position == 0 && before == len - 1));
        }
        None => {
            assert!(!wrap && position == 0);
        }
    }
}

/// The insertion point after the current entry is a valid `splice` position: never beyond the end.
#[kani::proof]
#[kani::unwind(6)]
fn insertion_point_is_in_bounds() {
    let ids = [EntryId(0), EntryId(1), EntryId(2), EntryId(3)];
    let len: usize = kani::any();
    kani::assume(len <= ids.len());
    let current: Option<EntryId> = if kani::any() {
        Some(EntryId(kani::any()))
    } else {
        None
    };
    assert!(position_after(&ids[..len], current) <= len);
}

/// Fisher-Yates keeps every id exactly once, for any choices of the generator.
#[kani::proof]
#[kani::unwind(7)]
fn fisher_yates_is_a_permutation() {
    let original = [EntryId(0), EntryId(1), EntryId(2), EntryId(3), EntryId(4)];
    let len: usize = kani::any();
    kani::assume(len <= original.len());
    let mut ids = original;
    fisher_yates(&mut ids[..len], &mut SplitMix64::new(kani::any()));
    for id in &original[..len] {
        assert!(ids[..len].iter().filter(|other| *other == id).count() == 1);
    }
    assert!(ids[len..] == original[len..]);
}

// Whole operations, inductively, on small queues.

/// One proof per starting length: Kani reasons far better about concrete lengths than about a symbolic one.
macro_rules! proofs_for_each_length {
    ($($check:ident),* $(,)?) => {
        $(
            mod $check {
                #[kani::proof]
                #[kani::unwind(8)]
                fn from_0_entries() {
                    super::$check(0);
                }

                #[kani::proof]
                #[kani::unwind(8)]
                fn from_1_entry() {
                    super::$check(1);
                }

                #[kani::proof]
                #[kani::unwind(8)]
                fn from_2_entries() {
                    super::$check(2);
                }

                #[kani::proof]
                #[kani::unwind(8)]
                fn from_3_entries() {
                    super::$check(3);
                }
            }
        )*
    };
}

proofs_for_each_length!(clear, unshuffle);
