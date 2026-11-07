//! Kani proofs of the queue invariants (core spec, section 3, invariants 1 to 5).
//!
//! Two kinds of proofs:
//!
//! - **Position logic, for any length.** Every move through the play order goes through `position_forward`,
//!   `position_back` and `position_after`, and every shuffle through `fisher_yates`. Their proofs take any length and
//!   any position, so a move can never point outside the queue and a shuffled order is always a permutation, whatever
//!   the queue's size.
//! - **Every operation, inductively, on small queues.** Starting from *any* queue of 0 to 3 entries that satisfies the
//!   invariants, with any current entry, repeat mode, shuffled order and next id, and with any arguments, each
//!   operation keeps the invariants, loses or duplicates no entry (invariant 5), and leaves the queue unchanged when it
//!   refuses. Operations that add entries are proved with 0 to 2 new entries, from the first free id, and again from
//!   every next id that leaves too few free ids, or just enough: they refuse exactly when the ids run out, and give
//!   out the ids that follow the next id.
//!
//! Kani also proves that none of this code panics on these inputs.
//!
//! The cost of these proofs comes from the heap and from values Kani cannot fold. Every `Vec` the code allocates, and
//! every write at a place that depends on symbolic data, multiplies the cases of each pointer dereference. That is
//! why the queue reads its play order in place, inserts rather than splices, moves entries by swapping neighbours,
//! and compares ids rather than `checked_add`ing them.

use super::*;

fn any_repeat() -> Repeat {
    match kani::any::<u8>() % 3 {
        0 => Repeat::Off,
        1 => Repeat::All,
        _ => Repeat::One,
    }
}

/// The starting queue, kept on the stack to check the queue against after an operation. Cloning the queue instead
/// would add two heap objects, and every heap object multiplies the cost of each pointer dereference under Kani.
struct Start {
    len: usize,
    entries: [Entry<u8>; 3],
    current: Option<EntryId>,
    repeat: Repeat,
    /// The play order when shuffled.
    order: Option<[EntryId; 3]>,
    rng: SplitMix64,
    next_id: u64,
}

impl Start {
    fn entries(&self) -> &[Entry<u8>] {
        &self.entries[..self.len]
    }

    /// Entry ids are their index in the starting queue.
    fn index_of(&self, id: EntryId) -> Option<usize> {
        let index = id.0 as usize;
        (index < self.len).then_some(index)
    }

    fn play_position(&self, id: EntryId) -> Option<usize> {
        match &self.order {
            Some(order) => order[..self.len].iter().position(|other| *other == id),
            None => self.index_of(id),
        }
    }

    /// Whether `queue` is exactly the starting queue.
    fn is(&self, queue: &Queue<u8>) -> bool {
        self.is_but_current(queue) && queue.current == self.current
    }

    /// Whether `queue` is the starting queue, except maybe for its current entry.
    fn is_but_current(&self, queue: &Queue<u8>) -> bool {
        let shuffle_is = match (&queue.shuffle, &self.order) {
            (Shuffle::Off, None) => true,
            (Shuffle::On { order, rng }, Some(start)) => {
                order[..] == start[..self.len] && *rng == self.rng
            }
            _ => false,
        };
        queue.entries[..] == *self.entries()
            && queue.source == QueueSource::Selection
            && queue.repeat == self.repeat
            && queue.next_id == self.next_id
            && shuffle_is
    }

    /// Invariant 5: every starting entry is still in `queue`, which has exactly `added` more. With unique ids
    /// (invariant 2), nothing was lost or duplicated.
    fn all_kept_in(&self, queue: &Queue<u8>, added: usize) -> bool {
        self.entries().iter().all(|e| queue.entry(e.id) == Some(e))
            && queue.entries.len() == self.len + added
    }
}

/// Any queue of `len` entries (at most 3) that satisfies the invariants: any media, current entry, repeat mode, next
/// id, and shuffle off or any shuffled order. Three entries already separate the first, the last and the current
/// position, which is all the operations tell apart; every extra entry multiplies the cases.
fn any_valid_queue(len: usize) -> (Queue<u8>, Start) {
    let next_id: u64 = kani::any();
    kani::assume(next_id >= 3);
    valid_queue(len, next_id)
}

/// As [`any_valid_queue`], with the given next id.
fn valid_queue(len: usize, next_id: u64) -> (Queue<u8>, Start) {
    valid_queue_shuffled(len, next_id, kani::any())
}

