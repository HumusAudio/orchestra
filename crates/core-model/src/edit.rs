// SPDX-License-Identifier: GPL-3.0-or-later

//! Primitive edits: the smallest changes the document knows, each returning its exact inverse
//! (ADR-0026 §1, SCALETTA F1.11b).
//!
//! # The vocabulary
//! Three verbs over one enum of object kinds ([`Object`], named by an [`ObjectRef`]), plus the bar
//! and meter operations and the values every document has once ([`Initial`]):
//!
//! ```text
//! Edit = Insert(Object) | Remove(ObjectRef) | Replace(Object)
//!      | InsertBars { at, ids, meter } | RemoveBars { at, count }
//!      | Meter(MeterEdit) | SetInitial(Initial)
//! ```
//!
//! Applying an edit returns the edit that undoes it: an `Insert` gives the `Remove` of the same
//! ID, a `Remove` gives the `Insert` of the whole removed object (with its exact place), a
//! `Replace` gives a `Replace` with the old content, `InsertBars` gives `RemoveBars` and back. So
//! undo is "apply the inverses in reverse order", and what undo returns is exactly the redo: one
//! mechanism, no second code path to keep in step (the history lives in `crate::editor`).
//!
//! # What an edit checks, and what it does not
//! An edit keeps the **structure** consistent: every object is in the right container, every list
//! stays sorted, the derived indices (event → location, note → event, slur → events) stay in step,
//! IDs are unique and were handed out by the document's counter. It fails, changing nothing, when
//! that would break.
//!
//! The **musical** invariants (a voice without overlaps, ties to the same pitch, a valid form, a
//! slur whose ends exist…) are the commands' job (F1.11b part 2, ADR-0026 §1) and are checked by
//! [`Document::validate`] in the tests. That is deliberate: a transaction may pass through
//! states that are musically invalid (a slur removed after its end event), and only the end
//! result must be valid.
//!
//! One rule follows, and every check below respects it: **an `Insert` checks only what no other
//! edit can take away**. Undoing a transaction applies its inverses in reverse order; if inserting
//! an object back could fail because of something a later edit of the same transaction removed,
//! undo itself could fail. So an event checks that its voice, bar, staff and tuplet exist (bars
//! are only removed when nothing refers to them, tuplets only when no event uses them, voices and
//! staves are never removed), but a slur does not check that its events exist, nor a jump that
//! its markers do.
//!
//! # Sort orders that do not depend on history
//! Where a list may hold several objects at one point (dynamics, hairpins, voltas, bar repeats,
//! pass rules), they are sorted by (point, ID). The builder already produced that order (IDs only
//! grow), and keeping it for edits means a list is a pure function of its objects: removing an
//! object and inserting it back gives an identical list, as undo needs. Events are the exception,
//! because their order at one (offset, slot) is musical (three grace notes in a row): a removed
//! event remembers its exact place in the chunk ([`Object::Event`]'s `place`).

use std::mem::size_of;

use orchestra_core_time::{BarIndex, BarSpan, BeatGrouping, Duration, IrregularBar, MeterChange};

use crate::bar_repeat::{BarRepeat, PassRule};
use crate::change::ChangeSet;
use crate::content::Chunk;
use crate::document::{Document, DocumentError, EventLocation, index_tie, unindex_tie};
use crate::dynamic::{Dynamic, DynamicPlacement, DynamicScope, Hairpin};
use crate::event::{MAX_TUPLET_DEPTH, Note, PlacedEvent, Tie, Tuplet};
use crate::form::{BarNumbering, MAX_PLAYS, RepeatEnd, RepeatStart, Volta};
use crate::id::{
    BarId, BarRepeatId, ClefId, DynamicId, EventId, HairpinId, IdSet, InstrumentId, JumpId,
    KeyChangeId, MarkerId, PassRuleId, RepeatId, SlurId, StaffId, TempoMarkId, TupletId, VoiceId,
    VoltaId,
};
use crate::jump::{Jump, Marker};
use crate::marks::{Clef, KeyOverride, Mark, ScorePoint, point_key};
use crate::pitch::Key;
use crate::slur::{Slur, SlurRefs};
use crate::tempo_mark::{TempoMark, TempoMarkChange};

// ------------------------------------------------------------------------------------------------
// The vocabulary
// ------------------------------------------------------------------------------------------------

/// The name of an object of the document: its kind and its ID, or one of the values the document
/// has exactly once. What [`Edit::Remove`] takes, and what a [`ChangeSet`] lists.
///
/// Ordered (by kind, then ID) so that change sets can be sorted and deduplicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectRef {
    /// A bar (only in change sets: bars are inserted and removed with [`Edit::InsertBars`] and
    /// [`Edit::RemoveBars`]).
    Bar(BarId),
    /// An event (a chord or a rest) with its noteheads.
    Event(EventId),
    /// A tuplet of a voice.
    Tuplet(TupletId),
    /// A slur of an instrument.
    Slur(SlurId),
    /// A dynamic of an instrument.
    Dynamic(DynamicId),
    /// A hairpin of an instrument.
    Hairpin(HairpinId),
    /// A tempo mark after the start of the score.
    TempoMark(TempoMarkId),
    /// A clef change of a staff.
    ClefChange(ClefId),
    /// A key change of the whole score.
    KeyChange(KeyChangeId),
    /// A key override of an instrument.
    KeyOverride(KeyChangeId),
    /// A start repeat barline.
    RepeatStart(RepeatId),
    /// An end repeat barline.
    RepeatEnd(RepeatId),
    /// A volta.
    Volta(VoltaId),
    /// A segno, coda, Fine or "To Coda".
    Marker(MarkerId),
    /// A jump (D.C., D.S. and their variants).
    Jump(JumpId),
    /// A bar repeat (%) on a staff.
    BarRepeat(BarRepeatId),
    /// A pass rule of an instrument.
    PassRule(PassRuleId),
    /// The tempo mark at the very start of the score.
    InitialTempo,
    /// The key at the very start of the score.
    InitialKey,
    /// The clef at the very start of a staff.
    InitialClef(StaffId),
    /// The bar-numbering style of the form.
    Numbering,
}

/// An object with everything needed to put it in its place: what [`Edit::Insert`] and
/// [`Edit::Replace`] take, and what [`Edit::Remove`]'s inverse carries.
///
/// The container of each object (its voice, staff or instrument) is part of the variant, so that
/// inserting never has to guess where an object goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Object {
    /// An event in the chunk of `voice` for `bar`, at the offset `placed` carries.
    Event {
        /// The voice holding it.
        voice: VoiceId,
        /// The bar whose chunk holds it.
        bar: BarId,
        /// The event with its offset.
        placed: PlacedEvent,
        /// Its exact place in the chunk, or `None` for "after the events with the same (offset,
        /// slot)", the place of a newly entered event. A removed event reports its place here, so
        /// that its inverse puts it back between the same neighbours (see the module docs).
        place: Option<u32>,
    },
    /// A tuplet in the table of `voice`.
    Tuplet {
        /// The voice whose table holds it.
        voice: VoiceId,
        /// Its ID (tuplets are keyed by ID in the table, they do not carry it).
        id: TupletId,
        /// The tuplet, with its parent.
        tuplet: Tuplet,
    },
    /// A slur in the table of `instrument`.
    Slur {
        /// The instrument holding it.
        instrument: InstrumentId,
        /// The slur.
        slur: Slur,
    },
    /// A dynamic of `instrument`.
    Dynamic {
        /// The instrument holding it.
        instrument: InstrumentId,
        /// The dynamic.
        dynamic: Dynamic,
    },
    /// A hairpin of `instrument`.
    Hairpin {
        /// The instrument holding it.
        instrument: InstrumentId,
        /// The hairpin.
        hairpin: Hairpin,
    },
    /// A tempo mark of the document.
    TempoMark(Mark<TempoMarkId, TempoMark>),
    /// A clef change of `staff`.
    ClefChange {
        /// The staff holding it.
        staff: StaffId,
        /// The change.
        mark: Mark<ClefId, Clef>,
    },
    /// A key change of the document.
    KeyChange(Mark<KeyChangeId, Key>),
    /// A key override of `instrument`.
    KeyOverride {
        /// The instrument holding it.
        instrument: InstrumentId,
        /// The override.
        mark: Mark<KeyChangeId, KeyOverride>,
    },
    /// A start repeat barline.
    RepeatStart(RepeatStart),
    /// An end repeat barline.
    RepeatEnd(RepeatEnd),
    /// A volta.
    Volta(Volta),
    /// A marker.
    Marker(Marker),
    /// A jump.
    Jump(Jump),
    /// A bar repeat; its staff names its instrument.
    BarRepeat(BarRepeat),
    /// A pass rule of `instrument`.
    PassRule {
        /// The instrument holding it.
        instrument: InstrumentId,
        /// The rule.
        rule: PassRule,
    },
}

