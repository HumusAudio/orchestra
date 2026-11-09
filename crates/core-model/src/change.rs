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
use crate::id::{BarId, InstrumentId, NoteId, VoiceId};

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
    /// The instruments whose dynamics or hairpins were inserted, removed or replaced (ADR-0027 §14,
    /// ADR-0027 H11): playback recomputes the velocities of these instruments only. The object
    /// lists name a removed dynamic by ID alone, and the document no longer holds it to say whose
    /// it was.
    dynamics: Vec<InstrumentId>,
    /// Whether the tolerance thresholds of the DAW move changed (ADR-0028 §1): nothing derived
    /// depends on them today, but the DAW view shows them.
    tolerance: bool,
    /// Whether the tempo map must be derived again when the transaction ends: a tempo mark changed,
    /// or bars moved. Internal bookkeeping of the editor, not part of what consumers read.
    pub(crate) tempo_dirty: bool,
    /// The noteheads that left the document: those of removed events, and those a replacement
    /// dropped from a chord. Internal bookkeeping too: the local check of the commands
    /// (`Document::check_cells`) looks at them to find ties still leading into a note that is
    /// gone. A notehead put back later in the same transaction is listed anyway; the check skips
    /// the ones that exist.
    pub(crate) notes_gone: Vec<NoteId>,
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

    /// The instruments whose dynamics or hairpins changed, sorted (ADR-0027 §14, H11). O(1).
    #[must_use]
    pub fn dynamics_changed(&self) -> &[InstrumentId] {
        &self.dynamics
    }

    /// Whether the tolerance thresholds changed (ADR-0028 §1). O(1).
    #[must_use]
    pub const fn tolerance_changed(&self) -> bool {
        self.tolerance
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
            && self.dynamics.is_empty()
            && !self.tolerance
    }

    // --------------------------------------------------------------------------------------------
    // Recording (used by the edits while they are applied)
    // --------------------------------------------------------------------------------------------

    /// Empties the set, keeping the capacity of its lists. O(entries).
    ///
    /// The builder records the effects of every object it inserts in one scratch set and clears
    /// it after each, so that building a large document allocates nothing for change sets nobody
    /// reads.
    pub(crate) fn clear(&mut self) {
        let Self {
            cells,
            inserted,
            removed,
            replaced,
            meter_from,
            tempo_from,
            form,
            dynamics,
            tolerance,
            tempo_dirty,
            notes_gone,
        } = self;
        cells.clear();
        inserted.clear();
        removed.clear();
        replaced.clear();
        *meter_from = None;
        *tempo_from = None;
        *form = false;
        dynamics.clear();
        *tolerance = false;
        *tempo_dirty = false;
        notes_gone.clear();
    }

    /// Records that the events of `voice` in `bar` changed. O(1) amortised.
    ///
    /// A cell equal to one of the last two recorded is not recorded again: moving many events of
    /// one bar records the same pair of cells (where an event left, where it arrived) over and
    /// over, and the list would otherwise grow with the events instead of the cells. The lists
    /// are deduplicated anyway when they are tidied; this only keeps them short (measured: the
    /// sort of the cells was a fifth of a meter change that rewrites 60 bars of the heavy score).
    pub(crate) fn cell(&mut self, voice: VoiceId, bar: BarId) {
        let cell = (voice, bar);
        let recent = self.cells.iter().rev().take(2).any(|&c| c == cell);
        if !recent {
            self.cells.push(cell);
        }
    }

    /// Records that the dynamics or hairpins of `instrument` changed. O(1) amortised; a repeat of
    /// the last entry is dropped at once, the rest when the lists are tidied.
    pub(crate) fn dynamics(&mut self, instrument: InstrumentId) {
        if self.dynamics.last() != Some(&instrument) {
            self.dynamics.push(instrument);
        }
    }

    /// Records that the tolerance thresholds changed. O(1).
    pub(crate) const fn tolerance(&mut self) {
        self.tolerance = true;
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
        // Through `cell`, so that the recent duplicates are dropped here too.
        for (voice, bar) in other.cells {
            self.cell(voice, bar);
        }
        if let Some(bar) = other.meter_from {
            self.meter(bar);
        }
        self.form |= other.form;
        self.tolerance |= other.tolerance;
        for instrument in other.dynamics {
            self.dynamics(instrument);
        }
        self.tempo_dirty |= other.tempo_dirty;
        self.notes_gone.extend(other.notes_gone);
    }

    /// Adds everything `other` changed, its object lists included: used to sum the change sets of
    /// the live updates of a gesture into the one its end reports (ADR-0026 §4). The earliest
    /// position of the tempo map is the earlier of the two. O(entries of `other`); call
    /// [`finish`](Self::finish) afterwards to tidy the lists.
    pub(crate) fn absorb(&mut self, other: Self) {
        self.inserted.extend_from_slice(&other.inserted);
        self.removed.extend_from_slice(&other.removed);
        self.replaced.extend_from_slice(&other.replaced);
        if let Some(from) = other.tempo_from {
            self.tempo_from = Some(self.tempo_from.map_or(from, |mine| mine.min(from)));
        }
        self.absorb_effects(other);
    }

    /// Sorts and deduplicates the cells, so that the local check (`Document::check_cells`) checks
    /// each cell once: a command that moves thousands of events records every cell twice per
    /// event (where the event left and where it arrived), and checking each entry would cost
    /// O(events × chunk). In place, no allocation; O(c log c). [`finish`](Self::finish) sorts them
    /// again later at no extra cost.
    pub(crate) fn tidy_cells(&mut self) {
        self.cells.sort_unstable();
        self.cells.dedup();
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
        tidy(&mut self.dynamics);
        tidy(&mut self.notes_gone);
    }
}