/// As [`valid_queue`], shuffled or not as told.
fn valid_queue_shuffled(len: usize, next_id: u64, shuffled: bool) -> (Queue<u8>, Start) {
    let entries = [
        Entry {
            id: EntryId(0),
            media: kani::any(),
            origin: Origin::Source,
        },
        Entry {
            id: EntryId(1),
            media: kani::any(),
            origin: Origin::Source,
        },
        Entry {
            id: EntryId(2),
            media: kani::any(),
            origin: Origin::Source,
        },
    ];
    kani::assume(len <= entries.len());
    let current = if kani::any() {
        let index: u64 = kani::any();
        kani::assume(index < len as u64);
        Some(EntryId(index))
    } else {
        None
    };
    let order = if shuffled {
        // Any order: distinct picks below the length.
        let picks: [u64; 3] = kani::any();
        let mut order = [EntryId(0); 3];
        for i in 0..len {
            kani::assume(picks[i] < len as u64);
            for j in 0..i {
                kani::assume(picks[j] != picks[i]);
            }
            order[i] = EntryId(picks[i]);
        }
        Some(order)
    } else {
        None
    };
    let start = Start {
        len,
        entries: entries.clone(),
        current,
        repeat: any_repeat(),
        order,
        rng: SplitMix64::new(kani::any()),
        next_id,
    };
    let queue = Queue {
        entries: entries[..len].to_vec(),
        source: QueueSource::Selection,
        current,
        repeat: start.repeat,
        shuffle: match order {
            Some(order) => Shuffle::On {
                order: order[..len].to_vec(),
                rng: start.rng.clone(),
            },
            None => Shuffle::Off,
        },
        next_id,
    };
    (queue, start)
}

/// Any id among those of the starting queue, plus one that is not in it.
fn any_id() -> EntryId {
    let id = EntryId(kani::any());
    kani::assume(id.0 <= 3);
    id
}

/// Invariant 4, as callers see it: with shuffle off, the play order is the queue order.
fn play_order_is_queue_order(queue: &Queue<u8>) -> bool {
    queue.is_shuffled()
        || (0..queue.entries.len())
            .all(|i| queue.play_id_at(i) == queue.entries.get(i).map(Entry::id))
}

fn holds(queue: &Queue<u8>) -> bool {
    queue.invariants_hold() && play_order_is_queue_order(queue)
}

/// The smallest next id of a starting queue: operations that add entries are proved from it, because a symbolic next
/// id makes every new id symbolic and the proofs run out of memory. The ids' values matter only where they run out,
/// so the same proofs run again from every next id that leaves too few free ids, or just enough.
const FIRST_FREE_ID: u64 = 3;

/// Whether `added` new entries fit in the ids left from `next_id`. The last id, `u64::MAX`, is never given out, since
/// the next id must stay above every id given out.
fn ids_left_for(next_id: u64, added: usize) -> bool {
    added as u64 <= u64::MAX - next_id
}

/// The id given to the entry added at `offset` from `next_id`; only called once the ids are known to fit.
fn new_id(next_id: u64, offset: usize) -> EntryId {
    EntryId(next_id + offset as u64)
}

/// After `ADDED` entries were added from `next_id`, the queue's next id follows the last id given out.
fn next_id_moved_on<const ADDED: usize>(queue: &Queue<u8>, next_id: u64) -> bool {
    queue.next_id == next_id + ADDED as u64
}

fn replace<const ADDED: usize>(len: usize, next_id: u64) {
    let (mut queue, start) = valid_queue(len, next_id);
    let media: [u8; ADDED] = kani::any();
    let position: Option<usize> = kani::any();
    match queue.replace(QueueSource::Collection(kani::any()), media, position) {
        Ok(()) => {
            assert!(ids_left_for(next_id, ADDED));
            assert!(position.is_none_or(|i| i < ADDED));
            assert!(holds(&queue));
            assert!(next_id_moved_on::<ADDED>(&queue, next_id));
            assert!(queue.len() == ADDED);
            assert!(
                queue
                    .entries
                    .iter()
                    .zip(media)
                    .enumerate()
                    .all(|(offset, (e, m))| e.id == new_id(next_id, offset)
                        && e.media == m
                        && e.origin == Origin::Source)
            );
            assert!(queue.current == position.and_then(|i| queue.entries.get(i)).map(Entry::id));
            assert!(queue.is_shuffled() == start.order.is_some() && queue.repeat == start.repeat);
        }
        Err(error) => {
            if ids_left_for(next_id, ADDED) {
                assert!(position.is_some_and(|i| i >= ADDED
                    && error
                        == QueueError::OutOfRange {
                            position: i,
                            len: ADDED
                        }));
            } else {
                assert!(error == QueueError::IdsExhausted);
            }
            assert!(start.is(&queue));
        }
    }
}

/// `play_next` is proved in two halves, the queue in order and shuffled: proved at once, it runs out of memory.
fn play_next_in_order<const ADDED: usize>(len: usize, next_id: u64) {
    play_next::<ADDED>(len, next_id, false);
}

fn play_next_shuffled<const ADDED: usize>(len: usize, next_id: u64) {
    play_next::<ADDED>(len, next_id, true);
}

