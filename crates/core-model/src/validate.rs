// SPDX-License-Identifier: GPL-3.0-or-later

//! The rules of a valid document that need the whole document to check (ADR-0024 §1, §2, §4, §5,
//! §7; SCALETTA F1.09b and F1.09c).
//!
//! # Which rules live here, and which elsewhere
//! The builder (and later the commands of F1.11) checks every rule it can see **locally** before
//! creating anything: the bar, voice, staff and tuplet exist and belong together, offsets are not
//! negative, tuplets nest at most four deep, a whole-bar rest or a grace chord is not in a tuplet,
//! a slur's ends exist. What needs more than one object is checked here, once, by
//! [`Document::validate`]:
//!
//! 1. **No overlap within a voice and bar** (ADR-0024 §1): each event of a chunk starts at or
//!    after the end of the previous one. Gaps are fine. The check is per bar, not across bars,
//!    because an **overfull** bar (ADR-0022 §4) is valid: its last events reach past the barline
//!    into the time of the next bar, and the score shows it as an error for the user to fix.
//! 2. **A whole-bar rest is alone** in its voice and bar (ADR-0024 §2).
//!    Grace chords last zero (ADR-0024 §5), so they never make a later event overlap.
//! 3. **Ties** (ADR-0024 §4): the target notehead exists, belongs to the same instrument, sounds
//!    at the same pitch, and comes strictly after the starting notehead in **score order**
//!    (position, then slot, then place in the chunk: see `crate::document`).
//! 4. **Grace chords have their main event** (ADR-0024 §5): a `Before` grace sits at the offset
//!    of a main (non-grace) event of the same voice and bar; an `After` grace sits exactly where
//!    the previous main event of its chunk ends. Several graces in a row share one main event.
//! 5. **Slurs** (ADR-0024 §7): both ends exist, belong to the slur's instrument, and the end
//!    comes strictly after the start in score order.
//!
//! `DocumentBuilder::build` calls it, so no invalid document can exist (owner's decision of
//! 2026-11-02); the file loader of F1.15 builds through the builder and gets it for free.

use std::error::Error;
use std::fmt;
use std::hash::BuildHasherDefault;

use orchestra_core_time::{BarIndex, Duration};

use crate::document::{Document, DocumentError, Instrument, is_whole_bar_rest};
use crate::event::{EventBody, Note, PlacedEvent, Tie};
use crate::id::{EventId, IdMap, InstrumentId, NoteId, SlurId, VoiceId};
use crate::marking::GraceAttach;

/// The first rule of a valid document that a document breaks (see the module documentation).
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Violation {
    /// This event starts before the previous event of its voice and bar has ended.
    Overlap(EventId),
    /// This whole-bar rest shares its voice and bar with other events.
    WholeBarRestNotAlone(EventId),
    /// This notehead is tied to a notehead of another instrument.
    TieToAnotherInstrument(NoteId),
    /// This notehead is tied to a notehead that does not sound the same.
    TieToAnotherPitch(NoteId),
    /// This notehead is tied to a notehead that does not start later.
    TieNotForward(NoteId),
    /// This notehead is tied to a notehead the document does not have.
    TieTargetMissing(NoteId),
    /// This `Before` grace chord has no main event at its offset in its voice and bar.
    BeforeGraceWithoutEvent(EventId),
    /// This `After` grace chord is not where the previous main event of its voice and bar ends.
    AfterGraceWithoutEvent(EventId),
    /// One end of this slur is an event the document does not have.
    SlurEndMissing(SlurId),
    /// One end of this slur belongs to another instrument than the slur.
    SlurToAnotherInstrument(SlurId),
    /// The end of this slur does not come after its start in score order.
    SlurNotForward(SlurId),
}