impl Object {
    /// The name of this object. O(1).
    #[must_use]
    pub const fn reference(&self) -> ObjectRef {
        match self {
            Self::Event { placed, .. } => ObjectRef::Event(placed.event().id()),
            Self::Tuplet { id, .. } => ObjectRef::Tuplet(*id),
            Self::Slur { slur, .. } => ObjectRef::Slur(slur.id()),
            Self::Dynamic { dynamic, .. } => ObjectRef::Dynamic(dynamic.id()),
            Self::Hairpin { hairpin, .. } => ObjectRef::Hairpin(hairpin.id()),
            Self::TempoMark(mark) => ObjectRef::TempoMark(mark.id()),
            Self::ClefChange { mark, .. } => ObjectRef::ClefChange(mark.id()),
            Self::KeyChange(mark) => ObjectRef::KeyChange(mark.id()),
            Self::KeyOverride { mark, .. } => ObjectRef::KeyOverride(mark.id()),
            Self::RepeatStart(r) => ObjectRef::RepeatStart(r.id()),
            Self::RepeatEnd(r) => ObjectRef::RepeatEnd(r.id()),
            Self::Volta(v) => ObjectRef::Volta(v.id()),
            Self::Marker(m) => ObjectRef::Marker(m.id()),
            Self::Jump(j) => ObjectRef::Jump(j.id()),
            Self::BarRepeat(r) => ObjectRef::BarRepeat(r.id()),
            Self::PassRule { rule, .. } => ObjectRef::PassRule(rule.id()),
        }
    }

    /// An estimate of the heap memory the object owns beyond its own size, in bytes: the extra
    /// noteheads of a chord (the first one is inline). Texts (tens of bytes) are not counted. Used
    /// to bound the history by memory (ADR-0026 §3). O(noteheads).
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Event { placed, .. } => placed.event().chord().map_or(0, |chord| {
                chord
                    .notes()
                    .count()
                    .saturating_sub(1)
                    .saturating_mul(size_of::<Note>())
            }),
            _ => 0,
        }
    }
}

/// An edit of the meter map's source data, at a bar named by its ID (ADR-0018, ADR-0019). `None`
/// (or `false`) removes the entry. Each returns the previous value as its inverse.
///
/// Not `Copy` since [`Span`](Self::Span) carries a [`BarSpan`] (vectors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeterEdit {
    /// The meter change at a bar (never `None` at the first bar: some meter is always in force).
    MeterChange {
        /// The bar.
        bar: BarId,
        /// The change, or `None` to remove it.
        change: Option<MeterChange>,
    },
    /// The irregular length of a bar.
    IrregularBar {
        /// The bar.
        bar: BarId,
        /// The irregular length and kind, or `None` for a regular bar.
        irregular: Option<IrregularBar>,
    },
    /// The beat grouping of one bar.
    BarGrouping {
        /// The bar.
        bar: BarId,
        /// The grouping, or `None` for the section's counting.
        grouping: Option<BeatGrouping>,
    },
    /// Whether a bar is left out of the displayed numbering.
    Uncounted {
        /// The bar.
        bar: BarId,
        /// `true` to leave it uncounted.
        uncounted: bool,
    },
    /// A restart of the displayed numbering at a bar.
    NumberingRestart {
        /// The bar.
        bar: BarId,
        /// The number it restarts with, or `None` to remove the restart.
        number: Option<u32>,
    },
    /// The whole source data of the `span.count()` bars from `bar` on, replaced at once
    /// (`MeterMap::replace_span`, ADR-0026 §11 Z1): what the meter change that rewrites the
    /// music needs, since its bars change together and may pass through states that are not
    /// valid one bar at a time. Its inverse is the same edit with the data the bars had.
    Span {
        /// The first bar of the run.
        bar: BarId,
        /// The new data of the run's bars, at bars relative to it.
        span: BarSpan,
    },
}

/// A value the document has exactly once (or once per staff), set by [`Edit::SetInitial`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initial {
    /// The tempo mark at the start of the score; it must set a tempo at once (ADR-0024 §11, E1).
    Tempo(TempoMark),
    /// The key at the start of the score, at concert pitch.
    Key(Key),
    /// The clef at the start of a staff.
    Clef {
        /// The staff.
        staff: StaffId,
        /// The clef.
        clef: Clef,
    },
    /// The bar-numbering style (ADR-0025 F9).
    Numbering(BarNumbering),
}

/// A primitive edit (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Puts a new object in its place. Its ID must have been handed out by the document's counter
    /// and must not be in use.
    Insert(Object),
    /// Takes an object out.
    Remove(ObjectRef),
    /// Gives an existing object (the same ID) a new content or a new place.
    Replace(Object),
    /// Inserts empty bars before the bar at `at` (`at` = the number of bars appends them), with
    /// these new IDs and the meter map data of `meter` (whose count must be `ids.len()`). Every
    /// voice gains an empty chunk per bar.
    InsertBars {
        /// Where the new bars go.
        at: BarIndex,
        /// Their IDs, in order.
        ids: Vec<BarId>,
        /// Their meter map data (an empty span for plain new bars).
        meter: BarSpan,
    },
    /// Removes the `count` bars from `at` on. They must carry nothing: no event in any voice, and
    /// no mark, spanner end or form marking anchored on them (the command that deletes bars first
    /// removes or moves what is there, ADR-0026 §6).
    RemoveBars {
        /// The first bar removed.
        at: BarIndex,
        /// How many bars are removed.
        count: u32,
    },
    /// Edits the meter map's source data.
    Meter(MeterEdit),
    /// Sets one of the values the document has once.
    SetInitial(Initial),
}

impl Edit {
    /// An estimate of the memory this edit holds, in bytes: its own size plus what it owns on the
    /// heap. The history adds these up to stay within its memory limit (ADR-0026 §3). O(size of
    /// the edit's data).
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        let owned = match self {
            Self::Insert(object) | Self::Replace(object) => object.heap_bytes(),
            Self::InsertBars { ids, meter, .. } => {
                ids.len().saturating_mul(size_of::<BarId>()).saturating_add(
                    span_entries(meter).saturating_mul(size_of::<(BarIndex, MeterChange)>()),
                )
            }
            Self::Meter(MeterEdit::Span { span, .. }) => {
                span_entries(span).saturating_mul(size_of::<(BarIndex, MeterChange)>())
            }
            Self::Remove(_) | Self::RemoveBars { .. } | Self::Meter(_) | Self::SetInitial(_) => 0,
        };
        size_of::<Self>().saturating_add(owned)
    }
}

/// How many source entries a span holds, of every kind: what its vectors cost in the history's
/// estimate (each entry is counted at the size of the largest kind, a meter change). O(1).
fn span_entries(span: &BarSpan) -> usize {
    span.meter_changes()
        .len()
        .saturating_add(span.irregular_bars().len())
        .saturating_add(span.bar_groupings().len())
        .saturating_add(span.uncounted_bars().len())
        .saturating_add(span.numbering_restarts().len())
}

// ------------------------------------------------------------------------------------------------
// Applying an edit
// ------------------------------------------------------------------------------------------------

/// Inserts `item` into a list kept sorted by `key`, before the first element with a larger or
/// equal key. The keys used here all end with the object's ID, so they are unique and the result
/// does not depend on the order of the insertions. O(log n) comparisons, O(n) to shift.
fn insert_sorted<T, K: Ord>(list: &mut Vec<T>, item: T, key: impl Fn(&T) -> K) {
    let k = key(&item);
    let at = list.partition_point(|x| key(x) < k);
    list.insert(at, item);
}

/// The ties that start from the noteheads of an event, as (source, target) pairs: what the
/// backward tie index (`Document::tie_index`) records for it. O(noteheads); allocates only for an
/// event with ties (most events have none, and an empty `Vec` does not allocate).
fn outgoing_ties(placed: &PlacedEvent) -> Vec<(crate::id::NoteId, crate::id::NoteId)> {
    placed
        .event()
        .chord()
        .into_iter()
        .flat_map(crate::event::Chord::notes)
        .filter_map(|note| match note.tie() {
            Tie::To(target) => Some((note.id(), target)),
            Tie::None | Tie::LetRing => None,
        })
        .collect()
}

