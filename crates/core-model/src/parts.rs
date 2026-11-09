// SPDX-License-Identifier: GPL-3.0-or-later

//! A plain description of everything a document saves, and the two ways between it and a
//! [`Document`] (ADR-0029 S4 and S7; owner's choices L1–L8 of 2026-11-06, SCALETTA F1.15b).
//!
//! # Why a description, and not serde on the model?
//! The project file (crate `orchestra-io`) has a schema of its own, so that a refactoring of the
//! model never changes the file by accident (ADR-0029 S4). This module is the meeting point:
//! [`DocumentParts`] holds the saved data in the model's **own** value types (notes, chords, marks,
//! slurs… with their IDs), in plain public containers. `io` maps it to and from its schema;
//! `core-model` stays free of any serialisation dependency.
//!
//! - [`Document::to_parts`] copies a document's saved data out (ADR-0029 L3);
//! - [`Document::from_parts`] builds a document back from a description, checking it **exactly
//!   as the builder checks what it creates** (ADR-0029 L4), and fails as a whole.
//!
//! # How `from_parts` checks: replaying through the builder
//! Instead of a second set of rules written for loading (which could drift from the builder's and
//! let a file create a document the program itself never could), the description is **replayed**
//! through the [`DocumentBuilder`] methods: one `add_bar` per bar, one `add_chord` per chord, one
//! `add_slur` per slur, and so on, in the saved order. The builder takes its IDs from the counter,
//! which is put in *script* mode (`IdCounter::set_script`): before each call the loader gives it
//! the saved IDs that call will take, in the builder's documented order (a chord: its event, then
//! its noteheads). Every local check of the builder (bars, voices, staves and tuplets exist and
//! belong together, offsets not negative, tuplet depth, graces and whole-bar rests outside
//! tuplets, one mark per point, texts not empty…) and then the whole-document [`validate`] apply
//! unchanged.
//!
//! What the builder does not see, the loader checks itself:
//! - **IDs** (ADR-0029 L4): every saved ID is unique across all kinds (one counter, ADR-0022 §1)
//!   and below the saved next value of the counter, so no later object can receive it again;
//! - **order** (L4, ADR-0029 S3, S8): the builder sorts what it inserts (events in a chunk,
//!   noteheads in a chord, marks by point); the rebuilt document's description must equal the
//!   given one, so a file whose lists are out of order is refused ([`DocumentError::NotCanonical`])
//!   instead of being silently reordered (a re-save would then differ from the file).
//!
//! # What is not saved
//! Derived data (the tempo map, every index) is rebuilt by the builder; the revision starts at 0;
//! the undo history is never saved (D08, ADR-0026).
//!
//! [`validate`]: Document::validate

use std::num::NonZeroU64;

use orchestra_core_time::MeterMap;

use crate::bar_repeat::{BarRepeat, PassRule};
use crate::document::{Document, DocumentBuilder, DocumentError, Placement};
use crate::dynamic::{Dynamic, Hairpin};
use crate::event::{EventBody, PlacedEvent, Tie, Tuplet};
use crate::form::{BarNumbering, RepeatEnd, RepeatStart, Volta};
use crate::id::{
    BarId, ClefId, IdCounter, IdSet, InstrumentId, KeyChangeId, StaffId, TempoMarkId, TrackId,
    TupletId, VoiceId,
};
use crate::jump::{Jump, Marker};
use crate::marking::ArticulationSet;
use crate::marks::{Clef, KeyOverride, Mark, ScorePoint};
use crate::pass_offsets::PassOffset;
use crate::pitch::{Interval, Key};
use crate::slur::Slur;
use crate::tempo_mark::TempoMark;
use crate::tolerance::Tolerance;

