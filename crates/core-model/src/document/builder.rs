// SPDX-License-Identifier: GPL-3.0-or-later

//! [`DocumentBuilder`]: how a **new** document is created (see the `document` module
//! documentation). A sub-module of `document` only to keep that file readable; everything public
//! here is re-exported by it.

use std::num::NonZeroU64;

use orchestra_core_time::{Duration, MeterMap, TempoMap};

use crate::bar_repeat::{BarRepeat, BarRepeatSpan, PassRule};
use crate::change::ChangeSet;
use crate::command::chord_from;
use crate::content::{Chunk, VoiceContent};
use crate::dynamic::{Dynamic, DynamicMark, DynamicPlacement, DynamicScope, Hairpin, HairpinShape};
use crate::edit::Object;
use crate::event::{
    Chord, Event, EventBody, Note, NoteValue, PlacedEvent, Rest, RestValue, Tie, Tuplet,
};
use crate::form::{BarNumbering, Form, PassSet, RepeatEnd, RepeatStart, Volta};
use crate::id::{
    BarId, BarRepeatId, ClefId, DynamicId, EventId, HairpinId, IdCounter, IdMap, InstrumentId,
    JumpId, KeyChangeId, MarkerId, NoteId, PassRuleId, RepeatId, SlurId, StaffId, TempoMarkId,
    TrackId, TupletId, VoiceId, VoltaId,
};
use crate::jump::{Jump, JumpSpec, Marker, MarkerKind};
use crate::mark_command::check_text;
use crate::marking::{Arpeggio, ArticulationSet, Fermata, Grace, Ornament, PerformanceOffsets};
use crate::marks::{Clef, KeyOverride, Mark, MarkList, ScorePoint};
use crate::pass_offsets::PassOffsets;
use crate::pitch::{Interval, Key, Pitch};
use crate::slur::{Slur, SlurPlacement};
use crate::tempo_mark::{TempoMark, TempoMarkChange};
use crate::tolerance::{Tolerance, ToleranceSettings};

use super::{BarList, DocumentError, Instrument, Staff, Track, Voice, index_tie, unindex_tie};
use super::{
    Document, EventLocation, MAX_STAVES_PER_INSTRUMENT, MAX_TRACKS, MAX_VOICES_PER_INSTRUMENT,
};

/// Where a new event goes: its voice, its bar, its offset in the bar, and optionally another
/// staff than the voice's home staff and the innermost tuplet containing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The voice.
    pub voice: VoiceId,
    /// The bar.
    pub bar: BarId,
    /// The offset from the start of the bar, ≥ 0 (it may reach past the bar's end: an overfull
    /// bar, valid by ADR-0022 §4).
    pub offset: Duration,
    /// The staff to draw it on, if not the voice's home staff (a cross-staff note).
    pub staff: Option<StaffId>,
    /// The innermost tuplet containing it, a tuplet of the same voice.
    pub tuplet: Option<TupletId>,
}

impl Placement {
    /// An event of `voice` in `bar` at `offset`, on the home staff, in no tuplet. O(1).
    #[must_use]
    pub const fn new(voice: VoiceId, bar: BarId, offset: Duration) -> Self {
        Self {
            voice,
            bar,
            offset,
            staff: None,
            tuplet: None,
        }
    }

    /// The same placement on another staff of the instrument. O(1).
    #[must_use]
    pub const fn on_staff(mut self, staff: StaffId) -> Self {
        self.staff = Some(staff);
        self
    }

    /// The same placement inside a tuplet of the voice. O(1).
    #[must_use]
    pub const fn in_tuplet(mut self, tuplet: TupletId) -> Self {
        self.tuplet = Some(tuplet);
        self
    }
}

/// The IDs of a new chord: its event and its noteheads, **in the order the pitches were given**
/// (the chord itself keeps them sorted by pitch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordIds {
    /// The chord's event.
    pub event: EventId,
    /// One ID per given pitch, in the given order.
    pub notes: Vec<NoteId>,
}

/// Creates a new [`Document`] (owner's decision of 2026-11-02).
///
/// The builder is for documents that do not exist yet: a file being opened (F1.15), the generated
/// reference score (F1.16), tests and benchmarks. Once [`build`](Self::build) returns, the document
/// changes only through commands (F1.11, ADR-0003 §4).
///
/// # Order of the IDs
/// Every method takes its IDs from the document's single counter in the order it creates the
/// objects: a bar takes one; an instrument track takes the track's, then the instrument's, then
/// one per staff top to bottom, then one per voice; a chord takes its event's, then one per
/// notehead in the given order; a slur takes one. Two builders given the same calls therefore produce identical
/// documents, IDs included, which keeps generated scores and golden files reproducible.
///
/// # What is checked when
/// Each method checks what it can see locally (the bar, voice, staff and tuplet exist and belong
/// together, the offset is not negative, the counter has room) **before** creating anything, so an
/// error never leaves a half-made object behind and never consumes an ID. The rules that need the
/// whole document (no overlap within a voice, ties, whole-bar rests alone) are checked by
/// [`build`](Self::build) through [`Document::validate`].
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    /// The document under construction (crate-visible for the loader of `crate::parts`, which
    /// restores the saved ID counter in it).
    pub(crate) doc: Document,
    /// Where the edit layer records the effects of each object the builder inserts. Nobody reads
    /// them while a document is being built: the set is cleared after every insertion, keeping
    /// its capacity, so building allocates nothing for it after the first objects.
    scratch: ChangeSet,
}

