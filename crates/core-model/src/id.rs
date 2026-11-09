// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable identifiers of the objects of the document (ADR-0022 §1).
//!
//! # What lives here
//! - `IdCounter` (crate-private): the **one counter per document** every ID comes from. It only grows: an ID is
//!   never handed out twice, not even after the object it named was deleted (and the deletion
//!   undone or not), so selections, caches and later redo steps can never confuse two objects.
//! - One newtype per kind of object ([`TrackId`], [`InstrumentId`], [`StaffId`], [`VoiceId`],
//!   [`BarId`], since F1.09b [`EventId`], [`NoteId`], [`TupletId`], [`ClefId`],
//!   [`KeyChangeId`], and since F1.09c [`SlurId`]): the compiler rejects a `StaffId` where a
//!   `VoiceId` is expected, although both are plain `u64` values underneath. The rest of F1.09c
//!   adds the IDs of dynamics, hairpins and tempo marks (ADR-0024).
//! - `IdMap` (crate-private): a hash map keyed by IDs with a cheap hasher, for the derived indices
//!   (`ID → where the object is`, ADR-0022 §1).
//!
//! # Why `NonZeroU64`?
//! The counter starts at 1, so 0 is never a valid ID. Storing the value as `NonZeroU64` lets the
//! compiler use 0 as the "nothing" of `Option`: an `Option<BarId>` takes 8 bytes instead of 16.
//! That matters for the per-bar and per-event tables of the next steps, which hold many of them.
//!
//! # Why one counter for every kind, and not one per kind?
//! ADR-0022 §1 chose it: a single number in the project file, and an ID value that identifies the
//! object even when its kind is lost (in a log, in a diff of two versions of the file). With 64
//! bits the counter cannot run out in practice (a billion new objects per second for 584 years),
//! but exhaustion is still a typed error, never a wrap-around that would reuse an ID.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::num::NonZeroU64;

// ------------------------------------------------------------------------------------------------
// The counter
// ------------------------------------------------------------------------------------------------

/// The source of every ID of one document (ADR-0022 §1).
///
/// IDs are handed out in increasing order, starting at 1. The counter is saved in the project file
/// (F1.15), so a document reopened from disk keeps handing out IDs that were never used before.
///
/// # Replaying saved IDs
/// Loading a saved document ([`Document::from_parts`](crate::Document::from_parts), ADR-0029 S4)
/// goes through the very builder methods that create new documents, so that a file is checked
/// exactly as the builder checks what it creates. Those methods take their IDs from this counter;
/// while loading, the counter is put in **script** mode: it hands out the saved IDs it was given
/// for the next call, in order, instead of new ones, and nothing else (an empty script hands out
/// nothing, so a builder method that wanted more IDs than the file gave fails instead of silently
/// inventing one). Outside loading the script is `None` and costs one branch per ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdCounter {
    /// The next ID to hand out, or `None` when all `u64::MAX` values have been used.
    next: Option<NonZeroU64>,
    /// While loading: the saved IDs still to hand out, **last one first** (a stack, so that taking
    /// one is a `pop`). `None` outside loading.
    script: Option<Vec<NonZeroU64>>,
}

impl IdCounter {
    /// A fresh counter: the first ID it hands out is 1. O(1).
    pub(crate) const fn new() -> Self {
        Self {
            next: Some(NonZeroU64::MIN),
            script: None,
        }
    }

    /// A counter whose next ID is `next`, for the unit tests of exhaustion (no real document gets
    /// anywhere near the end of the range). O(1).
    #[cfg(test)]
    pub(crate) const fn starting_at(next: NonZeroU64) -> Self {
        Self {
            next: Some(next),
            script: None,
        }
    }

    /// A counter restored from a saved document: its next ID is `next` (`None`: exhausted), and it
    /// starts in script mode with an empty script (see the type's documentation). O(1).
    pub(crate) const fn restored(next: Option<NonZeroU64>) -> Self {
        Self {
            next,
            script: Some(Vec::new()),
        }
    }

    /// The raw next ID, as saved in a file (`None`: exhausted). O(1).
    pub(crate) const fn next_value(&self) -> Option<NonZeroU64> {
        self.next
    }

