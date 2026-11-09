// SPDX-License-Identifier: GPL-3.0-or-later

//! The document: tracks, instruments, staves, voices with their events, bars, clefs, key
//! signatures, slurs, dynamics, hairpins, tempo marks and the two time maps (ADR-0022 §1–4,
//! ADR-0024 §1–§11; SCALETTA F1.08, F1.09b and F1.09c).
//!
//! # What lives here
//! - [`Document`]: the one project document of ADR-0003. It holds the structure (F1.08), the
//!   events of every voice, the clefs and the key signatures (F1.09b), the markings on the events
//!   and the slurs, dynamics and hairpins of each instrument, the tempo marks with the tempo map
//!   derived from them (F1.09c); the form arrives in F1.10.
//! - [`Track`] → [`Instrument`] → [`Staff`] and [`Voice`]: one instrument per instrument track,
//!   one or more staves per instrument, voices that **belong to the instrument** and have a home
//!   staff (ADR-0022 §2), so that a melody crossing from one staff to the other stays one voice.
//!   A voice holds one chunk of events per bar and its tuplet table (`crate::content`).
//! - [`BarList`]: the ordered list of [`BarId`]s (ADR-0022 §3). It says *which* bar is which and
//!   how many bars there are, and nothing else: bar lengths live only in the [`MeterMap`].
//! - [`DocumentBuilder`]: how a **new** document is created (by the file loader, the generator of
//!   the reference score, tests and benchmarks). Owner's decision of 2026-11-02: an existing
//!   document changes only through the commands of F1.11 (ADR-0003 §4); creating one is not
//!   changing one, so the builder does not need commands. [`DocumentBuilder::build`] runs
//!   [`Document::validate`], so an invalid document cannot exist (owner's decision of 2026-11-02).
//!
//! # Limits
//! At most [`MAX_STAVES_PER_INSTRUMENT`] staves and [`MAX_VOICES_PER_INSTRUMENT`] voices per
//! instrument (owner's decision of 2026-11-02). They are not musical limits (the interface may
//! present four voices per staff, or any other number) but technical ones: no real score gets
//! close, and they stop a corrupt or malicious file from asking for millions of voices and
//! exhausting the memory when it is opened. Raising them later changes a constant and a test, not
//! the file format.
//!
//! # Lookups and their cost
//! Bars are many (thousands), so [`BarList`] keeps an `IdMap` from `BarId` to index: O(1) on
//! average in both directions. Tracks, staves and voices are few (tens to a few hundred in a large
//! orchestral score), so they are found by a linear scan: a few hundred comparisons of `u64`, which
//! is cheaper than keeping a hash index in step with every structural change. Events and noteheads
//! are hundreds of thousands, so the document keeps ADR-0022 §1's derived index from their IDs to
//! where they are: one hash lookup, then the voice scan, then a scan of one chunk. Slurs are found
//! by their ID with a binary search in their instrument's table (sorted by ID), and from an event
//! through a second derived index, "which slurs start or end here" (ADR-0024 §7).
//!
//! # Score order
//! Two events are compared in **score order**: by absolute written position, then by slot (at one
//! offset the `After` grace chords come first, then the `Before` grace chords, then the main
//! event, ADR-0024 §5), then, inside one chunk, by their place in it. Ties and slurs must go
//! forwards in this order (owner's decision of 2026-11-02): so an acciaccatura can be slurred to
//! its main note, which has the same position, and for events without graces the rule is simply
//! "a later position".

use std::hash::Hash;

use orchestra_core_time::{BarIndex, Duration, MeterMap, Position, TempoMap};

use crate::event::{Chord, Event, EventBody, Note, PlacedEvent, RestValue, Tie};
use crate::form::Form;
use crate::id::{
    BarId, EventId, IdCounter, IdMap, IdRefs, InstrumentId, KeyChangeId, NoteId, SlurId, StaffId,
    TempoMarkId, TrackId, VoiceId,
};
use crate::marks::{Clef, KeyOverride, KeySignature, MarkList, ScorePoint};
use crate::pitch::Key;
use crate::slur::{Slur, SlurRefs};
use crate::tempo_mark::TempoMark;
use crate::tolerance::ToleranceSettings;

/// The most staves one instrument can have (owner's decision, 2026-11-02). Organ music uses
/// three, some piano and harp writing four; 16 leaves ample room for unusual layouts.
pub const MAX_STAVES_PER_INSTRUMENT: usize = 16;

