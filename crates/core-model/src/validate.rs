// SPDX-License-Identifier: GPL-3.0-or-later

//! The rules of a valid document that need the whole document to check (ADR-0024 §1, §2, §4;
//! SCALETTA F1.09b).
//!
//! # Which rules live here, and which elsewhere
//! The builder (and later the commands of F1.11) checks every rule it can see **locally** before
//! creating anything: the bar, voice, staff and tuplet exist and belong together, offsets are not
//! negative, tuplets nest at most four deep, a whole-bar rest is not in a tuplet. What needs more
//! than one object is checked here, once, by [`Document::validate`]:
//!
//! 1. **No overlap within a voice and bar** (ADR-0024 §1): each event of a chunk starts at or
//!    after the end of the previous one. Gaps are fine. The check is per bar, not across bars,
//!    because an **overfull** bar (ADR-0022 §4) is valid: its last events reach past the barline
//!    into the time of the next bar, and the score shows it as an error for the user to fix.
//! 2. **A whole-bar rest is alone** in its voice and bar (ADR-0024 §2).
//! 3. **Ties** (ADR-0024 §4): the target notehead exists, belongs to the same instrument, sounds
//!    at the same pitch, and starts strictly later than the starting notehead.
//!
//! `DocumentBuilder::build` calls it, so no invalid document can exist (owner's decision of
//! 2026-11-02); the file loader of F1.15 builds through the builder and gets it for free.

use std::error::Error;
use std::fmt;
use std::hash::BuildHasherDefault;

use orchestra_core_time::{BarIndex, Duration, Position};

use crate::document::{Document, DocumentError, is_whole_bar_rest};
use crate::event::{EventBody, Note, Tie};
use crate::id::{EventId, IdMap, InstrumentId, NoteId, VoiceId};

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
        }
    }
}

impl Error for Violation {}

impl Document {
    /// Checks the rules of a valid document that need the whole document (see the module
    /// documentation of `validate`). `DocumentBuilder::build` calls it.
    ///
    /// # Complexity
    /// O(n log m) for n events and m meter changes: every event is visited once, and every tie
    /// costs two event lookups and two bar starts. It allocates one small map from voices to
    /// instruments (a few hundred entries at most), sized up front: one allocation.
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
                    // Rules 1 and 2, inside one chunk.
                    let mut end_of_previous: Option<Duration> = None;
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
                        end_of_previous = Some(placed.offset().checked_add(duration)?);

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
                }
            }
        }
        Ok(())
    }

    /// Checks one tie (rule 3). O(event lookup) twice + O(log m) twice.
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
        let start: Position = self.event_position(event)?;
        let target_start: Position = self.event_position(target_event.event().id())?;
        if target_start <= start {
            return Err(DocumentError::Invalid(Violation::TieNotForward(note.id())));
        }
        Ok(())
    }
}
