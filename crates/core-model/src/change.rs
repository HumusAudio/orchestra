// SPDX-License-Identifier: GPL-3.0-or-later

//! What a transaction changed: the notifications of ADR-0003 §6 (ADR-0026 §5, SCALETTA F1.11b).
//!
//! # A value, not a callback
//! Every transaction, undo and redo returns one [`ChangeSet`]. Nobody registers observers inside
//! the model: the editor hands the set back, and the application passes it on to layout, playback
//! and the views. So the model has no threads, no callbacks running in the middle of an edit
//! (which would see half-changed states), and nothing that can be "forgotten": every primitive
//! edit records its own entries as it is applied (`crate::edit`).
//!
//! # What, never how
//! The set says *which* objects and *which* places changed, never how to redraw them: each
//! consumer decides what its caches need. It is `#[non_exhaustive]`, so that layout (F2.16) and
//! playback (F1.13) can ask for more detail later without breaking anyone.
//!
//! # Cost
//! Entries are pushed in O(1) while the edits run and are sorted and deduplicated once when the
//! transaction ends ([`ChangeSet::finish`]): O(e log e) for e entries, on the editing path.

use orchestra_core_time::{BarIndex, Position};

use crate::edit::ObjectRef;
use crate::id::{BarId, VoiceId};

/// What one transaction (or one undo, or one redo) changed in the document (ADR-0026 §5).
///
/// Each list is sorted and free of duplicates. An object may appear in more than one list (a
/// transaction that inserts a note and then changes it lists it as inserted and as replaced);
/// a consumer that needs the final state reads the document, which is the source of truth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChangeSet {
    /// The (voice, bar) cells whose events changed: inserted, removed, replaced or moved.
    cells: Vec<(VoiceId, BarId)>,
    /// The objects that were inserted (bars included, as [`ObjectRef::Bar`]).
    inserted: Vec<ObjectRef>,
    /// The objects that were removed.
    removed: Vec<ObjectRef>,
    /// The objects whose content or place changed, keeping their ID.
    replaced: Vec<ObjectRef>,
    /// The first bar from which the meter map's answers may differ: bar starts, lengths, meters,
    /// beats or displayed numbers. Inserting or removing bars sets it too.
    meter_from: Option<BarIndex>,
    /// The first written position from which the tempo map differs (set when the transaction
    /// rebuilds it, ADR-0024 §11), or `None` if the tempo map is the same as before.
    tempo_from: Option<Position>,
    /// Whether the form (repeats, voltas, markers, jumps, the numbering style, bar repeats or pass
    /// rules) changed, so that the unrolled path of F1.12 must be recomputed.
    form: bool,
    /// Whether the tempo map must be derived again when the transaction ends: a tempo mark changed,
    /// or bars moved. Internal bookkeeping of the editor, not part of what consumers read.
    pub(crate) tempo_dirty: bool,
}

impl ChangeSet {
    /// The (voice, bar) cells whose events changed, sorted by voice then bar ID. O(1).
    #[must_use]
    pub fn cells(&self) -> &[(VoiceId, BarId)] {
        &self.cells
    }

    /// The objects inserted, sorted. O(1).
    #[must_use]
    pub fn inserted(&self) -> &[ObjectRef] {
        &self.inserted
    }

    /// The objects removed, sorted. O(1).
    #[must_use]
    pub fn removed(&self) -> &[ObjectRef] {
        &self.removed
    }

    /// The objects replaced (same ID, new content or place), sorted. O(1).
    #[must_use]
    pub fn replaced(&self) -> &[ObjectRef] {
        &self.replaced
    }

    /// The first bar from which the meter map may answer differently, if any. O(1).
    #[must_use]
    pub const fn meter_from(&self) -> Option<BarIndex> {
        self.meter_from
    }

    /// The first written position from which the tempo map differs, if it changed. O(1).
    #[must_use]
    pub const fn tempo_from(&self) -> Option<Position> {
        self.tempo_from
    }

    /// Whether the form changed (the unrolled path must be recomputed). O(1).
    #[must_use]
    pub const fn form_changed(&self) -> bool {
        self.form
    }

    /// Whether nothing at all changed. O(1).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
            && self.inserted.is_empty()
            && self.removed.is_empty()
            && self.replaced.is_empty()
            && self.meter_from.is_none()
            && self.tempo_from.is_none()
            && !self.form
    }

    // --------------------------------------------------------------------------------------------
    // Recording (used by the edits while they are applied)
    // --------------------------------------------------------------------------------------------

    /// Records that the events of `voice` in `bar` changed. O(1) amortised.
    pub(crate) fn cell(&mut self, voice: VoiceId, bar: BarId) {
        self.cells.push((voice, bar));
    }

    /// Records an inserted object. O(1) amortised.
    pub(crate) fn insert(&mut self, object: ObjectRef) {
        self.note_kind(object);
        self.inserted.push(object);
    }

    /// Records a removed object. O(1) amortised.
    pub(crate) fn remove(&mut self, object: ObjectRef) {
        self.note_kind(object);
        self.removed.push(object);
    }

    /// Records a replaced object. O(1) amortised.
    pub(crate) fn replace(&mut self, object: ObjectRef) {
        self.note_kind(object);
        self.replaced.push(object);
    }

    /// Records that the meter map may answer differently from `bar` on (keeping the earliest).
    /// Bars that move also move every tempo mark after them, so the tempo map must be derived
    /// again. O(1).
    pub(crate) fn meter(&mut self, bar: BarIndex) {
        self.meter_from = Some(self.meter_from.map_or(bar, |b| b.min(bar)));
        self.tempo_dirty = true;
    }

    /// Records the first position from which the rebuilt tempo map differs. O(1).
    pub(crate) const fn set_tempo_from(&mut self, from: Option<Position>) {
        self.tempo_from = from;
    }

    /// The flags implied by the kind of an object: a tempo mark makes the tempo map dirty, a form
    /// object marks the form as changed. O(1).
    const fn note_kind(&mut self, object: ObjectRef) {
        match object {
            ObjectRef::TempoMark(_) | ObjectRef::InitialTempo => self.tempo_dirty = true,
            ObjectRef::RepeatStart(_)
            | ObjectRef::RepeatEnd(_)
            | ObjectRef::Volta(_)
            | ObjectRef::Marker(_)
            | ObjectRef::Jump(_)
            | ObjectRef::BarRepeat(_)
            | ObjectRef::PassRule(_)
            | ObjectRef::Numbering
            | ObjectRef::Bar(_) => self.form = true,
            _ => {}
        }
    }

    /// Adds what `other` changed **around** its objects (cells, the earliest bar and position,
    /// the flags), but not its object lists: a replacement made of a removal and an insertion is
    /// reported once, as "replaced", by the caller. O(cells of `other`).
    pub(crate) fn absorb_effects(&mut self, other: Self) {
        self.cells.extend(other.cells);
        if let Some(bar) = other.meter_from {
            self.meter(bar);
        }
        self.form |= other.form;
        self.tempo_dirty |= other.tempo_dirty;
    }

    /// Sorts and deduplicates every list, once the transaction is over. O(e log e).
    pub(crate) fn finish(&mut self) {
        /// Sorts a list and drops repeated entries.
        fn tidy<T: Ord>(list: &mut Vec<T>) {
            list.sort_unstable();
            list.dedup();
        }
        tidy(&mut self.cells);
        tidy(&mut self.inserted);
        tidy(&mut self.removed);
        tidy(&mut self.replaced);
    }
}