/// The most tracks one document can have (ADR-0029 S9, owner's approval of 2026-11-06): a
/// technical limit against corrupt or hostile files, not a musical one (the heavy test score has
/// 28 tracks, a large orchestral score about 40). The builder enforces it, so that no document
/// can be created that a file could not hold.
pub const MAX_TRACKS: usize = 1024;

/// The most voices one instrument can have (owner's decision, 2026-11-02). Notation programs
/// commonly offer four voices per staff; 256 per instrument is far beyond any real score.
pub const MAX_VOICES_PER_INSTRUMENT: usize = 256;

mod builder;
mod error;
mod structure;

pub use builder::{ChordIds, DocumentBuilder, Placement};
pub use error::DocumentError;
pub use structure::{BarList, Instrument, Staff, Track, Voice};

// ------------------------------------------------------------------------------------------------
// The document
// ------------------------------------------------------------------------------------------------

/// Where an event is: its voice and its bar. The entry of the derived index of ADR-0022 §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EventLocation {
    /// The voice holding the event.
    pub(crate) voice: VoiceId,
    /// The bar whose chunk holds the event.
    pub(crate) bar: BarId,
}

/// An event found by its ID, with where it is.
#[derive(Debug, Clone, Copy)]
pub struct EventRef<'a> {
    /// The voice holding the event.
    voice: VoiceId,
    /// The bar whose chunk holds the event.
    bar: BarId,
    /// The event with its offset.
    placed: &'a PlacedEvent,
}

impl<'a> EventRef<'a> {
    /// The voice holding the event. O(1).
    #[must_use]
    pub const fn voice(&self) -> VoiceId {
        self.voice
    }

    /// The bar holding the event. O(1).
    #[must_use]
    pub const fn bar(&self) -> BarId {
        self.bar
    }

    /// The distance from the start of the bar. O(1).
    #[must_use]
    pub const fn offset(&self) -> Duration {
        self.placed.offset()
    }

    /// The event. O(1).
    #[must_use]
    pub const fn event(&self) -> &'a Event {
        self.placed.event()
    }
}