/// Everything a document saves (ADR-0029 S7), in the model's own value types.
///
/// Every list is in the order the document keeps it (score order for bars and marks, chunk order
/// for events, lowest first for noteheads, ID order for tuplets); [`Document::from_parts`] refuses
/// any other order. Fields are public: this is a plain description, checked only when a document
/// is built from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentParts {
    /// The ID the counter hands out next (`None`: every value has been used). Above every ID of
    /// the description.
    pub next_id: Option<NonZeroU64>,
    /// The meter map: bar lengths, meters, beat groupings, numbering exceptions.
    pub meter_map: MeterMap,
    /// The bars, in score order.
    pub bars: Vec<BarId>,
    /// The tempo mark at the very start, with its ID (always an immediate tempo).
    pub initial_tempo: (TempoMarkId, TempoMark),
    /// The tempo marks after the start, in score order.
    pub tempo_marks: Vec<Mark<TempoMarkId, TempoMark>>,
    /// The key at the start, at concert pitch.
    pub initial_key: Key,
    /// The key changes after the start, at concert pitch, in score order.
    pub key_changes: Vec<Mark<KeyChangeId, Key>>,
    /// The start repeat barlines, in score order.
    pub repeat_starts: Vec<RepeatStart>,
    /// The end repeat barlines, in score order.
    pub repeat_ends: Vec<RepeatEnd>,
    /// The voltas, in score order.
    pub voltas: Vec<Volta>,
    /// The markers (segno, coda, fine, to coda), in score order.
    pub markers: Vec<Marker>,
    /// The jumps (D.C., D.S. and the like), in score order.
    pub jumps: Vec<Jump>,
    /// The bar-numbering style.
    pub numbering: BarNumbering,
    /// The tracks, top to bottom.
    pub tracks: Vec<TrackParts>,
    /// The project's tolerance threshold for the DAW move (ADR-0028 §1).
    pub tolerance: Tolerance,
    /// The per-track tolerance overrides, in the document's order.
    pub track_tolerances: Vec<(TrackId, Tolerance)>,
}

/// One track and its instrument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackParts {
    /// The track's ID.
    pub id: TrackId,
    /// The instrument's ID.
    pub instrument: InstrumentId,
    /// The instrument's name, as the user typed it.
    pub name: String,
    /// The interval from written to sounding pitch (ADR-0023 §7).
    pub transposition: Interval,
    /// The staves, top to bottom.
    pub staves: Vec<StaffParts>,
    /// The voices, in the instrument's order.
    pub voices: Vec<VoiceParts>,
    /// The instrument's key overrides, in score order.
    pub key_overrides: Vec<Mark<KeyChangeId, KeyOverride>>,
    /// The slurs, by ID.
    pub slurs: Vec<Slur>,
    /// The dynamics, by (point, ID).
    pub dynamics: Vec<Dynamic>,
    /// The hairpins, by (point, ID).
    pub hairpins: Vec<Hairpin>,
    /// The bar repeats, by (point, ID).
    pub bar_repeats: Vec<BarRepeat>,
    /// The pass rules, by (point, ID).
    pub pass_rules: Vec<PassRule>,
    /// The pass-specific offsets (ADR-0028 §5), in the table's order.
    pub pass_offsets: Vec<PassOffset>,
}

/// One staff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaffParts {
    /// The staff's ID.
    pub id: StaffId,
    /// The clef at the start.
    pub initial_clef: Clef,
    /// The clef changes after the start, in score order.
    pub clef_changes: Vec<Mark<ClefId, Clef>>,
}

/// One voice and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceParts {
    /// The voice's ID.
    pub id: VoiceId,
    /// Its home staff (one of its instrument's).
    pub home_staff: StaffId,
    /// Its tuplets with their IDs, by ID (a parent always has a smaller ID than its children).
    pub tuplets: Vec<(TupletId, Tuplet)>,
    /// Its **non-empty** cells, in score order of their bars.
    pub cells: Vec<CellParts>,
}

/// The events of one voice in one bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellParts {
    /// The bar.
    pub bar: BarId,
    /// The events, in chunk order (never empty).
    pub events: Vec<PlacedEvent>,
}

