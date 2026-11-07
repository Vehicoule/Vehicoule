//! Runs any sequence of operations on a queue: never a panic, the invariants of the core spec (section 3) hold after
//! every operation, and each operation does what its documentation says and nothing else.
//!
//! The bytes are read as operations one after another until they run out; the media are small numbers, so the same
//! media often appears twice, as two entries. Ids are only taken from the queue itself (an `EntryId` cannot be made
//! from outside), and every id ever seen stays a candidate, so removed ids exercise the `UnknownEntry` paths.

#![no_main]

use libfuzzer_sys::arbitrary::{Result, Unstructured};
use libfuzzer_sys::fuzz_target;
use std::collections::HashSet;
use vehicoule_core::queue::{Advance, EntryId, Origin, Queue, QueueError, QueueSource, Repeat};

/// The most entries one operation adds: enough for every position case, few enough that a run stays fast.
const MAX_ADDED: usize = 16;

fn media(u: &mut Unstructured<'_>) -> Result<Vec<u8>> {
    let len = u.int_in_range(0..=MAX_ADDED)?;
    (0..len).map(|_| u.int_in_range(0..=7)).collect()
}

fn ids(queue: &Queue<u8>) -> Vec<EntryId> {
    queue.entries().iter().map(|entry| entry.id()).collect()
}

fn sorted(mut ids: Vec<EntryId>) -> Vec<EntryId> {
    ids.sort_unstable();
    ids
}

fn media_of(queue: &Queue<u8>) -> Vec<u8> {
    queue.entries().iter().map(|entry| *entry.media()).collect()
}

fn play_ids(queue: &Queue<u8>) -> Vec<EntryId> {
    queue.play_order().iter().map(|entry| entry.id()).collect()
}

fn current_id(queue: &Queue<u8>) -> Option<EntryId> {
    queue.current().map(|entry| entry.id())
}

/// Checks what every operation keeps: the invariants, and the play order of an unshuffled queue being its source order.
fn check_state(queue: &Queue<u8>) {
    assert!(queue.invariants_hold());
    assert_eq!(queue.len(), queue.entries().len());
    assert_eq!(queue.is_empty(), queue.entries().is_empty());
    assert_eq!(queue.play_order().len(), queue.len());
    if !queue.is_shuffled() {
        assert_eq!(play_ids(queue), ids(queue));
    }
    if let Some(current) = queue.current() {
        assert_eq!(queue.entry(current.id()), Some(current));
    }
}

/// Checks the entries added by `play_next` or `append`: fresh ids, the given media in order, the given origin.
fn check_added(
    queue: &Queue<u8>,
    added: &[EntryId],
    media: &[u8],
    origin: Origin,
    seen: &HashSet<EntryId>,
) {
    assert_eq!(added.len(), media.len());
    for (id, media) in added.iter().zip(media) {
        assert!(!seen.contains(id), "an id was given twice: {id:?}");
        let entry = queue.entry(*id).expect("an added entry is in the queue");
        assert_eq!(entry.media(), media);
        assert_eq!(entry.origin(), origin);
    }
}