/// The one project document (ADR-0003).
///
/// Create one with a [`DocumentBuilder`]. Every reader (the views, playback, the file writer)
/// reads it through the methods below; the commands of F1.11 will be the only way to change it.
///
/// # Example
/// ```
/// use orchestra_core_model::{
///     BaseValue, Clef, DocumentBuilder, Interval, Key, Metronome, Mode, NoteValue, Pitch,
///     Placement, Step, TempoMark, Alteration,
/// };
/// use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
///
/// let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4)?))?;
/// let quarter = Duration::from_rational(Rational::new(1, 4)?);
/// // "Andante ♩ = 96": the initial tempo mark, from which the tempo map is derived.
/// let andante = TempoMark::immediate(
///     Some("Andante"),
///     Metronome::shown(Tempo::new(quarter, Rational::from_integer(96)?)?),
/// )?;
///
/// let mut builder = DocumentBuilder::new(meters, andante, Key::traditional(0, Mode::Major)?)?;
/// let bar = builder.add_bar()?;
/// // A piano: two staves with their clefs, so two voices, one per staff.
/// let piano = builder.add_instrument_track(
///     "Piano",
///     Interval::new(0, Rational::ZERO),
///     &[Clef::TREBLE, Clef::BASS],
/// )?;
/// let right_hand = builder.document().track(piano).map(|t| t.instrument().voices()[0].id());
/// let c4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
/// let chord = builder.add_chord(
///     Placement::new(right_hand.ok_or("no voice")?, bar, Duration::ZERO),
///     NoteValue::plain(BaseValue::Whole),
///     &[c4],
/// )?;
/// let document = builder.build()?;
///
/// assert_eq!(document.metric_duration(chord.event)?, Duration::from_rational(Rational::ONE));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Where every ID of this document comes from (saved in the file, F1.15).
    pub(crate) ids: IdCounter,
    /// The bar lengths, meters, beat groupings and numbering (ADR-0018, ADR-0019): the only place
    /// a bar's length is stored.
    pub(crate) meter_map: MeterMap,
    /// The tempo changes on written positions (ADR-0020, ADR-0021): a **derived cache** of the
    /// tempo marks (ADR-0024 §11), rebuilt by `Document::derive_tempo_map` after every edit that
    /// touches them, never edited directly. Not saved (the file stores the marks).
    pub(crate) tempo_map: TempoMap,
    /// The tempo mark at the very start of the score, with its ID (ADR-0024 §11, E1): it always
    /// sets a tempo at once and cannot be removed.
    pub(crate) initial_tempo: (TempoMarkId, TempoMark),
    /// The tempo marks after the start, at (bar, offset) points.
    pub(crate) tempo_marks: MarkList<TempoMarkId, TempoMark>,
    /// The form markings and the bar-numbering style (ADR-0025).
    pub(crate) form: Form,
    /// The key at the start of the score, at concert pitch (ADR-0024 §9, K1).
    pub(crate) initial_key: Key,
    /// The key changes after the start, at concert pitch.
    pub(crate) key_changes: MarkList<KeyChangeId, Key>,
    /// Which bar is which (ADR-0022 §3).
    pub(crate) bars: BarList,
    /// The tracks, top to bottom as the score lists them.
    pub(crate) tracks: Vec<Track>,
    /// Derived index (ADR-0022 §1): where each event is. Rebuilt at load, never saved.
    pub(crate) events: IdMap<EventId, EventLocation>,
    /// Derived index: which event each notehead belongs to.
    pub(crate) notes: IdMap<NoteId, EventId>,
    /// Derived index (ADR-0024 §7): the slurs that start or end on each event. Events touched by
    /// no slur have no entry. Rebuilt at load, never saved.
    pub(crate) slur_index: IdMap<EventId, SlurRefs>,
    /// Derived index (ADR-0026 §2): the noteheads tied **into** each notehead, the backward
    /// direction of [`Tie::To`]. A tie is stored once, on the notehead it starts from (ADR-0024
    /// §4); the commands need the other direction too, to clear the ties that lead into a deleted
    /// note and to check that a moved or changed note can still be reached by its ties, without
    /// scanning the instrument. Keyed by the **target**, which may be missing while a transaction
    /// is open (a tie whose target was just removed); noteheads tied into by nothing have no
    /// entry. Rebuilt at load, never saved.
    pub(crate) tie_index: IdMap<NoteId, IdRefs<NoteId>>,
    /// Derived index: where each voice is, as (its track's position in `tracks`, its position in
    /// its instrument's voices). Voices and tracks are only ever appended, by the builder, so the
    /// positions never move. Rebuilt at load, never saved.
    ///
    /// # Why an index?
    /// Voices are few, but every event placed, looked up or edited needs its voice: scanning every
    /// instrument instead costs ≈ 70 M instructions for loading the heavy reference score
    /// (measured with the benchmark gate, 2026-11-08).
    pub(crate) voice_index: IdMap<VoiceId, (usize, usize)>,
    /// The tolerance thresholds of the DAW move (ADR-0028 §1): the project value and the
    /// per-track overrides. Saved in the file (F1.15).
    pub(crate) tolerance: ToleranceSettings,
    /// How many transactions, undos and redos have changed the document since it was built
    /// (ADR-0026 §5): a background reader holding a snapshot compares it to know whether it is
    /// stale, and the interface to know whether there are unsaved changes. Not saved.
    pub(crate) revision: u64,
}

impl Document {
    /// How many transactions, undos and redos have changed the document since it was built: 0
    /// for a new document, one more after each change through the editor (ADR-0026 §5). O(1).
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether two documents hold the same music: equal in everything **except the ID counter
    /// and the revision**, which only grow (ADR-0022 §1, ADR-0026 §2). Doing a transaction and
    /// undoing it gives a document with the same content as before, and that is what this
    /// compares; the derived indices are compared too, since they must be identical as well.
    ///
    /// O(size of the document); allocates nothing.
    pub fn same_content(&self, other: &Self) -> bool {
        // Destructuring both documents makes this method fail to compile when a field is added,
        // so it can never silently forget one.
        let Self {
            ids: _,
            revision: _,
            meter_map,
            tempo_map,
            initial_tempo,
            tempo_marks,
            form,
            initial_key,
            key_changes,
            bars,
            tracks,
            events,
            notes,
            slur_index,
            tie_index,
            voice_index,
            tolerance,
        } = self;
        let Self {
            ids: _,
            revision: _,
            meter_map: other_meter_map,
            tempo_map: other_tempo_map,
            initial_tempo: other_initial_tempo,
            tempo_marks: other_tempo_marks,
            form: other_form,
            initial_key: other_initial_key,
            key_changes: other_key_changes,
            bars: other_bars,
            tracks: other_tracks,
            events: other_events,
            notes: other_notes,
            slur_index: other_slur_index,
            tie_index: other_tie_index,
            voice_index: other_voice_index,
            tolerance: other_tolerance,
        } = other;
        meter_map == other_meter_map
            && tempo_map == other_tempo_map
            && initial_tempo == other_initial_tempo
            && tempo_marks == other_tempo_marks
            && form == other_form
            && initial_key == other_initial_key
            && key_changes == other_key_changes
            && bars == other_bars
            && tracks == other_tracks
            && events == other_events
            && notes == other_notes
            && slur_index == other_slur_index
            && tie_index == other_tie_index
            && voice_index == other_voice_index
            && tolerance == other_tolerance
    }