    /// Gives the saved IDs that the next builder call must hand out, in the order it takes them.
    /// Only in script mode (a no-op otherwise, which only a bug could reach). O(n).
    pub(crate) fn set_script(&mut self, ids: &[NonZeroU64]) {
        if let Some(script) = &mut self.script {
            script.clear();
            script.extend(ids.iter().rev());
        }
    }

    /// Whether every scripted ID has been handed out (always true outside script mode). O(1).
    pub(crate) fn script_is_spent(&self) -> bool {
        self.script.as_ref().is_none_or(Vec::is_empty)
    }

    /// Leaves script mode: from now on the counter hands out new IDs again. O(1).
    pub(crate) fn end_script(&mut self) {
        self.script = None;
    }

    /// Whether `n` more IDs can be handed out. O(1).
    ///
    /// The builder checks this **before** creating an object that needs several IDs (an instrument
    /// track takes 2 + 2 × staves), so that a failure leaves the document unchanged instead of
    /// half-built.
    pub(crate) fn has_room_for(&self, n: u64) -> bool {
        // While loading, the room is exactly what the script still holds.
        if let Some(script) = &self.script {
            return usize::try_from(n).is_ok_and(|n| n <= script.len());
        }
        match (self.next, n.checked_sub(1)) {
            // Asking for nothing always succeeds.
            (_, None) => true,
            // The values `next ..= u64::MAX` are still free: that is `u64::MAX - next + 1` IDs,
            // so `n` fit when `n - 1 <= u64::MAX - next`. Written this way nothing can overflow.
            (Some(next), Some(extra)) => extra <= u64::MAX - next.get(),
            // An exhausted counter has room for nothing.
            (None, Some(_)) => false,
        }
    }

    /// The ID [`next`](Self::next) would hand out, without handing it out; `None` if the counter
    /// is exhausted. O(1).
    ///
    /// Only the tests use it now (to name an ID not handed out yet): the builder, which tried a
    /// tempo mark with the peeked ID, now takes the ID and gives it back on refusal
    /// ([`give_back`](Self::give_back)), the same way for every object.
    #[cfg(test)]
    pub(crate) fn peek(&self) -> Option<NonZeroU64> {
        match &self.script {
            // While loading, the next ID is the next scripted one.
            Some(script) => script.last().copied(),
            None => self.next,
        }
    }

    /// Whether the counter has already handed out the raw ID `raw`: it is below the next value
    /// (or the counter is exhausted, having handed out everything). An edit refuses to insert an
    /// object with an ID not handed out yet, which a later object would receive again
    /// (ADR-0022 §1, ADR-0026 §1). O(1).
    pub(crate) fn has_issued(&self, raw: u64) -> bool {
        raw != 0 && self.next.is_none_or(|next| raw < next.get())
    }

    /// Takes back the IDs just handed out, `taken` in the order they were handed out, so that the
    /// counter is exactly as before (normal mode: its next value is the first of them again; script
    /// mode: they are back on the script, in the same order). O(taken).
    ///
    /// # Contract
    /// `taken` must be the **last** IDs handed out, with nothing handed out after them. The builder
    /// guarantees it: it takes the IDs of one object, tries to insert the object, and gives them
    /// back at once if the insertion is refused, so that a refused object never consumes an ID (the
    /// checks themselves are the edit layer's).
    pub(crate) fn give_back(&mut self, taken: &[NonZeroU64]) {
        match &mut self.script {
            // The script is a stack whose top is the next ID: push the last taken first.
            Some(script) => script.extend(taken.iter().rev()),
            None => {
                if let Some(&first) = taken.first() {
                    self.next = Some(first);
                }
            }
        }
    }

    /// Hands out the next ID, or `None` if the counter is exhausted. O(1).
    pub(crate) fn next(&mut self) -> Option<NonZeroU64> {
        // While loading, hand out the next scripted ID, and nothing once the script is spent.
        if let Some(script) = &mut self.script {
            return script.pop();
        }
        let id = self.next?;
        // `checked_add` returns `None` past `u64::MAX`: the counter is then exhausted for good,
        // and `u64::MAX` itself has just been handed out exactly once.
        self.next = id.checked_add(1);
        Some(id)
    }
}

