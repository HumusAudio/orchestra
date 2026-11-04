// SPDX-License-Identifier: GPL-3.0-or-later

//! Slurs: curved lines joining two events of one instrument (ADR-0024 §7, SCALETTA F1.09c).
//!
//! # Anchored on events, kept per instrument
//! A slur is **note-anchored** (ADR-0022 §6): it joins a start event and an end event (chords,
//! or a grace chord and its main note), and follows them wherever they move. Since a slur may join
//! two voices of the same instrument (a piano phrase passing between the hands), it does not
//! belong to a voice but to the **instrument**, in one table sorted by `SlurId`.
//!
//! Slurs may nest (a phrase mark over shorter slurs) and may share ends (one slur ends on the
//! event where the next one starts): nothing here forbids it.
//!
//! # The derived index "which slurs touch this event"
//! Layout asks it for every event it draws, and the commands of F1.11 ask it when an event is
//! deleted (deleting an end event deletes the slur in the same command, ADR-0024 §7). The document
//! keeps it as a derived map from an event to the slurs that **start or end** on it
//! ([`SlurRefs`]), maintained by the builder as slurs are added. The events a slur merely passes
//! over are a range question (layout knows them by walking from the start to the end) and are not
//! indexed.
//!
//! [`SlurRefs`] keeps up to two slur IDs inline: almost every event touches at most two slurs (the
//! end of one and the start of the next), so the index costs no allocation per entry; only an
//! event where three or more slurs meet (nested phrase marks starting together) allocates.

use crate::id::{EventId, SlurId};

/// Where a slur is drawn (ADR-0024 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SlurPlacement {
    /// The engraver decides from the stems and the other voices (F2B).
    #[default]
    Auto,
    /// Above the notes.
    Above,
    /// Below the notes.
    Below,
}

/// A slur from one event to a later event of the same instrument (ADR-0024 §7).
///
/// The model checks ([`crate::Document::validate`]) that both ends exist, belong to the same
/// instrument, and that the end comes after the start in score order (a later position, or the
/// same position in a later slot: a grace note slurred to its main note).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slur {
    /// The slur's identity.
    id: SlurId,
    /// The event it starts on.
    start: EventId,
    /// The event it ends on.
    end: EventId,
    /// Where it is drawn.
    placement: SlurPlacement,
}

impl Slur {
    /// A slur: only the document creates them, with an ID from its counter. O(1).
    pub(crate) const fn new(
        id: SlurId,
        start: EventId,
        end: EventId,
        placement: SlurPlacement,
    ) -> Self {
        Self {
            id,
            start,
            end,
            placement,
        }
    }

    /// The slur's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> SlurId {
        self.id
    }

    /// The event the slur starts on. O(1).
    #[must_use]
    pub const fn start(&self) -> EventId {
        self.start
    }

    /// The event the slur ends on. O(1).
    #[must_use]
    pub const fn end(&self) -> EventId {
        self.end
    }

    /// Where the slur is drawn. O(1).
    #[must_use]
    pub const fn placement(&self) -> SlurPlacement {
        self.placement
    }
}

/// The slurs that start or end on one event: the entry of the derived index (see the module
/// documentation). Up to two IDs inline, more in a boxed slice.
///
/// # Why not a `Vec`?
/// A `Vec` per indexed event would be one allocation each: with slurs on a third of the notes of
/// the heavy reference score that is tens of thousands of small allocations, and 24 bytes plus
/// the heap block per entry. The inline cases cover nearly every real event and cost nothing more
/// than the enum (24 bytes, the size of the `Box<[SlurId]>` variant).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlurRefs {
    /// One slur.
    One([SlurId; 1]),
    /// Two slurs, in ID order.
    Two([SlurId; 2]),
    /// Three or more, in ID order.
    Many(Box<[SlurId]>),
}

impl SlurRefs {
    /// The index entry of an event touched by one slur. O(1).
    pub(crate) const fn one(slur: SlurId) -> Self {
        Self::One([slur])
    }

    /// The slurs, in ID order (creation order). O(1); never allocates.
    pub(crate) fn as_slice(&self) -> &[SlurId] {
        match self {
            Self::One(ids) => ids,
            Self::Two(ids) => ids,
            Self::Many(ids) => ids,
        }
    }

    /// Adds a slur in ID order (which is creation order: IDs only grow, so the builder always
    /// appends). Keeping the entry sorted by ID rather than by the order of the edits makes it a
    /// pure function of the slurs, so that undoing and redoing edits gives back an identical
    /// index (ADR-0026 §8). A slur already in the entry is not added twice. O(1) up to two slurs;
    /// O(n) and one allocation from the third on (the boxed slice is rebuilt: such events are
    /// rare, and a slice is half the size of a `Vec` for the common entries).
    pub(crate) fn insert(&mut self, slur: SlurId) {
        let ids = self.as_slice();
        let Err(at) = ids.binary_search(&slur) else {
            return;
        };
        let (before, after) = ids.split_at(at);
        let merged: Vec<SlurId> = before.iter().chain([&slur]).chain(after).copied().collect();
        *self = Self::from_sorted(&merged).unwrap_or(Self::one(slur));
    }

    /// Removes a slur from the entry. Returns `false` if the entry is left empty (the caller then
    /// drops it from the index, so that no empty entry ever exists). O(n) for n slurs in the entry.
    pub(crate) fn remove(&mut self, slur: SlurId) -> bool {
        let kept: Vec<SlurId> = self
            .as_slice()
            .iter()
            .copied()
            .filter(|&s| s != slur)
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
    fn from_sorted(ids: &[SlurId]) -> Option<Self> {
        match ids {
            [] => None,
            [a] => Some(Self::One([*a])),
            [a, b] => Some(Self::Two([*a, *b])),
            many => Some(Self::Many(many.into())),
        }
    }
}
