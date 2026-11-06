use super::*;
use proptest::prelude::*;

fn queue_of(media: &[u8], start: Option<usize>) -> Queue<u8> {
    let mut queue = Queue::new();
    queue
        .replace(QueueSource::Selection, media.iter().copied(), start)
        .unwrap();
    queue
}

fn media_in_play_order(queue: &Queue<u8>) -> Vec<u8> {
    queue.play_order().iter().map(|e| *e.media()).collect()
}

fn media_in_source_order(queue: &Queue<u8>) -> Vec<u8> {
    queue.entries().iter().map(|e| *e.media()).collect()
}

fn current_media(queue: &Queue<u8>) -> Option<u8> {
    queue.current().map(|e| *e.media())
}

fn id_at(queue: &Queue<u8>, position: usize) -> EntryId {
    queue.entries()[position].id()
}

// Construction and accessors.

#[test]
fn a_new_queue_is_empty_with_repeat_and_shuffle_off() {
    let queue: Queue<u8> = Queue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert_eq!(queue.current(), None);
    assert_eq!(queue.repeat(), Repeat::Off);
    assert!(!queue.is_shuffled());
    assert_eq!(queue.source(), &QueueSource::Selection);
    assert!(queue.invariants_hold());
}

#[test]
fn ids_start_at_zero_and_count_up() {
    let queue = queue_of(&[7, 8], None);
    assert_eq!(id_at(&queue, 0), EntryId(0));
    assert_eq!(id_at(&queue, 1), EntryId(1));
}

#[test]
fn entry_finds_by_id_and_reports_media_and_origin() {
    let queue = queue_of(&[7, 8], None);
    let entry = queue.entry(id_at(&queue, 1)).unwrap();
    assert_eq!(*entry.media(), 8);
    assert_eq!(entry.origin(), Origin::Source);
    assert_eq!(queue.entry(EntryId(99)), None);
}

// Replace.

#[test]
fn replace_sets_entries_source_and_current() {
    let mut queue = Queue::new();
    queue
        .replace(QueueSource::Collection(42), [1, 2, 3], Some(1))
        .unwrap();
    assert_eq!(media_in_source_order(&queue), [1, 2, 3]);
    assert_eq!(queue.len(), 3);
    assert!(!queue.is_empty());
    assert_eq!(queue.source(), &QueueSource::Collection(42));
    assert_eq!(current_media(&queue), Some(2));
}

#[test]
fn replace_without_start_has_no_current() {
    let queue = queue_of(&[1, 2], None);
    assert_eq!(queue.current(), None);
}

#[test]
fn replace_out_of_range_changes_nothing() {
    let mut queue = queue_of(&[1, 2], Some(0));
    let before = queue.clone();
    assert_eq!(
        queue.replace(QueueSource::Selection, [5, 6, 7], Some(3)),
        Err(QueueError::OutOfRange {
            position: 3,
            len: 3
        })
    );
    assert_eq!(queue, before);
}

#[test]
fn replace_never_reuses_an_id() {
    let mut queue = queue_of(&[1, 2], None);
    let old: Vec<EntryId> = queue.entries().iter().map(Entry::id).collect();
    queue.replace(QueueSource::Selection, [1, 2], None).unwrap();
    assert!(queue.entries().iter().all(|e| !old.contains(&e.id())));
}

#[test]
fn replace_keeps_shuffle_and_repeat_and_puts_current_first() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    queue.set_repeat(Repeat::All);
    queue.shuffle(5);
    queue
        .replace(QueueSource::Selection, [10, 11, 12, 13, 14], Some(3))
        .unwrap();
    assert!(queue.is_shuffled());
    assert_eq!(queue.repeat(), Repeat::All);
    assert_eq!(media_in_play_order(&queue)[0], 13);
    assert!(queue.invariants_hold());
}

#[test]
fn replace_reshuffles_from_the_kept_generator() {
    let mut a = queue_of(&[1, 2, 3], Some(0));
    a.shuffle(5);
    let mut b = a.clone();
    a.replace(QueueSource::Selection, [10, 11, 12, 13, 14, 15], Some(0))
        .unwrap();
    b.replace(QueueSource::Selection, [10, 11, 12, 13, 14, 15], Some(0))
        .unwrap();
    assert_eq!(media_in_play_order(&a), media_in_play_order(&b));
}