// ------------------------------------------------------------------------------------------------
// The typed IDs
// ------------------------------------------------------------------------------------------------

/// Defines one ID newtype: a `NonZeroU64` created by this crate from the counter, with a public
/// getter for the raw value (what the project file stores) and `from_raw` for loading a saved
/// document (ADR-0029 S4).
///
/// A macro, because the types are identical apart from their name and documentation; writing
/// them out by hand would be five copies of the same code to keep in step.
macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        ///
        /// IDs are ordered by creation: a smaller value was handed out earlier. Comparing them is
        /// useful for deterministic iteration orders, never for musical order.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Wraps a value handed out by the document's [`IdCounter`]. O(1).
            ///
            /// Crate-private: a new ID only exists because the document created the object it
            /// names (`from_raw` is for saved ones).
            pub(crate) const fn from_counter(raw: NonZeroU64) -> Self {
                Self(raw)
            }

            /// The raw value, unique within the document across every kind of ID. O(1).
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }

            /// The ID with this raw value, or `None` for 0 (never an ID). O(1).
            ///
            /// **For loading a saved document only** (ADR-0029 S4, owner's choice L1 of
            /// 2026-11-06): it lets a description of a saved document name its objects by their
            /// saved IDs. An ID made up here harms nothing: [`Document::from_parts`] refuses a
            /// document whose IDs repeat or were not handed out by its counter, and an edit
            /// refuses an object whose ID is already in use or not yet handed out
            /// (`DocumentError::DuplicateObject`, `DocumentError::UnknownObject`).
            ///
            /// [`Document::from_parts`]: crate::Document::from_parts
            #[must_use]
            pub const fn from_raw(raw: u64) -> Option<Self> {
                match NonZeroU64::new(raw) {
                    Some(raw) => Some(Self(raw)),
                    None => None,
                }
            }
        }
    };
}

define_id!(
    /// The identity of a track of the document (ADR-0022 §2).
    TrackId
);
define_id!(
    /// The identity of the instrument of an instrument track (ADR-0022 §2).
    InstrumentId
);
define_id!(
    /// The identity of a staff of an instrument (ADR-0022 §2).
    StaffId
);
define_id!(
    /// The identity of a voice of an instrument (ADR-0022 §2). Voices belong to the instrument,
    /// not to a staff: every voice has a home staff, and every event its own staff.
    VoiceId
);
define_id!(
    /// The identity of a bar (ADR-0022 §3): it stays the same when bars are inserted or deleted
    /// before it, unlike its [`BarIndex`](orchestra_core_time::BarIndex).
    BarId
);
define_id!(
    /// The identity of an event of a voice: a chord or a rest (ADR-0022 §4, ADR-0024 §1).
    EventId
);
define_id!(
    /// The identity of one notehead of a chord (ADR-0022 §4): ties, spelling and performance
    /// offsets are per notehead, so each has its own ID even when it shares its chord's stem.
    NoteId
);
define_id!(
    /// The identity of a tuplet of a voice (ADR-0024 §3).
    TupletId
);
define_id!(
    /// The identity of a clef change on a staff (ADR-0024 §9). A staff's initial clef is part of
    /// the staff and has no ID of its own: it cannot be removed or moved.
    ClefId
);
define_id!(
    /// The identity of a key signature change of the document, or of an instrument's override
    /// (ADR-0024 §9). The initial key of the document is part of the document and has no ID.
    KeyChangeId
);

define_id!(
    /// The identity of a slur of an instrument (ADR-0024 §7).
    SlurId
);
define_id!(
    /// The identity of a dynamic of an instrument (ADR-0024 §8).
    DynamicId
);
define_id!(
    /// The identity of a hairpin of an instrument (ADR-0024 §8).
    HairpinId
);
define_id!(
    /// The identity of a tempo mark of the document (ADR-0024 §11). Unlike the initial clef and
    /// key, the initial tempo mark has one: "Allegro ♩ = 132" at the top of the score is printed
    /// text that can be selected and edited, even though it cannot be removed.
    TempoMarkId
);
define_id!(
    /// The identity of a repeat barline of the document, start or end (ADR-0025 §2).
    RepeatId
);
define_id!(
    /// The identity of a volta of the document (ADR-0025 §3).
    VoltaId
);
define_id!(
    /// The identity of a marker of the document: segno, coda, Fine, "To Coda" (ADR-0025 §4).
    MarkerId
);
define_id!(
    /// The identity of a jump of the document: D.C., D.S. and their variants (ADR-0025 §4).
    JumpId
);
define_id!(
    /// The identity of a bar repeat (%) on a staff (ADR-0025 §6).
    BarRepeatId
);
define_id!(
    /// The identity of a pass rule of an instrument ("tacet 1st time", ADR-0025 §7).
    PassRuleId
);

