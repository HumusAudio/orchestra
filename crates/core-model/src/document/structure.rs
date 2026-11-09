// SPDX-License-Identifier: GPL-3.0-or-later

//! The structure of the document: [`Track`] → [`Instrument`] → [`Staff`] and [`Voice`], and the
//! [`BarList`] (ADR-0022 §2–3). A sub-module of `document` only to keep that file readable;
//! everything here is re-exported by it.

use orchestra_core_time::{BarIndex, Duration};

use crate::bar_repeat::{BarRepeat, PassRule};
use crate::content::{Chunk, VoiceContent, metric_duration};
use crate::dynamic::{Dynamic, Hairpin};
use crate::event::{Event, PlacedEvent, Tuplet};
use crate::id::{
    BarId, ClefId, IdMap, InstrumentId, KeyChangeId, SlurId, StaffId, TrackId, TupletId, VoiceId,
};
use crate::marks::{Clef, KeyOverride, MarkList};
use crate::pass_offsets::PassOffsets;
use crate::pitch::Interval;
use crate::slur::Slur;

use super::DocumentError;

/// A staff of an instrument (ADR-0022 §2) with its clefs (ADR-0024 §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staff {
    /// The staff's identity.
    pub(crate) id: StaffId,
    /// The clef at the start of the score. Part of the staff, so a staff without clef cannot
    /// exist (owner's decision of 2026-11-02).
    pub(crate) initial_clef: Clef,
    /// The clef changes after the start, sorted by (bar, offset).
    pub(crate) clef_changes: MarkList<ClefId, Clef>,
}

impl Staff {
    /// The staff's identity. O(1).
    pub const fn id(&self) -> StaffId {
        self.id
    }

    /// The clef at the start of the score. O(1).
    pub const fn initial_clef(&self) -> Clef {
        self.initial_clef
    }

    /// The clef changes after the start, in score order. O(1).
    pub const fn clef_changes(&self) -> &MarkList<ClefId, Clef> {
        &self.clef_changes
    }
}

/// A voice of an instrument (ADR-0022 §2) with its events and tuplets (ADR-0024 §1, §3).
///
/// A voice is an independent line, with its own rhythm and stems. It belongs to the instrument,
/// not to a staff: its **home staff** is where its events are drawn by default, and each event
/// records the staff it is actually drawn on, so a melody passing from the right hand to the left
/// hand of a piano part stays one voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    /// The voice's identity.
    pub(crate) id: VoiceId,
    /// The staff its events are drawn on by default: always a staff of the same instrument.
    pub(crate) home_staff: StaffId,
    /// One chunk of events per bar, and the tuplet table.
    pub(crate) content: VoiceContent,
}

impl Voice {
    /// The voice's identity. O(1).
    pub const fn id(&self) -> VoiceId {
        self.id
    }

    /// The staff the voice's events are drawn on by default (a staff of the same instrument).
    /// O(1).
    pub const fn home_staff(&self) -> StaffId {
        self.home_staff
    }

    /// The voice's events in the bar with this index, sorted by offset; empty if the voice has
    /// nothing there or the index is past the end. O(1), never allocates.
    pub fn events_at(&self, bar: BarIndex) -> &[PlacedEvent] {
        usize::try_from(bar.get())
            .ok()
            .and_then(|i| self.content.chunks.get(i))
            .map_or(&[], Chunk::events)
    }

    /// The tuplet with this ID, if the voice has it. O(1) on average.
    pub fn tuplet(&self, id: TupletId) -> Option<&Tuplet> {
        self.content.tuplets.get(&id)
    }

    /// How many tuplets the voice has. O(1).
    pub fn tuplet_count(&self) -> usize {
        self.content.tuplets.len()
    }

