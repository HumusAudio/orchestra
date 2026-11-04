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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IdCounter {
    /// The next ID to hand out, or `None` when all `u64::MAX` values have been used.
    next: Option<NonZeroU64>,
}

impl IdCounter {
    /// A fresh counter: the first ID it hands out is 1. O(1).
    pub(crate) const fn new() -> Self {
        Self {
            next: Some(NonZeroU64::MIN),
        }
    }

    /// A counter whose next ID is `next`, for the unit tests of exhaustion (no real document gets
    /// anywhere near the end of the range). O(1).
    #[cfg(test)]
    pub(crate) const fn starting_at(next: NonZeroU64) -> Self {
        Self { next: Some(next) }
    }

    /// Whether `n` more IDs can be handed out. O(1).
    ///
    /// The builder checks this **before** creating an object that needs several IDs (an instrument
    /// track takes 2 + 2 × staves), so that a failure leaves the document unchanged instead of
    /// half-built.
    pub(crate) fn has_room_for(&self, n: u64) -> bool {
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
    /// For edits that can only be judged once the new object is in place (a tempo mark, whose
    /// derived tempo map may turn out invalid): the object is tried with this ID, and the ID is
    /// taken only if it stays, so that a rejected edit never consumes one.
    pub(crate) const fn peek(&self) -> Option<NonZeroU64> {
        self.next
    }

    /// Whether the counter has already handed out the raw ID `raw`: it is below the next value
    /// (or the counter is exhausted, having handed out everything). An edit refuses to insert an
    /// object with an ID not handed out yet, which a later object would receive again
    /// (ADR-0022 §1, ADR-0026 §1). O(1).
    pub(crate) fn has_issued(&self, raw: u64) -> bool {
        raw != 0 && self.next.is_none_or(|next| raw < next.get())
    }

    /// Hands out the next ID, or `None` if the counter is exhausted. O(1).
    pub(crate) fn next(&mut self) -> Option<NonZeroU64> {
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

/// Defines one ID newtype: a `NonZeroU64` that only this crate can create (from the counter), with
/// a public getter for the raw value (what the project file stores, F1.15).
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
            /// Crate-private: an ID only exists because the document created the object it names.
            pub(crate) const fn from_counter(raw: NonZeroU64) -> Self {
                Self(raw)
            }

            /// The raw value, unique within the document across every kind of ID. O(1).
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
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
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct IdHasher(u64);

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
pub(crate) type IdMap<K, V> = HashMap<K, V, BuildHasherDefault<IdHasher>>;

/// A hash set of IDs, with [`IdHasher`]. Membership tests are O(1) on average.
pub(crate) type IdSet<K> = HashSet<K, BuildHasherDefault<IdHasher>>;

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