    /// Whether every derived index agrees with the content it is derived from (ADR-0022 §1,
    /// ADR-0024 §7): the inverse of the bar list, the event → location and note → event indices,
    /// the slur index, and one chunk per bar in every voice. The edits keep them up to date one
    /// entry at a time; this rebuilds them from scratch and compares, so that the property tests
    /// can check that no edit ever lets them drift (ADR-0026 §8).
    ///
    /// O(events + noteheads + slurs + bars); allocates the rebuilt indices.
    pub fn indices_are_consistent(&self) -> bool {
        let bars = self.bars.len();
        let bar_index_ok = self.bars.index.len() == bars
            && self
                .bars
                .order
                .iter()
                .enumerate()
                .all(|(i, id)| self.bars.index_of(*id).map(|b| b.get() as usize) == Some(i));
        let mut events: IdMap<EventId, EventLocation> = IdMap::default();
        let mut notes: IdMap<NoteId, EventId> = IdMap::default();
        let mut slurs: IdMap<EventId, SlurRefs> = IdMap::default();
        let mut ties: IdMap<NoteId, IdRefs<NoteId>> = IdMap::default();
        let mut voices: IdMap<VoiceId, (usize, usize)> = IdMap::default();
        for (t, track) in self.tracks.iter().enumerate() {
            for (v, voice) in track.instrument.voices.iter().enumerate() {
                voices.insert(voice.id, (t, v));
            }
        }
        let mut chunks_ok = true;
        for instrument in self.instruments() {
            for voice in &instrument.voices {
                chunks_ok &= voice.content.chunks.len() == bars;
                for (bar, chunk) in self.bars.order.iter().zip(&voice.content.chunks) {
                    for placed in chunk.events() {
                        let id = placed.event().id();
                        events.insert(
                            id,
                            EventLocation {
                                voice: voice.id,
                                bar: *bar,
                            },
                        );
                        for note in placed.event().chord().into_iter().flat_map(Chord::notes) {
                            notes.insert(note.id(), id);
                            if let Tie::To(target) = note.tie() {
                                index_tie(&mut ties, note.id(), target);
                            }
                        }
                    }
                }
            }
            for slur in &instrument.slurs {
                for end in [slur.start(), slur.end()] {
                    index_slur(&mut slurs, end, slur.id());
                }
            }
        }
        bar_index_ok
            && chunks_ok
            && events == self.events
            && notes == self.notes
            && slurs == self.slur_index
            && ties == self.tie_index
            && voices == self.voice_index
    }

    /// The meter map: bar lengths, meters, beat groupings, numbering. O(1).
    pub const fn meter_map(&self) -> &MeterMap {
        &self.meter_map
    }

    /// The tempo map, on written positions, derived from the tempo marks. O(1).
    pub const fn tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    /// The tempo mark at the very start of the score, with its ID. O(1).
    pub const fn initial_tempo(&self) -> (TempoMarkId, &TempoMark) {
        (self.initial_tempo.0, &self.initial_tempo.1)
    }

    /// The tempo marks after the start, in score order. O(1).
    pub const fn tempo_marks(&self) -> &MarkList<TempoMarkId, TempoMark> {
        &self.tempo_marks
    }

    /// The form: repeats, voltas and the bar-numbering style (ADR-0025). O(1).
    pub const fn form(&self) -> &Form {
        &self.form
    }

    /// The key at the start of the score, at concert pitch. O(1).
    pub const fn initial_key(&self) -> Key {
        self.initial_key
    }

    /// The key changes after the start, at concert pitch, in score order. O(1).
    pub const fn key_changes(&self) -> &MarkList<KeyChangeId, Key> {
        &self.key_changes
    }

    /// The bar list. O(1).
    pub const fn bars(&self) -> &BarList {
        &self.bars
    }

    /// The tolerance thresholds of the DAW move (ADR-0028 §1). O(1).
    pub const fn tolerance(&self) -> &ToleranceSettings {
        &self.tolerance
    }