impl fmt::Display for Violation {
    /// Writes a short English description of the violation, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overlap(id) => write!(
                f,
                "event {} starts before the previous event of its voice ends",
                id.get()
            ),
            Self::WholeBarRestNotAlone(id) => write!(
                f,
                "whole-bar rest {} is not alone in its voice and bar",
                id.get()
            ),
            Self::TieToAnotherInstrument(id) => {
                write!(f, "notehead {} is tied to another instrument", id.get())
            }
            Self::TieToAnotherPitch(id) => {
                write!(f, "notehead {} is tied to a different pitch", id.get())
            }
            Self::TieNotForward(id) => write!(
                f,
                "notehead {} is tied to a notehead that does not start later",
                id.get()
            ),
            Self::TieTargetMissing(id) => write!(
                f,
                "notehead {} is tied to a notehead that does not exist",
                id.get()
            ),
            Self::BeforeGraceWithoutEvent(id) => write!(
                f,
                "grace chord {} has no main event at its offset",
                id.get()
            ),
            Self::AfterGraceWithoutEvent(id) => write!(
                f,
                "grace chord {} is not where the previous event of its voice ends",
                id.get()
            ),
            Self::SlurEndMissing(id) => {
                write!(f, "slur {} has an end that does not exist", id.get())
            }
            Self::SlurToAnotherInstrument(id) => {
                write!(f, "slur {} has an end in another instrument", id.get())
            }
            Self::SlurNotForward(id) => write!(
                f,
                "slur {} does not end after it starts in score order",
                id.get()
            ),
        }
    }
}

impl Error for Violation {}

impl Document {
    /// Checks the rules of a valid document that need the whole document (see the module
    /// documentation of `validate`). `DocumentBuilder::build` calls it.
    ///
    /// # Complexity
    /// O(n log m) for n events and m meter changes: every event is visited once, and every tie and
    /// every slur costs two event lookups and two bar starts. It allocates one small map from
    /// voices to instruments (a few hundred entries at most), sized up front: one allocation.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with the first violation found, in score order of tracks, voices
    /// and bars; the arithmetic and meter map errors of the durations (absurd meters only).
    pub fn validate(&self) -> Result<(), DocumentError> {
        // Which instrument owns each voice: the tie rule compares the two ends' instruments.
        // Sized up front, so that building it is one allocation however many voices there are.
        let voices = self.instruments().map(|i| i.voices().len()).sum();
        let mut owner: IdMap<VoiceId, InstrumentId> =
            IdMap::with_capacity_and_hasher(voices, BuildHasherDefault::default());
        for instrument in self.instruments() {
            for voice in instrument.voices() {
                owner.insert(voice.id(), instrument.id());
            }
        }

        for instrument in self.instruments() {
            for voice in instrument.voices() {
                for (index, _) in self.bars().ids().iter().enumerate() {
                    let index = BarIndex::new(
                        u32::try_from(index).map_err(|_| DocumentError::TooManyBars)?,
                    );
                    let events = voice.events_at(index);
                    // Rules 1, 2 and 4, inside one chunk, in one pass: the duration each event
                    // needs for rule 1 is the one rule 4 needs, so it is computed once.
                    let mut end_of_previous: Option<Duration> = None;
                    let mut graces = GraceCheck::default();
                    for placed in events {
                        let event = placed.event();
                        let bar_length = if is_whole_bar_rest(event) {
                            if events.len() > 1 {
                                return Err(DocumentError::Invalid(
                                    Violation::WholeBarRestNotAlone(event.id()),
                                ));
                            }
                            self.meter_map().bar_length(index)?
                        } else {
                            Duration::ZERO
                        };
                        if end_of_previous.is_some_and(|end| placed.offset() < end) {
                            return Err(DocumentError::Invalid(Violation::Overlap(event.id())));
                        }
                        let duration = voice.metric_duration(event, bar_length)?;
                        let end = placed.offset().checked_add(duration)?;
                        end_of_previous = Some(end);
                        graces.step(placed, end)?;

                        // Rule 3, for every tie that starts in this event.
                        if let EventBody::Chord(chord) = event.body() {
                            for note in chord.notes() {
                                if let Tie::To(target) = note.tie() {
                                    self.check_tie(
                                        &owner,
                                        instrument.id(),
                                        event.id(),
                                        note,
                                        target,
                                    )?;
                                }
                            }
                        }
                    }
                    graces.finish()?;
                }
            }
            // Rule 5, for every slur of the instrument.
            for slur in instrument.slurs() {
                self.check_slur(&owner, instrument, slur.id(), slur.start(), slur.end())?;
            }
        }
        Ok(())
    }

    /// Checks one slur (rule 5). Two event lookups + [`Document::comes_before`].
    fn check_slur(
        &self,
        owner: &IdMap<VoiceId, InstrumentId>,
        instrument: &Instrument,
        slur: SlurId,
        start: EventId,
        end: EventId,
    ) -> Result<(), DocumentError> {
        for event in [start, end] {
            let found = self
                .event(event)
                .ok_or(DocumentError::Invalid(Violation::SlurEndMissing(slur)))?;
            if owner.get(&found.voice()) != Some(&instrument.id()) {
                return Err(DocumentError::Invalid(Violation::SlurToAnotherInstrument(
                    slur,
                )));
            }
        }
        if !self.comes_before(start, end)? {
            return Err(DocumentError::Invalid(Violation::SlurNotForward(slur)));
        }
        Ok(())
    }