fn step(
    u: &mut Unstructured<'_>,
    queue: &mut Queue<u8>,
    seen: &mut HashSet<EntryId>,
    pool: &mut Vec<EntryId>,
) -> Result<()> {
    let before = queue.clone();
    let before_ids = ids(queue);
    let before_current = current_id(queue);
    let pick = |u: &mut Unstructured<'_>, pool: &[EntryId]| -> Result<Option<EntryId>> {
        if pool.is_empty() {
            Ok(None)
        } else {
            u.choose(pool).map(|id| Some(*id))
        }
    };

    let operation = u.int_in_range(0..=11_u8)?;
    match operation {
        0 => {
            let source = if u.arbitrary()? {
                QueueSource::Collection(u.arbitrary()?)
            } else {
                QueueSource::Selection
            };
            let media = media(u)?;
            // Mostly a position of the media, sometimes past it.
            let start = if u.arbitrary()? {
                None
            } else {
                Some(u.int_in_range(0..=media.len().saturating_add(1))?)
            };
            match queue.replace(source.clone(), media.clone(), start) {
                Ok(()) => {
                    assert_eq!(media_of(queue), media);
                    assert_eq!(queue.source(), &source);
                    assert!(
                        queue
                            .entries()
                            .iter()
                            .all(|entry| entry.origin() == Origin::Source)
                    );
                    assert!(
                        ids(queue).iter().all(|id| !seen.contains(id)),
                        "replace reused an id"
                    );
                    assert_eq!(
                        queue.current().map(|entry| entry.id()),
                        start.map(|at| ids(queue)[at])
                    );
                    assert_eq!(queue.repeat(), before.repeat());
                    assert_eq!(queue.is_shuffled(), before.is_shuffled());
                    if let (true, Some(current)) = (queue.is_shuffled(), current_id(queue)) {
                        assert_eq!(play_ids(queue).first(), Some(&current));
                    }
                }
                Err(error) => {
                    assert_eq!(
                        error,
                        QueueError::OutOfRange {
                            position: start.unwrap_or(0),
                            len: media.len()
                        }
                    );
                    assert!(start.is_some_and(|at| at >= media.len()));
                    assert_eq!(queue, &before);
                }
            }
        }
        1 => {
            let media = media(u)?;
            queue
                .play_next(media.clone())
                .expect("a fuzz run never uses 2^64 ids");
            let after = ids(queue);
            let added: Vec<EntryId> = after
                .iter()
                .copied()
                .filter(|id| !before_ids.contains(id))
                .collect();
            check_added(queue, &added, &media, Origin::Manual, seen);
            // The old entries keep their order, and the new ones follow the current entry (or come first).
            let kept: Vec<EntryId> = after
                .iter()
                .copied()
                .filter(|id| before_ids.contains(id))
                .collect();
            assert_eq!(kept, before_ids);
            let at = before_current
                .and_then(|current| before_ids.iter().position(|id| *id == current))
                .map_or(0, |position| position + 1);
            assert_eq!(after.get(at..at + added.len()), Some(added.as_slice()));
            // The same holds in the play order, which differs from the source order when the queue is shuffled.
            let mut expected_play = play_ids(&before);
            let play_at = before_current
                .and_then(|current| expected_play.iter().position(|id| *id == current))
                .map_or(0, |position| position + 1);
            expected_play.splice(play_at..play_at, added.iter().copied());
            assert_eq!(play_ids(queue), expected_play);
            assert_eq!(current_id(queue), before_current);
        }
        2 => {
            let media = media(u)?;
            let origin = *u.choose(&[Origin::Source, Origin::Manual, Origin::Radio])?;
            queue
                .append(media.clone(), origin)
                .expect("a fuzz run never uses 2^64 ids");
            let after = ids(queue);
            assert_eq!(after.get(..before_ids.len()), Some(before_ids.as_slice()));
            let added = after.get(before_ids.len()..).unwrap_or_default().to_vec();
            check_added(queue, &added, &media, origin, seen);
            // In the play order too, after the old play order, which stays as it was even when shuffled.
            let mut expected_play = play_ids(&before);
            expected_play.extend_from_slice(&added);
            assert_eq!(play_ids(queue), expected_play);
            assert_eq!(current_id(queue), before_current);
        }
        3 => {
            let Some(id) = pick(u, pool)? else {
                return Ok(());
            };
            match queue.remove(id) {
                Ok(()) => {
                    let expected: Vec<EntryId> = before_ids
                        .iter()
                        .copied()
                        .filter(|other| *other != id)
                        .collect();
                    assert_eq!(ids(queue), expected);
                    assert!(queue.entry(id).is_none());
                    let before_play: Vec<EntryId> = play_ids(&before)
                        .into_iter()
                        .filter(|other| *other != id)
                        .collect();
                    assert_eq!(play_ids(queue), before_play);
                    if before_current == Some(id) {
                        // The entry that would have played next, or none (core spec, section 3, invariant 6).
                        let next =
                            expected_next(&before, Advance::Requested).filter(|next| *next != id);
                        assert_eq!(current_id(queue), next);
                    } else {
                        assert_eq!(current_id(queue), before_current);
                    }
                }
                Err(error) => {
                    assert_eq!(error, QueueError::UnknownEntry(id));
                    assert!(!before_ids.contains(&id));
                    assert_eq!(queue, &before);
                }
            }
        }
        4 => {
            let Some(id) = pick(u, pool)? else {
                return Ok(());
            };
            let position = u.int_in_range(0..=queue.len())?;
            match queue.move_to(id, position) {
                Ok(()) => {
                    // Only `id` moves: the play order is the old one with `id` taken out and put back at `position`.
                    let mut expected = play_ids(&before);
                    expected.retain(|other| *other != id);
                    expected.insert(position, id);
                    assert_eq!(play_ids(queue), expected);
                    assert_eq!(sorted(ids(queue)), sorted(before_ids.clone()));
                    if queue.is_shuffled() {
                        assert_eq!(
                            ids(queue),
                            before_ids,
                            "a shuffled move changed the source order"
                        );
                    }
                    assert_eq!(current_id(queue), before_current);
                }
                Err(QueueError::OutOfRange {
                    position: refused,
                    len,
                }) => {
                    assert_eq!((refused, len), (position, before_ids.len()));
                    assert!(position >= len);
                    assert_eq!(queue, &before);
                }
                Err(error) => {
                    assert_eq!(error, QueueError::UnknownEntry(id));
                    assert!(!before_ids.contains(&id));
                    assert_eq!(queue, &before);
                }
            }
        }
        5 => {
            queue.clear();
            assert!(queue.is_empty());
            assert_eq!(queue.current(), None);
            assert_eq!(queue.source(), &QueueSource::Selection);
            assert_eq!(queue.repeat(), before.repeat());
            assert_eq!(queue.is_shuffled(), before.is_shuffled());
        }
        6 => {
            let repeat = *u.choose(&[Repeat::Off, Repeat::All, Repeat::One])?;
            queue.set_repeat(repeat);
            assert_eq!(queue.repeat(), repeat);
            assert_eq!(ids(queue), before_ids);
            assert_eq!(play_ids(queue), play_ids(&before));
            assert_eq!(current_id(queue), before_current);
        }
        7 => {
            let seed = u.arbitrary()?;
            queue.shuffle(seed);
            assert!(queue.is_shuffled());
            assert_eq!(ids(queue), before_ids);
            assert_eq!(sorted(play_ids(queue)), sorted(before_ids.clone()));
            assert_eq!(current_id(queue), before_current);
            if let Some(current) = before_current {
                assert_eq!(play_ids(queue).first(), Some(&current));
            }
            // The same queue and seed give the same order.
            let mut again = before.clone();
            again.shuffle(seed);
            assert_eq!(play_ids(&again), play_ids(queue));
        }
        8 => {
            queue.unshuffle();
            assert!(!queue.is_shuffled());
            assert_eq!(ids(queue), before_ids);
            assert_eq!(current_id(queue), before_current);
        }
        9 => {
            let advance = *u.choose(&[Advance::Requested, Advance::Finished])?;
            let next = queue.next(advance);
            check_move(queue, &before, next);
            assert_eq!(next, expected_next(&before, advance));
        }
        10 => {
            let previous = queue.previous();
            check_move(queue, &before, previous);
            assert_eq!(previous, expected_previous(&before));
        }
        _ => {
            let Some(id) = pick(u, pool)? else {
                return Ok(());
            };
            match queue.jump_to(id) {
                Ok(()) => {
                    assert_eq!(current_id(queue), Some(id));
                    assert_eq!(ids(queue), before_ids);
                    assert_eq!(play_ids(queue), play_ids(&before));
                }
                Err(error) => {
                    assert_eq!(error, QueueError::UnknownEntry(id));
                    assert!(!before_ids.contains(&id));
                    assert_eq!(queue, &before);
                }
            }
        }
    }

    // What no operation changes besides its own documented effect. An entry is never edited: one that is still in the
    // queue keeps its media and origin. Only `replace` and `clear` change the source, only `set_repeat` the repeat
    // mode, and only `shuffle` and `unshuffle` turn the shuffle on or off.
    for entry in queue.entries() {
        if let Some(old) = before.entry(entry.id()) {
            assert_eq!(entry, old);
        }
    }
    if !matches!(operation, 0 | 5) {
        assert_eq!(queue.source(), before.source());
    }
    if operation != 6 {
        assert_eq!(queue.repeat(), before.repeat());
    }
    if !matches!(operation, 7 | 8) {
        assert_eq!(queue.is_shuffled(), before.is_shuffled());
    }
    check_state(queue);
    for id in ids(queue) {
        if seen.insert(id) {
            pool.push(id);
        }
    }
    Ok(())
}