    /// The track that plays an instrument. O(tracks).
    pub fn track_of(&self, instrument: InstrumentId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.instrument.id == instrument)
    }

    /// The tracks, in score order. O(1).
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// The track with this ID. O(tracks).
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    /// The instrument with this ID. O(tracks).
    pub fn instrument(&self, id: InstrumentId) -> Option<&Instrument> {
        self.instruments().find(|i| i.id == id)
    }

    /// The staff with this ID and the instrument it belongs to. O(staves of the whole document):
    /// a few hundred comparisons in a large orchestral score.
    pub fn staff(&self, id: StaffId) -> Option<(&Instrument, &Staff)> {
        self.instruments()
            .find_map(|i| i.staff(id).map(|staff| (i, staff)))
    }

    /// The voice with this ID and the instrument it belongs to. O(1) average: one lookup in the
    /// voice index.
    pub fn voice(&self, id: VoiceId) -> Option<(&Instrument, &Voice)> {
        // One hash lookup in the voice index, then two direct positions.
        let &(t, v) = self.voice_index.get(&id)?;
        let instrument = &self.tracks.get(t)?.instrument;
        Some((instrument, instrument.voices.get(v)?))
    }

    /// Where this bar starts, as an absolute written position: the bar's index in the bar list,
    /// then the meter map. O(1) average + O(log m) for m meter changes (ADR-0018).
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`] if the bar does not belong to the document,
    /// [`DocumentError::MeterMap`] if the meter map arithmetic overflows (absurd meters only).
    pub fn bar_start(&self, id: BarId) -> Result<Position, DocumentError> {
        Ok(self.meter_map.bar_start(self.bar_index(id)?)?)
    }

    /// The events of a voice in a bar, sorted by offset. O(voices) to find the voice, then O(1);
    /// never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownVoice`] or [`DocumentError::UnknownBar`].
    pub fn events(&self, voice: VoiceId, bar: BarId) -> Result<&[PlacedEvent], DocumentError> {
        let index = self.bar_index(bar)?;
        let (_, voice) = self
            .voice(voice)
            .ok_or(DocumentError::UnknownVoice(voice))?;
        Ok(voice.events_at(index))
    }

    /// The event with this ID and where it is. O(1) average for the index, O(voices) for the
    /// voice, O(k) for the scan of its chunk (k = events of that voice in that bar). Never
    /// allocates.
    pub fn event(&self, id: EventId) -> Option<EventRef<'_>> {
        let location = *self.events.get(&id)?;
        let index = self.bars.index_of(location.bar)?;
        let (_, voice) = self.voice(location.voice)?;
        let placed = voice
            .events_at(index)
            .iter()
            .find(|e| e.event().id() == id)?;
        Some(EventRef {
            voice: location.voice,
            bar: location.bar,
            placed,
        })
    }

    /// The notehead with this ID and the event it belongs to. As [`event`](Self::event), plus a
    /// scan of the chord's noteheads. Never allocates.
    pub fn note(&self, id: NoteId) -> Option<(EventRef<'_>, &Note)> {
        let event = self.event(*self.notes.get(&id)?)?;
        let note = event.event().chord()?.notes().find(|n| n.id() == id)?;
        Some((event, note))
    }

    /// The metric duration of an event (ADR-0024 §2). As [`event`](Self::event), plus
    /// [`Voice::metric_duration`]; the meter map is asked for the bar's length only for a
    /// whole-bar rest. Never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`]; [`DocumentError::MeterMap`] or
    /// [`DocumentError::Arithmetic`] on overflow (absurd meters only).
    pub fn metric_duration(&self, id: EventId) -> Result<Duration, DocumentError> {
        let found = self.event(id).ok_or(DocumentError::UnknownEvent(id))?;
        let (_, voice) = self
            .voice(found.voice)
            .ok_or(DocumentError::UnknownVoice(found.voice))?;
        let bar_length = if is_whole_bar_rest(found.event()) {
            self.meter_map.bar_length(self.bar_index(found.bar)?)?
        } else {
            // Not read for any other event: any value would do.
            Duration::ZERO
        };
        voice.metric_duration(found.event(), bar_length)
    }

    /// The absolute written position of an event: its bar's start plus its offset (ADR-0022 §4).
    /// O(event lookup) + O(log m). Never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`]; [`DocumentError::MeterMap`] or
    /// [`DocumentError::Arithmetic`] on overflow (absurd meters only).
    pub fn event_position(&self, id: EventId) -> Result<Position, DocumentError> {
        let found = self.event(id).ok_or(DocumentError::UnknownEvent(id))?;
        self.point_position(ScorePoint::new(found.bar, found.offset()))
    }

    /// The absolute written position of a point of the score: its bar's start plus its offset.
    /// The offset is not clamped to the bar (an overfull bar's point may lie past its end, as
    /// the events there do). O(1) average for the bar + O(log m) in the meter map. Never
    /// allocates.
    ///
    /// The one conversion from points to positions, for the model and for `playback`.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`]; [`DocumentError::MeterMap`] or
    /// [`DocumentError::Arithmetic`] on overflow (absurd meters only).
    pub fn point_position(&self, point: ScorePoint) -> Result<Position, DocumentError> {
        Ok(self.bar_start(point.bar)?.checked_add(point.offset)?)
    }

    /// The clef in force on a staff at (bar, offset): the last clef change at or before the point,
    /// or the staff's initial clef (ADR-0024 §9). O(staves) + O(log c); never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownStaff`] or [`DocumentError::UnknownBar`].
    pub fn clef_at(
        &self,
        staff: StaffId,
        bar: BarId,
        offset: Duration,
    ) -> Result<Clef, DocumentError> {
        let index = self.bar_index(bar)?;
        let (_, staff) = self
            .staff(staff)
            .ok_or(DocumentError::UnknownStaff(staff))?;
        Ok(staff
            .clef_changes
            .in_force_at(&self.bars, index, offset)
            .map_or(staff.initial_clef, |mark| *mark.value()))
    }

    /// The document's key in force at (bar, offset), at concert pitch, ignoring the instruments'
    /// overrides. O(log k); never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`].
    pub fn key_at(&self, bar: BarId, offset: Duration) -> Result<Key, DocumentError> {
        let index = self.bar_index(bar)?;
        Ok(self
            .key_changes
            .in_force_at(&self.bars, index, offset)
            .map_or(self.initial_key, |mark| *mark.value()))
    }

    /// What an instrument shows at (bar, offset) **at concert pitch**: its override in force, if
    /// it is a signature, otherwise the document's key (ADR-0024 §9, K1). O(tracks) + O(log k);
    /// never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownInstrument`] or [`DocumentError::UnknownBar`].
    pub fn instrument_key_at(
        &self,
        instrument: InstrumentId,
        bar: BarId,
        offset: Duration,
    ) -> Result<KeySignature, DocumentError> {
        let index = self.bar_index(bar)?;
        let found = self
            .instrument(instrument)
            .ok_or(DocumentError::UnknownInstrument(instrument))?;
        match found
            .key_overrides
            .in_force_at(&self.bars, index, offset)
            .map(|mark| *mark.value())
        {
            Some(KeyOverride::Signature(signature)) => Ok(signature),
            Some(KeyOverride::FollowScore) | None => {
                Ok(KeySignature::Key(self.key_at(bar, offset)?))
            }
        }
    }

    /// What an instrument's **written** part shows at (bar, offset): the concert signature moved
    /// by the inverse of the instrument's transposition (ADR-0023 §7); "no signature" stays "no
    /// signature". Derived, never stored (ADR-0004). As [`instrument_key_at`](Self::instrument_key_at)
    /// plus O(1).
    ///
    /// # Errors
    /// As [`instrument_key_at`](Self::instrument_key_at); [`DocumentError::Pitch`] if the written
    /// key goes beyond fourteen sharps or flats, or the transposition is microtonal.
    pub fn written_key_at(
        &self,
        instrument: InstrumentId,
        bar: BarId,
        offset: Duration,
    ) -> Result<KeySignature, DocumentError> {
        let concert = self.instrument_key_at(instrument, bar, offset)?;
        let transposition = self
            .instrument(instrument)
            .ok_or(DocumentError::UnknownInstrument(instrument))?
            .transposition;
        match concert {
            // Written = concert moved by the inverse of the written-to-sounding interval.
            KeySignature::Key(key) => {
                Ok(KeySignature::Key(key.transposed(transposition.reversed())?))
            }
            KeySignature::NoSignature => Ok(KeySignature::NoSignature),
        }
    }

    /// The slur with this ID and the instrument it belongs to. O(tracks × log s). Never
    /// allocates.
    pub fn slur(&self, id: SlurId) -> Option<(&Instrument, &Slur)> {
        self.instruments()
            .find_map(|i| i.slur(id).map(|slur| (i, slur)))
    }

    /// The slurs that start or end on this event, in ID order (creation order); empty if none
    /// does or the event does not exist (ADR-0024 §7). One hash lookup, O(1) on average; never
    /// allocates.
    ///
    /// The events a slur only passes over are not listed: layout finds them by walking from the
    /// slur's start to its end.
    pub fn slurs_at(&self, event: EventId) -> &[SlurId] {
        self.slur_index.get(&event).map_or(&[], SlurRefs::as_slice)
    }

    /// The noteheads tied **into** this notehead (whose [`Tie::To`] names it), in ID order; empty
    /// if none is or the notehead does not exist (ADR-0024 §4, ADR-0026 §2). One hash lookup,
    /// O(1) on average; never allocates.
    pub fn ties_into(&self, note: NoteId) -> &[NoteId] {
        self.tie_index.get(&note).map_or(&[], IdRefs::as_slice)
    }

    /// Whether event `a` comes strictly before event `b` in **score order** (see the module
    /// documentation): an earlier position; or the same position and an earlier slot; or the same
    /// position, slot, voice and bar and an earlier place in the chunk. Two events of different
    /// voices with the same position and slot are simultaneous: neither comes before the other.
    ///
    /// # Complexity
    /// Two event lookups and two bar starts, O(1) average + O(voices + k + log m); a scan of one
    /// chunk only in the rare tie-break. Never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`] for either event; the meter map and arithmetic errors of
    /// [`event_position`](Self::event_position).
    pub fn comes_before(&self, a: EventId, b: EventId) -> Result<bool, DocumentError> {
        let first = self.event(a).ok_or(DocumentError::UnknownEvent(a))?;
        let second = self.event(b).ok_or(DocumentError::UnknownEvent(b))?;
        let key_a = (
            self.bar_start(first.bar)?.checked_add(first.offset())?,
            first.event().slot(),
        );
        let key_b = (
            self.bar_start(second.bar)?.checked_add(second.offset())?,
            second.event().slot(),
        );
        if key_a != key_b {
            return Ok(key_a < key_b);
        }
        // Same position and slot: only events of one chunk have a defined order (the order they
        // were entered in, which the chunk keeps). `a == b` gives "not before", as it should.
        if first.voice != second.voice || first.bar != second.bar || a == b {
            return Ok(false);
        }
        let index = self.bar_index(first.bar)?;
        let (_, voice) = self
            .voice(first.voice)
            .ok_or(DocumentError::UnknownVoice(first.voice))?;
        // The first of the two found in the chunk is the earlier one.
        let earlier = voice
            .events_at(index)
            .iter()
            .map(|e| e.event().id())
            .find(|&id| id == a || id == b);
        Ok(earlier == Some(a))
    }

    /// The index of a bar, or the typed error. O(1) average.
    pub(crate) fn bar_index(&self, id: BarId) -> Result<BarIndex, DocumentError> {
        self.bars.index_of(id).ok_or(DocumentError::UnknownBar(id))
    }

    /// Every instrument, in track order. O(1) to create, O(tracks) to exhaust.
    pub(crate) fn instruments(&self) -> impl Iterator<Item = &Instrument> {
        self.tracks.iter().map(|t| &t.instrument)
    }
}