/// The ID of a bar's position, for sorting: its index, or `u32::MAX` for an unknown bar (which
/// sorts last instead of panicking; a placed object's bar is always checked first). O(1) average.
fn bar_key(doc_bars: &crate::document::BarList, bar: BarId) -> u32 {
    doc_bars.index_of(bar).map_or(u32::MAX, BarIndex::get)
}

impl Document {
    /// Applies one primitive edit, records what it changed in `changes`, and returns its inverse.
    /// On error nothing changed. See the module documentation for what is checked.
    ///
    /// # Complexity
    /// Events: O(1) average for the indices + O(voices) + O(k) for the chunk (k events in it).
    /// Marks and spanners: O(n) in their list (tens to hundreds). Bars: O(B × V) pointer moves for
    /// B bars and V voices, plus the meter map's O(m log m) rebuild. Never O(events of the score).
    ///
    /// # Errors
    /// The typed [`DocumentError`] of the first check that fails.
    pub(crate) fn apply_edit(
        &mut self,
        edit: Edit,
        changes: &mut ChangeSet,
    ) -> Result<Edit, DocumentError> {
        match edit {
            Edit::Insert(object) => {
                let name = object.reference();
                self.insert_object(object, changes)?;
                Ok(Edit::Remove(name))
            }
            Edit::Remove(name) => Ok(Edit::Insert(self.remove_object(name, changes)?)),
            Edit::Replace(object) => Ok(Edit::Replace(self.replace_object(object, changes)?)),
            Edit::InsertBars { at, ids, meter } => {
                self.insert_bars(at, &ids, &meter, changes)?;
                let count = u32::try_from(ids.len()).map_err(|_| DocumentError::TooManyBars)?;
                Ok(Edit::RemoveBars { at, count })
            }
            Edit::RemoveBars { at, count } => {
                let (ids, meter) = self.remove_bars(at, count, changes)?;
                Ok(Edit::InsertBars { at, ids, meter })
            }
            Edit::Meter(edit) => Ok(Edit::Meter(self.edit_meter(edit, changes)?)),
            Edit::SetInitial(initial) => Ok(Edit::SetInitial(self.set_initial(initial, changes)?)),
        }
    }

    // --------------------------------------------------------------------------------------------
    // Finding objects
    // --------------------------------------------------------------------------------------------

    /// The track position of an instrument. O(tracks).
    fn track_of_instrument(&self, id: InstrumentId) -> Result<usize, DocumentError> {
        self.tracks
            .iter()
            .position(|t| t.instrument.id == id)
            .ok_or(DocumentError::UnknownInstrument(id))
    }