/// The entry `next` moves to, written again from its documentation and `Repeat`'s, over the public play order.
fn expected_next(before: &Queue<u8>, advance: Advance) -> Option<EntryId> {
    let play = play_ids(before);
    let Some(current) = current_id(before) else {
        return play.first().copied();
    };
    if (before.repeat(), advance) == (Repeat::One, Advance::Finished) {
        return Some(current);
    }
    let position = play
        .iter()
        .position(|id| *id == current)
        .expect("the current entry is in the play order");
    match play.get(position + 1) {
        Some(after) => Some(*after),
        None if before.repeat() == Repeat::All => play.first().copied(),
        None => None,
    }
}

/// The entry `previous` moves to, written again from its documentation: none without a current entry or before the
/// first entry, except with repeat all, where the previous of the first is the last (when that is another entry).
fn expected_previous(before: &Queue<u8>) -> Option<EntryId> {
    let play = play_ids(before);
    let current = current_id(before)?;
    let position = play
        .iter()
        .position(|id| *id == current)
        .expect("the current entry is in the play order");
    match position.checked_sub(1) {
        Some(earlier) => play.get(earlier).copied(),
        None if before.repeat() == Repeat::All => {
            play.last().copied().filter(|last| *last != current)
        }
        None => None,
    }
}

/// Checks `next` and `previous`: a returned entry is current, `None` leaves the current entry, and neither changes the
/// entries, the play order, the repeat mode or the shuffle.
fn check_move(queue: &Queue<u8>, before: &Queue<u8>, moved_to: Option<EntryId>) {
    match moved_to {
        Some(id) => assert_eq!(current_id(queue), Some(id)),
        None => assert_eq!(current_id(queue), current_id(before)),
    }
    assert_eq!(ids(queue), ids(before));
    assert_eq!(play_ids(queue), play_ids(before));
    assert_eq!(queue.repeat(), before.repeat());
    assert_eq!(queue.is_shuffled(), before.is_shuffled());
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let mut queue = Queue::new();
    let mut seen = HashSet::new();
    let mut pool = Vec::new();
    check_state(&queue);
    while !u.is_empty() {
        if step(&mut u, &mut queue, &mut seen, &mut pool).is_err() {
            break;
        }
    }
});