#[test]
fn ids_running_out_is_an_error_that_changes_nothing() {
    let mut queue = queue_of(&[1], Some(0));
    queue.next_id = u64::MAX;
    let before = queue.clone();
    assert_eq!(
        queue.append([2], Origin::Manual),
        Err(QueueError::IdsExhausted)
    );
    assert_eq!(queue.play_next([2]), Err(QueueError::IdsExhausted));
    assert_eq!(
        queue.replace(QueueSource::Selection, [2], Some(0)),
        Err(QueueError::IdsExhausted)
    );
    assert_eq!(queue, before);
}

// Play next and append.

#[test]
fn play_next_inserts_after_current_keeping_its_order() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    queue.play_next([8, 9]).unwrap();
    assert_eq!(media_in_source_order(&queue), [1, 8, 9, 2, 3]);
    assert_eq!(current_media(&queue), Some(1));
    assert!(
        queue.entries()[1..3]
            .iter()
            .all(|e| e.origin() == Origin::Manual)
    );
}

#[test]
fn play_next_without_current_inserts_first() {
    let mut queue = queue_of(&[1, 2], None);
    queue.play_next([9]).unwrap();
    assert_eq!(media_in_source_order(&queue), [9, 1, 2]);
}

#[test]
fn play_next_when_shuffled_goes_right_after_current_in_play_order() {
    let mut queue = queue_of(&[1, 2, 3, 4], Some(2));
    queue.shuffle(11);
    queue.play_next([9]).unwrap();
    let order = media_in_play_order(&queue);
    assert_eq!(&order[..2], [3, 9]);
    assert!(queue.invariants_hold());
}

#[test]
fn append_adds_at_the_end_with_its_origin() {
    let mut queue = queue_of(&[1], Some(0));
    queue.append([5, 6], Origin::Radio).unwrap();
    assert_eq!(media_in_source_order(&queue), [1, 5, 6]);
    assert_eq!(queue.entries()[2].origin(), Origin::Radio);
}

#[test]
fn append_when_shuffled_adds_at_the_end_of_play_order() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    queue.shuffle(3);
    queue.append([9], Origin::Manual).unwrap();
    assert_eq!(media_in_play_order(&queue).last(), Some(&9));
    assert!(queue.invariants_hold());
}

// Remove.

#[test]
fn remove_unknown_changes_nothing() {
    let mut queue = queue_of(&[1, 2], Some(0));
    let before = queue.clone();
    assert_eq!(
        queue.remove(EntryId(99)),
        Err(QueueError::UnknownEntry(EntryId(99)))
    );
    assert_eq!(queue, before);
}

#[test]
fn removing_another_entry_keeps_current() {
    let mut queue = queue_of(&[1, 2, 3], Some(1));
    queue.remove(id_at(&queue, 2)).unwrap();
    assert_eq!(media_in_source_order(&queue), [1, 2]);
    assert_eq!(current_media(&queue), Some(2));
}

#[test]
fn removing_current_moves_to_the_next() {
    let mut queue = queue_of(&[1, 2, 3], Some(1));
    queue.remove(id_at(&queue, 1)).unwrap();
    assert_eq!(current_media(&queue), Some(3));
}

#[test]
fn removing_the_last_current_entry_leaves_none_with_repeat_off() {
    let mut queue = queue_of(&[1, 2], Some(1));
    queue.remove(id_at(&queue, 1)).unwrap();
    assert_eq!(queue.current(), None);
    assert!(queue.invariants_hold());
}

#[test]
fn removing_the_last_current_entry_wraps_with_repeat_all() {
    let mut queue = queue_of(&[1, 2], Some(1));
    queue.set_repeat(Repeat::All);
    queue.remove(id_at(&queue, 1)).unwrap();
    assert_eq!(current_media(&queue), Some(1));
}

#[test]
fn removing_the_only_entry_with_repeat_all_leaves_none() {
    let mut queue = queue_of(&[1], Some(0));
    queue.set_repeat(Repeat::All);
    queue.remove(id_at(&queue, 0)).unwrap();
    assert_eq!(queue.current(), None);
    assert!(queue.is_empty());
}

#[test]
fn removing_current_with_repeat_one_moves_on_without_wrapping() {
    let mut queue = queue_of(&[1, 2], Some(1));
    queue.set_repeat(Repeat::One);
    queue.remove(id_at(&queue, 1)).unwrap();
    assert_eq!(queue.current(), None);
}

#[test]
fn removing_when_shuffled_follows_the_play_order() {
    let mut queue = queue_of(&[1, 2, 3, 4, 5], Some(0));
    queue.shuffle(21);
    let order = media_in_play_order(&queue);
    let current = queue.current().unwrap().id();
    queue.remove(current).unwrap();
    assert_eq!(current_media(&queue), Some(order[1]));
    assert!(queue.invariants_hold());
}