impl DocumentBuilder {
    /// Starts a document with this meter map, this initial tempo mark and this initial key (at
    /// concert pitch), no bars and no tracks. The tempo map is derived from the tempo mark
    /// (ADR-0024 §11); the mark takes the document's first ID. O(1) plus the small allocation of
    /// the tempo map.
    ///
    /// There is deliberately no default meter, tempo or key: they are the caller's choice (the
    /// interface's "new score" dialog, a file, the generator), not the model's.
    ///
    /// # Errors
    /// [`DocumentError::InitialTempoNotImmediate`] unless the mark sets a tempo at once (E1);
    /// [`DocumentError::TempoMap`] for an absurd tempo.
    pub fn new(
        meter_map: MeterMap,
        initial_tempo: TempoMark,
        initial_key: Key,
    ) -> Result<Self, DocumentError> {
        let Some(TempoMarkChange::Immediate(metronome)) = initial_tempo.change() else {
            return Err(DocumentError::InitialTempoNotImmediate);
        };
        let tempo_map = TempoMap::new(metronome.tempo).map_err(DocumentError::TempoMap)?;
        let mut ids = IdCounter::new();
        // A fresh counter always has room for its first ID.
        let id = TempoMarkId::from_counter(issue(&mut ids)?);
        Ok(Self {
            doc: Document {
                ids,
                meter_map,
                tempo_map,
                initial_tempo: (id, initial_tempo),
                tempo_marks: MarkList::default(),
                form: Form::default(),
                initial_key,
                key_changes: MarkList::default(),
                bars: BarList::new(),
                tracks: Vec::new(),
                events: IdMap::default(),
                notes: IdMap::default(),
                slur_index: IdMap::default(),
                tie_index: IdMap::default(),
                voice_index: IdMap::default(),
                tolerance: ToleranceSettings::new()?,
                revision: 0,
            },
            scratch: ChangeSet::default(),
        })
    }

    /// Appends a bar at the end of the bar list and returns its ID. Its length, meter and number
    /// come from the meter map, as for every bar. Every voice gains an empty chunk for it.
    /// O(voices) amortised; an empty chunk does not allocate.
    ///
    /// # Errors
    /// [`DocumentError::TooManyBars`] past 2³² − 1 bars, [`DocumentError::IdsExhausted`] when the
    /// counter is exhausted. The builder is unchanged on error.
    pub fn add_bar(&mut self) -> Result<BarId, DocumentError> {
        // Check the bar limit first, so that a failure does not consume an ID.
        if u32::try_from(self.doc.bars.len()).map_or(true, |n| n == u32::MAX) {
            return Err(DocumentError::TooManyBars);
        }
        let id = BarId::from_counter(issue(&mut self.doc.ids)?);
        self.doc.bars.push(id)?;
        // Keep every voice's chunks aligned with the bar list.
        for track in &mut self.doc.tracks {
            for voice in &mut track.instrument.voices {
                voice.content.chunks.push(Chunk::EMPTY);
            }
        }
        Ok(id)
    }

    /// Appends an instrument track with one staff per given clef (top to bottom, each starting
    /// with its clef) and **one voice per staff**, each voice's home staff being its staff
    /// (owner's decisions of 2026-11-02): enough to write one part per staff at once;
    /// [`add_voice`](Self::add_voice) adds more. O(staves + staves × bars) for the voices' empty
    /// chunks.
    ///
    /// `transposition` is the interval from written to sounding pitch (ADR-0023 §7): a unison
    /// (`Interval::new(0, Rational::ZERO)`) for an instrument at concert pitch.
    ///
    /// # Errors
    /// [`DocumentError::NoStaves`] for no clef, [`DocumentError::TooManyStaves`] beyond
    /// [`MAX_STAVES_PER_INSTRUMENT`], [`DocumentError::IdsExhausted`] when the counter cannot hand
    /// out the 2 + 2 × staves IDs needed. The builder is unchanged on error.
    pub fn add_instrument_track(
        &mut self,
        name: impl Into<String>,
        transposition: Interval,
        clefs: &[Clef],
    ) -> Result<TrackId, DocumentError> {
        self.insert_instrument_track(name.into(), transposition, clefs, true)
    }

    /// The body of [`add_instrument_track`](Self::add_instrument_track). With `with_voices` false
    /// the instrument gets **no** voice and takes 2 + staves IDs: loading a saved document
    /// ([`Document::from_parts`]) adds its voices one by one with [`add_voice`](Self::add_voice),
    /// in their saved order and with their saved home staves, which need not be one per staff.
    ///
    /// # Errors
    /// As [`add_instrument_track`](Self::add_instrument_track), plus
    /// [`DocumentError::TooManyTracks`] beyond [`MAX_TRACKS`].
    pub(crate) fn insert_instrument_track(
        &mut self,
        name: String,
        transposition: Interval,
        clefs: &[Clef],
        with_voices: bool,
    ) -> Result<TrackId, DocumentError> {
        // A technical limit against corrupt files (ADR-0029 S9), checked here so that no path,
        // builder or file, creates a document that could not be opened again.
        if self.doc.tracks.len() >= MAX_TRACKS {
            return Err(DocumentError::TooManyTracks);
        }
        let staves = clefs.len();
        if staves == 0 {
            return Err(DocumentError::NoStaves);
        }
        if staves > MAX_STAVES_PER_INSTRUMENT {
            return Err(DocumentError::TooManyStaves);
        }
        // One track ID, one instrument ID, one per staff, one per voice (if any). `staves` ≤ 16,
        // so the conversion and the arithmetic cannot fail; `checked_*` keeps that visible anyway.
        let per_staff = if with_voices { 2 } else { 1 };
        let needed = u64::try_from(staves)
            .ok()
            .and_then(|s| s.checked_mul(per_staff))
            .and_then(|s| s.checked_add(2))
            .ok_or(DocumentError::TooManyStaves)?;
        if !self.doc.ids.has_room_for(needed) {
            return Err(DocumentError::IdsExhausted);
        }

        // From here on every `next()` succeeds (checked just above); the `?` stays because the
        // compiler cannot know that, and an unwrap is not allowed in library code.
        let bars = self.doc.bars.len();
        let ids = &mut self.doc.ids;
        let track_id = TrackId::from_counter(issue(ids)?);
        let instrument_id = InstrumentId::from_counter(issue(ids)?);
        let mut staff_list = Vec::with_capacity(staves);
        for &clef in clefs {
            let id = StaffId::from_counter(issue(ids)?);
            staff_list.push(Staff {
                id,
                initial_clef: clef,
                clef_changes: MarkList::default(),
            });
        }
        let mut voices = Vec::with_capacity(staves);
        for staff in staff_list.iter().filter(|_| with_voices) {
            let id = VoiceId::from_counter(issue(ids)?);
            voices.push(Voice {
                id,
                home_staff: staff.id,
                content: VoiceContent::with_bars(bars),
            });
        }

        // The new track goes last: its voices are indexed at that position.
        let t = self.doc.tracks.len();
        for (v, voice) in voices.iter().enumerate() {
            self.doc.voice_index.insert(voice.id, (t, v));
        }
        self.doc.tracks.push(Track {
            id: track_id,
            instrument: Instrument {
                id: instrument_id,
                name,
                transposition,
                staves: staff_list,
                voices,
                key_overrides: MarkList::default(),
                slurs: Vec::new(),
                dynamics: Vec::new(),
                hairpins: Vec::new(),
                bar_repeats: Vec::new(),
                pass_rules: Vec::new(),
                pass_offsets: PassOffsets::default(),
            },
        });
        Ok(track_id)
    }