impl Document {
    /// Everything this document saves, as a plain description (ADR-0029 S4, S7; L3).
    ///
    /// The lists come out in the document's own orders, so the same document always gives the
    /// same description (and the same file: ADR-0029 S3). Empty cells are left out. Tuplets are
    /// sorted by ID (the document keeps them in a hash map).
    ///
    /// # Complexity
    /// O(size of the document) and one copy of all of it: about the memory of the document again
    /// while the description lives (measured in `benches/parts.rs`).
    #[must_use]
    pub fn to_parts(&self) -> DocumentParts {
        DocumentParts {
            next_id: self.ids.next_value(),
            meter_map: self.meter_map.clone(),
            bars: self.bars.ids().to_vec(),
            initial_tempo: self.initial_tempo.clone(),
            tempo_marks: self.tempo_marks.as_slice().to_vec(),
            initial_key: self.initial_key,
            key_changes: self.key_changes.as_slice().to_vec(),
            repeat_starts: self.form.repeat_starts.clone(),
            repeat_ends: self.form.repeat_ends.clone(),
            voltas: self.form.voltas.clone(),
            markers: self.form.markers.clone(),
            jumps: self.form.jumps.clone(),
            numbering: self.form.numbering,
            tracks: self.tracks.iter().map(|t| self.track_parts(t)).collect(),
            tolerance: self.tolerance.project(),
            track_tolerances: self.tolerance.per_track().to_vec(),
        }
    }

    /// The description of one track (see [`to_parts`](Self::to_parts)). O(size of the track).
    fn track_parts(&self, track: &crate::document::Track) -> TrackParts {
        let instrument = &track.instrument;
        TrackParts {
            id: track.id,
            instrument: instrument.id,
            name: instrument.name.clone(),
            transposition: instrument.transposition,
            staves: instrument
                .staves
                .iter()
                .map(|staff| StaffParts {
                    id: staff.id,
                    initial_clef: staff.initial_clef,
                    clef_changes: staff.clef_changes.as_slice().to_vec(),
                })
                .collect(),
            voices: instrument
                .voices
                .iter()
                .map(|voice| {
                    let mut tuplets: Vec<(TupletId, Tuplet)> = voice
                        .content
                        .tuplets
                        .iter()
                        .map(|(&id, &tuplet)| (id, tuplet))
                        .collect();
                    tuplets.sort_unstable_by_key(|&(id, _)| id);
                    VoiceParts {
                        id: voice.id,
                        home_staff: voice.home_staff,
                        tuplets,
                        // Chunks are aligned with the bar list (an invariant of the document).
                        cells: self
                            .bars
                            .ids()
                            .iter()
                            .zip(&voice.content.chunks)
                            .filter(|(_, chunk)| !chunk.events().is_empty())
                            .map(|(&bar, chunk)| CellParts {
                                bar,
                                events: chunk.events().to_vec(),
                            })
                            .collect(),
                    }
                })
                .collect(),
            key_overrides: instrument.key_overrides.as_slice().to_vec(),
            slurs: instrument.slurs.clone(),
            dynamics: instrument.dynamics.clone(),
            hairpins: instrument.hairpins.clone(),
            bar_repeats: instrument.bar_repeats.clone(),
            pass_rules: instrument.pass_rules.clone(),
            pass_offsets: instrument.pass_offsets.entries().to_vec(),
        }
    }

    /// Builds the document a description describes, checking it as the builder checks what it
    /// creates (ADR-0029 S4, S8; L4). All or nothing: on error nothing exists.
    ///
    /// See the module documentation for how: a replay through the [`DocumentBuilder`] with the
    /// saved IDs, the full [`validate`](Self::validate), then the comparison of the rebuilt
    /// description with the given one. The result has the same IDs and the same counter as the
    /// saved document, so it keeps handing out IDs never used before; its revision is 0.
    ///
    /// # Errors
    /// - [`DocumentError::DuplicateId`] if a raw ID names two objects;
    /// - [`DocumentError::IdNotIssued`] if an ID is not below `next_id`;
    /// - every error of the builder methods (an unknown bar, voice, staff or tuplet, a negative
    ///   offset, an empty text, [`DocumentError::TooManyTracks`]…) and of
    ///   [`validate`](Self::validate) ([`DocumentError::Invalid`]);
    /// - [`DocumentError::NotCanonical`] if a list is not in the document's order.
    ///
    /// # Complexity
    /// About the cost of building the same document with the builder (O(n log n) for n objects,
    /// dominated by the chunk insertions and `validate`), plus one [`to_parts`](Self::to_parts)
    /// and one comparison; one hash set of all IDs.
    pub fn from_parts(parts: &DocumentParts) -> Result<Self, DocumentError> {
        let mut loader = Loader::new(parts)?;
        loader.tracks(parts)?;
        loader.marks(parts)?;
        loader.events(parts)?;
        loader.links(parts)?;
        loader.form(parts)?;
        let mut builder = loader.builder;
        builder.doc.ids.end_script();
        let doc = builder.build()?;
        if doc.to_parts() != *parts {
            return Err(DocumentError::NotCanonical);
        }
        Ok(doc)
    }
}