// Move.

#[test]
fn move_reorders_the_source_when_not_shuffled() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    queue.move_to(id_at(&queue, 0), 2).unwrap();
    assert_eq!(media_in_source_order(&queue), [2, 3, 1]);
    assert_eq!(current_media(&queue), Some(1));
}

#[test]
fn move_reorders_only_the_play_order_when_shuffled() {
    let mut queue = queue_of(&[1, 2, 3, 4], Some(0));
    queue.shuffle(8);
    let id = queue.play_order()[3].id();
    queue.move_to(id, 1).unwrap();
    assert_eq!(queue.play_order()[1].id(), id);
    assert_eq!(media_in_source_order(&queue), [1, 2, 3, 4]);
    assert!(queue.invariants_hold());
}

#[test]
fn move_out_of_range_or_unknown_changes_nothing() {
    let mut queue = queue_of(&[1, 2], Some(0));
    let before = queue.clone();
    assert_eq!(
        queue.move_to(id_at(&queue, 0), 2),
        Err(QueueError::OutOfRange {
            position: 2,
            len: 2
        })
    );
    assert_eq!(
        queue.move_to(EntryId(99), 0),
        Err(QueueError::UnknownEntry(EntryId(99)))
    );
    queue.shuffle(1);
    let shuffled = queue.clone();
    assert_eq!(
        queue.move_to(EntryId(99), 0),
        Err(QueueError::UnknownEntry(EntryId(99)))
    );
    assert_eq!(queue, shuffled);
    queue.unshuffle();
    assert_eq!(
        media_in_source_order(&queue),
        media_in_source_order(&before)
    );
}

// Clear and settings.

#[test]
fn clear_empties_and_keeps_settings() {
    let mut queue = Queue::new();
    queue
        .replace(QueueSource::Collection(3), [1, 2], Some(0))
        .unwrap();
    queue.set_repeat(Repeat::One);
    queue.shuffle(2);
    queue.clear();
    assert!(queue.is_empty());
    assert_eq!(queue.current(), None);
    assert_eq!(queue.source(), &QueueSource::Selection);
    assert_eq!(queue.repeat(), Repeat::One);
    assert!(queue.is_shuffled());
    assert_eq!(queue.play_order().len(), 0);
    assert!(queue.invariants_hold());
}

// Shuffle.

#[test]
fn shuffle_puts_current_first_and_is_deterministic() {
    let mut a = queue_of(&[1, 2, 3, 4, 5, 6], Some(3));
    let mut b = a.clone();
    a.shuffle(42);
    b.shuffle(42);
    assert_eq!(media_in_play_order(&a), media_in_play_order(&b));
    assert_eq!(media_in_play_order(&a)[0], 4);
    assert_eq!(media_in_source_order(&a), [1, 2, 3, 4, 5, 6]);
}

/// Pins the order for one seed, so that a change to the shuffle algorithm or the generator is a visible change. The
/// expected order was computed by a separate Python transcription of `shuffled_order` and `splitmix.c`.
#[test]
fn shuffle_order_for_a_known_seed() {
    let mut queue = queue_of(&[1, 2, 3, 4, 5, 6], None);
    queue.shuffle(42);
    assert_eq!(media_in_play_order(&queue), [5, 4, 1, 3, 6, 2]);
}

#[test]
fn different_seeds_give_different_orders() {
    let mut a = queue_of(&[1, 2, 3, 4, 5, 6, 7, 8], None);
    let mut b = a.clone();
    a.shuffle(1);
    b.shuffle(2);
    assert_ne!(media_in_play_order(&a), media_in_play_order(&b));
}

#[test]
fn unshuffle_restores_source_order_and_keeps_current() {
    let mut queue = queue_of(&[1, 2, 3, 4], Some(2));
    queue.shuffle(9);
    queue.unshuffle();
    assert!(!queue.is_shuffled());
    assert_eq!(media_in_play_order(&queue), [1, 2, 3, 4]);
    assert_eq!(current_media(&queue), Some(3));
}

// Next.

#[test]
fn next_without_current_starts_at_the_first() {
    let mut queue = queue_of(&[1, 2], None);
    assert_eq!(queue.next(Advance::Requested), Some(id_at(&queue, 0)));
    assert_eq!(current_media(&queue), Some(1));
}

#[test]
fn next_on_an_empty_queue_does_nothing() {
    let mut queue: Queue<u8> = Queue::new();
    assert_eq!(queue.next(Advance::Finished), None);
    assert_eq!(queue.current(), None);
}