    /// The track position of the instrument owning a staff, and the staff's position in it.
    /// O(staves of the document).
    fn position_of_staff(&self, id: StaffId) -> Result<(usize, usize), DocumentError> {
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(t, track)| {
                track
                    .instrument
                    .staves
                    .iter()
                    .position(|s| s.id == id)
                    .map(|s| (t, s))
            })
            .ok_or(DocumentError::UnknownStaff(id))
    }

    /// The track position of the instrument owning a voice, and the voice's position in it.
    /// O(voices of the document).
    fn position_of_voice(&self, id: VoiceId) -> Result<(usize, usize), DocumentError> {
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(t, track)| {
                track
                    .instrument
                    .voices
                    .iter()
                    .position(|v| v.id == id)
                    .map(|v| (t, v))
            })
            .ok_or(DocumentError::UnknownVoice(id))
    }

    /// The position in its instrument's list of the object found by `find`, with the track
    /// position. O(objects of that kind in the document).
    fn find_in_instruments(
        &self,
        find: impl Fn(&crate::document::Instrument) -> Option<usize>,
    ) -> Option<(usize, usize)> {
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(t, track)| find(&track.instrument).map(|i| (t, i)))
    }

    /// Whether an object with this name exists. Used to refuse a duplicate ID. O(objects of that
    /// kind in the document) at worst; O(1) average for events.
    fn contains(&self, name: ObjectRef) -> bool {
        match name {
            ObjectRef::Bar(id) => self.bars.index_of(id).is_some(),
            ObjectRef::Event(id) => self.events.contains_key(&id),
            ObjectRef::Tuplet(id) => self
                .instruments()
                .flat_map(|i| i.voices.iter())
                .any(|v| v.content.tuplets.contains_key(&id)),
            ObjectRef::Slur(id) => self.slur(id).is_some(),
            ObjectRef::Dynamic(id) => self
                .find_in_instruments(|i| i.dynamics.iter().position(|d| d.id() == id))
                .is_some(),
            ObjectRef::Hairpin(id) => self
                .find_in_instruments(|i| i.hairpins.iter().position(|h| h.id() == id))
                .is_some(),
            ObjectRef::TempoMark(id) => {
                self.initial_tempo.0 == id
                    || self.tempo_marks.as_slice().iter().any(|m| m.id() == id)
            }
            ObjectRef::ClefChange(id) => self
                .instruments()
                .flat_map(|i| i.staves.iter())
                .any(|s| s.clef_changes.as_slice().iter().any(|m| m.id() == id)),
            // Key changes and key overrides share their kind of ID.
            ObjectRef::KeyChange(id) | ObjectRef::KeyOverride(id) => {
                self.key_changes.as_slice().iter().any(|m| m.id() == id)
                    || self
                        .instruments()
                        .any(|i| i.key_overrides.as_slice().iter().any(|m| m.id() == id))
            }
            // Start and end repeats share their kind of ID.
            ObjectRef::RepeatStart(id) | ObjectRef::RepeatEnd(id) => {
                self.form.repeat_starts.iter().any(|r| r.id() == id)
                    || self.form.repeat_ends.iter().any(|r| r.id() == id)
            }
            ObjectRef::Volta(id) => self.form.voltas.iter().any(|v| v.id() == id),
            ObjectRef::Marker(id) => self.form.markers.iter().any(|m| m.id() == id),
            ObjectRef::Jump(id) => self.form.jumps.iter().any(|j| j.id() == id),
            ObjectRef::BarRepeat(id) => self
                .find_in_instruments(|i| i.bar_repeats.iter().position(|r| r.id() == id))
                .is_some(),
            ObjectRef::PassRule(id) => self
                .find_in_instruments(|i| i.pass_rules.iter().position(|r| r.id() == id))
                .is_some(),
            ObjectRef::InitialTempo
            | ObjectRef::InitialKey
            | ObjectRef::InitialClef(_)
            | ObjectRef::Numbering => true,
        }
    }

    /// Checks that an object's ID was handed out by the counter and is not in use. O(as
    /// [`contains`](Self::contains)).
    fn check_new(&self, name: ObjectRef) -> Result<(), DocumentError> {
        let raw = match name {
            ObjectRef::Bar(id) => Some(id.get()),
            ObjectRef::Event(id) => Some(id.get()),
            ObjectRef::Tuplet(id) => Some(id.get()),
            ObjectRef::Slur(id) => Some(id.get()),
            ObjectRef::Dynamic(id) => Some(id.get()),
            ObjectRef::Hairpin(id) => Some(id.get()),
            ObjectRef::TempoMark(id) => Some(id.get()),
            ObjectRef::ClefChange(id) => Some(id.get()),
            ObjectRef::KeyChange(id) | ObjectRef::KeyOverride(id) => Some(id.get()),
            ObjectRef::RepeatStart(id) | ObjectRef::RepeatEnd(id) => Some(id.get()),
            ObjectRef::Volta(id) => Some(id.get()),
            ObjectRef::Marker(id) => Some(id.get()),
            ObjectRef::Jump(id) => Some(id.get()),
            ObjectRef::BarRepeat(id) => Some(id.get()),
            ObjectRef::PassRule(id) => Some(id.get()),
            ObjectRef::InitialTempo
            | ObjectRef::InitialKey
            | ObjectRef::InitialClef(_)
            | ObjectRef::Numbering => None,
        };
        // An ID the counter has not handed out yet would collide with a future object.
        if raw.is_none_or(|raw| !self.ids.has_issued(raw)) || self.contains(name) {
            return Err(DocumentError::DuplicateObject(name));
        }
        Ok(())
    }

    /// Checks a (bar, offset) point: the bar exists and the offset is not negative. Returns the
    /// bar's index. O(1) average.
    fn check_point(&self, point: ScorePoint) -> Result<BarIndex, DocumentError> {
        let index = self.bar_index(point.bar)?;
        if point.offset.is_negative() {
            return Err(DocumentError::NegativeOffset);
        }
        Ok(index)
    }

    /// Checks a dynamic's (or hairpin's) placement against its instrument: the staff and the voice
    /// of the scope belong to it. O(staves + voices).
    fn check_placement(&self, t: usize, placement: DynamicPlacement) -> Result<(), DocumentError> {
        let instrument = &self
            .tracks
            .get(t)
            .ok_or(DocumentError::Inconsistent)?
            .instrument;
        if instrument.staff(placement.staff).is_none() {
            return Err(DocumentError::StaffOfAnotherInstrument(placement.staff));
        }
        if let DynamicScope::Voice(voice) = placement.scope
            && instrument.voice(voice).is_none()
        {
            return Err(DocumentError::VoiceOfAnotherInstrument(voice));
        }
        Ok(())
    }

    // --------------------------------------------------------------------------------------------
    // Insert
    // --------------------------------------------------------------------------------------------

    /// Puts an object in its place (see [`Edit::Insert`]). On error nothing changed.
    #[allow(
        clippy::too_many_lines,
        reason = "one short arm per kind of object: splitting the match would only scatter it"
    )]
    fn insert_object(
        &mut self,
        object: Object,
        changes: &mut ChangeSet,
    ) -> Result<(), DocumentError> {
        let name = object.reference();
        self.check_new(name)?;
        match object {
            Object::Event {
                voice,
                bar,
                placed,
                place,
            } => self.insert_event(voice, bar, placed, place, changes)?,
            Object::Tuplet { voice, id, tuplet } => {
                let (t, v) = self.position_of_voice(voice)?;
                let content = &self.voice_at(t, v)?.content;
                if let Some(parent) = tuplet.parent() {
                    if !content.tuplets.contains_key(&parent) {
                        return Err(DocumentError::UnknownTuplet(parent));
                    }
                    if content.depth_of(parent) >= MAX_TUPLET_DEPTH {
                        return Err(DocumentError::TupletTooDeep);
                    }
                }
                self.voice_at_mut(t, v)?.content.tuplets.insert(id, tuplet);
            }
            Object::Slur { instrument, slur } => {
                if slur.start() == slur.end() {
                    return Err(DocumentError::SlurToItself);
                }
                let t = self.track_of_instrument(instrument)?;
                let slurs = &mut self.track_at_mut(t)?.instrument.slurs;
                insert_sorted(slurs, slur, Slur::id);
                for end in [slur.start(), slur.end()] {
                    self.slur_index
                        .entry(end)
                        .and_modify(|refs| refs.insert(slur.id()))
                        .or_insert_with(|| SlurRefs::one(slur.id()));
                }
            }
            Object::Dynamic {
                instrument,
                dynamic,
            } => {
                self.check_point(dynamic.at())?;
                let t = self.track_of_instrument(instrument)?;
                self.check_placement(t, dynamic.placement())?;
                let Self { bars, tracks, .. } = self;
                let list = &mut tracks
                    .get_mut(t)
                    .ok_or(DocumentError::UnknownInstrument(instrument))?
                    .instrument
                    .dynamics;
                let at = point_key(bars, dynamic.at());
                // At most one dynamic of a scope at one point (rule of the builder, ADR-0024 §8).
                if list.iter().any(|d| {
                    point_key(bars, d.at()) == at
                        && d.placement().scope == dynamic.placement().scope
                }) {
                    return Err(DocumentError::DuplicateMark);
                }
                insert_sorted(list, dynamic, |d| (point_key(bars, d.at()), d.id()));
            }
            Object::Hairpin {
                instrument,
                hairpin,
            } => {
                self.check_point(hairpin.start())?;
                self.check_point(hairpin.end())?;
                if point_key(&self.bars, hairpin.end()) <= point_key(&self.bars, hairpin.start()) {
                    return Err(DocumentError::HairpinNotForward);
                }
                let t = self.track_of_instrument(instrument)?;
                self.check_placement(t, hairpin.placement())?;
                let Self { bars, tracks, .. } = self;
                let list = &mut tracks
                    .get_mut(t)
                    .ok_or(DocumentError::UnknownInstrument(instrument))?
                    .instrument
                    .hairpins;
                insert_sorted(list, hairpin, |h| (point_key(bars, h.start()), h.id()));
            }
            Object::TempoMark(mark) => {
                let index = self.check_point(mark.point())?;
                if let Some(TempoMarkChange::Gradual(gradual)) = mark.value().change() {
                    self.check_point(gradual.end)?;
                }
                if index == BarIndex::FIRST && mark.offset().is_zero() {
                    return Err(DocumentError::DuplicateMark);
                }
                let Self {
                    bars, tempo_marks, ..
                } = self;
                tempo_marks.insert(mark, bars)?;
            }
            Object::ClefChange { staff, mark } => {
                let index = self.check_point(mark.point())?;
                if index == BarIndex::FIRST && mark.offset().is_zero() {
                    return Err(DocumentError::DuplicateMark);
                }
                let (t, s) = self.position_of_staff(staff)?;
                let Self { bars, tracks, .. } = self;
                tracks
                    .get_mut(t)
                    .and_then(|track| track.instrument.staves.get_mut(s))
                    .ok_or(DocumentError::UnknownStaff(staff))?
                    .clef_changes
                    .insert(mark, bars)?;
            }
            Object::KeyChange(mark) => {
                let index = self.check_point(mark.point())?;
                if index == BarIndex::FIRST && mark.offset().is_zero() {
                    return Err(DocumentError::DuplicateMark);
                }
                let Self {
                    bars, key_changes, ..
                } = self;
                key_changes.insert(mark, bars)?;
            }
            Object::KeyOverride { instrument, mark } => {
                self.check_point(mark.point())?;
                let t = self.track_of_instrument(instrument)?;
                let Self { bars, tracks, .. } = self;
                tracks
                    .get_mut(t)
                    .ok_or(DocumentError::UnknownInstrument(instrument))?
                    .instrument
                    .key_overrides
                    .insert(mark, bars)?;
            }
            Object::RepeatStart(repeat) => {
                let at = self.bar_index(repeat.bar())?.get();
                let Self { bars, form, .. } = self;
                if form
                    .repeat_starts
                    .iter()
                    .any(|r| bar_key(bars, r.bar()) == at)
                {
                    return Err(DocumentError::DuplicateMark);
                }
                insert_sorted(&mut form.repeat_starts, repeat, |r| bar_key(bars, r.bar()));
            }
            Object::RepeatEnd(repeat) => {
                if !(2..=MAX_PLAYS).contains(&repeat.times()) {
                    return Err(DocumentError::PlaysOutOfRange(repeat.times()));
                }
                let at = self.bar_index(repeat.bar())?.get();
                let Self { bars, form, .. } = self;
                if form
                    .repeat_ends
                    .iter()
                    .any(|r| bar_key(bars, r.bar()) == at)
                {
                    return Err(DocumentError::DuplicateMark);
                }
                insert_sorted(&mut form.repeat_ends, repeat, |r| bar_key(bars, r.bar()));
            }
            Object::Volta(volta) => {
                let first = self.bar_index(volta.first())?;
                let last = self.bar_index(volta.last())?;
                if last < first {
                    return Err(DocumentError::VoltaNotForward);
                }
                if volta.passes().is_empty() {
                    return Err(DocumentError::EmptyPassSet);
                }
                let Self { bars, form, .. } = self;
                insert_sorted(&mut form.voltas, volta, |v| {
                    (bar_key(bars, v.first()), v.id())
                });
            }
            Object::Marker(marker) => {
                let at = self.bar_index(marker.bar())?.get();
                let Self { bars, form, .. } = self;
                // Score order of markers: bar, then the start side before the end side.
                let key = |m: &Marker| (bar_key(bars, m.bar()), u8::from(!m.kind().at_start()));
                let own = (at, u8::from(!marker.kind().at_start()));
                if form.markers.iter().any(|m| key(m) == own) {
                    return Err(DocumentError::DuplicateMark);
                }
                insert_sorted(&mut form.markers, marker, key);
            }
            Object::Jump(jump) => {
                let at = self.bar_index(jump.bar())?.get();
                let Self { bars, form, .. } = self;
                if form.jumps.iter().any(|j| bar_key(bars, j.bar()) == at) {
                    return Err(DocumentError::DuplicateMark);
                }
                insert_sorted(&mut form.jumps, jump, |j| bar_key(bars, j.bar()));
            }
            Object::BarRepeat(repeat) => {
                self.bar_index(repeat.first())?;
                if repeat.count() == 0 || !repeat.count().is_multiple_of(repeat.span().bars()) {
                    return Err(DocumentError::BarRepeatCount);
                }
                let (t, _) = self.position_of_staff(repeat.staff())?;
                let Self { bars, tracks, .. } = self;
                let list = &mut tracks
                    .get_mut(t)
                    .ok_or(DocumentError::UnknownStaff(repeat.staff()))?
                    .instrument
                    .bar_repeats;
                insert_sorted(list, repeat, |r| (bar_key(bars, r.first()), r.id()));
            }
            Object::PassRule { instrument, rule } => {
                self.check_point(rule.start())?;
                self.check_point(rule.end())?;
                if point_key(&self.bars, rule.end()) <= point_key(&self.bars, rule.start()) {
                    return Err(DocumentError::PassRuleNotForward);
                }
                if rule.plays().is_empty() {
                    return Err(DocumentError::EmptyPassSet);
                }
                let t = self.track_of_instrument(instrument)?;
                let Self { bars, tracks, .. } = self;
                let target = &mut tracks
                    .get_mut(t)
                    .ok_or(DocumentError::UnknownInstrument(instrument))?
                    .instrument;
                if let DynamicScope::Voice(voice) = rule.scope()
                    && target.voice(voice).is_none()
                {
                    return Err(DocumentError::VoiceOfAnotherInstrument(voice));
                }
                insert_sorted(&mut target.pass_rules, rule, |r| {
                    (point_key(bars, r.start()), r.id())
                });
            }
        }
        changes.insert(name);
        Ok(())
    }

    /// Inserts an event into its chunk and the indices (see [`Object::Event`]). The event's own
    /// ID has been checked by the caller; its noteheads' IDs are checked here. On error nothing
    /// changed. O(voices + staves + k + noteheads).
    fn insert_event(
        &mut self,
        voice: VoiceId,
        bar: BarId,
        placed: PlacedEvent,
        place: Option<u32>,
        changes: &mut ChangeSet,
    ) -> Result<(), DocumentError> {
        let index = self.check_point(ScorePoint::new(bar, placed.offset()))?;
        let (t, v) = self.position_of_voice(voice)?;
        let track = self.track_at(t)?;
        let target = track
            .instrument
            .voices
            .get(v)
            .ok_or(DocumentError::UnknownVoice(voice))?;
        let staff = placed.event().staff();
        if track.instrument.staff(staff).is_none() {
            return Err(DocumentError::StaffOfAnotherInstrument(staff));
        }
        if let Some(tuplet) = placed.event().tuplet()
            && !target.content.tuplets.contains_key(&tuplet)
        {
            return Err(DocumentError::UnknownTuplet(tuplet));
        }
        // Every notehead needs an issued, unused ID, distinct from the chord's other noteheads.
        let id = placed.event().id();
        let notes: Vec<_> = placed
            .event()
            .chord()
            .map(|chord| chord.notes().map(Note::id).collect())
            .unwrap_or_default();
        for (i, note) in notes.iter().enumerate() {
            let repeated = notes.get(..i).is_some_and(|earlier| earlier.contains(note));
            if repeated || self.notes.contains_key(note) || !self.ids.has_issued(note.get()) {
                return Err(DocumentError::DuplicateObject(ObjectRef::Event(id)));
            }
        }
        let ties = outgoing_ties(&placed);
        let chunk = self.chunk_at_mut(t, v, index, bar)?;
        match place {
            None => chunk.insert(placed),
            Some(place) => {
                let place = usize::try_from(place).map_err(|_| DocumentError::PlaceOutOfOrder)?;
                if !chunk.insert_at(place, placed) {
                    return Err(DocumentError::PlaceOutOfOrder);
                }
            }
        }
        self.events.insert(id, EventLocation { voice, bar });
        for note in notes {
            self.notes.insert(note, id);
        }
        for (source, target) in ties {
            index_tie(&mut self.tie_index, source, target);
        }
        changes.cell(voice, bar);
        Ok(())
    }

    // --------------------------------------------------------------------------------------------
    // Remove
    // --------------------------------------------------------------------------------------------

    /// Takes an object out and returns it, ready to be inserted back exactly where it was (see
    /// [`Edit::Remove`]). On error nothing changed.
    #[allow(
        clippy::too_many_lines,
        reason = "one short arm per kind of object: splitting the match would only scatter it"
    )]
    fn remove_object(
        &mut self,
        name: ObjectRef,
        changes: &mut ChangeSet,
    ) -> Result<Object, DocumentError> {
        let unknown = DocumentError::UnknownObject(name);
        let object = match name {
            ObjectRef::Event(id) => self.remove_event(id, changes)?,
            ObjectRef::Tuplet(id) => {
                let (t, v) = self
                    .tracks
                    .iter()
                    .enumerate()
                    .find_map(|(t, track)| {
                        track
                            .instrument
                            .voices
                            .iter()
                            .position(|v| v.content.tuplets.contains_key(&id))
                            .map(|v| (t, v))
                    })
                    .ok_or(unknown)?;
                let target = self.voice_at(t, v)?;
                // A tuplet still used by an event or by a nested tuplet stays: removing it would
                // leave references nothing can follow (see the module documentation).
                let in_use = target
                    .content
                    .chunks
                    .iter()
                    .any(|c| c.events().iter().any(|e| e.event().tuplet() == Some(id)))
                    || target
                        .content
                        .tuplets
                        .values()
                        .any(|t| t.parent() == Some(id));
                if in_use {
                    return Err(DocumentError::TupletInUse(id));
                }
                let voice = target.id;
                let tuplet = self
                    .voice_at_mut(t, v)?
                    .content
                    .tuplets
                    .remove(&id)
                    .ok_or(unknown)?;
                Object::Tuplet { voice, id, tuplet }
            }
            ObjectRef::Slur(id) => {
                let (t, i) = self
                    .find_in_instruments(|ins| ins.slurs.iter().position(|s| s.id() == id))
                    .ok_or(unknown)?;
                let instrument = self.track_at_mut(t)?;
                let slur = instrument.instrument.slurs.remove(i);
                let owner = instrument.instrument.id;
                for end in [slur.start(), slur.end()] {
                    let keep = self.slur_index.get_mut(&end).is_some_and(|r| r.remove(id));
                    if !keep {
                        self.slur_index.remove(&end);
                    }
                }
                Object::Slur {
                    instrument: owner,
                    slur,
                }
            }
            ObjectRef::Dynamic(id) => {
                let (t, i) = self
                    .find_in_instruments(|ins| ins.dynamics.iter().position(|d| d.id() == id))
                    .ok_or(unknown)?;
                let instrument = &mut self.track_at_mut(t)?.instrument;
                Object::Dynamic {
                    instrument: instrument.id,
                    dynamic: instrument.dynamics.remove(i),
                }
            }
            ObjectRef::Hairpin(id) => {
                let (t, i) = self
                    .find_in_instruments(|ins| ins.hairpins.iter().position(|h| h.id() == id))
                    .ok_or(unknown)?;
                let instrument = &mut self.track_at_mut(t)?.instrument;
                Object::Hairpin {
                    instrument: instrument.id,
                    hairpin: instrument.hairpins.remove(i),
                }
            }
            ObjectRef::TempoMark(id) => {
                Object::TempoMark(self.tempo_marks.remove(id).ok_or(unknown)?)
            }
            ObjectRef::ClefChange(id) => {
                let (t, s) = self
                    .tracks
                    .iter()
                    .enumerate()
                    .find_map(|(t, track)| {
                        track
                            .instrument
                            .staves
                            .iter()
                            .position(|s| s.clef_changes.as_slice().iter().any(|m| m.id() == id))
                            .map(|s| (t, s))
                    })
                    .ok_or(unknown)?;
                let staff = self
                    .track_at_mut(t)?
                    .instrument
                    .staves
                    .get_mut(s)
                    .ok_or(unknown)?;
                Object::ClefChange {
                    staff: staff.id,
                    mark: staff.clef_changes.remove(id).ok_or(unknown)?,
                }
            }
            ObjectRef::KeyChange(id) => {
                Object::KeyChange(self.key_changes.remove(id).ok_or(unknown)?)
            }
            ObjectRef::KeyOverride(id) => {
                let (t, _) = self
                    .find_in_instruments(|ins| {
                        ins.key_overrides
                            .as_slice()
                            .iter()
                            .position(|m| m.id() == id)
                    })
                    .ok_or(unknown)?;
                let instrument = &mut self.track_at_mut(t)?.instrument;
                Object::KeyOverride {
                    instrument: instrument.id,
                    mark: instrument.key_overrides.remove(id).ok_or(unknown)?,
                }
            }
            ObjectRef::RepeatStart(id) => {
                let i = self
                    .form
                    .repeat_starts
                    .iter()
                    .position(|r| r.id() == id)
                    .ok_or(unknown)?;
                Object::RepeatStart(self.form.repeat_starts.remove(i))
            }
            ObjectRef::RepeatEnd(id) => {
                let i = self
                    .form
                    .repeat_ends
                    .iter()
                    .position(|r| r.id() == id)
                    .ok_or(unknown)?;
                Object::RepeatEnd(self.form.repeat_ends.remove(i))
            }
            ObjectRef::Volta(id) => {
                let i = self
                    .form
                    .voltas
                    .iter()
                    .position(|v| v.id() == id)
                    .ok_or(unknown)?;
                Object::Volta(self.form.voltas.remove(i))
            }
            ObjectRef::Marker(id) => {
                let i = self
                    .form
                    .markers
                    .iter()
                    .position(|m| m.id() == id)
                    .ok_or(unknown)?;
                Object::Marker(self.form.markers.remove(i))
            }
            ObjectRef::Jump(id) => {
                let i = self
                    .form
                    .jumps
                    .iter()
                    .position(|j| j.id() == id)
                    .ok_or(unknown)?;
                Object::Jump(self.form.jumps.remove(i))
            }
            ObjectRef::BarRepeat(id) => {
                let (t, i) = self
                    .find_in_instruments(|ins| ins.bar_repeats.iter().position(|r| r.id() == id))
                    .ok_or(unknown)?;
                Object::BarRepeat(self.track_at_mut(t)?.instrument.bar_repeats.remove(i))
            }
            ObjectRef::PassRule(id) => {
                let (t, i) = self
                    .find_in_instruments(|ins| ins.pass_rules.iter().position(|r| r.id() == id))
                    .ok_or(unknown)?;
                let instrument = &mut self.track_at_mut(t)?.instrument;
                Object::PassRule {
                    instrument: instrument.id,
                    rule: instrument.pass_rules.remove(i),
                }
            }
            // Bars go with `RemoveBars`; the values the document has once cannot be removed, only
            // set (`SetInitial`).
            ObjectRef::Bar(_)
            | ObjectRef::InitialTempo
            | ObjectRef::InitialKey
            | ObjectRef::InitialClef(_)
            | ObjectRef::Numbering => return Err(DocumentError::NotRemovable(name)),
        };
        changes.remove(name);
        Ok(object)
    }

    /// Takes an event out of its chunk and the indices, with its place in the chunk. On error
    /// nothing changed. O(1) average + O(voices) + O(k).
    fn remove_event(
        &mut self,
        id: EventId,
        changes: &mut ChangeSet,
    ) -> Result<Object, DocumentError> {
        let location = *self
            .events
            .get(&id)
            .ok_or(DocumentError::UnknownObject(ObjectRef::Event(id)))?;
        let index = self.bar_index(location.bar)?;
        let (t, v) = self.position_of_voice(location.voice)?;
        let (place, placed) = self
            .chunk_at_mut(t, v, index, location.bar)?
            .remove(id)
            .ok_or(DocumentError::UnknownObject(ObjectRef::Event(id)))?;
        self.events.remove(&id);
        if let Some(chord) = placed.event().chord() {
            for note in chord.notes() {
                self.notes.remove(&note.id());
                changes.notes_gone.push(note.id());
            }
        }
        for (source, target) in outgoing_ties(&placed) {
            unindex_tie(&mut self.tie_index, source, target);
        }
        changes.cell(location.voice, location.bar);
        Ok(Object::Event {
            voice: location.voice,
            bar: location.bar,
            placed,
            // A chunk holds far fewer than 2³² events.
            place: u32::try_from(place).ok(),
        })
    }

    // --------------------------------------------------------------------------------------------
    // Replace
    // --------------------------------------------------------------------------------------------

    /// Gives an existing object a new content or place, keeping its ID, and returns the old one
    /// (see [`Edit::Replace`]). On error nothing changed.
    ///
    /// # How
    /// An event that stays in the same chunk with the same sort key is replaced **in place**, so
    /// that its neighbours with the same key keep their order. Anything else is a removal followed
    /// by an insertion; if the insertion fails, the old object is put back exactly where it was
    /// (it fits there by construction) and the error is returned.
    fn replace_object(
        &mut self,
        object: Object,
        changes: &mut ChangeSet,
    ) -> Result<Object, DocumentError> {
        let name = object.reference();
        let mut scratch = ChangeSet::default();
        if let Object::Event {
            voice,
            bar,
            placed,
            place: None,
        } = &object
            && let Some(old) = self.replace_event_in_place(*voice, *bar, placed, &mut scratch)?
        {
            changes.absorb_effects(scratch);
            changes.replace(name);
            return Ok(old);
        }
        if let Object::Tuplet { voice, id, tuplet } = &object
            && let Some(old) = self.replace_tuplet_in_place(*voice, *id, *tuplet, &mut scratch)?
        {
            changes.absorb_effects(scratch);
            changes.replace(name);
            return Ok(old);
        }
        let old = self.remove_object(name, &mut scratch)?;
        if let Err(e) = self.insert_object(object, &mut scratch) {
            // Put the old object back where it was: it was there a moment ago, so it fits.
            self.insert_object(old, &mut ChangeSet::default())
                .map_err(|_| e)?;
            return Err(e);
        }
        changes.absorb_effects(scratch);
        changes.replace(name);
        Ok(old)
    }

    /// Replaces a tuplet in place when it stays in the same voice with the same parent, and
    /// returns the old one. Returns `None` (changing nothing) otherwise, so that the caller
    /// removes and inserts it instead, which a tuplet still in use refuses
    /// ([`DocumentError::TupletInUse`]): a tuplet with events never changes voice or parent.
    ///
    /// # Why in place
    /// A tuplet's ratio or look changes while its events stay in it (X7): removing it first
    /// would be refused, since its events use it. The events themselves do not change, but their
    /// durations do (the ratio scales them), so every cell (voice, bar) holding an event inside
    /// the tuplet, directly or through nested tuplets, is recorded as changed: layout redraws
    /// them, and the local check looks for overlaps there.
    ///
    /// # Complexity
    /// O(voices) to find the voice, plus O(events of the voice × depth) to find the cells (at most
    /// [`MAX_TUPLET_DEPTH`] parent links per event).
    fn replace_tuplet_in_place(
        &mut self,
        voice: VoiceId,
        id: TupletId,
        tuplet: Tuplet,
        changes: &mut ChangeSet,
    ) -> Result<Option<Object>, DocumentError> {
        let Ok((t, v)) = self.position_of_voice(voice) else {
            return Ok(None);
        };
        let content = &self.voice_at(t, v)?.content;
        let Some(old) = content.tuplets.get(&id).copied() else {
            return Ok(None);
        };
        if old.parent() != tuplet.parent() {
            return Ok(None);
        }
        // Whether an event's innermost tuplet is `id` or nested in it.
        let inside = |innermost: TupletId| {
            let mut next = Some(innermost);
            while let Some(current) = next {
                if current == id {
                    return true;
                }
                next = content.tuplets.get(&current).and_then(|tu| tu.parent());
            }
            false
        };
        for (index, chunk) in content.chunks.iter().enumerate() {
            if chunk
                .events()
                .iter()
                .any(|e| e.event().tuplet().is_some_and(inside))
            {
                let bar = self
                    .bars
                    .ids()
                    .get(index)
                    .copied()
                    .ok_or(DocumentError::Inconsistent)?;
                changes.cell(voice, bar);
            }
        }
        self.voice_at_mut(t, v)?.content.tuplets.insert(id, tuplet);
        Ok(Some(Object::Tuplet {
            voice,
            id,
            tuplet: old,
        }))
    }

    /// Replaces an event in place when it stays in the same chunk with the same (offset, slot)
    /// key, and returns the old event with its place. Returns `None` (changing nothing) when the
    /// event moves, so that the caller removes and inserts it instead. On error nothing changed.
    /// O(voices + k + noteheads).
    fn replace_event_in_place(
        &mut self,
        voice: VoiceId,
        bar: BarId,
        placed: &PlacedEvent,
        changes: &mut ChangeSet,
    ) -> Result<Option<Object>, DocumentError> {
        let id = placed.event().id();
        let location = *self
            .events
            .get(&id)
            .ok_or(DocumentError::UnknownObject(ObjectRef::Event(id)))?;
        if location.voice != voice || location.bar != bar {
            return Ok(None);
        }
        let index = self.bar_index(bar)?;
        let (t, v) = self.position_of_voice(voice)?;
        let target = self.voice_at(t, v)?;
        let chunk_index = usize::try_from(index.get()).map_err(|_| DocumentError::TooManyBars)?;
        let chunk = target
            .content
            .chunks
            .get(chunk_index)
            .ok_or(DocumentError::UnknownBar(bar))?;
        let place = chunk
            .position_of(id)
            .ok_or(DocumentError::UnknownObject(ObjectRef::Event(id)))?;
        let old = chunk
            .events()
            .get(place)
            .ok_or(DocumentError::UnknownObject(ObjectRef::Event(id)))?;
        if old.order_key() != placed.order_key() {
            return Ok(None);
        }
        // The same checks as an insertion, minus the event's own ID (it is the same event).
        let staff = placed.event().staff();
        let instrument = &self.track_at(t)?.instrument;
        if instrument.staff(staff).is_none() {
            return Err(DocumentError::StaffOfAnotherInstrument(staff));
        }
        if let Some(tuplet) = placed.event().tuplet()
            && !target.content.tuplets.contains_key(&tuplet)
        {
            return Err(DocumentError::UnknownTuplet(tuplet));
        }
        let old_notes: Vec<_> = old
            .event()
            .chord()
            .map(|c| c.notes().map(Note::id).collect())
            .unwrap_or_default();
        let new_notes: Vec<_> = placed
            .event()
            .chord()
            .map(|c| c.notes().map(Note::id).collect())
            .unwrap_or_default();
        for (i, note) in new_notes.iter().enumerate() {
            let repeated = new_notes
                .get(..i)
                .is_some_and(|earlier| earlier.contains(note));
            let taken = self.notes.get(note).is_some_and(|owner| *owner != id);
            if repeated || taken || !self.ids.has_issued(note.get()) {
                return Err(DocumentError::DuplicateObject(ObjectRef::Event(id)));
            }
        }
        let slot = self
            .chunk_at_mut(t, v, index, bar)?
            .get_mut(id)
            .ok_or(DocumentError::UnknownObject(ObjectRef::Event(id)))?;
        let previous = std::mem::replace(slot, placed.clone());
        for note in old_notes {
            self.notes.remove(&note);
            if !new_notes.contains(&note) {
                changes.notes_gone.push(note);
            }
        }
        for note in new_notes {
            self.notes.insert(note, id);
        }
        for (source, target) in outgoing_ties(&previous) {
            unindex_tie(&mut self.tie_index, source, target);
        }
        for (source, target) in outgoing_ties(placed) {
            index_tie(&mut self.tie_index, source, target);
        }
        changes.cell(voice, bar);
        Ok(Some(Object::Event {
            voice,
            bar,
            placed: previous,
            place: u32::try_from(place).ok(),
        }))
    }

    // --------------------------------------------------------------------------------------------
    // Bars
    // --------------------------------------------------------------------------------------------

    /// Inserts empty bars (see [`Edit::InsertBars`]). On error nothing changed. O(B × V) pointer
    /// moves plus the meter map's rebuild.
    fn insert_bars(
        &mut self,
        at: BarIndex,
        ids: &[BarId],
        meter: &BarSpan,
        changes: &mut ChangeSet,
    ) -> Result<(), DocumentError> {
        let position = usize::try_from(at.get()).map_err(|_| DocumentError::TooManyBars)?;
        let count = u32::try_from(ids.len()).map_err(|_| DocumentError::TooManyBars)?;
        if meter.count() != count {
            return Err(DocumentError::BarSpanMismatch);
        }
        let total = self.bars.len().checked_add(ids.len());
        if position > self.bars.len() || total.is_none_or(|n| u32::try_from(n).is_err()) {
            return Err(DocumentError::TooManyBars);
        }
        // New, issued and distinct IDs.
        for (i, &id) in ids.iter().enumerate() {
            if ids.get(..i).is_some_and(|earlier| earlier.contains(&id)) {
                return Err(DocumentError::DuplicateObject(ObjectRef::Bar(id)));
            }
            self.check_new(ObjectRef::Bar(id))?;
        }
        // The meter map is all or nothing; once it has accepted, nothing below can fail.
        self.meter_map.insert_bars(at, meter)?;
        self.bars.insert(position, ids)?;
        for voice in self
            .tracks
            .iter_mut()
            .flat_map(|t| t.instrument.voices.iter_mut())
        {
            let chunks = &mut voice.content.chunks;
            let at = position.min(chunks.len());
            chunks.splice(at..at, std::iter::repeat_n(Chunk::EMPTY, ids.len()));
        }
        for &id in ids {
            changes.insert(ObjectRef::Bar(id));
        }
        changes.meter(at);
        Ok(())
    }

    /// Removes bars that carry nothing (see [`Edit::RemoveBars`]) and returns their IDs and meter
    /// map data. On error nothing changed. O(B × V) pointer moves, O(objects anchored on bars) for
    /// the check, plus the meter map's rebuild.
    fn remove_bars(
        &mut self,
        at: BarIndex,
        count: u32,
        changes: &mut ChangeSet,
    ) -> Result<(Vec<BarId>, BarSpan), DocumentError> {
        let start = usize::try_from(at.get()).map_err(|_| DocumentError::TooManyBars)?;
        let len = usize::try_from(count).map_err(|_| DocumentError::TooManyBars)?;
        let end = start
            .checked_add(len)
            .filter(|&end| end <= self.bars.len())
            .ok_or(DocumentError::TooManyBars)?;
        let removed: Vec<BarId> = self.bars.ids().get(start..end).unwrap_or(&[]).to_vec();
        let gone: IdSet<BarId> = removed.iter().copied().collect();
        self.check_bars_carry_nothing(start..end, &gone)?;
        // Removing the first bars brings the next bar to the very start, where the initial clef,
        // key and tempo are: a change at its start would collide with them.
        if start == 0
            && let Some(&next) = self.bars.ids().get(end)
        {
            self.check_nothing_at_start_of(next)?;
        }
        let meter = self.meter_map.remove_bars(at, count)?;
        self.bars.remove(start, len);
        for voice in self
            .tracks
            .iter_mut()
            .flat_map(|t| t.instrument.voices.iter_mut())
        {
            let chunks = &mut voice.content.chunks;
            let stop = end.min(chunks.len());
            chunks.drain(start.min(stop)..stop);
        }
        for &id in &removed {
            changes.remove(ObjectRef::Bar(id));
        }
        changes.meter(at);
        Ok((removed, meter))
    }

    /// Checks that the bars of `range` (whose IDs are the keys of `gone`) hold no event and that
    /// nothing is anchored on them. O(V × range + every mark, spanner and form marking).
    fn check_bars_carry_nothing(
        &self,
        range: std::ops::Range<usize>,
        gone: &IdSet<BarId>,
    ) -> Result<(), DocumentError> {
        let busy = |bar: BarId| -> Result<(), DocumentError> {
            if gone.contains(&bar) {
                Err(DocumentError::BarsNotEmpty(bar))
            } else {
                Ok(())
            }
        };
        for voice in self.instruments().flat_map(|i| i.voices.iter()) {
            for (offset, chunk) in voice
                .content
                .chunks
                .get(range.clone())
                .unwrap_or(&[])
                .iter()
                .enumerate()
            {
                if !chunk.is_empty() {
                    let bar = self
                        .bars
                        .ids()
                        .get(range.start.saturating_add(offset))
                        .copied();
                    return Err(
                        bar.map_or(DocumentError::Inconsistent, DocumentError::BarsNotEmpty)
                    );
                }
            }
        }
        for mark in self.tempo_marks.as_slice() {
            busy(mark.bar())?;
            if let Some(TempoMarkChange::Gradual(gradual)) = mark.value().change() {
                busy(gradual.end.bar)?;
            }
        }
        if let Some(TempoMarkChange::Gradual(gradual)) = self.initial_tempo.1.change() {
            busy(gradual.end.bar)?;
        }
        for mark in self.key_changes.as_slice() {
            busy(mark.bar())?;
        }
        for instrument in self.instruments() {
            for staff in &instrument.staves {
                for mark in staff.clef_changes.as_slice() {
                    busy(mark.bar())?;
                }
            }
            for mark in instrument.key_overrides.as_slice() {
                busy(mark.bar())?;
            }
            for dynamic in &instrument.dynamics {
                busy(dynamic.at().bar)?;
            }
            for hairpin in &instrument.hairpins {
                busy(hairpin.start().bar)?;
                busy(hairpin.end().bar)?;
            }
            for repeat in &instrument.bar_repeats {
                busy(repeat.first())?;
            }
            for rule in &instrument.pass_rules {
                busy(rule.start().bar)?;
                busy(rule.end().bar)?;
            }
        }
        let form = &self.form;
        for bar in form
            .repeat_starts
            .iter()
            .map(RepeatStart::bar)
            .chain(form.repeat_ends.iter().map(RepeatEnd::bar))
            .chain(form.voltas.iter().flat_map(|v| [v.first(), v.last()]))
            .chain(form.markers.iter().map(Marker::bar))
            .chain(form.jumps.iter().map(Jump::bar))
        {
            busy(bar)?;
        }
        Ok(())
    }

    /// Checks that no clef, key or tempo change sits at the very start of `bar`, which is about to
    /// become the first bar of the score. O(staves + log of each list).
    fn check_nothing_at_start_of(&self, bar: BarId) -> Result<(), DocumentError> {
        let at_start = |b: BarId, offset: Duration| b == bar && offset.is_zero();
        let clash = self
            .tempo_marks
            .as_slice()
            .iter()
            .any(|m| at_start(m.bar(), m.offset()))
            || self
                .key_changes
                .as_slice()
                .iter()
                .any(|m| at_start(m.bar(), m.offset()))
            || self.instruments().flat_map(|i| i.staves.iter()).any(|s| {
                s.clef_changes
                    .as_slice()
                    .iter()
                    .any(|m| at_start(m.bar(), m.offset()))
            });
        if clash {
            Err(DocumentError::DuplicateMark)
        } else {
            Ok(())
        }
    }

    // --------------------------------------------------------------------------------------------
    // Meter map and initial values
    // --------------------------------------------------------------------------------------------

    /// Applies a meter map edit and returns the edit that restores the previous value. On error
    /// nothing changed. O(m log m) for the rebuild (none for the numbering entries).
    #[allow(
        clippy::too_many_lines,
        reason = "one short arm per kind of meter edit: splitting the match would only scatter it"
    )]
    fn edit_meter(
        &mut self,
        edit: MeterEdit,
        changes: &mut ChangeSet,
    ) -> Result<MeterEdit, DocumentError> {
        let map = &mut self.meter_map;
        let (index, inverse) = match edit {
            MeterEdit::MeterChange { bar, change } => {
                let index = self
                    .bars
                    .index_of(bar)
                    .ok_or(DocumentError::UnknownBar(bar))?;
                let previous = match change {
                    Some(change) => map.set_meter_change(index, change)?,
                    None => map.remove_meter_change(index)?,
                };
                (
                    index,
                    MeterEdit::MeterChange {
                        bar,
                        change: previous,
                    },
                )
            }
            MeterEdit::IrregularBar { bar, irregular } => {
                let index = self
                    .bars
                    .index_of(bar)
                    .ok_or(DocumentError::UnknownBar(bar))?;
                let previous = match irregular {
                    Some(irregular) => map.set_irregular_bar(index, irregular)?,
                    None => map.remove_irregular_bar(index)?,
                };
                (
                    index,
                    MeterEdit::IrregularBar {
                        bar,
                        irregular: previous,
                    },
                )
            }
            MeterEdit::BarGrouping { bar, grouping } => {
                let index = self
                    .bars
                    .index_of(bar)
                    .ok_or(DocumentError::UnknownBar(bar))?;
                let previous = match grouping {
                    Some(grouping) => map.set_bar_grouping(index, grouping)?,
                    None => map.remove_bar_grouping(index)?,
                };
                (
                    index,
                    MeterEdit::BarGrouping {
                        bar,
                        grouping: previous,
                    },
                )
            }
            MeterEdit::Uncounted { bar, uncounted } => {
                let index = self
                    .bars
                    .index_of(bar)
                    .ok_or(DocumentError::UnknownBar(bar))?;
                let previous = map.set_uncounted(index, uncounted);
                (
                    index,
                    MeterEdit::Uncounted {
                        bar,
                        uncounted: previous,
                    },
                )
            }
            MeterEdit::Span { bar, span } => {
                let index = self
                    .bars
                    .index_of(bar)
                    .ok_or(DocumentError::UnknownBar(bar))?;
                // A run past the last bar would leave entries on bars the document does not have.
                let end = u64::from(index.get()).saturating_add(u64::from(span.count()));
                if end > u64::try_from(self.bars.len()).unwrap_or(u64::MAX) {
                    return Err(DocumentError::TooManyBars);
                }
                let previous = map.replace_span(index, &span)?;
                (
                    index,
                    MeterEdit::Span {
                        bar,
                        span: previous,
                    },
                )
            }
            MeterEdit::NumberingRestart { bar, number } => {
                let index = self
                    .bars
                    .index_of(bar)
                    .ok_or(DocumentError::UnknownBar(bar))?;
                let previous = map.set_numbering_restart(index, number);
                (
                    index,
                    MeterEdit::NumberingRestart {
                        bar,
                        number: previous,
                    },
                )
            }
        };
        changes.meter(index);
        Ok(inverse)
    }

    /// Sets a value the document has once and returns the previous one. On error nothing changed.
    /// O(1), or O(staves) for a staff's clef.
    fn set_initial(
        &mut self,
        initial: Initial,
        changes: &mut ChangeSet,
    ) -> Result<Initial, DocumentError> {
        let (name, previous) = match initial {
            Initial::Tempo(mark) => {
                if !matches!(mark.change(), Some(TempoMarkChange::Immediate(_))) {
                    return Err(DocumentError::InitialTempoNotImmediate);
                }
                let previous = std::mem::replace(&mut self.initial_tempo.1, mark);
                (ObjectRef::InitialTempo, Initial::Tempo(previous))
            }
            Initial::Key(key) => {
                let previous = std::mem::replace(&mut self.initial_key, key);
                (ObjectRef::InitialKey, Initial::Key(previous))
            }
            Initial::Clef { staff, clef } => {
                let (t, s) = self.position_of_staff(staff)?;
                let target = self
                    .track_at_mut(t)?
                    .instrument
                    .staves
                    .get_mut(s)
                    .ok_or(DocumentError::UnknownStaff(staff))?;
                let previous = std::mem::replace(&mut target.initial_clef, clef);
                (
                    ObjectRef::InitialClef(staff),
                    Initial::Clef {
                        staff,
                        clef: previous,
                    },
                )
            }
            Initial::Numbering(style) => {
                let previous = std::mem::replace(&mut self.form.numbering, style);
                (ObjectRef::Numbering, Initial::Numbering(previous))
            }
        };
        changes.replace(name);
        Ok(previous)
    }

    // --------------------------------------------------------------------------------------------
    // Positional access
    //
    // The positions always come from a lookup made a moment before on the same document, so these
    // cannot fail; [`DocumentError::Inconsistent`] reports a bug instead of panicking.
    // --------------------------------------------------------------------------------------------

    /// The track at a position. O(1).
    fn track_at(&self, t: usize) -> Result<&crate::document::Track, DocumentError> {
        self.tracks.get(t).ok_or(DocumentError::Inconsistent)
    }

    /// The track at a position, mutably. O(1).
    fn track_at_mut(&mut self, t: usize) -> Result<&mut crate::document::Track, DocumentError> {
        self.tracks.get_mut(t).ok_or(DocumentError::Inconsistent)
    }

    /// The voice at these positions. O(1).
    fn voice_at(&self, t: usize, v: usize) -> Result<&crate::document::Voice, DocumentError> {
        self.track_at(t)?
            .instrument
            .voices
            .get(v)
            .ok_or(DocumentError::Inconsistent)
    }

    /// The voice at these positions, mutably. O(1).
    fn voice_at_mut(
        &mut self,
        t: usize,
        v: usize,
    ) -> Result<&mut crate::document::Voice, DocumentError> {
        self.track_at_mut(t)?
            .instrument
            .voices
            .get_mut(v)
            .ok_or(DocumentError::Inconsistent)
    }

    /// The chunk of the voice at these positions for the bar at `index` (named `bar` in the error).
    /// O(1).
    fn chunk_at_mut(
        &mut self,
        t: usize,
        v: usize,
        index: BarIndex,
        bar: BarId,
    ) -> Result<&mut Chunk, DocumentError> {
        let chunk = usize::try_from(index.get()).map_err(|_| DocumentError::TooManyBars)?;
        self.voice_at_mut(t, v)?
            .content
            .chunks
            .get_mut(chunk)
            .ok_or(DocumentError::UnknownBar(bar))
    }
}