fn play_next<const ADDED: usize>(len: usize, next_id: u64, shuffled: bool) {
    let (mut queue, start) = valid_queue_shuffled(len, next_id, shuffled);
    let media: [u8; ADDED] = kani::any();
    match queue.play_next(media) {
        Ok(()) => {
            assert!(ids_left_for(next_id, ADDED));
            assert!(holds(&queue));
            assert!(next_id_moved_on::<ADDED>(&queue, next_id));
            assert!(start.all_kept_in(&queue, ADDED));
            assert!(queue.current == start.current);
            // The new entries come right after the current one, in their order, in the queue and the play order.
            let at = index_after(start.current.and_then(|c| start.index_of(c)));
            let play_at = index_after(start.current.and_then(|c| start.play_position(c)));
            for (offset, m) in media.into_iter().enumerate() {
                let entry = &queue.entries[at + offset];
                assert!(entry.id == new_id(next_id, offset));
                assert!(entry.media == m && entry.origin == Origin::Manual);
                assert!(queue.play_id_at(play_at + offset) == Some(entry.id));
            }
        }
        Err(error) => {
            assert!(error == QueueError::IdsExhausted && !ids_left_for(next_id, ADDED));
            assert!(start.is(&queue));
        }
    }
}

fn append<const ADDED: usize>(len: usize, next_id: u64) {
    let (mut queue, start) = valid_queue(len, next_id);
    let media: [u8; ADDED] = kani::any();
    let origin = if kani::any() {
        Origin::Manual
    } else {
        Origin::Radio
    };
    match queue.append(media, origin) {
        Ok(()) => {
            assert!(ids_left_for(next_id, ADDED));
            assert!(holds(&queue));
            assert!(next_id_moved_on::<ADDED>(&queue, next_id));
            assert!(start.all_kept_in(&queue, ADDED));
            assert!(queue.current == start.current);
            for (offset, m) in media.into_iter().enumerate() {
                let entry = &queue.entries[len + offset];
                assert!(entry.id == new_id(next_id, offset));
                assert!(entry.media == m && entry.origin == origin);
                assert!(queue.play_id_at(len + offset) == Some(entry.id));
            }
        }
        Err(error) => {
            assert!(error == QueueError::IdsExhausted && !ids_left_for(next_id, ADDED));
            assert!(start.is(&queue));
        }
    }
}

fn remove(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    let id = any_id();
    match queue.remove(id) {
        Ok(()) => {
            assert!(holds(&queue));
            assert!(queue.entry(id).is_none());
            assert!(
                start
                    .entries()
                    .iter()
                    .filter(|e| e.id != id)
                    .all(|e| queue.entry(e.id) == Some(e))
            );
            assert!(queue.len() + 1 == len);
            assert!(queue.current != Some(id));
            if start.current != Some(id) {
                assert!(queue.current == start.current);
            }
        }
        Err(_) => {
            assert!(start.is(&queue));
        }
    }
}

fn move_to(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    let id = any_id();
    let position: usize = kani::any();
    match queue.move_to(id, position) {
        Ok(()) => {
            assert!(holds(&queue));
            assert!(start.all_kept_in(&queue, 0));
            assert!(queue.play_id_at(position) == Some(id));
            assert!(queue.current == start.current);
            if queue.is_shuffled() {
                assert!(queue.entries[..] == *start.entries());
            }
        }
        Err(_) => {
            assert!(start.is(&queue));
        }
    }
}

fn clear(len: usize) {
    let (mut queue, _) = any_valid_queue(len);
    queue.clear();
    assert!(holds(&queue));
    assert!(queue.is_empty() && queue.current().is_none());
}

fn shuffle(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    queue.shuffle(kani::any());
    assert!(holds(&queue));
    assert!(queue.is_shuffled());
    assert!(queue.entries[..] == *start.entries() && queue.current == start.current);
    if let Some(current) = queue.current {
        assert!(queue.play_id_at(0) == Some(current));
    }
}

fn unshuffle(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    queue.unshuffle();
    assert!(holds(&queue));
    assert!(!queue.is_shuffled());
    assert!(queue.entries[..] == *start.entries() && queue.current == start.current);
}

fn set_repeat(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    let repeat = any_repeat();
    queue.set_repeat(repeat);
    assert!(holds(&queue));
    assert!(queue.repeat == repeat);
    assert!(queue.entries[..] == *start.entries() && queue.current == start.current);
}

fn next(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    let advance = if kani::any() {
        Advance::Requested
    } else {
        Advance::Finished
    };
    let next = queue.next(advance);
    assert!(holds(&queue));
    assert!(start.is_but_current(&queue));
    match next {
        Some(id) => {
            assert!(queue.current == Some(id));
        }
        None => {
            assert!(queue.current == start.current);
        }
    }
    if start.repeat == Repeat::One && advance == Advance::Finished && start.current.is_some() {
        assert!(next == start.current);
    }
}

