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
//! 6. **Hairpins** (ADR-0024 §8): each ends strictly after it starts, and two hairpins of the
//!    same scope of one instrument never overlap (the end is exclusive, so touching is fine). A
//!    hairpin of the whole instrument and one of a single voice may overlap: which one a voice
//!    follows is a playback rule (F1.13), like for dynamics.
//! 7. **Tempo marks** (ADR-0024 §11): a gradual change ends strictly after it starts, and no
//!    other tempo change starts strictly inside it, both in score order (`crate::tempo_mark`).
//! 8. **Repeats** (ADR-0025 §2): sections do not nest or overlap; an end repeat without a start
//!    goes back to the start of the score, so only the first end of the score may lack one, apart
//!    from the further ends of the voltas of one section.
//! 9. **Voltas** (ADR-0025 §3): groups of at least two, consecutive, with ascending disjoint
//!    passes that are exactly `1..=n`; every volta but the last ends with an end repeat of its
//!    section whose `times` is 2 or n; no start repeat inside a group.
//! 10. **Markers and jumps** (ADR-0025 §4): targets of the right kind, the "until" marker between
//!     the target and the jump, `OnPass(n)` only inside a repeated section with at most n plays
//!     (`crate::jump`).
//! 11. **Bar repeats and pass rules** (ADR-0025 §6–§7): see `crate::bar_repeat`.
//!
//! `DocumentBuilder::build` calls it, so no invalid document can exist (owner's decision of
//! 2026-11-02); the file loader of F1.15 builds through the builder and gets it for free.

use std::error::Error;
use std::fmt;
use std::hash::BuildHasherDefault;

use orchestra_core_time::{BarIndex, Duration};

use crate::document::{Document, DocumentError, Instrument, Voice, is_whole_bar_rest};
use crate::dynamic::DynamicScope;
use crate::event::{EventBody, Note, PlacedEvent, Tie};
use crate::id::{
    BarRepeatId, EventId, HairpinId, IdMap, InstrumentId, JumpId, NoteId, PassRuleId, RepeatId,
    SlurId, TempoMarkId, VoiceId, VoltaId,
};
use crate::marking::GraceAttach;
use crate::marks::point_key;

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
    /// The end of this hairpin does not come after its start in score order.
    HairpinNotForward(HairpinId),
    /// This hairpin starts before the previous hairpin of the same scope has ended.
    HairpinOverlap(HairpinId),
    /// The end of this gradual tempo change does not come after its start in score order.
    GradualTempoNotForward(TempoMarkId),
    /// This tempo change starts strictly inside a gradual tempo change.
    TempoChangeInsideGradual(TempoMarkId),
    /// This repeat barline nests in or overlaps another repeated section.
    RepeatsOverlap(RepeatId),
    /// This end repeat's number of plays contradicts the voltas of its section.
    RepeatTimesMismatch(RepeatId),
    /// This volta starts before the previous one has ended.
    VoltaOverlap(VoltaId),
    /// This volta does not directly follow the previous volta of its group with higher passes, or
    /// it starts a group without pass 1.
    VoltaOrder(VoltaId),
    /// The passes of the group starting at this volta are not exactly `1..=n`.
    VoltaPassesIncomplete(VoltaId),
    /// This volta lacks the end repeat it needs (every volta but the last of a group has one), or
    /// is a group of its own.
    VoltaWithoutRepeat(VoltaId),
    /// A marker of this jump has the wrong kind, or `then` is missing or extra.
    JumpTargetKind(JumpId),
    /// The "until" marker of this jump is not between its target and the jump.
    JumpUntilOutOfRange(JumpId),
    /// This jump's `OnPass(n)` is outside a repeated section or beyond its plays.
    JumpPassOutOfRange(JumpId),
    /// This bar repeat has fewer bars before it than its span.
    BarRepeatNoSource(BarRepeatId),
    /// This bar repeat overlaps another one on the same staff.
    BarRepeatOverlap(BarRepeatId),
    /// The bars of this bar repeat hold events drawn on its staff.
    BarRepeatNotEmpty(BarRepeatId),
    /// A form marking lies between the source and the end of this bar repeat.
    BarRepeatAcrossForm(BarRepeatId),
    /// This pass rule is not inside one repeated section.
    PassRuleOutsideRepeat(PassRuleId),
    /// The passes of this pass rule go beyond the plays of its section.
    PassRulePasses(PassRuleId),
    /// This pass rule starts before the previous rule of its scope has ended.
    PassRuleOverlap(PassRuleId),
}