/// The state of one [`Document::from_parts`]: the builder being replayed into, and the IDs met so
/// far.
struct Loader {
    /// The document under reconstruction, with its counter in script mode.
    builder: DocumentBuilder,
    /// The saved next value of the counter: every saved ID must be below it.
    next: Option<NonZeroU64>,
    /// Every raw ID claimed so far (all kinds share one space, ADR-0022 §1).
    seen: IdSet<u64>,
}

impl Loader {
    /// Starts the reconstruction: the meter map, the initial tempo mark (with its saved ID), the
    /// initial key and the bars. O(bars).
    fn new(parts: &DocumentParts) -> Result<Self, DocumentError> {
        let (tempo_id, tempo) = &parts.initial_tempo;
        let mut builder =
            DocumentBuilder::new(parts.meter_map.clone(), tempo.clone(), parts.initial_key)?;
        // The fresh builder took ID 1 for the initial mark; replace its counter with the saved
        // one, in script mode, and give the mark its saved ID (claimed like every other).
        builder.doc.ids = IdCounter::restored(parts.next_id);
        builder.doc.initial_tempo.0 = *tempo_id;
        let mut loader = Self {
            builder,
            next: parts.next_id,
            seen: IdSet::default(),
        };
        loader.claim(tempo_id.get())?;
        for &bar in &parts.bars {
            loader.call(&[bar.get()], DocumentBuilder::add_bar)?;
        }
        Ok(loader)
    }

    /// Claims one saved raw ID: not issued twice, below the counter's next value. O(1) average.
    fn claim(&mut self, raw: u64) -> Result<NonZeroU64, DocumentError> {
        let id = NonZeroU64::new(raw).ok_or(DocumentError::ZeroId)?;
        if self.next.is_some_and(|next| id >= next) {
            return Err(DocumentError::IdNotIssued(raw));
        }
        if !self.seen.insert(raw) {
            return Err(DocumentError::DuplicateId(raw));
        }
        Ok(id)
    }

    /// Runs one builder method that takes exactly the IDs `raw`, in that order: claims them, gives
    /// them to the counter's script, runs the method, and checks that it took them all.
    ///
    /// A builder method that wanted more IDs than given finds the script spent and fails with
    /// `IdsExhausted`; one that took fewer leaves some in the script. Both can only be a mismatch
    /// between this loader and the builder's documented ID order, never a property of the file,
    /// so both become [`DocumentError::Inconsistent`]. O(ids) plus the method.
    fn call<T>(
        &mut self,
        raw: &[u64],
        method: impl FnOnce(&mut DocumentBuilder) -> Result<T, DocumentError>,
    ) -> Result<T, DocumentError> {
        let ids = raw
            .iter()
            .map(|&r| self.claim(r))
            .collect::<Result<Vec<_>, _>>()?;
        self.builder.doc.ids.set_script(&ids);
        let result = method(&mut self.builder).map_err(|e| match e {
            DocumentError::IdsExhausted => DocumentError::inconsistent(),
            other => other,
        })?;
        if !self.builder.doc.ids.script_is_spent() {
            return Err(DocumentError::inconsistent());
        }
        Ok(result)
    }