    /// Checks one tie (rule 3). O(event lookup) twice + [`Document::comes_before`].
    fn check_tie(
        &self,
        owner: &IdMap<VoiceId, InstrumentId>,
        instrument: InstrumentId,
        event: EventId,
        note: &Note,
        target: NoteId,
    ) -> Result<(), DocumentError> {
        let Some((target_event, target_note)) = self.note(target) else {
            return Err(DocumentError::Invalid(Violation::TieTargetMissing(
                note.id(),
            )));
        };
        // The index must agree with what the chunk holds: a target found by `note` always has an
        // owner, so a missing one is reported as another instrument rather than a panic.
        if owner.get(&target_event.voice()) != Some(&instrument) {
            return Err(DocumentError::Invalid(Violation::TieToAnotherInstrument(
                note.id(),
            )));
        }
        if !note.pitch().sounds_like(target_note.pitch())? {
            return Err(DocumentError::Invalid(Violation::TieToAnotherPitch(
                note.id(),
            )));
        }
        if !self.comes_before(event, target_event.event().id())? {
            return Err(DocumentError::Invalid(Violation::TieNotForward(note.id())));
        }
        Ok(())
    }
}

/// Rule 4 on one chunk, fed one event at a time by the pass of rule 1: every grace chord has its
/// main event. O(1) per event, never allocates.
///
/// # How one pass suffices
/// The chunk is sorted by (offset, slot), and at one offset the `After` graces come first, then
/// the `Before` graces, then the main event. So:
/// - an `After` grace is valid exactly when the **last main event seen so far** ends at its
///   offset (several `After` graces in a row all compare with the same main event);
/// - a `Before` grace stays *pending* until the next event that is not a `Before` grace: it is
///   valid exactly when that event is a main event at the same offset. Any other next event (an
///   `After` grace, necessarily at a later offset, or a main event at a later offset, or the end
///   of the chunk, see [`finish`](Self::finish)) leaves it without its main event.
///
/// Only the first pending `Before` grace is remembered: all pending graces share the outcome of
/// the same next event, and the first one is the one reported.
///
/// # Why fed by rule 1 rather than a pass of its own
/// The `After` rule needs the end of every main event, which rule 1 has just computed; a separate
/// pass recomputed every metric duration and doubled the cost of `validate` (measured: +94% on the
/// 10 000-event score of `benches/voice.rs`).
#[derive(Debug, Default)]
struct GraceCheck {
    /// (ID, offset) of the first `Before` grace still waiting for its main event.
    pending: Option<(EventId, Duration)>,
    /// Where the last main event seen so far ends.
    last_main_end: Option<Duration>,
}

impl GraceCheck {
    /// Takes the next event of the chunk and where it ends (its offset for a grace). O(1).
    fn step(&mut self, placed: &PlacedEvent, end: Duration) -> Result<(), DocumentError> {
        let event = placed.event();
        match event.grace().map(|g| g.attach) {
            Some(GraceAttach::Before) => {
                if let Some((first, offset)) = self.pending
                    && offset != placed.offset()
                {
                    return Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
                        first,
                    )));
                }
                self.pending.get_or_insert((event.id(), placed.offset()));
            }
            Some(GraceAttach::After) => {
                if let Some((first, _)) = self.pending {
                    return Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
                        first,
                    )));
                }
                if self.last_main_end != Some(placed.offset()) {
                    return Err(DocumentError::Invalid(Violation::AfterGraceWithoutEvent(
                        event.id(),
                    )));
                }
            }
            None => {
                if let Some((first, offset)) = self.pending.take()
                    && offset != placed.offset()
                {
                    return Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
                        first,
                    )));
                }
                self.last_main_end = Some(end);
            }
        }
        Ok(())
    }

    /// Ends the chunk: a `Before` grace still pending has no main event. O(1).
    fn finish(self) -> Result<(), DocumentError> {
        match self.pending {
            Some((first, _)) => Err(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
                first,
            ))),
            None => Ok(()),
        }
    }
}