    /// Adds a voice to the instrument that owns `home_staff`, with that staff as its home, and
    /// returns its ID. O(staves of the document) to find the instrument, O(bars) for the empty
    /// chunks.
    ///
    /// Only the staff is asked for: staff IDs are unique in the whole document, so the staff names
    /// its instrument too, and a voice can never be given the staff of another instrument.
    ///
    /// # Errors
    /// [`DocumentError::UnknownStaff`] if no staff has this ID, [`DocumentError::TooManyVoices`]
    /// if the instrument already has [`MAX_VOICES_PER_INSTRUMENT`] voices,
    /// [`DocumentError::IdsExhausted`] when the counter is exhausted. The builder is unchanged on
    /// error.
    pub fn add_voice(&mut self, home_staff: StaffId) -> Result<VoiceId, DocumentError> {
        // Find the owning track by position rather than by reference: the ID comes from the
        // counter, another field of the same document, and holding a `&mut` into the tracks
        // while taking it would borrow the document twice.
        let track_index = self
            .doc
            .tracks
            .iter()
            .position(|t| t.instrument.staff(home_staff).is_some())
            .ok_or(DocumentError::UnknownStaff(home_staff))?;
        let bars = self.doc.bars.len();
        let track = self
            .doc
            .tracks
            .get_mut(track_index)
            .ok_or(DocumentError::UnknownStaff(home_staff))?;
        // Every check happens before the counter is touched, so an error consumes no ID.
        if track.instrument.voices.len() >= MAX_VOICES_PER_INSTRUMENT {
            return Err(DocumentError::TooManyVoices);
        }
        let id = VoiceId::from_counter(issue(&mut self.doc.ids)?);
        let v = track.instrument.voices.len();
        track.instrument.voices.push(Voice {
            id,
            home_staff,
            content: VoiceContent::with_bars(bars),
        });
        self.doc.voice_index.insert(id, (track_index, v));
        Ok(id)
    }

    /// Inserts one object with a fresh ID: takes the ID, makes the object with `make`, and puts it
    /// in the document with the rules of the edit layer (`Document::place_object`), the same
    /// rules every command obeys. If the object is refused the ID is given back, so an error
    /// leaves the builder exactly as it was and never consumes an ID. Returns the raw ID.
    ///
    /// # Why through the edit layer?
    /// The rules for inserting each kind of object (where a mark may go, which duplicates are
    /// refused, the order of each list) exist once, in the edit layer, and the builder and the
    /// commands both go through it, so the documents a builder makes and the ones commands make
    /// cannot drift apart.
    ///
    /// # Errors
    /// [`DocumentError::IdsExhausted`]; whatever `make` or the edit layer refuses.
    ///
    /// # Complexity
    /// O(1) for the ID, plus the cost of the insertion (see `Document::place_object`).
    fn insert_new(
        &mut self,
        make: impl FnOnce(NonZeroU64) -> Result<Object, DocumentError>,
    ) -> Result<NonZeroU64, DocumentError> {
        let raw = issue(&mut self.doc.ids)?;
        let result = make(raw).and_then(|object| self.doc.place_object(object, &mut self.scratch));
        self.scratch.clear();
        match result {
            Ok(()) => Ok(raw),
            Err(e) => {
                self.doc.ids.give_back(&[raw]);
                Err(e)
            }
        }
    }

    /// Adds a tuplet to a voice, nested in `parent` (a tuplet of the same voice) or at top level,
    /// and returns its ID (ADR-0024 §3). Its members are the events later placed in it. O(voices)
    /// to find the voice, O(depth) to check the nesting.
    ///
    /// # Errors
    /// [`DocumentError::UnknownVoice`]; [`DocumentError::UnknownTuplet`] if the parent is not a
    /// tuplet of this voice; [`DocumentError::TupletTooDeep`] beyond [`MAX_TUPLET_DEPTH`];
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_tuplet(
        &mut self,
        voice: VoiceId,
        tuplet: Tuplet,
        parent: Option<TupletId>,
    ) -> Result<TupletId, DocumentError> {
        // The nesting is checked by the edit layer (`Document::place_object`), as for a command.
        let mut tuplet = tuplet;
        tuplet.set_parent(parent);
        let raw = self.insert_new(|raw| {
            Ok(Object::Tuplet {
                voice,
                id: TupletId::from_counter(raw),
                tuplet,
            })
        })?;
        Ok(TupletId::from_counter(raw))
    }