    /// The metric duration of one of the voice's events (ADR-0024 §2): its written length times
    /// the ratios of the tuplets containing it, or `bar_length` for a whole-bar rest. Layout and
    /// playback call this once per event while walking a bar, passing the bar's length they
    /// already have. O(tuplet depth) ≤ 4 lookups, never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownTuplet`] if the event is not one of this voice's (its tuplet is in
    /// another voice's table), [`DocumentError::Arithmetic`] on overflow (impossible within the
    /// bounds of ADR-0024).
    pub fn metric_duration(
        &self,
        event: &Event,
        bar_length: Duration,
    ) -> Result<Duration, DocumentError> {
        metric_duration(&self.content.tuplets, event.body(), bar_length)
    }
}

/// The instrument of an instrument track (ADR-0022 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    /// The instrument's identity.
    pub(crate) id: InstrumentId,
    /// The name the user gave it ("Flute", "Clarinetto in Si♭ 1"…). It is the user's own text,
    /// not a Fluent key: it is shown as written, in whatever language the user wrote it. Default
    /// names in the interface language come with the interface (F2), not from the model.
    pub(crate) name: String,
    /// The interval from written to sounding pitch (ADR-0023 §7): a major second down for a B♭
    /// clarinet, a unison for a concert-pitch instrument. Pitches are stored at concert pitch and
    /// the written ones derived (ADR-0004), so this is the only place the transposition lives.
    pub(crate) transposition: Interval,
    /// The staves, top to bottom; between 1 and [`MAX_STAVES_PER_INSTRUMENT`].
    pub(crate) staves: Vec<Staff>,
    /// The voices, in creation order; between 1 and [`MAX_VOICES_PER_INSTRUMENT`]. Each home
    /// staff is one of `staves`.
    pub(crate) voices: Vec<Voice>,
    /// Where the instrument departs from the document's key (ADR-0024 §9, K1). Usually empty.
    pub(crate) key_overrides: MarkList<KeyChangeId, KeyOverride>,
    /// The instrument's slurs, sorted by ID (ADR-0024 §7). IDs only grow, so a new slur is always
    /// appended at the end and the order is also the creation order: deterministic for the file
    /// writer, searchable in O(log s).
    pub(crate) slurs: Vec<Slur>,
    /// The instrument's dynamics (ADR-0024 §8), sorted by point in score order and, at one point,
    /// by ID (creation order). A sorted vector: a few per bar at most, read in order by playback
    /// and layout; inserting is O(log d) to find the place and O(d) to shift.
    pub(crate) dynamics: Vec<Dynamic>,
    /// The instrument's hairpins (ADR-0024 §8), sorted like `dynamics` by their start point.
    pub(crate) hairpins: Vec<Hairpin>,
    /// The bar repeats (%) on the instrument's staves (ADR-0025 §6), sorted by first bar.
    pub(crate) bar_repeats: Vec<BarRepeat>,
    /// The instrument's pass rules (ADR-0025 §7), sorted by start point.
    pub(crate) pass_rules: Vec<PassRule>,
    /// The pass-specific performance offsets of the instrument's notes (ADR-0028 §5): usually
    /// empty. See `crate::pass_offsets`.
    pub(crate) pass_offsets: PassOffsets,
}

impl Instrument {
    /// The instrument's identity. O(1).
    pub const fn id(&self) -> InstrumentId {
        self.id
    }

    /// The name the user gave the instrument. O(1).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The interval from written to sounding pitch (ADR-0023 §7). O(1).
    pub const fn transposition(&self) -> Interval {
        self.transposition
    }

    /// The staves, top to bottom (never empty). O(1).
    pub fn staves(&self) -> &[Staff] {
        &self.staves
    }

    /// The voices, in creation order (never empty). O(1).
    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// The instrument's departures from the document's key, in score order. O(1).
    pub const fn key_overrides(&self) -> &MarkList<KeyChangeId, KeyOverride> {
        &self.key_overrides
    }

    /// The staff with this ID, if it belongs to this instrument. O(staves) ≤ 16 comparisons.
    pub fn staff(&self, id: StaffId) -> Option<&Staff> {
        self.staves.iter().find(|s| s.id == id)
    }