fn previous(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    let previous = queue.previous();
    assert!(holds(&queue));
    assert!(start.is_but_current(&queue));
    match previous {
        Some(id) => {
            assert!(queue.current == Some(id));
            assert!(start.current != Some(id));
        }
        None => {
            assert!(queue.current == start.current);
        }
    }
}

fn jump_to(len: usize) {
    let (mut queue, start) = any_valid_queue(len);
    let id = any_id();
    match queue.jump_to(id) {
        Ok(()) => {
            assert!(holds(&queue));
            assert!(queue.current == Some(id));
            assert!(start.is_but_current(&queue));
        }
        Err(_) => {
            assert!(start.is(&queue));
        }
    }
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

// Every operation, inductively, on small queues.

/// One proof per starting length: Kani reasons far better about concrete lengths than about a symbolic one.
macro_rules! proofs_for_each_length {
    ($($check:ident),* $(,)?) => {
        $(
            mod $check {
                #[kani::proof]
                #[kani::unwind(6)]
                fn from_0_entries() {
                    super::$check(0);
                }

                #[kani::proof]
                #[kani::unwind(6)]
                fn from_1_entry() {
                    super::$check(1);
                }

                #[kani::proof]
                #[kani::unwind(6)]
                fn from_2_entries() {
                    super::$check(2);
                }

                #[kani::proof]
                #[kani::unwind(6)]
                fn from_3_entries() {
                    super::$check(3);
                }
            }
        )*
    };
}

/// One proof per starting length and number of added entries, 0 to 2.
macro_rules! proofs_for_each_length_and_added {
    ($($check:ident $(with $solver:ident)?),* $(,)?) => {
        $(
            mod $check {
                proofs_for_each_length_and_added!(@added $check, adding_0, 0 $(, $solver)?);
                proofs_for_each_length_and_added!(@added $check, adding_1, 1 $(, $solver)?);
                proofs_for_each_length_and_added!(@added $check, adding_2, 2 $(, $solver)?);
            }
        )*
    };
    (@added $check:ident, $module:ident, $added:literal $(, $solver:ident)?) => {
        mod $module {
            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_0_entries() {
                super::super::$check::<$added>(0, super::super::FIRST_FREE_ID);
            }

            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_1_entry() {
                super::super::$check::<$added>(1, super::super::FIRST_FREE_ID);
            }

            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_2_entries() {
                super::super::$check::<$added>(2, super::super::FIRST_FREE_ID);
            }

            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_3_entries() {
                super::super::$check::<$added>(3, super::super::FIRST_FREE_ID);
            }
        }
    };
}

proofs_for_each_length!(
    remove, move_to, clear, shuffle, unshuffle, set_repeat, next, previous, jump_to
);
// `play_next` on a shuffled queue runs out of memory with Kani's default solver, CaDiCaL, from 3 entries with one
// added (measured on 2026-11-06, Kani 0.68.0, a 4-core 16 GB Linux container, one run); MiniSat proves it in 35 s.
proofs_for_each_length_and_added!(replace, play_next_in_order, play_next_shuffled with minisat, append);

/// One proof per operation that adds entries, number added, free ids left (one short and refused, or just enough and
/// accepted) and starting length, with the same checks as from the first free id.
macro_rules! ids_run_out_proofs {
    ($($check:ident $(with $solver:ident)?),* $(,)?) => {
        mod when_ids_run_out {
            $(
                mod $check {
                    ids_run_out_proofs!(@case $check, adding_1_with_0_left, 1, 0 $(, $solver)?);
                    ids_run_out_proofs!(@case $check, adding_1_with_1_left, 1, 1 $(, $solver)?);
                    ids_run_out_proofs!(@case $check, adding_2_with_0_left, 2, 0 $(, $solver)?);
                    ids_run_out_proofs!(@case $check, adding_2_with_1_left, 2, 1 $(, $solver)?);
                    ids_run_out_proofs!(@case $check, adding_2_with_2_left, 2, 2 $(, $solver)?);
                }
            )*
        }
    };
    (@case $check:ident, $name:ident, $added:literal, $left:literal $(, $solver:ident)?) => {
        mod $name {
            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_0_entries() {
                super::super::super::$check::<$added>(0, u64::MAX - $left);
            }

            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_1_entry() {
                super::super::super::$check::<$added>(1, u64::MAX - $left);
            }

            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_2_entries() {
                super::super::super::$check::<$added>(2, u64::MAX - $left);
            }

            #[kani::proof]
            #[kani::unwind(6)]
            $(#[kani::solver($solver)])?
            fn from_3_entries() {
                super::super::super::$check::<$added>(3, u64::MAX - $left);
            }
        }
    };
}

ids_run_out_proofs!(replace, play_next_in_order, play_next_shuffled with minisat, append);