#[test]
fn next_moves_forward() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    assert_eq!(queue.next(Advance::Finished), Some(id_at(&queue, 1)));
    assert_eq!(current_media(&queue), Some(2));
}

#[test]
fn next_at_the_end_with_repeat_off_stays() {
    let mut queue = queue_of(&[1, 2], Some(1));
    assert_eq!(queue.next(Advance::Finished), None);
    assert_eq!(current_media(&queue), Some(2));
}

#[test]
fn next_at_the_end_with_repeat_all_wraps() {
    let mut queue = queue_of(&[1, 2], Some(1));
    queue.set_repeat(Repeat::All);
    assert_eq!(queue.next(Advance::Requested), Some(id_at(&queue, 0)));
}

#[test]
fn repeat_one_replays_only_when_the_entry_finishes() {
    let mut queue = queue_of(&[1, 2], Some(0));
    queue.set_repeat(Repeat::One);
    assert_eq!(queue.next(Advance::Finished), Some(id_at(&queue, 0)));
    assert_eq!(queue.next(Advance::Requested), Some(id_at(&queue, 1)));
    assert_eq!(queue.next(Advance::Requested), None);
    assert_eq!(current_media(&queue), Some(2));
}

#[test]
fn next_follows_the_shuffled_order() {
    let mut queue = queue_of(&[1, 2, 3, 4, 5], Some(0));
    queue.shuffle(77);
    let order = media_in_play_order(&queue);
    queue.next(Advance::Finished);
    assert_eq!(current_media(&queue), Some(order[1]));
}

// Previous.

#[test]
fn previous_moves_back() {
    let mut queue = queue_of(&[1, 2, 3], Some(2));
    assert_eq!(queue.previous(), Some(id_at(&queue, 1)));
    assert_eq!(current_media(&queue), Some(2));
}

#[test]
fn previous_at_the_first_stays_unless_repeat_all() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    assert_eq!(queue.previous(), None);
    assert_eq!(current_media(&queue), Some(1));
    queue.set_repeat(Repeat::One);
    assert_eq!(queue.previous(), None);
    queue.set_repeat(Repeat::All);
    assert_eq!(queue.previous(), Some(id_at(&queue, 2)));
    assert_eq!(current_media(&queue), Some(3));
}

#[test]
fn previous_of_a_single_entry_with_repeat_all_stays() {
    let mut queue = queue_of(&[1], Some(0));
    queue.set_repeat(Repeat::All);
    assert_eq!(queue.previous(), None);
}

#[test]
fn previous_without_current_does_nothing() {
    let mut queue = queue_of(&[1, 2], None);
    assert_eq!(queue.previous(), None);
    assert_eq!(queue.current(), None);
}

#[test]
fn previous_follows_the_shuffled_order() {
    let mut queue = queue_of(&[1, 2, 3, 4, 5], Some(0));
    queue.shuffle(77);
    let order = media_in_play_order(&queue);
    queue.next(Advance::Requested);
    queue.next(Advance::Requested);
    queue.previous();
    assert_eq!(current_media(&queue), Some(order[1]));
}

// Jump.

#[test]
fn jump_to_makes_an_entry_current() {
    let mut queue = queue_of(&[1, 2, 3], Some(0));
    queue.jump_to(id_at(&queue, 2)).unwrap();
    assert_eq!(current_media(&queue), Some(3));
}

#[test]
fn jump_to_unknown_changes_nothing() {
    let mut queue = queue_of(&[1, 2], Some(0));
    let before = queue.clone();
    assert_eq!(
        queue.jump_to(EntryId(99)),
        Err(QueueError::UnknownEntry(EntryId(99)))
    );
    assert_eq!(queue, before);
}

// The invariant checker itself must catch what it claims to catch.

#[test]
fn invariants_detect_each_violation() {
    let base = queue_of(&[1, 2, 3], Some(0));
    assert!(base.invariants_hold());

    let mut duplicate = base.clone();
    duplicate.entries[1].id = duplicate.entries[0].id;
    assert!(!duplicate.invariants_hold());

    let mut not_given_out = base.clone();
    not_given_out.entries[2].id = EntryId(not_given_out.next_id);
    assert!(!not_given_out.invariants_hold());

    let mut stray_current = base.clone();
    stray_current.current = Some(EntryId(50));
    assert!(!stray_current.invariants_hold());

    let mut shuffled = base.clone();
    shuffled.shuffle(4);
    let mut missing = shuffled.clone();
    if let Shuffle::On { order, .. } = &mut missing.shuffle {
        order.pop();
    }
    assert!(!missing.invariants_hold());

    let mut repeated = shuffled.clone();
    if let Shuffle::On { order, .. } = &mut repeated.shuffle {
        order[1] = order[0];
    }
    assert!(!repeated.invariants_hold());
}