    /// The voice with this ID, if it belongs to this instrument. O(voices) ≤ 256 comparisons.
    pub fn voice(&self, id: VoiceId) -> Option<&Voice> {
        self.voices.iter().find(|v| v.id == id)
    }

    /// The instrument's slurs, in creation order (which is ID order). O(1).
    pub fn slurs(&self) -> &[Slur] {
        &self.slurs
    }

    /// The instrument's dynamics, in score order (at one point, in creation order). O(1).
    pub fn dynamics(&self) -> &[Dynamic] {
        &self.dynamics
    }

    /// The instrument's hairpins, in score order of their starts (at one point, in creation
    /// order). O(1).
    pub fn hairpins(&self) -> &[Hairpin] {
        &self.hairpins
    }

    /// The bar repeats on the instrument's staves, in score order of their first bars. O(1).
    pub fn bar_repeats(&self) -> &[BarRepeat] {
        &self.bar_repeats
    }

    /// The instrument's pass rules, in score order of their starts. O(1).
    pub fn pass_rules(&self) -> &[PassRule] {
        &self.pass_rules
    }

    /// The pass-specific performance offsets of the instrument's notes (ADR-0028 §5). O(1).
    pub const fn pass_offsets(&self) -> &PassOffsets {
        &self.pass_offsets
    }

    /// The slur with this ID, if it belongs to this instrument. O(log s): a binary search in the
    /// table, which is sorted by ID.
    pub fn slur(&self, id: SlurId) -> Option<&Slur> {
        self.slurs
            .binary_search_by_key(&id, Slur::id)
            .ok()
            .and_then(|i| self.slurs.get(i))
    }
}

/// A track of the document (ADR-0022 §2).
///
/// Only instrument tracks exist today. Audio tracks (F3) and video tracks (F5) will make this a
/// choice of kinds, with a new version of the file format and its migration (ADR-0010); nothing
/// is written for them before they are needed (SCALETTA §1, rule 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// The track's identity.
    pub(crate) id: TrackId,
    /// The instrument the track plays.
    pub(crate) instrument: Instrument,
}

impl Track {
    /// The track's identity. O(1).
    pub const fn id(&self) -> TrackId {
        self.id
    }

    /// The instrument of the track. O(1).
    pub const fn instrument(&self) -> &Instrument {
        &self.instrument
    }
}

// ------------------------------------------------------------------------------------------------
// The bar list
// ------------------------------------------------------------------------------------------------

/// The ordered list of the document's bars (ADR-0022 §3).
///
/// Position `i` of the list is the bar with [`BarIndex`] `i` in the meter map. The list holds no
/// length, no meter, no number: those are in the [`MeterMap`], and repeating them here would be
/// the duplication ADR-0003 forbids. What it gives is a **stable identity** for each bar: what is
/// attached to a bar (barlines, voltas, rehearsal marks, the event chunks, clef and key changes)
/// is keyed by `BarId`, so inserting a bar (F1.11) shifts this list and the meter map's few
/// entries, and nothing else.
///
/// # Invariant
/// `index` is exactly the inverse of `order`: it has one entry per bar, `index[order[i]] == i`.
/// It is a derived cache (ADR-0003 §5), rebuilt from `order` whenever bars move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarList {
    /// The bars in score order.
    pub(crate) order: Vec<BarId>,
    /// The inverse of `order`: the index of each bar. Rebuilt when bars move.
    pub(crate) index: IdMap<BarId, u32>,
}

impl BarList {
    /// An empty bar list. Does not allocate. O(1).
    pub(super) fn new() -> Self {
        Self {
            order: Vec::new(),
            index: IdMap::default(),
        }
    }

