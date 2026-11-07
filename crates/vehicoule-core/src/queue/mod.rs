//! The playback queue of one session (core spec, section 3:
//! <https://github.com/Vehicoule/Vehicoule/blob/main/docs/core-playback-spec.md>).
//!
//! A queue holds entries in the order of its source, an optional current entry, a repeat mode and a shuffle state. It
//! is generic over the media id `M`, so the music and video sessions share it (books are read, not played, and have no
//! queue). It does no I/O and never draws randomness: the shuffle seed comes in with the call.

mod rng;

#[cfg(kani)]
mod kani_rng;
#[cfg(kani)]
mod proofs;

use rng::SplitMix64;
use thiserror::Error;

/// Identifies one entry of a queue.
///
/// Unique for the queue's whole life: the same media added twice gives two entries with two ids, and an id is never
/// given again after its entry is removed.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(u64);

/// Why an entry is in the queue.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// It came with the queue's source (the album, playlist or selection the queue was filled from).
    Source,
    /// The user added it after the source.
    Manual,
    /// Radio appended it when the queue reached its end.
    Radio,
}

/// One entry of a queue: an id of its own and the media it plays.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry<M> {
    id: EntryId,
    media: M,
    origin: Origin,
}

impl<M> Entry<M> {
    /// The entry's id.
    pub fn id(&self) -> EntryId {
        self.id
    }

    /// The media this entry plays.
    pub fn media(&self) -> &M {
        &self.media
    }

    /// Why the entry is in the queue.
    pub fn origin(&self) -> Origin {
        self.origin
    }
}

/// What filled the queue.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum QueueSource<M> {
    /// A collection of the library or of a plugin: an album, a playlist, an artist.
    Collection(M),
    /// Media the user picked one by one, or nothing yet.
    #[default]
    Selection,
}

/// How the queue repeats.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Play to the end, then stop.
    #[default]
    Off,
    /// After the last entry, start again from the first.
    All,
    /// Replay the current entry when it ends by itself. `next` and `previous` still move, as if repeat were off.
    One,
}

/// Why the queue moves to the next entry.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advance {
    /// The user asked for the next entry.
    Requested,
    /// The current entry ended by itself.
    Finished,
}

/// An operation the queue refused. The queue is unchanged after an error.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum QueueError {
    /// No entry with this id is in the queue.
    #[error("no entry {0:?} in the queue")]
    UnknownEntry(EntryId),
    /// The position is not in the queue.
    #[error("position {position} is not in a queue of {len} entries")]
    OutOfRange {
        /// The position asked for.
        position: usize,
        /// The number of entries.
        len: usize,
    },
    /// Every entry id has been used. Needs 2^64 entries over the queue's life; reported rather than wrapped, so ids
    /// stay unique.
    #[error("the queue has used every entry id")]
    IdsExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Shuffle {
    Off,
    /// The play order, and the generator that made it, kept so that a later `replace` reshuffles deterministically.
    On {
        order: Vec<EntryId>,
        rng: SplitMix64,
    },
}

/// The queue of one playback session.
///
/// ```
/// use vehicoule_core::queue::{Advance, Queue, QueueSource};
///
/// let mut queue = Queue::new();
/// queue.replace(QueueSource::Selection, ["a", "b", "c"], Some(0))?;
/// assert_eq!(queue.current().map(|e| *e.media()), Some("a"));
/// queue.next(Advance::Requested);
/// assert_eq!(queue.current().map(|e| *e.media()), Some("b"));
/// # Ok::<(), vehicoule_core::queue::QueueError>(())
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queue<M> {
    entries: Vec<Entry<M>>,
    source: QueueSource<M>,
    current: Option<EntryId>,
    repeat: Repeat,
    shuffle: Shuffle,
    next_id: u64,
}

impl<M> Default for Queue<M> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            source: QueueSource::Selection,
            current: None,
            repeat: Repeat::Off,
            shuffle: Shuffle::Off,
            next_id: 0,
        }
    }
}

impl<M> Queue<M> {
    /// An empty queue, repeat off, shuffle off.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The entries in the order of the source, whatever the shuffle.
    pub fn entries(&self) -> &[Entry<M>] {
        &self.entries
    }