// ------------------------------------------------------------------------------------------------
// The ID hash map
// ------------------------------------------------------------------------------------------------

/// A multiplicative (Fibonacci) hasher for IDs (ADR-0022 §1).
///
/// # Why not the standard hasher?
/// The standard `SipHash` resists *hash flooding*: an attacker choosing keys that all collide. Our
/// keys are IDs handed out by the document's own counter, never chosen by an outsider, so that
/// protection buys nothing and costs about a hundred instructions per lookup. One multiplication
/// by 2⁶⁴/φ spreads consecutive IDs evenly over the high bits, which `HashMap` uses to pick its
/// buckets. A file loaded from disk (F1.15) could contain hand-made IDs; the worst it can do is
/// make lookups slower, and the loader can reject absurd files by size.
///
/// No new dependency (`rustc-hash` would do the same): the whole hasher is a few lines.
///
/// # Public, for the crates above
/// `playback` and `io` key their own maps by IDs too (notes of a tie chain, events of an
/// arpeggio, bar IDs of a file being loaded); they use these types instead of the standard
/// `HashMap`, so every map keyed by IDs in the workspace hashes the same way.
/// Keys that are not IDs (small integers, tuples of IDs and indices) work as well: integers other
/// than `u64` go through [`write`](Hasher::write), which folds their bytes.
#[derive(Debug, Default, Clone, Copy)]
pub struct IdHasher(u64);

/// The multiplier: 2⁶⁴ divided by the golden ratio, rounded to an odd number (Knuth, *The Art of
/// Computer Programming*, vol. 3, §6.4). Being odd, multiplying by it is a bijection on `u64`, so
/// distinct IDs never get the same hash.
const FIBONACCI_MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

impl Hasher for IdHasher {
    /// The hash computed so far. O(1).
    fn finish(&self) -> u64 {
        self.0
    }

    /// Generic byte input. IDs always arrive through [`write_u64`](Self::write_u64) (the derived
    /// `Hash` of `NonZeroU64` hashes its `u64`); this fallback folds the bytes one by one, so that
    /// the hasher stays correct for any key. O(bytes).
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(FIBONACCI_MULTIPLIER);
        }
    }

    /// The path every ID takes: one multiplication (wrapping on purpose: it is a hash). O(1).
    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0 ^ n).wrapping_mul(FIBONACCI_MULTIPLIER);
    }
}

/// A hash map keyed by IDs, with [`IdHasher`]. Lookups and insertions are O(1) on average.
/// Build one with `IdMap::default()` (`HashMap::new` exists only for the standard hasher).
pub type IdMap<K, V> = HashMap<K, V, BuildHasherDefault<IdHasher>>;

/// A hash set of IDs, with [`IdHasher`]. Membership tests are O(1) on average.
/// Build one with `IdSet::default()`.
pub type IdSet<K> = HashSet<K, BuildHasherDefault<IdHasher>>;

/// A small, sorted set of IDs: the value of a derived index whose entries almost always hold one
/// or two IDs (the slurs at an event, `crate::slur::SlurRefs`; the noteheads tied into a
/// notehead, ADR-0026 §2). Up to two IDs inline, more in a boxed slice.
///
/// # Why not a `Vec`?
/// A `Vec` per entry would be one heap allocation per indexed object and 24 bytes plus the heap
/// block. The two inline cases cover nearly every real entry and cost nothing more than the enum
/// (24 bytes, the size of the `Box<[T]>` variant); only an entry with three or more IDs allocates.
///
/// # Why sorted?
/// The IDs are kept in ascending order rather than in the order the edits added them. That makes
/// an entry a pure function of the objects it refers to, so that undoing and redoing edits gives
/// back an identical index (ADR-0026 §8), and lets [`insert`](Self::insert) refuse a duplicate
/// with a binary search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IdRefs<T> {
    /// One ID.
    One([T; 1]),
    /// Two IDs, in ascending order.
    Two([T; 2]),
    /// Three or more, in ascending order.
    Many(Box<[T]>),
}