    /// How many bars the document has. O(1).
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether the document has no bars at all. O(1).
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The sort key of a bar: its index, or `u32::MAX` for a bar the document does not have
    /// (which sorts after everything; the builder and the edits never let one in, so it only
    /// matters for half-built states). O(1) average.
    ///
    /// The one place of this rule, used by the form, the edit layer, the marks and the builder,
    /// so they all order bars the same way.
    ///
    /// `#[inline]`, like [`index_of`](Self::index_of): it runs for every mark compared in the
    /// sorted insertions and duplicate scans, and is one hash lookup. This file is its own
    /// codegen unit, and across codegen units LLVM inlines a non-generic function only when it is
    /// marked: without the hint, after `document.rs` was split, the editor benchmarks measured up
    /// to 1.5% more instructions (`insert_bar`, `undo_paste`; second codebase review, 2026-11-09).
    #[inline]
    pub(crate) fn key(&self, bar: BarId) -> u32 {
        self.index_of(bar).map_or(u32::MAX, BarIndex::get)
    }

    /// The bars in score order. O(1).
    pub fn ids(&self) -> &[BarId] {
        &self.order
    }

    /// The bar at this index, if the document has that many bars. O(1).
    pub fn id_at(&self, index: BarIndex) -> Option<BarId> {
        // `u32 → usize` is lossless on every platform we support (usize has at least 32 bits).
        self.order.get(usize::try_from(index.get()).ok()?).copied()
    }

    /// The index of this bar, if it belongs to the document. O(1) on average (one hash lookup).
    /// `#[inline]`: see [`key`](Self::key).
    #[inline]
    pub fn index_of(&self, id: BarId) -> Option<BarIndex> {
        self.index.get(&id).copied().map(BarIndex::new)
    }

    /// Appends a bar at the end. O(1) amortised: the new bar's index is the old length, and no
    /// other bar moves, so the inverse index only gains one entry.
    ///
    /// # Errors
    /// [`DocumentError::TooManyBars`] when the new index would not fit a [`BarIndex`] with the
    /// count still fitting a `u32` (so at most 2³² − 1 bars).
    pub(super) fn push(&mut self, id: BarId) -> Result<(), DocumentError> {
        let index = u32::try_from(self.order.len()).map_err(|_| DocumentError::TooManyBars)?;
        // `u32::MAX` as an index would make the count 2³², which does not fit a `u32`.
        if index == u32::MAX {
            return Err(DocumentError::TooManyBars);
        }
        self.order.push(id);
        self.index.insert(id, index);
        Ok(())
    }

    /// Inserts `ids` before the bar at position `at` (`at == len` appends). The caller has
    /// checked that the IDs are new and distinct. O(B) for B bars: the later bars move one place
    /// each in `order`, and their entries in the inverse index are rewritten.
    ///
    /// # Errors
    /// [`DocumentError::TooManyBars`] for an `at` past the end or a count that would no longer fit
    /// a `u32`; the list is then unchanged.
    pub(crate) fn insert(&mut self, at: usize, ids: &[BarId]) -> Result<(), DocumentError> {
        let total = self.order.len().checked_add(ids.len());
        // The count must fit a `u32` with the last index below `u32::MAX` (see `push`).
        if at > self.order.len() || total.is_none_or(|n| u32::try_from(n).is_err()) {
            return Err(DocumentError::TooManyBars);
        }
        self.order.splice(at..at, ids.iter().copied());
        self.reindex_from(at);
        Ok(())
    }

    /// Removes the `count` bars from position `at` on and returns their IDs in order. The caller
    /// has checked that the range lies inside the list. O(B), as [`insert`](Self::insert).
    pub(crate) fn remove(&mut self, at: usize, count: usize) -> Vec<BarId> {
        let end = at.saturating_add(count).min(self.order.len());
        let start = at.min(end);
        let removed: Vec<BarId> = self.order.drain(start..end).collect();
        for id in &removed {
            self.index.remove(id);
        }
        self.reindex_from(start);
        removed
    }

    /// Rewrites the inverse index of every bar from position `from` on. O(B − from).
    fn reindex_from(&mut self, from: usize) {
        for (i, id) in self.order.iter().enumerate().skip(from) {
            // Every position fits a `u32`: `insert` and `push` keep the count within it.
            if let Ok(index) = u32::try_from(i) {
                self.index.insert(*id, index);
            }
        }
    }
}