    /// The tracks: instrument, staves (with their clef changes), voices (with their tuplets) and
    /// key overrides. Events come later ([`events`](Self::events)), once every voice and tuplet
    /// exists. O(size of the structure).
    fn tracks(&mut self, parts: &DocumentParts) -> Result<(), DocumentError> {
        for track in &parts.tracks {
            // The builder's order for a track without voices: track, instrument, one per staff.
            let mut ids = vec![track.id.get(), track.instrument.get()];
            ids.extend(track.staves.iter().map(|s| s.id.get()));
            let clefs: Vec<Clef> = track.staves.iter().map(|s| s.initial_clef).collect();
            self.call(&ids, |b| {
                b.insert_instrument_track(track.name.clone(), track.transposition, &clefs, false)
            })?;
            for staff in &track.staves {
                for change in &staff.clef_changes {
                    self.call(&[change.id().get()], |b| {
                        b.add_clef_change(staff.id, change.bar(), change.offset(), *change.value())
                    })?;
                }
            }
            for voice in &track.voices {
                self.call(&[voice.id.get()], |b| b.add_voice(voice.home_staff))?;
                // Parents before children: a tuplet's parent always has a smaller ID (it existed
                // when the child was created), and the list is by ID; `add_tuplet` refuses an
                // unknown parent, so a file breaking this is refused there.
                for &(id, tuplet) in &voice.tuplets {
                    self.call(&[id.get()], |b| {
                        b.add_tuplet(voice.id, tuplet, tuplet.parent())
                    })?;
                }
            }
            for mark in &track.key_overrides {
                self.call(&[mark.id().get()], |b| {
                    b.add_key_override(track.instrument, mark.bar(), mark.offset(), *mark.value())
                })?;
            }
        }
        Ok(())
    }

    /// The document-wide marks: key changes and tempo marks. O(k + t²) (each tempo mark derives
    /// the tempo map again, as in the builder).
    fn marks(&mut self, parts: &DocumentParts) -> Result<(), DocumentError> {
        for mark in &parts.key_changes {
            self.call(&[mark.id().get()], |b| {
                b.add_key_change(mark.bar(), mark.offset(), *mark.value())
            })?;
        }
        for mark in &parts.tempo_marks {
            self.call(&[mark.id().get()], |b| {
                b.add_tempo_mark(
                    ScorePoint::new(mark.bar(), mark.offset()),
                    mark.value().clone(),
                )
            })?;
        }
        Ok(())
    }