    /// Adds a chord (a single note is a chord of one pitch) and returns its IDs (ADR-0024 §1).
    ///
    /// The noteheads are sorted by sounding pitch inside the chord; the returned note IDs follow
    /// the order of `pitches`. O(voices) to find the voice, O(n log n) for n noteheads, O(k) to
    /// insert into the chunk; one allocation for a chord of two or more noteheads.
    ///
    /// # Errors
    /// [`DocumentError::EmptyChord`]; the placement errors of [`Placement`] (unknown voice, bar or
    /// tuplet, a staff of another instrument, a negative offset); [`DocumentError::Pitch`] if a
    /// sounding pitch overflows; [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_chord(
        &mut self,
        at: Placement,
        value: NoteValue,
        pitches: &[Pitch],
    ) -> Result<ChordIds, DocumentError> {
        self.add_chord_with(at, None, value, pitches)
    }

    /// Adds a **grace chord** and returns its IDs (ADR-0024 §5). It keeps its written `value`
    /// (the figure drawn) but has zero metric duration.
    ///
    /// A `Before` grace goes at the offset of the event it ornaments, an `After` grace at the
    /// offset where the event it follows ends; [`build`](Self::build) checks that such an event
    /// is there (ADR-0024 §5). Costs as [`add_chord`](Self::add_chord).
    ///
    /// # Errors
    /// [`DocumentError::GraceInTuplet`] if the placement names a tuplet; otherwise as
    /// [`add_chord`](Self::add_chord). The builder is unchanged on error.
    pub fn add_grace_chord(
        &mut self,
        at: Placement,
        grace: Grace,
        value: NoteValue,
        pitches: &[Pitch],
    ) -> Result<ChordIds, DocumentError> {
        if at.tuplet.is_some() {
            return Err(DocumentError::GraceInTuplet);
        }
        self.add_chord_with(at, Some(grace), value, pitches)
    }

    /// The body of [`add_chord`](Self::add_chord) and [`add_grace_chord`](Self::add_grace_chord).
    fn add_chord_with(
        &mut self,
        at: Placement,
        grace: Option<Grace>,
        value: NoteValue,
        pitches: &[Pitch],
    ) -> Result<ChordIds, DocumentError> {
        if pitches.is_empty() {
            return Err(DocumentError::EmptyChord);
        }
        // The event needs its staff before it is made: the placement's, or the voice's home staff.
        // The bar and the offset are checked first, as the edit layer does.
        self.doc.check_point(ScorePoint::new(at.bar, at.offset))?;
        let staff = match at.staff {
            Some(staff) => staff,
            None => {
                self.doc
                    .voice(at.voice)
                    .ok_or(DocumentError::UnknownVoice(at.voice))?
                    .1
                    .home_staff
            }
        };
        // One ID for the event, one per notehead: all taken up front (and given back together if
        // the chord is refused), so a failure never leaves the counter advanced.
        let needed = u64::try_from(pitches.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(DocumentError::IdsExhausted)?;
        if !self.doc.ids.has_room_for(needed) {
            return Err(DocumentError::IdsExhausted);
        }
        let mut taken: Vec<NonZeroU64> = Vec::with_capacity(pitches.len().saturating_add(1));
        for _ in 0..=pitches.len() {
            // Cannot fail after `has_room_for`; if it ever did, nothing stays taken.
            let Some(raw) = self.doc.ids.next() else {
                self.doc.ids.give_back(&taken);
                return Err(DocumentError::IdsExhausted);
            };
            taken.push(raw);
        }
        let Some((&event_raw, note_raws)) = taken.split_first() else {
            return Err(DocumentError::inconsistent());
        };
        let event = EventId::from_counter(event_raw);
        let notes: Vec<NoteId> = note_raws
            .iter()
            .copied()
            .map(NoteId::from_counter)
            .collect();
        // The noteheads in the order of `pitches`; `chord_from` sorts them by sounding pitch (the
        // same rule, and the same implementation, as a command).
        let made = notes
            .iter()
            .zip(pitches)
            .map(|(&id, &pitch)| Note::new(id, pitch))
            .collect();
        let result = chord_from(value, at.tuplet, grace, made, None).and_then(|chord| {
            self.doc.place_object(
                Object::Event {
                    voice: at.voice,
                    bar: at.bar,
                    placed: PlacedEvent::new(
                        at.offset,
                        Event::new(event, staff, EventBody::Chord(chord)),
                    ),
                    place: None,
                },
                &mut self.scratch,
            )
        });
        self.scratch.clear();
        if let Err(e) = result {
            self.doc.ids.give_back(&taken);
            return Err(e);
        }
        Ok(ChordIds { event, notes })
    }

    /// Adds a rest and returns its event's ID (ADR-0024 §1–§2). Costs as
    /// [`add_chord`](Self::add_chord) without the noteheads.
    ///
    /// # Errors
    /// [`DocumentError::WholeBarRestInTuplet`]; the placement errors of [`Placement`];
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_rest(&mut self, at: Placement, value: RestValue) -> Result<EventId, DocumentError> {
        if matches!(value, RestValue::WholeBar) && at.tuplet.is_some() {
            return Err(DocumentError::WholeBarRestInTuplet);
        }
        // As for a chord: the bar and offset first, then the staff the rest is drawn on.
        self.doc.check_point(ScorePoint::new(at.bar, at.offset))?;
        let staff = match at.staff {
            Some(staff) => staff,
            None => {
                self.doc
                    .voice(at.voice)
                    .ok_or(DocumentError::UnknownVoice(at.voice))?
                    .1
                    .home_staff
            }
        };
        let raw = self.insert_new(|raw| {
            let rest = Rest::new(value, at.tuplet);
            Ok(Object::Event {
                voice: at.voice,
                bar: at.bar,
                placed: PlacedEvent::new(
                    at.offset,
                    Event::new(EventId::from_counter(raw), staff, EventBody::Rest(rest)),
                ),
                place: None,
            })
        })?;
        Ok(EventId::from_counter(raw))
    }

    /// Sets the tie that starts from a notehead (ADR-0024 §4). Whether the target is legal (same
    /// instrument, same sound, later) is checked by [`build`](Self::build), which sees the whole
    /// document. O(1) average + O(voices) + O(k).
    ///
    /// # Errors
    /// [`DocumentError::UnknownNote`] for either notehead, [`DocumentError::TieToItself`]. The
    /// builder is unchanged on error.
    pub fn set_tie(&mut self, note: NoteId, tie: Tie) -> Result<(), DocumentError> {
        if let Tie::To(target) = tie {
            if target == note {
                return Err(DocumentError::TieToItself);
            }
            if !self.doc.notes.contains_key(&target) {
                return Err(DocumentError::UnknownNote(target));
            }
        }
        let previous = self.note_mut(note)?.tie();
        self.note_mut(note)?.set_tie(tie);
        // Keep the backward index in step: forget the old target, remember the new one.
        if let Tie::To(old) = previous {
            unindex_tie(&mut self.doc.tie_index, note, old);
        }
        if let Tie::To(target) = tie {
            index_tie(&mut self.doc.tie_index, note, target);
        }
        Ok(())
    }

    /// Sets the performance offsets of a notehead (ADR-0024 §10), replacing the previous ones;
    /// [`PerformanceOffsets::NONE`] clears them. O(1) average + O(voices) + O(k).
    ///
    /// # Errors
    /// [`DocumentError::UnknownNote`]. The builder is unchanged on error.
    pub fn set_offsets(
        &mut self,
        note: NoteId,
        offsets: PerformanceOffsets,
    ) -> Result<(), DocumentError> {
        self.note_mut(note)?.set_offsets(offsets);
        Ok(())
    }

    /// Sets the project's tolerance threshold (`track: None`) or a track's override (ADR-0028
    /// §1); `tolerance: None` removes a track's override. O(tracks + overrides).
    ///
    /// # Errors
    /// [`DocumentError::UnknownTrack`], [`DocumentError::ToleranceNotPositive`],
    /// [`DocumentError::ProjectToleranceRequired`]. The builder is unchanged on error.
    pub fn set_tolerance(
        &mut self,
        track: Option<TrackId>,
        tolerance: Option<Tolerance>,
    ) -> Result<(), DocumentError> {
        if let Some(id) = track
            && self.doc.track(id).is_none()
        {
            return Err(DocumentError::UnknownTrack(id));
        }
        self.doc.tolerance.set(track, tolerance)?;
        Ok(())
    }

    /// Sets the offsets of a notehead in one pass (ADR-0028 §5); [`PerformanceOffsets::NONE`]
    /// removes the entry. O(1) average + O(voices) + O(log n + n) in the instrument's table.
    ///
    /// # Errors
    /// [`DocumentError::UnknownNote`]. The builder is unchanged on error.
    pub fn set_pass_offsets(
        &mut self,
        note: NoteId,
        pass: u32,
        offsets: PerformanceOffsets,
    ) -> Result<(), DocumentError> {
        let voice = self
            .doc
            .note(note)
            .map(|(event, _)| event.voice())
            .ok_or(DocumentError::UnknownNote(note))?;
        let instrument = self
            .doc
            .tracks
            .iter_mut()
            .map(|t| &mut t.instrument)
            .find(|i| i.voices.iter().any(|v| v.id == voice))
            .ok_or(DocumentError::UnknownVoice(voice))?;
        instrument.pass_offsets.set(note, pass, offsets);
        Ok(())
    }

    /// Sets the articulations of a chord (ADR-0024 §6), replacing the previous ones. O(1)
    /// average + O(voices) + O(k).
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`], [`DocumentError::NotAChord`] for a rest. The builder is
    /// unchanged on error.
    pub fn set_articulations(
        &mut self,
        event: EventId,
        articulations: ArticulationSet,
    ) -> Result<(), DocumentError> {
        self.chord_mut(event)?.set_articulations(articulations);
        Ok(())
    }

    /// Sets (or with `None` removes) the ornament of a chord (ADR-0024 §6). Costs as
    /// [`set_articulations`](Self::set_articulations).
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`], [`DocumentError::NotAChord`] for a rest. The builder is
    /// unchanged on error.
    pub fn set_ornament(
        &mut self,
        event: EventId,
        ornament: Option<Ornament>,
    ) -> Result<(), DocumentError> {
        self.chord_mut(event)?.set_ornament(ornament);
        Ok(())
    }

    /// Sets (or with `None` removes) the arpeggio sign of a chord (ADR-0024 §6). Costs as
    /// [`set_articulations`](Self::set_articulations).
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`], [`DocumentError::NotAChord`] for a rest. The builder is
    /// unchanged on error.
    pub fn set_arpeggio(
        &mut self,
        event: EventId,
        arpeggio: Option<Arpeggio>,
    ) -> Result<(), DocumentError> {
        self.chord_mut(event)?.set_arpeggio(arpeggio);
        Ok(())
    }

    /// Sets (or with `None` removes) the fermata of a chord or a rest (ADR-0024 §6). Costs as
    /// [`set_articulations`](Self::set_articulations).
    ///
    /// # Errors
    /// [`DocumentError::UnknownEvent`]. The builder is unchanged on error.
    pub fn set_fermata(
        &mut self,
        event: EventId,
        fermata: Option<Fermata>,
    ) -> Result<(), DocumentError> {
        self.event_mut(event)?.set_fermata(fermata);
        Ok(())
    }

    /// Adds a slur from `start` to `end` and returns its ID (ADR-0024 §7). The slur goes into the
    /// table of the instrument that owns `start`, and both ends into the "slurs at an event"
    /// index.
    ///
    /// That `end` belongs to the same instrument and comes after `start` in score order is
    /// checked by [`build`](Self::build), which sees the whole document (like ties).
    ///
    /// # Complexity
    /// Two event lookups, O(voices) to find the instrument, O(1) amortised to append (IDs only
    /// grow, so the table stays sorted), O(1) average for each index entry.
    ///
    /// # Errors
    /// [`DocumentError::SlurToItself`], [`DocumentError::UnknownEvent`] for either end,
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_slur(
        &mut self,
        start: EventId,
        end: EventId,
        placement: SlurPlacement,
    ) -> Result<SlurId, DocumentError> {
        if start == end {
            return Err(DocumentError::SlurToItself);
        }
        if !self.doc.events.contains_key(&end) {
            return Err(DocumentError::UnknownEvent(end));
        }
        // The slur belongs to the instrument of its first event.
        let location = *self
            .doc
            .events
            .get(&start)
            .ok_or(DocumentError::UnknownEvent(start))?;
        let (instrument, _) = self
            .doc
            .voice(location.voice)
            .ok_or(DocumentError::UnknownVoice(location.voice))?;
        let instrument = instrument.id;
        let raw = self.insert_new(|raw| {
            Ok(Object::Slur {
                instrument,
                slur: Slur::new(SlurId::from_counter(raw), start, end, placement),
            })
        })?;
        Ok(SlurId::from_counter(raw))
    }

    /// Adds a clef change on a staff at (bar, offset) and returns its ID (ADR-0024 §9). O(staves)
    /// + O(c).
    ///
    /// # Errors
    /// [`DocumentError::UnknownStaff`], [`DocumentError::UnknownBar`],
    /// [`DocumentError::NegativeOffset`], [`DocumentError::DuplicateMark`] (another change at the
    /// point, or the very start, where the initial clef is), [`DocumentError::IdsExhausted`]. The
    /// builder is unchanged on error.
    pub fn add_clef_change(
        &mut self,
        staff: StaffId,
        bar: BarId,
        offset: Duration,
        clef: Clef,
    ) -> Result<ClefId, DocumentError> {
        // The edit layer refuses the first point of the score (the initial clef is there) and a
        // second clef change at the same point of the staff.
        let raw = self.insert_new(|raw| {
            Ok(Object::ClefChange {
                staff,
                mark: Mark::new(ClefId::from_counter(raw), bar, offset, clef),
            })
        })?;
        Ok(ClefId::from_counter(raw))
    }

    /// Adds a key change of the whole score at (bar, offset), at concert pitch, and returns its ID
    /// (ADR-0024 §9, K1). O(k).
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`], [`DocumentError::NegativeOffset`],
    /// [`DocumentError::DuplicateMark`] (another change at the point, or the very start),
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_key_change(
        &mut self,
        bar: BarId,
        offset: Duration,
        key: Key,
    ) -> Result<KeyChangeId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::KeyChange(Mark::new(
                KeyChangeId::from_counter(raw),
                bar,
                offset,
                key,
            )))
        })?;
        Ok(KeyChangeId::from_counter(raw))
    }

    /// Adds an override of an instrument's key from (bar, offset) on, and returns its ID
    /// (ADR-0024 §9, K1). Unlike the other changes it may sit at the very start: an instrument
    /// written without signature overrides from bar 0. O(tracks) + O(o).
    ///
    /// # Errors
    /// [`DocumentError::UnknownInstrument`], [`DocumentError::UnknownBar`],
    /// [`DocumentError::NegativeOffset`], [`DocumentError::DuplicateMark`],
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_key_override(
        &mut self,
        instrument: InstrumentId,
        bar: BarId,
        offset: Duration,
        key_override: KeyOverride,
    ) -> Result<KeyChangeId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::KeyOverride {
                instrument,
                mark: Mark::new(KeyChangeId::from_counter(raw), bar, offset, key_override),
            })
        })?;
        Ok(KeyChangeId::from_counter(raw))
    }

    /// Adds a tempo mark at a point and returns its ID (ADR-0024 §11), then derives the tempo map
    /// again from all the marks, so that [`Document::tempo_map`] is always up to date while
    /// building (owner's decision of 2026-11-03).
    ///
    /// # All or nothing
    /// Whether the mark fits (a change inside a gradual one, a gradual change that does not go
    /// forwards) is only known once it is among the others. So it is tried in place with the ID
    /// the counter would hand out next ([`IdCounter::peek`]), the map is derived, and on failure
    /// the mark is taken out again: the builder is unchanged and no ID is consumed.
    ///
    /// # Complexity
    /// O(t) to insert among t marks, plus the derivation, O(t log m): building a score with t tempo
    /// marks costs O(t² log m), negligible for the hundreds of marks of a real score.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`] or [`DocumentError::NegativeOffset`] for the point or the end
    /// of a gradual change; [`DocumentError::DuplicateMark`] (another mark at the point, or the
    /// very start, where the initial mark is); [`DocumentError::Invalid`] with the violations of
    /// the marks; [`DocumentError::TempoMap`]; [`DocumentError::IdsExhausted`].
    pub fn add_tempo_mark(
        &mut self,
        at: ScorePoint,
        mark: TempoMark,
    ) -> Result<TempoMarkId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::TempoMark(Mark::new(
                TempoMarkId::from_counter(raw),
                at.bar,
                at.offset,
                mark,
            )))
        })?;
        let id = TempoMarkId::from_counter(raw);
        // Only the derived tempo map can tell whether the mark fits (a change inside a gradual
        // one): if it does not, the mark and its ID are taken back, all or nothing.
        match self.doc.derive_tempo_map() {
            Ok(map) => {
                self.doc.tempo_map = map;
                Ok(id)
            }
            Err(e) => {
                self.doc.tempo_marks.remove(id);
                self.doc.ids.give_back(&[raw]);
                Err(e)
            }
        }
    }

    /// Adds a dynamic to an instrument at a point and returns its ID (ADR-0024 §8).
    ///
    /// # Complexity
    /// O(tracks) to find the instrument, O(staves + voices) for the placement, O(log d) to find
    /// the place among d dynamics, O(d) to shift the later ones.
    ///
    /// # Errors
    /// [`DocumentError::UnknownInstrument`], [`DocumentError::UnknownBar`],
    /// [`DocumentError::NegativeOffset`], [`DocumentError::StaffOfAnotherInstrument`],
    /// [`DocumentError::VoiceOfAnotherInstrument`], [`DocumentError::DuplicateMark`] (a dynamic of
    /// the same scope at the point), [`DocumentError::IdsExhausted`]. The builder is unchanged on
    /// error.
    pub fn add_dynamic(
        &mut self,
        instrument: InstrumentId,
        at: ScorePoint,
        placement: DynamicPlacement,
        mark: DynamicMark,
    ) -> Result<DynamicId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::Dynamic {
                instrument,
                dynamic: Dynamic::new(DynamicId::from_counter(raw), at, placement, mark),
            })
        })?;
        Ok(DynamicId::from_counter(raw))
    }

    /// Adds a hairpin to an instrument, from `start` (included) to `end` (excluded), and returns
    /// its ID (ADR-0024 §8). That it does not overlap another hairpin of the same scope is checked
    /// by [`build`](Self::build), which sees all of them (rule 6 of [`Document::validate`]).
    ///
    /// # Complexity
    /// As [`add_dynamic`](Self::add_dynamic).
    ///
    /// # Errors
    /// [`DocumentError::UnknownInstrument`], [`DocumentError::UnknownBar`] or
    /// [`DocumentError::NegativeOffset`] for either end, [`DocumentError::HairpinNotForward`],
    /// [`DocumentError::StaffOfAnotherInstrument`], [`DocumentError::VoiceOfAnotherInstrument`],
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_hairpin(
        &mut self,
        instrument: InstrumentId,
        start: ScorePoint,
        end: ScorePoint,
        placement: DynamicPlacement,
        shape: HairpinShape,
    ) -> Result<HairpinId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::Hairpin {
                instrument,
                hairpin: Hairpin::new(HairpinId::from_counter(raw), start, end, placement, shape),
            })
        })?;
        Ok(HairpinId::from_counter(raw))
    }

    /// Adds a start repeat (‖:) at the start of `bar` and returns its ID (ADR-0025 §2). How it
    /// pairs with the end repeats is checked by [`build`](Self::build) (rule 8 of
    /// [`Document::validate`]). O(log r) to find the place, O(r) to shift the later ones.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`], [`DocumentError::DuplicateMark`] (another start repeat on
    /// the bar), [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_repeat_start(&mut self, bar: BarId) -> Result<RepeatId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::RepeatStart(RepeatStart::new(
                RepeatId::from_counter(raw),
                bar,
            )))
        })?;
        Ok(RepeatId::from_counter(raw))
    }

    /// Adds an end repeat (:‖) at the end of `bar`, played `times` times in all, and returns its
    /// ID (ADR-0025 §2). As [`add_repeat_start`](Self::add_repeat_start).
    ///
    /// # Errors
    /// [`DocumentError::PlaysOutOfRange`] unless `times` is 2–[`MAX_PLAYS`];
    /// [`DocumentError::UnknownBar`], [`DocumentError::DuplicateMark`],
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_repeat_end(&mut self, bar: BarId, times: u8) -> Result<RepeatId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::RepeatEnd(RepeatEnd::new(
                RepeatId::from_counter(raw),
                bar,
                times,
            )))
        })?;
        Ok(RepeatId::from_counter(raw))
    }

    /// Adds a volta over `first..=last`, played on `passes`, and returns its ID (ADR-0025 §3).
    /// How it fits with the other voltas and the repeats is checked by [`build`](Self::build)
    /// (rule 9). O(log v) to find the place, O(v) to shift the later ones.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`] for either bar, [`DocumentError::VoltaNotForward`],
    /// [`DocumentError::EmptyPassSet`], [`DocumentError::EmptyText`],
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_volta(
        &mut self,
        first: BarId,
        last: BarId,
        passes: PassSet,
        closed: bool,
        text: Option<&str>,
    ) -> Result<VoltaId, DocumentError> {
        // The bars first, then the text (the edit layer checks the order and the passes).
        self.doc.bar_index(first)?;
        self.doc.bar_index(last)?;
        check_text(text)?;
        let raw = self.insert_new(|raw| {
            Ok(Object::Volta(Volta::new(
                VoltaId::from_counter(raw),
                first,
                last,
                passes,
                closed,
                text.map(Box::from),
            )))
        })?;
        Ok(VoltaId::from_counter(raw))
    }

    /// Adds a marker on a barline and returns its ID (ADR-0025 §4): a segno or a coda at the start
    /// of `bar`, a Fine or a "To Coda" at its end. O(log m) + O(m).
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`], [`DocumentError::DuplicateMark`] (another marker on the
    /// same side of the bar), [`DocumentError::EmptyText`], [`DocumentError::IdsExhausted`]. The
    /// builder is unchanged on error.
    pub fn add_marker(
        &mut self,
        bar: BarId,
        kind: MarkerKind,
        text: Option<&str>,
    ) -> Result<MarkerId, DocumentError> {
        self.doc.bar_index(bar)?;
        check_text(text)?;
        let raw = self.insert_new(|raw| {
            Ok(Object::Marker(Marker::new(
                MarkerId::from_counter(raw),
                bar,
                kind,
                text.map(Box::from),
            )))
        })?;
        Ok(MarkerId::from_counter(raw))
    }

    /// Adds a jump at the end of `bar` and returns its ID (ADR-0025 §4). That its markers have the
    /// right kinds and places is checked by [`build`](Self::build) (rule 10). O(m) + O(j).
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`], [`DocumentError::UnknownMarker`] for a marker the document
    /// does not have, [`DocumentError::DuplicateMark`] (another jump on the bar),
    /// [`DocumentError::EmptyText`], [`DocumentError::IdsExhausted`]. The builder is unchanged on
    /// error.
    pub fn add_jump(
        &mut self,
        bar: BarId,
        spec: JumpSpec,
        text: Option<&str>,
    ) -> Result<JumpId, DocumentError> {
        self.doc.bar_index(bar)?;
        check_text(text)?;
        // The markers are the builder's check, as for the mark command: an edit does not check
        // them (see `crate::edit`).
        self.doc.check_jump_markers(&spec)?;
        let raw = self.insert_new(|raw| {
            Ok(Object::Jump(Jump::new(
                JumpId::from_counter(raw),
                bar,
                spec,
                text.map(Box::from),
            )))
        })?;
        Ok(JumpId::from_counter(raw))
    }

    /// Adds a bar repeat (%) on `staff`: `count` bars from `first` repeat the `span` bars before
    /// them (ADR-0025 §6). Whether the bars are empty, the source exists and no form marking is in
    /// the way is checked by [`build`](Self::build) (rule 11). O(tracks + staves) + O(b).
    ///
    /// # Errors
    /// [`DocumentError::UnknownStaff`], [`DocumentError::UnknownBar`],
    /// [`DocumentError::BarRepeatCount`] unless `count` is a positive multiple of the span,
    /// [`DocumentError::IdsExhausted`]. The builder is unchanged on error.
    pub fn add_bar_repeat(
        &mut self,
        staff: StaffId,
        first: BarId,
        count: u32,
        span: BarRepeatSpan,
    ) -> Result<BarRepeatId, DocumentError> {
        let raw = self.insert_new(|raw| {
            Ok(Object::BarRepeat(BarRepeat::new(
                BarRepeatId::from_counter(raw),
                staff,
                first,
                count,
                span,
            )))
        })?;
        Ok(BarRepeatId::from_counter(raw))
    }

    /// Adds a pass rule to an instrument: the passage from `start` (included) to `end` (excluded)
    /// is played only on the passes in `plays` of the repeated section containing it (ADR-0025
    /// §7). That it lies inside one section, with passes within its plays and without overlapping
    /// another rule of the scope, is checked by [`build`](Self::build) (rule 11).
    /// O(tracks + voices) + O(p).
    ///
    /// # Errors
    /// [`DocumentError::UnknownInstrument`], [`DocumentError::UnknownBar`] or
    /// [`DocumentError::NegativeOffset`] for either end, [`DocumentError::PassRuleNotForward`],
    /// [`DocumentError::VoiceOfAnotherInstrument`], [`DocumentError::EmptyPassSet`],
    /// [`DocumentError::EmptyText`], [`DocumentError::IdsExhausted`]. The builder is unchanged on
    /// error.
    pub fn add_pass_rule(
        &mut self,
        instrument: InstrumentId,
        start: ScorePoint,
        end: ScorePoint,
        scope: DynamicScope,
        plays: PassSet,
        text: Option<&str>,
    ) -> Result<PassRuleId, DocumentError> {
        check_text(text)?;
        let raw = self.insert_new(|raw| {
            Ok(Object::PassRule {
                instrument,
                rule: PassRule::new(
                    PassRuleId::from_counter(raw),
                    start,
                    end,
                    scope,
                    plays,
                    text.map(Box::from),
                ),
            })
        })?;
        Ok(PassRuleId::from_counter(raw))
    }

    /// Sets how the bars of later voltas are numbered (ADR-0025 F9); [`BarNumbering::Continue`]
    /// until it is called. O(1).
    pub const fn set_bar_numbering(&mut self, style: BarNumbering) {
        self.doc.form.numbering = style;
    }

    /// The document as built so far, for callers that need to read it while building (a
    /// generator asking for the staves of the track it just added). O(1).
    pub const fn document(&self) -> &Document {
        &self.doc
    }

    /// Finishes the document after checking it with [`Document::validate`] (owner's decision of
    /// 2026-11-02: no invalid document can exist). O(n log m) for n events and m meter changes.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with the first rule broken, or the arithmetic errors of the
    /// check (absurd meters only).
    pub fn build(self) -> Result<Document, DocumentError> {
        self.doc.validate()?;
        Ok(self.doc)
    }

    /// The positions of a voice: its track in the document, itself in its instrument. O(voices).
    fn voice_position(&self, voice: VoiceId) -> Result<(usize, usize), DocumentError> {
        self.doc.position_of_voice(voice)
    }

    /// The voice at these positions, mutably. O(1).
    fn voice_mut(&mut self, t: usize, v: usize, id: VoiceId) -> Result<&mut Voice, DocumentError> {
        self.doc
            .tracks
            .get_mut(t)
            .and_then(|track| track.instrument.voices.get_mut(v))
            .ok_or(DocumentError::UnknownVoice(id))
    }

    /// The chunk at an event location, mutably. O(voices) + O(1).
    fn chunk_mut(&mut self, location: EventLocation) -> Result<&mut Chunk, DocumentError> {
        let index = self.doc.bar_index(location.bar)?;
        let (t, v) = self.voice_position(location.voice)?;
        let chunk = usize::try_from(index.get()).map_err(|_| DocumentError::TooManyBars)?;
        self.voice_mut(t, v, location.voice)?
            .content
            .chunks
            .get_mut(chunk)
            .ok_or(DocumentError::UnknownBar(location.bar))
    }

    /// The event with this ID, mutably (copying its chunk first if a snapshot shares it).
    /// O(1) average + O(voices) + O(k).
    fn event_mut(&mut self, event: EventId) -> Result<&mut Event, DocumentError> {
        let location = *self
            .doc
            .events
            .get(&event)
            .ok_or(DocumentError::UnknownEvent(event))?;
        Ok(self
            .chunk_mut(location)?
            .get_mut(event)
            .ok_or(DocumentError::UnknownEvent(event))?
            .event_mut())
    }

    /// The chord with this event ID, mutably. As [`event_mut`](Self::event_mut).
    fn chord_mut(&mut self, event: EventId) -> Result<&mut Chord, DocumentError> {
        match self.event_mut(event)?.body_mut() {
            EventBody::Chord(chord) => Ok(chord),
            EventBody::Rest(_) => Err(DocumentError::NotAChord(event)),
        }
    }

    /// The notehead with this ID, mutably. As [`event_mut`](Self::event_mut) plus O(noteheads).
    fn note_mut(&mut self, note: NoteId) -> Result<&mut Note, DocumentError> {
        let event = *self
            .doc
            .notes
            .get(&note)
            .ok_or(DocumentError::UnknownNote(note))?;
        match self.event_mut(event)?.body_mut() {
            EventBody::Chord(chord) => chord.note_mut(note).ok_or(DocumentError::UnknownNote(note)),
            // A notehead always belongs to a chord: the index never points at a rest.
            EventBody::Rest(_) => Err(DocumentError::UnknownNote(note)),
        }
    }
}

/// The next raw ID of the document's counter, or [`DocumentError::IdsExhausted`] when the counter
/// has handed out its last one. O(1).
///
/// The builder takes an ID in two dozen places; this keeps the error in one.
fn issue(ids: &mut IdCounter) -> Result<NonZeroU64, DocumentError> {
    ids.next().ok_or(DocumentError::IdsExhausted)
}