// Long random sequences: every operation keeps the invariants, a refused operation changes nothing, and only
// `remove`, `clear` and `replace` can make an entry disappear.

#[derive(Clone, Debug)]
enum Op {
    Replace(Vec<u8>, Option<usize>),
    PlayNext(Vec<u8>),
    Append(Vec<u8>),
    Remove(usize),
    Move(usize, usize),
    Clear,
    Repeat(u8),
    Shuffle(u64),
    Unshuffle,
    Next(bool),
    Previous,
    JumpTo(usize),
}

fn op() -> impl Strategy<Value = Op> {
    let media = || prop::collection::vec(any::<u8>(), 0..6);
    prop_oneof![
        (media(), prop::option::of(0usize..8)).prop_map(|(m, s)| Op::Replace(m, s)),
        media().prop_map(Op::PlayNext),
        media().prop_map(Op::Append),
        (0usize..40).prop_map(Op::Remove),
        ((0usize..40), (0usize..12)).prop_map(|(i, to)| Op::Move(i, to)),
        Just(Op::Clear),
        (0u8..3).prop_map(Op::Repeat),
        any::<u64>().prop_map(Op::Shuffle),
        Just(Op::Unshuffle),
        any::<bool>().prop_map(Op::Next),
        Just(Op::Previous),
        (0usize..40).prop_map(Op::JumpTo),
    ]
}

/// An index into the queue's ids, or an id that is not in the queue when the index is past the end.
fn pick_id(queue: &Queue<u8>, index: usize) -> EntryId {
    queue
        .entries()
        .get(index)
        .map_or(EntryId(u64::MAX), Entry::id)
}

fn apply(queue: &mut Queue<u8>, op: &Op) -> Result<(), QueueError> {
    match op {
        Op::Replace(media, start) => queue.replace(QueueSource::Selection, media.clone(), *start),
        Op::PlayNext(media) => queue.play_next(media.clone()),
        Op::Append(media) => queue.append(media.clone(), Origin::Manual),
        Op::Remove(i) => queue.remove(pick_id(queue, *i)),
        Op::Move(i, to) => queue.move_to(pick_id(queue, *i), *to),
        Op::Clear => {
            queue.clear();
            Ok(())
        }
        Op::Repeat(r) => {
            queue.set_repeat(match r {
                0 => Repeat::Off,
                1 => Repeat::All,
                _ => Repeat::One,
            });
            Ok(())
        }
        Op::Shuffle(seed) => {
            queue.shuffle(*seed);
            Ok(())
        }
        Op::Unshuffle => {
            queue.unshuffle();
            Ok(())
        }
        Op::Next(finished) => {
            queue.next(if *finished {
                Advance::Finished
            } else {
                Advance::Requested
            });
            Ok(())
        }
        Op::Previous => {
            queue.previous();
            Ok(())
        }
        Op::JumpTo(i) => queue.jump_to(pick_id(queue, *i)),
    }
}

fn sorted_ids(queue: &Queue<u8>) -> Vec<EntryId> {
    let mut ids: Vec<EntryId> = queue.entries().iter().map(Entry::id).collect();
    ids.sort();
    ids
}

proptest! {
    #[test]
    fn random_operation_sequences_keep_the_invariants(ops in prop::collection::vec(op(), 0..64)) {
        let mut queue = Queue::new();
        for op in &ops {
            let before = queue.clone();
            let result = apply(&mut queue, op);
            prop_assert!(queue.invariants_hold(), "invariants broken by {:?}", op);
            if result.is_err() {
                prop_assert_eq!(&queue, &before, "a refused {:?} changed the queue", op);
            }
            let (old, new) = (sorted_ids(&before), sorted_ids(&queue));
            match op {
                Op::Replace(..) | Op::Clear => {}
                Op::Remove(_) if result.is_ok() => {
                    prop_assert_eq!(new.len().checked_add(1), Some(old.len()));
                    prop_assert!(new.iter().all(|id| old.contains(id)));
                }
                Op::PlayNext(m) | Op::Append(m) => {
                    prop_assert_eq!(Some(new.len()), old.len().checked_add(m.len()));
                    prop_assert!(old.iter().all(|id| new.contains(id)));
                }
                _ => prop_assert_eq!(new, old, "{:?} changed the set of entries", op),
            }
        }
    }
}