    /// Every event, cell by cell in the saved order, with its markings. Ties and offsets wait for
    /// [`links`](Self::links): a tie may point to a notehead not created yet. O(events), each
    /// insertion O(log k + k) in its chunk as in the builder.
    fn events(&mut self, parts: &DocumentParts) -> Result<(), DocumentError> {
        for track in &parts.tracks {
            for voice in &track.voices {
                for cell in &voice.cells {
                    for placed in &cell.events {
                        self.event(voice.id, cell.bar, placed)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// One event (see [`events`](Self::events)).
    fn event(
        &mut self,
        voice: VoiceId,
        bar: BarId,
        placed: &PlacedEvent,
    ) -> Result<(), DocumentError> {
        let event = placed.event();
        let at = Placement::new(voice, bar, placed.offset()).on_staff(event.staff());
        match event.body() {
            EventBody::Chord(chord) => {
                let at = match chord.tuplet() {
                    Some(t) => at.in_tuplet(t),
                    None => at,
                };
                // The builder's order: the event, then one per notehead in the given order.
                let mut ids = vec![event.id().get()];
                ids.extend(chord.notes().map(|n| n.id().get()));
                let pitches: Vec<_> = chord.notes().map(crate::event::Note::pitch).collect();
                self.call(&ids, |b| match chord.grace() {
                    Some(grace) => b.add_grace_chord(at, grace, chord.value(), &pitches),
                    None => b.add_chord(at, chord.value(), &pitches),
                })?;
                let b = &mut self.builder;
                if chord.articulations() != ArticulationSet::EMPTY {
                    b.set_articulations(event.id(), chord.articulations())?;
                }
                if chord.ornament().is_some() {
                    b.set_ornament(event.id(), chord.ornament())?;
                }
                if chord.arpeggio().is_some() {
                    b.set_arpeggio(event.id(), chord.arpeggio())?;
                }
                if chord.fermata().is_some() {
                    b.set_fermata(event.id(), chord.fermata())?;
                }
            }
            EventBody::Rest(rest) => {
                let at = match rest.tuplet() {
                    Some(t) => at.in_tuplet(t),
                    None => at,
                };
                self.call(&[event.id().get()], |b| b.add_rest(at, rest.value()))?;
                if rest.fermata().is_some() {
                    self.builder.set_fermata(event.id(), rest.fermata())?;
                }
            }
        }
        Ok(())
    }

    /// What links events once they all exist: ties, performance offsets, slurs, dynamics,
    /// hairpins, bar repeats, pass rules, pass-specific offsets and the tolerance settings.
    /// O(links), each as in the builder.
    fn links(&mut self, parts: &DocumentParts) -> Result<(), DocumentError> {
        for track in &parts.tracks {
            for voice in &track.voices {
                for placed in voice.cells.iter().flat_map(|c| &c.events) {
                    let EventBody::Chord(chord) = placed.event().body() else {
                        continue;
                    };
                    for note in chord.notes() {
                        if note.tie() != Tie::None {
                            self.builder.set_tie(note.id(), note.tie())?;
                        }
                        if !note.offsets().is_none() {
                            self.builder.set_offsets(note.id(), note.offsets())?;
                        }
                    }
                }
            }
            for slur in &track.slurs {
                self.call(&[slur.id().get()], |b| {
                    b.add_slur(slur.start(), slur.end(), slur.placement())
                })?;
            }
            for dynamic in &track.dynamics {
                self.call(&[dynamic.id().get()], |b| {
                    b.add_dynamic(
                        track.instrument,
                        dynamic.at(),
                        dynamic.placement(),
                        dynamic.mark().clone(),
                    )
                })?;
            }
            for hairpin in &track.hairpins {
                self.call(&[hairpin.id().get()], |b| {
                    b.add_hairpin(
                        track.instrument,
                        hairpin.start(),
                        hairpin.end(),
                        hairpin.placement(),
                        hairpin.shape(),
                    )
                })?;
            }
            for entry in &track.pass_offsets {
                self.builder
                    .set_pass_offsets(entry.note, entry.pass, entry.offsets)?;
            }
        }
        self.builder.set_tolerance(None, Some(parts.tolerance))?;
        for &(track, tolerance) in &parts.track_tolerances {
            self.builder.set_tolerance(Some(track), Some(tolerance))?;
        }
        Ok(())
    }

    /// The form: repeat barlines, voltas, markers, jumps (after the markers they name), bar
    /// repeats and pass rules (after the events they must not cover), and the numbering style.
    /// O(form markings), each as in the builder.
    fn form(&mut self, parts: &DocumentParts) -> Result<(), DocumentError> {
        for start in &parts.repeat_starts {
            self.call(&[start.id().get()], |b| b.add_repeat_start(start.bar()))?;
        }
        for end in &parts.repeat_ends {
            self.call(&[end.id().get()], |b| {
                b.add_repeat_end(end.bar(), end.times())
            })?;
        }
        for volta in &parts.voltas {
            self.call(&[volta.id().get()], |b| {
                b.add_volta(
                    volta.first(),
                    volta.last(),
                    volta.passes(),
                    volta.closed(),
                    volta.text(),
                )
            })?;
        }
        for marker in &parts.markers {
            self.call(&[marker.id().get()], |b| {
                b.add_marker(marker.bar(), marker.kind(), marker.text())
            })?;
        }
        for jump in &parts.jumps {
            self.call(&[jump.id().get()], |b| {
                b.add_jump(jump.bar(), jump.spec(), jump.text())
            })?;
        }
        for track in &parts.tracks {
            for repeat in &track.bar_repeats {
                self.call(&[repeat.id().get()], |b| {
                    b.add_bar_repeat(
                        repeat.staff(),
                        repeat.first(),
                        repeat.count(),
                        repeat.span(),
                    )
                })?;
            }
            for rule in &track.pass_rules {
                self.call(&[rule.id().get()], |b| {
                    b.add_pass_rule(
                        track.instrument,
                        rule.start(),
                        rule.end(),
                        rule.scope(),
                        rule.plays(),
                        rule.text(),
                    )
                })?;
            }
        }
        self.builder.set_bar_numbering(parts.numbering);
        Ok(())
    }
}