/// Whether an event is a whole-bar rest, the only event whose metric duration needs its bar's
/// length. O(1).
pub(crate) const fn is_whole_bar_rest(event: &Event) -> bool {
    matches!(
        event.body(),
        EventBody::Rest(r) if matches!(r.value(), RestValue::WholeBar)
    )
}

/// Adds `value` to the small set stored under `key` in a derived index, creating the entry if
/// needed. O(1) average (the sets hold one or two IDs inline).
///
/// The one implementation behind the tie index and the slur index, so that both derived indices
/// are updated by the same code.
fn add_ref<K: Hash + Eq, T: Copy + Ord>(index: &mut IdMap<K, IdRefs<T>>, key: K, value: T) {
    index
        .entry(key)
        .and_modify(|refs| refs.insert(value))
        .or_insert_with(|| IdRefs::one(value));
}

/// Removes `value` from the set stored under `key`, dropping the entry when it becomes empty, so
/// that a derived index never holds an empty entry (`indices_are_consistent` compares indices
/// with `==`). O(1) average.
fn drop_ref<K: Hash + Eq, T: Copy + Ord>(index: &mut IdMap<K, IdRefs<T>>, key: &K, value: T) {
    let keep = index.get_mut(key).is_some_and(|refs| refs.remove(value));
    if !keep {
        index.remove(key);
    }
}