impl fmt::Display for Violation {
    /// Writes a short English description of the violation, for developers.
    #[allow(
        clippy::too_many_lines,
        reason = "one short arm per violation: splitting the match would only scatter it"
    )]
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
            Self::HairpinNotForward(id) => write!(
                f,
                "hairpin {} does not end after it starts in score order",
                id.get()
            ),
            Self::HairpinOverlap(id) => write!(
                f,
                "hairpin {} starts before the previous hairpin of its scope ends",
                id.get()
            ),
            Self::GradualTempoNotForward(id) => write!(
                f,
                "gradual tempo change {} does not end after it starts in score order",
                id.get()
            ),
            Self::TempoChangeInsideGradual(id) => write!(
                f,
                "tempo change {} starts inside a gradual tempo change",
                id.get()
            ),
            Self::RepeatsOverlap(id) => write!(
                f,
                "repeat barline {} nests in or overlaps another repeated section",
                id.get()
            ),
            Self::RepeatTimesMismatch(id) => write!(
                f,
                "end repeat {} has a number of plays that contradicts its voltas",
                id.get()
            ),
            Self::VoltaOverlap(id) => {
                write!(f, "volta {} starts before the previous one ends", id.get())
            }
            Self::VoltaOrder(id) => write!(
                f,
                "volta {} does not follow the previous volta of its group with higher passes",
                id.get()
            ),
            Self::VoltaPassesIncomplete(id) => write!(
                f,
                "the voltas from {} do not cover the passes 1 to n exactly",
                id.get()
            ),
            Self::VoltaWithoutRepeat(id) => write!(
                f,
                "volta {} lacks its end repeat or has no other volta",
                id.get()
            ),
            Self::JumpTargetKind(id) => {
                write!(f, "jump {} points at a marker of the wrong kind", id.get())
            }
            Self::JumpUntilOutOfRange(id) => write!(
                f,
                "the until marker of jump {} is not between its target and the jump",
                id.get()
            ),
            Self::JumpPassOutOfRange(id) => write!(
                f,
                "jump {} fires on a pass its section does not have",
                id.get()
            ),
            Self::BarRepeatNoSource(id) => {
                write!(f, "bar repeat {} has no bars to repeat before it", id.get())
            }
            Self::BarRepeatOverlap(id) => write!(
                f,
                "bar repeat {} overlaps another one on its staff",
                id.get()
            ),
            Self::BarRepeatNotEmpty(id) => write!(
                f,
                "the bars of bar repeat {} hold events on its staff",
                id.get()
            ),
            Self::BarRepeatAcrossForm(id) => write!(
                f,
                "a form marking lies inside bar repeat {} and its source",
                id.get()
            ),
            Self::PassRuleOutsideRepeat(id) => write!(
                f,
                "pass rule {} is not inside one repeated section",
                id.get()
            ),
            Self::PassRulePasses(id) => write!(
                f,
                "pass rule {} names passes its section does not have",
                id.get()
            ),
            Self::PassRuleOverlap(id) => write!(
                f,
                "pass rule {} starts before the previous rule of its scope ends",
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
    /// every slur costs two event lookups and two bar starts. Hairpins cost O(h · s) for h
    /// hairpins and s scopes in use in their instrument (one or a few), tempo marks O(t log m). It
    /// allocates one small map from voices to instruments (a few hundred entries at most), sized
    /// up front, and, if there are hairpins, one list of their scopes, reused by every
    /// instrument.
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

        // Rule 6: the end of the last hairpin seen in each scope, reused by every instrument.
        let mut hairpin_ends: Vec<(DynamicScope, (u32, Duration))> = Vec::new();

        for instrument in self.instruments() {
            for voice in instrument.voices() {
                for (index, _) in self.bars().ids().iter().enumerate() {
                    let index = BarIndex::new(
                        u32::try_from(index).map_err(|_| DocumentError::TooManyBars)?,
                    );
                    self.check_chunk(&owner, instrument.id(), voice, index)?;
                }
            }
            // Rule 5, for every slur of the instrument.
            for slur in instrument.slurs() {
                self.check_slur(&owner, instrument, slur.id(), slur.start(), slur.end())?;
            }
            // Rule 6, for every hairpin of the instrument.
            self.check_hairpins(instrument, &mut hairpin_ends)?;
        }
        // Rules 8 and 9 (the form) come before the rules that look sections up.
        self.check_form()?;
        // Rule 10: markers and jumps.
        self.check_jumps()?;
        // Rule 11: bar repeats and pass rules, per instrument.
        for instrument in self.instruments() {
            self.check_bar_repeats_and_pass_rules(instrument)?;
        }
        // Rule 7: the walk that derives the tempo map checks the rules of the marks. The map
        // itself is not rebuilt (the builder derived it when each mark was added), so nothing is
        // allocated.
        self.walk_tempo_marks(|_, _| {})?;
        Ok(())
    }

    /// Checks rules 1, 2 and 4 on the chunk of `voice` for the bar at `index`, and rule 3 for every
    /// tie that starts in it. O(k) for k events in the chunk, plus two event lookups per tie.
    pub(crate) fn check_chunk(
        &self,
        owner: &impl Owner,
        instrument: InstrumentId,
        voice: &Voice,
        index: BarIndex,
    ) -> Result<(), DocumentError> {
        let events = voice.events_at(index);
        // Rules 1, 2 and 4, inside one chunk, in one pass: the duration each event needs for rule
        // 1 is the one rule 4 needs, so it is computed once.
        let mut end_of_previous: Option<Duration> = None;
        let mut graces = GraceCheck::default();
        for placed in events {
            let event = placed.event();
            let bar_length = if is_whole_bar_rest(event) {
                if events.len() > 1 {
                    return Err(DocumentError::Invalid(Violation::WholeBarRestNotAlone(
                        event.id(),
                    )));
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
                        self.check_tie(owner, instrument, event.id(), note, target)?;
                    }
                }
            }
        }
        graces.finish()
    }

    /// Checks the hairpins of one instrument (rule 6). `ends` is scratch space, emptied first.
    ///
    /// # How one pass suffices
    /// The hairpins are sorted by start. Within one scope, they do not overlap exactly when each
    /// starts at or after the end of the previous one of that scope; and since the previous one
    /// then ended before this one starts, this one's end is the latest so far. So one end per
    /// scope is all that needs remembering. O(h · s) for h hairpins and s scopes (the scopes are
    /// few, so a linear search beats a map), plus two O(1) bar lookups per hairpin.
    fn check_hairpins(
        &self,
        instrument: &Instrument,
        ends: &mut Vec<(DynamicScope, (u32, Duration))>,
    ) -> Result<(), DocumentError> {
        ends.clear();
        for hairpin in instrument.hairpins() {
            let start = point_key(self.bars(), hairpin.start());
            let end = point_key(self.bars(), hairpin.end());
            if end <= start {
                return Err(DocumentError::Invalid(Violation::HairpinNotForward(
                    hairpin.id(),
                )));
            }
            let scope = hairpin.placement().scope;
            match ends.iter_mut().find(|(s, _)| *s == scope) {
                Some((_, previous)) if start < *previous => {
                    return Err(DocumentError::Invalid(Violation::HairpinOverlap(
                        hairpin.id(),
                    )));
                }
                Some((_, previous)) => *previous = end,
                None => ends.push((scope, end)),
            }
        }
        Ok(())
    }

    /// Checks one slur (rule 5). Two event lookups + [`Document::comes_before`].
    fn check_slur(
        &self,
        owner: &impl Owner,
        instrument: &Instrument,
        slur: SlurId,
        start: EventId,
        end: EventId,
    ) -> Result<(), DocumentError> {
        for event in [start, end] {
            let found = self
                .event(event)
                .ok_or(DocumentError::Invalid(Violation::SlurEndMissing(slur)))?;
            if owner.instrument_of(found.voice()) != Some(instrument.id()) {
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
        owner: &impl Owner,
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
        if owner.instrument_of(target_event.voice()) != Some(instrument) {
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

impl Document {
    /// The **local check** of the commands (ADR-0026 §1, SCALETTA F1.11b, owner's decision V8 of
    /// 2026-11-04): checks, on the cells (voice, bar) a transaction touched, every rule of
    /// [`validate`](Self::validate) that a change of events can break, without visiting the rest
    /// of the score.
    ///
    /// # What is checked
    /// For each touched cell that still exists:
    /// - rules 1, 2 and 4 on its chunk, and rule 3 for the ties starting in it
    ///   ([`check_chunk`](Self::check_chunk));
    /// - rule 3 for the ties leading **into** its noteheads (the backward tie index), whose source
    ///   may be in an untouched cell;
    /// - rule 5 for the slurs starting or ending on its events (the slur index);
    /// - the "no event under a bar repeat" part of rule 11, for the bar repeats of its
    ///   instrument that cover the bar.
    ///
    /// Plus, for what left the document: a removed event that slurs still name, and a notehead
    /// that is gone while ties still lead into it.
    ///
    /// # Why this is enough for events
    /// Rules 6–11 are about hairpins, tempo marks and the form, which an event command does not
    /// touch; the only link between them and the events is the bar-repeat rule above. The property
    /// tests check it: after every command, the full [`validate`](Self::validate) passes.
    ///
    /// # Complexity
    /// O(k) per touched cell for its k events, plus two event lookups per tie and slur end, plus
    /// O(voices) to find each cell's voice and O(b) for the b bar repeats of its instrument. Never
    /// O(score). Allocates nothing.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with the first violation found.
    pub(crate) fn check_cells(
        &self,
        changes: &crate::change::ChangeSet,
    ) -> Result<(), DocumentError> {
        for &(voice_id, bar) in changes.cells() {
            // A cell whose bar was removed has no events left to check.
            let Some(index) = self.bars().index_of(bar) else {
                continue;
            };
            let Some((instrument, voice)) = self.voice(voice_id) else {
                continue;
            };
            self.check_chunk(self, instrument.id(), voice, index)?;
            for placed in voice.events_at(index) {
                let event = placed.event();
                // Rule 3 for the ties leading into this event's noteheads.
                for note in event
                    .chord()
                    .into_iter()
                    .flat_map(crate::event::Chord::notes)
                {
                    for &source in self.ties_into(note.id()) {
                        // The index only names noteheads that exist (a removal unindexes their
                        // ties), so a missing source is a bug, reported rather than a panic.
                        let (from, from_note) =
                            self.note(source).ok_or(DocumentError::Inconsistent)?;
                        let owner = self
                            .instrument_of(from.voice())
                            .ok_or(DocumentError::Inconsistent)?;
                        self.check_tie(self, owner, from.event().id(), from_note, note.id())?;
                    }
                }
                // Rule 5 for the slurs touching this event.
                for &slur in self.slurs_at(event.id()) {
                    let (owner, found) = self
                        .slur(slur)
                        .ok_or(DocumentError::Invalid(Violation::SlurEndMissing(slur)))?;
                    self.check_slur(self, owner, slur, found.start(), found.end())?;
                }
            }
            // Rule 11: no event drawn on the staff of a bar repeat covering this bar.
            let at = index.get();
            for repeat in instrument.bar_repeats() {
                let first = crate::form::index(self.bars(), repeat.first());
                let covers = first <= at && at < first.saturating_add(repeat.count());
                if covers
                    && voice
                        .events_at(index)
                        .iter()
                        .any(|placed| placed.event().staff() == repeat.staff())
                {
                    return Err(DocumentError::Invalid(Violation::BarRepeatNotEmpty(
                        repeat.id(),
                    )));
                }
            }
        }
        // What left the document: a removed event still named by a slur, a notehead that is gone
        // while a tie still leads into it.
        for removed in changes.removed() {
            if let crate::edit::ObjectRef::Event(id) = *removed
                && self.event(id).is_none()
                && let Some(&slur) = self.slurs_at(id).first()
            {
                return Err(DocumentError::Invalid(Violation::SlurEndMissing(slur)));
            }
        }
        for &note in &changes.notes_gone {
            if self.note(note).is_none()
                && let Some(&source) = self.ties_into(note).first()
            {
                return Err(DocumentError::Invalid(Violation::TieTargetMissing(source)));
            }
        }
        Ok(())
    }
}

/// Which instrument owns a voice: the tie and slur rules compare the instruments of their two ends.
///
/// [`Document::validate`] answers from a map of every voice built once (O(1) per question);
/// the local check of the commands ([`Document::check_cells`]) answers by searching the document
/// (O(voices) per question), because building the map would cost more than the few questions a
/// command asks.
pub(crate) trait Owner {
    /// The instrument owning `voice`, if the document has that voice.
    fn instrument_of(&self, voice: VoiceId) -> Option<InstrumentId>;
}

impl Owner for IdMap<VoiceId, InstrumentId> {
    /// One hash lookup, O(1) on average.
    fn instrument_of(&self, voice: VoiceId) -> Option<InstrumentId> {
        self.get(&voice).copied()
    }
}

impl Owner for Document {
    /// A search of the tracks, O(voices).
    fn instrument_of(&self, voice: VoiceId) -> Option<InstrumentId> {
        self.voice(voice).map(|(instrument, _)| instrument.id())
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