    /// The entries in the order they play: the source order, or the shuffled order when shuffle is on.
    pub fn play_order(&self) -> Vec<&Entry<M>> {
        match &self.shuffle {
            Shuffle::Off => self.entries.iter().collect(),
            Shuffle::On { order, .. } => {
                // Sorted by id once, so that each id of the order is found by a binary search: looking each one up
                // among the entries would cost the square of the queue's length (ADR 0016, benchmark `play_order`).
                let mut by_id: Vec<&Entry<M>> = self.entries.iter().collect();
                by_id.sort_unstable_by_key(|entry| entry.id);
                order
                    .iter()
                    .filter_map(|id| {
                        by_id
                            .binary_search_by_key(id, |entry| entry.id)
                            .ok()
                            .and_then(|at| by_id.get(at).copied())
                    })
                    .collect()
            }
        }
    }

    /// The entry being played, if any.
    pub fn current(&self) -> Option<&Entry<M>> {
        self.current.and_then(|id| self.entry(id))
    }

    /// The entry with this id, if it is in the queue.
    pub fn entry(&self, id: EntryId) -> Option<&Entry<M>> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// What filled the queue.
    pub fn source(&self) -> &QueueSource<M> {
        &self.source
    }

    /// The repeat mode.
    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Whether the play order is shuffled.
    pub fn is_shuffled(&self) -> bool {
        matches!(self.shuffle, Shuffle::On { .. })
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the queue has no entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replaces every entry with `media` from `source`, and makes the entry at `start` current (none if `start` is
    /// `None`). Shuffle and repeat are kept; a shuffled queue is reshuffled with the current entry first.
    ///
    /// # Errors
    ///
    /// [`QueueError::OutOfRange`] when `start` is not a position of `media`, [`QueueError::IdsExhausted`] when the ids
    /// run out. The queue is unchanged after an error.
    pub fn replace(
        &mut self,
        source: QueueSource<M>,
        media: impl IntoIterator<Item = M, IntoIter: ExactSizeIterator>,
        start: Option<usize>,
    ) -> Result<(), QueueError> {
        let mut next_id = self.next_id;
        let entries = make_entries(&mut next_id, media, Origin::Source)?;
        let current = match start {
            None => None,
            Some(position) => Some(entries.get(position).map(|e| e.id).ok_or(
                QueueError::OutOfRange {
                    position,
                    len: entries.len(),
                },
            )?),
        };
        if let Shuffle::On { rng, .. } = &self.shuffle {
            let mut rng = rng.clone();
            let order = shuffled_order(&entries, start, &mut rng);
            self.shuffle = Shuffle::On { order, rng };
        }
        self.next_id = next_id;
        self.entries = entries;
        self.source = source;
        self.current = current;
        Ok(())
    }

    /// Inserts `media` right after the current entry, in the source order and in the play order, keeping their own
    /// order. Without a current entry, inserts them first. They are marked [`Origin::Manual`].
    ///
    /// # Errors
    ///
    /// [`QueueError::IdsExhausted`] when the ids run out; the queue is unchanged.
    pub fn play_next(
        &mut self,
        media: impl IntoIterator<Item = M, IntoIter: ExactSizeIterator>,
    ) -> Result<(), QueueError> {
        let mut next_id = self.next_id;
        let added = make_entries(&mut next_id, media, Origin::Manual)?;
        self.next_id = next_id;
        let count = added.len();
        // Inserted one by one rather than spliced: `splice` is far heavier for Kani to model, and the queue only ever
        // inserts a handful of entries at a time.
        let at = self.position_after_current_in_entries();
        for (offset, entry) in added.into_iter().enumerate() {
            self.entries.insert(at.saturating_add(offset), entry);
        }
        if let Shuffle::On { order, .. } = &mut self.shuffle {
            let at_order = position_after(order, self.current);
            for (offset, entry) in self.entries.iter().skip(at).take(count).enumerate() {
                order.insert(at_order.saturating_add(offset), entry.id);
            }
        }
        Ok(())
    }

    /// Adds `media` at the end, in the source order and in the play order, marked with `origin`.
    ///
    /// # Errors
    ///
    /// [`QueueError::IdsExhausted`] when the ids run out; the queue is unchanged.
    pub fn append(
        &mut self,
        media: impl IntoIterator<Item = M, IntoIter: ExactSizeIterator>,
        origin: Origin,
    ) -> Result<(), QueueError> {
        let mut next_id = self.next_id;
        let added = make_entries(&mut next_id, media, origin)?;
        self.next_id = next_id;
        if let Shuffle::On { order, .. } = &mut self.shuffle {
            order.extend(added.iter().map(|e| e.id));
        }
        self.entries.extend(added);
        Ok(())
    }

    /// Removes the entry `id`. When it was current, the entry that would have played next becomes current (with
    /// repeat [`Repeat::All`], the first one after the last); when there is none, no entry is current.
    ///
    /// # Errors
    ///
    /// [`QueueError::UnknownEntry`] when `id` is not in the queue; the queue is unchanged.
    pub fn remove(&mut self, id: EntryId) -> Result<(), QueueError> {
        let index = self.index_of(id).ok_or(QueueError::UnknownEntry(id))?;
        if self.current == Some(id) {
            self.current = self
                .step_forward(id, self.repeat == Repeat::All)
                .filter(|next| *next != id);
        }
        self.entries.remove(index);
        if let Shuffle::On { order, .. } = &mut self.shuffle
            && let Some(position) = order.iter().position(|other| *other == id)
        {
            order.remove(position);
        }
        Ok(())
    }

    /// Moves the entry `id` to `position` in the play order (the shuffled order when shuffle is on, which leaves the
    /// source order as it is).
    ///
    /// # Errors
    ///
    /// [`QueueError::UnknownEntry`] when `id` is not in the queue, [`QueueError::OutOfRange`] when `position` is not
    /// below the number of entries. The queue is unchanged after an error.
    pub fn move_to(&mut self, id: EntryId, position: usize) -> Result<(), QueueError> {
        let len = self.entries.len();
        if position >= len {
            return Err(QueueError::OutOfRange { position, len });
        }
        match &mut self.shuffle {
            Shuffle::Off => {
                let from = self
                    .entries
                    .iter()
                    .position(|e| e.id == id)
                    .ok_or(QueueError::UnknownEntry(id))?;
                move_item(&mut self.entries, from, position);
            }
            Shuffle::On { order, .. } => {
                let from = order
                    .iter()
                    .position(|other| *other == id)
                    .ok_or(QueueError::UnknownEntry(id))?;
                move_item(order, from, position);
            }
        }
        Ok(())
    }

    /// Removes every entry. The source becomes [`QueueSource::Selection`]; repeat and shuffle are kept.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.source = QueueSource::Selection;
        self.current = None;
        if let Shuffle::On { order, .. } = &mut self.shuffle {
            order.clear();
        }
    }

    /// Sets the repeat mode.
    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// Shuffles the play order from `seed`: the current entry first, then every other entry once. The same queue and
    /// seed always give the same order. Shuffling a shuffled queue reshuffles it.
    pub fn shuffle(&mut self, seed: u64) {
        let mut rng = SplitMix64::new(seed);
        let front = self.current.and_then(|current| self.index_of(current));
        let order = shuffled_order(&self.entries, front, &mut rng);
        self.shuffle = Shuffle::On { order, rng };
    }

    /// Goes back to the source order. The current entry stays current.
    pub fn unshuffle(&mut self) {
        self.shuffle = Shuffle::Off;
    }

    /// Moves to the next entry and returns it, or returns `None` and leaves the current entry as it is when there is
    /// none (the end of the queue with repeat off). Without a current entry, the first entry of the play order becomes
    /// current.
    ///
    /// With [`Repeat::One`], an entry that [`Advance::Finished`] plays again, and a [`Advance::Requested`] next moves as
    /// with repeat off, as Media3 does (`BasePlayer.getRepeatModeForNavigation`).
    pub fn next(&mut self, advance: Advance) -> Option<EntryId> {
        let next = match self.current {
            None => self.play_id_at(0),
            Some(current) => match (self.repeat, advance) {
                (Repeat::One, Advance::Finished) => Some(current),
                (Repeat::All, _) => self.step_forward(current, true),
                (Repeat::Off | Repeat::One, _) => self.step_forward(current, false),
            },
        };
        if next.is_some() {
            self.current = next;
        }
        next
    }

    /// Moves to the previous entry and returns it, or returns `None` and leaves the current entry as it is when there
    /// is none (the first entry with repeat off or one; the caller then restarts the current entry). With
    /// [`Repeat::All`], the previous of the first entry is the last.
    pub fn previous(&mut self) -> Option<EntryId> {
        let current = self.current?;
        let position = self.play_position(current)?;
        let previous = position_back(self.play_len(), position, self.repeat == Repeat::All)
            .and_then(|before| self.play_id_at(before))
            .filter(|before| *before != current);
        if previous.is_some() {
            self.current = previous;
        }
        previous
    }

    /// Makes the entry `id` current.
    ///
    /// # Errors
    ///
    /// [`QueueError::UnknownEntry`] when `id` is not in the queue; the queue is unchanged.
    pub fn jump_to(&mut self, id: EntryId) -> Result<(), QueueError> {
        self.index_of(id).ok_or(QueueError::UnknownEntry(id))?;
        self.current = Some(id);
        Ok(())
    }

    /// The queue's invariants (core spec, section 3): `current` is an entry, ids are unique and were all given out,
    /// and the play order holds every entry exactly once.
    ///
    /// Only in tests, Kani proofs and builds with the `fuzzing` feature, for the fuzz targets (ADR 0015): no caller in
    /// the app needs it, since every operation keeps the invariants.
    ///
    /// ```
    /// use vehicoule_core::queue::{Queue, QueueSource};
    ///
    /// let mut queue = Queue::new();
    /// queue.replace(QueueSource::Selection, ["a", "b"], Some(1))?;
    /// assert!(queue.invariants_hold());
    /// # Ok::<(), vehicoule_core::queue::QueueError>(())
    /// ```
    #[cfg(any(test, kani, feature = "fuzzing"))]
    #[must_use]
    pub fn invariants_hold(&self) -> bool {
        // No allocation here: under Kani every heap object multiplies the cost of each pointer dereference.
        let entries = &self.entries;
        let unique = entries.iter().enumerate().all(|(i, entry)| {
            !entries
                .iter()
                .skip(i.saturating_add(1))
                .any(|other| other.id == entry.id)
        });
        let given_out = entries.iter().all(|entry| entry.id.0 < self.next_id);
        let current_is_entry = self
            .current
            .is_none_or(|current| entries.iter().any(|entry| entry.id == current));
        let order_is_permutation = match &self.shuffle {
            Shuffle::Off => true,
            Shuffle::On { order, .. } => {
                order.len() == entries.len()
                    && entries
                        .iter()
                        .all(|entry| order.iter().filter(|other| **other == entry.id).count() == 1)
            }
        };
        unique && given_out && current_is_entry && order_is_permutation
    }

    fn index_of(&self, id: EntryId) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    // The play order is read in place, never copied: navigation allocates nothing.

    fn play_len(&self) -> usize {
        match &self.shuffle {
            Shuffle::Off => self.entries.len(),
            Shuffle::On { order, .. } => order.len(),
        }
    }

    fn play_id_at(&self, position: usize) -> Option<EntryId> {
        match &self.shuffle {
            Shuffle::Off => self.entries.get(position).map(|e| e.id),
            Shuffle::On { order, .. } => order.get(position).copied(),
        }
    }

    fn play_position(&self, id: EntryId) -> Option<usize> {
        match &self.shuffle {
            Shuffle::Off => self.index_of(id),
            Shuffle::On { order, .. } => order.iter().position(|other| *other == id),
        }
    }

    /// The entry after `from` in the play order; after the last, the first when `wrap` is set.
    fn step_forward(&self, from: EntryId, wrap: bool) -> Option<EntryId> {
        let position = self.play_position(from)?;
        self.play_id_at(position_forward(self.play_len(), position, wrap)?)
    }

    fn position_after_current_in_entries(&self) -> usize {
        index_after(self.current.and_then(|current| self.index_of(current)))
    }
}

/// Moves the item at `from` to `to`, shifting the items between them by one place, by swapping neighbours. Removing
/// then inserting, or `rotate`, cost Kani far more to model, and a move spans a few places at most in practice.
fn move_item<T>(items: &mut [T], from: usize, to: usize) {
    // At most one of the two spans exists: `get_mut` has no span for a range that ends before it starts.
    if let Some(span) = items.get_mut(from..=to) {
        for left in 0..span.len().saturating_sub(1) {
            span.swap(left, left.saturating_add(1));
        }
    }
    if let Some(span) = items.get_mut(to..=from) {
        for left in (0..span.len().saturating_sub(1)).rev() {
            span.swap(left, left.saturating_add(1));
        }
    }
}

/// The index right after `current` in `ids`, or 0 when there is no current entry.
fn position_after(ids: &[EntryId], current: Option<EntryId>) -> usize {
    index_after(current.and_then(|current| ids.iter().position(|id| *id == current)))
}

/// The index right after `found`, or 0 when nothing was found.
fn index_after(found: Option<usize>) -> usize {
    found
        .and_then(|position| position.checked_add(1))
        .unwrap_or(0)
}

/// New entries for `media`, with the ids from `next_id` on, and `next_id` moved past them. The ids are checked before
/// any entry is made, so a refusal makes nothing.
fn make_entries<M>(
    next_id: &mut u64,
    media: impl IntoIterator<Item = M, IntoIter: ExactSizeIterator>,
    origin: Origin,
) -> Result<Vec<Entry<M>>, QueueError> {
    let media = media.into_iter();
    let count = u64::try_from(media.len()).map_err(|_| QueueError::IdsExhausted)?;
    // The next id stays above every id given out, so the ids end one before `end`, which must fit. Compared rather
    // than `checked_add`ed: Kani folds a comparison of known values, so a refusal stops its analysis right here.
    let free = u64::MAX.wrapping_sub(*next_id);
    if count > free {
        return Err(QueueError::IdsExhausted);
    }
    let end = next_id.wrapping_add(count);
    let mut entries = Vec::with_capacity(media.len());
    // Zipped with the ids, so an iterator that yields more than its `len` cannot give out an id twice.
    for (id, media) in (*next_id..end).zip(media) {
        entries.push(Entry {
            id: EntryId(id),
            media,
            origin,
        });
    }
    *next_id = end;
    Ok(entries)
}

/// The entry at `front` first, then every other entry in a Fisher-Yates order drawn from `rng`.
fn shuffled_order<M>(
    entries: &[Entry<M>],
    front: Option<usize>,
    rng: &mut SplitMix64,
) -> Vec<EntryId> {
    // Every id is written at a known place, then the current one is moved to the front: pushing all but the current
    // one writes at places that depend on it, which Kani cannot afford to track. Each branch calls `fisher_yates` on
    // a slice of known bounds for the same reason.
    let mut order = Vec::with_capacity(entries.len());
    order.extend(entries.iter().map(|e| e.id));
    if let Some(at) = front {
        move_item(&mut order, at, 0);
        if let Some(rest) = order.get_mut(1..) {
            fisher_yates(rest, rng);
        }
    } else {
        fisher_yates(&mut order, rng);
    }
    order
}

/// The position after `position` in a play order of `len` entries; after the last, the first when `wrap` is set.
/// For a `position` below `len`, returns a position below `len`, or `None`.
fn position_forward(len: usize, position: usize, wrap: bool) -> Option<usize> {
    match position.checked_add(1) {
        Some(after) if after < len => Some(after),
        _ if wrap => Some(0),
        _ => None,
    }
}

/// The position before `position` in a play order of `len` entries; before the first, the last when `wrap` is set.
/// For a `position` below `len`, returns a position below `len`, or `None`.
fn position_back(len: usize, position: usize, wrap: bool) -> Option<usize> {
    match position.checked_sub(1) {
        Some(before) => Some(before),
        None if wrap => len.checked_sub(1),
        None => None,
    }
}

/// Shuffles `ids` in place: Fisher-Yates, with the choices drawn from `rng`.
fn fisher_yates(ids: &mut [EntryId], rng: &mut SplitMix64) {
    let mut upper = ids.len();
    while let Some(last) = upper.checked_sub(1) {
        if let Some(pick) = rng.below(upper) {
            ids.swap(last, pick);
        }
        upper = last;
    }
}

#[cfg(test)]
mod tests;