impl<T: Copy + Ord> IdRefs<T> {
    /// The entry holding one ID. O(1).
    pub(crate) const fn one(id: T) -> Self {
        Self::One([id])
    }

    /// The IDs, in ascending order (which is creation order: IDs only grow). O(1); never
    /// allocates.
    pub(crate) fn as_slice(&self) -> &[T] {
        match self {
            Self::One(ids) => ids,
            Self::Two(ids) => ids,
            Self::Many(ids) => ids,
        }
    }

    /// Adds an ID in ascending order; an ID already in the entry is not added twice. O(1) up to
    /// two IDs; O(n) and one allocation from the third on (the boxed slice is rebuilt: such
    /// entries are rare, and a slice is half the size of a `Vec` for the common ones).
    pub(crate) fn insert(&mut self, id: T) {
        let ids = self.as_slice();
        let Err(at) = ids.binary_search(&id) else {
            return;
        };
        let (before, after) = ids.split_at(at);
        let merged: Vec<T> = before.iter().chain([&id]).chain(after).copied().collect();
        *self = Self::from_sorted(&merged).unwrap_or(Self::one(id));
    }

    /// Removes an ID from the entry. Returns `false` if the entry is left empty (the caller then
    /// drops it from the index, so that no empty entry ever exists). O(n) for n IDs in the entry.
    pub(crate) fn remove(&mut self, id: T) -> bool {
        let kept: Vec<T> = self
            .as_slice()
            .iter()
            .copied()
            .filter(|&other| other != id)
            .collect();
        match Self::from_sorted(&kept) {
            Some(refs) => {
                *self = refs;
                true
            }
            None => false,
        }
    }

    /// The entry for these IDs (already sorted), or `None` for no IDs. O(n); allocates only from
    /// three IDs on.
    fn from_sorted(ids: &[T]) -> Option<Self> {
        match ids {
            [] => None,
            [a] => Some(Self::One([*a])),
            [a, b] => Some(Self::Two([*a, *b])),
            many => Some(Self::Many(many.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counter starts at 1 and hands out consecutive values.
    #[test]
    fn counter_starts_at_one_and_grows() {
        let mut ids = IdCounter::new();
        assert_eq!(ids.next().map(NonZeroU64::get), Some(1));
        assert_eq!(ids.next().map(NonZeroU64::get), Some(2));
        assert_eq!(ids.next().map(NonZeroU64::get), Some(3));
    }

    /// `u64::MAX` is handed out exactly once, then the counter stays exhausted.
    #[test]
    fn counter_exhausts_without_wrapping() {
        let mut ids = IdCounter::starting_at(NonZeroU64::new(u64::MAX - 1).unwrap());
        assert!(ids.has_room_for(2));
        assert!(!ids.has_room_for(3));
        assert_eq!(ids.next().map(NonZeroU64::get), Some(u64::MAX - 1));
        assert!(ids.has_room_for(1));
        assert!(!ids.has_room_for(2));
        assert_eq!(ids.next().map(NonZeroU64::get), Some(u64::MAX));
        assert!(ids.has_room_for(0));
        assert!(!ids.has_room_for(1));
        assert_eq!(ids.next(), None);
        assert_eq!(ids.next(), None);
    }

    /// A fresh counter has room for every value of the range, and not one more.
    #[test]
    fn fresh_counter_room_is_the_whole_range() {
        let ids = IdCounter::new();
        assert!(ids.has_room_for(u64::MAX));
        assert!(ids.has_room_for(0));
    }

    /// Distinct IDs hash differently (the multiplier is odd, so the map is a bijection).
    #[test]
    fn hasher_separates_consecutive_ids() {
        let hash = |n: u64| {
            let mut h = IdHasher::default();
            h.write_u64(n);
            h.finish()
        };
        let hashes: Vec<u64> = (1..=1000).map(hash).collect();
        let mut sorted = hashes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), hashes.len());
    }
}