/// Records in the backward tie index that `source` is tied into `target`. O(1) average.
pub(crate) fn index_tie(index: &mut IdMap<NoteId, IdRefs<NoteId>>, source: NoteId, target: NoteId) {
    add_ref(index, target, source);
}

/// Forgets in the backward tie index that `source` is tied into `target`. O(1) average.
pub(crate) fn unindex_tie(
    index: &mut IdMap<NoteId, IdRefs<NoteId>>,
    source: NoteId,
    target: NoteId,
) {
    drop_ref(index, &target, source);
}

/// Records in the slur index that `slur` starts or ends at `event`. O(1) average.
pub(crate) fn index_slur(index: &mut IdMap<EventId, SlurRefs>, event: EventId, slur: SlurId) {
    add_ref(index, event, slur);
}

/// Forgets in the slur index that `slur` starts or ends at `event`. O(1) average.
pub(crate) fn unindex_slur(index: &mut IdMap<EventId, SlurRefs>, event: EventId, slur: SlurId) {
    drop_ref(index, &event, slur);
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use orchestra_core_time::{Meter, MeterChange, Rational, Tempo};

    use super::*;
    use crate::event::NoteValue;
    use crate::pitch::{Interval, Mode, Pitch};
    use crate::tempo_mark::Metronome;

    /// An empty builder in 4/4 at ♩ = 120, in C major.
    fn builder() -> DocumentBuilder {
        let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
        let quarter = Duration::from_rational(Rational::new(1, 4).unwrap());
        let tempo = Tempo::new(quarter, Rational::from_integer(120).unwrap()).unwrap();
        DocumentBuilder::new(
            meters,
            TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
            Key::traditional(0, Mode::Major).unwrap(),
        )
        .unwrap()
    }

    /// With the counter near its end, an instrument that does not fit is refused without consuming
    /// any ID or adding anything, and a smaller one that fits takes exactly the last IDs.
    #[test]
    fn exhaustion_leaves_the_builder_unchanged() {
        let mut b = builder();
        // Room for exactly 6 IDs: an instrument with 2 staves (2 + 4 = 6) fits, one with 3 does not.
        b.doc.ids = IdCounter::starting_at(NonZeroU64::new(u64::MAX - 5).unwrap());
        let unison = Interval::new(0, Rational::ZERO);
        let before = b.doc.clone();
        assert_eq!(
            b.add_instrument_track("Organ", unison, &[Clef::TREBLE, Clef::BASS, Clef::BASS]),
            Err(DocumentError::IdsExhausted)
        );
        assert_eq!(b.doc, before);
        let piano = b
            .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
            .unwrap();
        assert_eq!(piano.get(), u64::MAX - 5);
        let staff = b.doc.tracks[0].instrument.staves[0].id;
        let before = b.doc.clone();
        assert_eq!(b.add_voice(staff), Err(DocumentError::IdsExhausted));
        assert_eq!(b.add_bar(), Err(DocumentError::IdsExhausted));
        assert_eq!(b.doc, before);
    }

    /// A chord that needs more IDs than are left is refused before anything is created.
    #[test]
    fn chord_exhaustion_leaves_the_builder_unchanged() {
        let mut b = builder();
        let bar = b.add_bar().unwrap();
        let unison = Interval::new(0, Rational::ZERO);
        b.add_instrument_track("Flute", unison, &[Clef::TREBLE])
            .unwrap();
        let voice = b.doc.tracks[0].instrument.voices[0].id;
        // Room for exactly two IDs: a chord of two noteheads needs three.
        b.doc.ids = IdCounter::starting_at(NonZeroU64::new(u64::MAX - 1).unwrap());
        let c4 = Pitch::new(crate::Step::C, crate::Alteration::NATURAL, 4);
        let e4 = Pitch::new(crate::Step::E, crate::Alteration::NATURAL, 4);
        let before = b.doc.clone();
        let quarter = NoteValue::plain(crate::BaseValue::Quarter);
        assert_eq!(
            b.add_chord(
                Placement::new(voice, bar, Duration::ZERO),
                quarter,
                &[c4, e4]
            ),
            Err(DocumentError::IdsExhausted)
        );
        assert_eq!(b.doc, before);
        assert!(
            b.add_chord(Placement::new(voice, bar, Duration::ZERO), quarter, &[c4])
                .is_ok()
        );
    }
}
