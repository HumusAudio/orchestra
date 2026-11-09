// SPDX-License-Identifier: GPL-3.0-or-later

//! [`DocumentError`]: why building, querying or editing the document failed. A sub-module of
//! `document` only to keep that file readable; everything here is re-exported by it.

use std::error::Error;
use std::fmt;
use std::hash::Hash;
use std::panic::Location;

use orchestra_core_time::{MeterMapError, RationalError, TempoMapError};

use crate::edit::ObjectRef;
use crate::event::{MAX_TUPLET_DEPTH, NoteValueError};
use crate::form::MAX_PLAYS;
use crate::id::{
    BarId, BarRepeatId, EventId, InstrumentId, MarkerId, NoteId, SlurId, StaffId, TrackId,
    TupletId, VoiceId,
};
use crate::pitch::PitchError;
use crate::validate::Violation;

use super::{MAX_STAVES_PER_INSTRUMENT, MAX_VOICES_PER_INSTRUMENT};

/// Why building or querying the document failed.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentError {
    /// The document's ID counter has handed out every `u64` value (ADR-0022 §1). Unreachable in
    /// practice; reported rather than wrapping around, which would reuse IDs.
    IdsExhausted,
    /// An instrument was requested with no staves: every instrument has at least one.
    NoStaves,
    /// An instrument was requested with more than [`MAX_STAVES_PER_INSTRUMENT`] staves.
    TooManyStaves,
    /// A voice would be the instrument's voice number [`MAX_VOICES_PER_INSTRUMENT`] + 1.
    TooManyVoices,
    /// The document already has the most bars a [`BarIndex`] can address (2³² − 1, the last index
    /// being 2³² − 2 so that the count itself fits in a `u32`).
    TooManyBars,
    /// The document already has [`MAX_TRACKS`] tracks.
    TooManyTracks,
    /// A saved ID is 0, which is never an ID (loading a saved document, ADR-0029 S4).
    ZeroId,
    /// This raw ID names two objects of a saved document: IDs are unique across every kind of
    /// object (ADR-0022 §1).
    DuplicateId(u64),
    /// This raw ID of a saved document was never handed out by its counter: it is not below the
    /// counter's next value, so a later object would receive it again (ADR-0022 §1).
    IdNotIssued(u64),
    /// A saved document lists its objects in an order the document would not keep (a chunk's
    /// events, a chord's noteheads, a list of marks): loading never reorders silently, so that a
    /// re-save gives back the same file (ADR-0029 S3, S8).
    NotCanonical,
    /// No staff of the document has this ID.
    UnknownStaff(StaffId),
    /// No bar of the document has this ID.
    UnknownBar(BarId),
    /// No voice of the document has this ID.
    UnknownVoice(VoiceId),
    /// No instrument of the document has this ID.
    UnknownInstrument(InstrumentId),
    /// No event of the document has this ID.
    UnknownEvent(EventId),
    /// No notehead of the document has this ID.
    UnknownNote(NoteId),
    /// The voice has no tuplet with this ID.
    UnknownTuplet(TupletId),
    /// No slur of the document has this ID.
    UnknownSlur(SlurId),
    /// An event's staff belongs to another instrument than its voice (ADR-0022 §2), or a
    /// dynamic's or hairpin's staff to another instrument than the dynamic.
    StaffOfAnotherInstrument(StaffId),
    /// The voice of a dynamic's or hairpin's scope belongs to another instrument (ADR-0024 §8).
    VoiceOfAnotherInstrument(VoiceId),
    /// An offset before the start of its bar: offsets are ≥ 0 (ADR-0022 §4).
    NegativeOffset,
    /// A chord was requested without any notehead.
    EmptyChord,
    /// A whole-bar rest was put in a tuplet (ADR-0024 §2).
    WholeBarRestInTuplet,
    /// A grace chord was put in a tuplet (ADR-0024 §5: graces are never tuplet members).
    GraceInTuplet,
    /// A marking that only a chord can carry (articulations, ornament, arpeggio) was set on a
    /// rest.
    NotAChord(EventId),
    /// A tuplet would be nested deeper than [`MAX_TUPLET_DEPTH`] (ADR-0024 C3).
    TupletTooDeep,
    /// A notehead was tied to itself.
    TieToItself,
    /// A slur was requested from an event to the same event.
    SlurToItself,
    /// A clef, key or tempo change was put where one already is (or where the initial clef, key
    /// or tempo is), or a dynamic where one of the same scope already is.
    DuplicateMark,
    /// A free text (a dynamic, a tempo mark) is empty or only white space.
    EmptyText,
    /// A tempo mark has neither text nor tempo change: it would print and play nothing.
    EmptyTempoMark,
    /// The initial tempo mark does not set a tempo at once: some tempo must be in force from the
    /// very start (ADR-0024 §11, E1).
    InitialTempoNotImmediate,
    /// A hairpin does not end after it starts in score order.
    HairpinNotForward,
    /// A pass number outside 1–[`MAX_PLAYS`] (ADR-0025 F10).
    PassOutOfRange(u8),
    /// A repeat's number of plays outside 2–[`MAX_PLAYS`] (ADR-0025 §2).
    PlaysOutOfRange(u8),
    /// A volta without any pass.
    EmptyPassSet,
    /// A volta whose last bar comes before its first.
    VoltaNotForward,
    /// No marker of the document has this ID.
    UnknownMarker(MarkerId),
    /// A bar repeat whose count is not a positive multiple of its span.
    BarRepeatCount,
    /// A pass rule that does not end after it starts in score order.
    PassRuleNotForward,
    /// A clef on a line outside 1–5 or with an octave shift outside ±2.
    ClefOutOfRange,
    /// A note value or a tuplet could not be built.
    NoteValue(NoteValueError),
    /// A pitch operation failed (sounding pitch, transposed key).
    Pitch(PitchError),
    /// Rational arithmetic overflowed (impossible within the bounds of ADR-0024, checked anyway).
    Arithmetic(RationalError),
    /// A meter map lookup failed (only reachable with absurd meters, ADR-0018).
    MeterMap(MeterMapError),
    /// The tempo map derived from the tempo marks could not be built (absurd tempi only, once the
    /// rules of the marks hold: ADR-0024 §11).
    TempoMap(TempoMapError),
    /// The document breaks a rule that needs the whole document to check ([`Document::validate`]).
    Invalid(Violation),
    /// An edit inserts an object whose ID is already in use, or was not handed out by the
    /// document's counter (ADR-0026 §1). Also reported for a notehead ID of an event.
    DuplicateObject(ObjectRef),
    /// An edit names an object the document does not have.
    UnknownObject(ObjectRef),
    /// An edit tries to remove what cannot be removed this way: a bar (removed with
    /// `RemoveBars`) or a value the document always has (the initial tempo, key, clefs, the
    /// numbering style), which can only be set.
    NotRemovable(ObjectRef),
    /// A command that changes a mark named an object of one kind and gave the new content of
    /// another (a dynamic's ID with a hairpin's content): a mark keeps its kind for its whole
    /// life, so a change of kind is a removal and an addition (ADR-0026 X5).
    MarkKindMismatch(ObjectRef),
    /// A command that works on marks was given an object that is not one: an event, a tuplet, a
    /// bar or a value the document has once (each has its own command).
    NotAMark(ObjectRef),
    /// A tuplet still used by an event or by a nested tuplet cannot be removed.
    TupletInUse(TupletId),
    /// An event's exact place in its chunk would break the chunk's order (only an undo gives a
    /// place, and it always fits).
    PlaceOutOfOrder,
    /// The meter map data given with new bars covers another number of bars.
    BarSpanMismatch,
    /// Bars to be removed still hold an event or carry something anchored on them; the bar named
    /// is the first one found.
    BarsNotEmpty(BarId),
    /// A command enters an event that would reach past the end of the last bar (ADR-0026 W3): the
    /// bars must be added first, in the same transaction if need be.
    PastLastBar,
    /// A command enters an event starting at or after the end of its bar (at most at the end of
    /// a free bar, which it then lengthens).
    StartsPastBarline,
    /// A length that no sum of written note values can express (a remainder of a third of a
    /// beat, after a tuplet, for example): the tie chain of a note across a barline cannot be
    /// written.
    NotWritable,
    /// A command inserts bars between the source and the end of this bar repeat (ADR-0026 W6).
    InsideBarRepeat(BarRepeatId),
    /// A command would delete every bar: at least one bar remains (ADR-0026 §6).
    NoBarsLeft,
    /// No track has this ID.
    UnknownTrack(TrackId),
    /// A tolerance threshold of zero or less (ADR-0028 §1): it must be strictly positive.
    ToleranceNotPositive,
    /// An attempt to remove the project's tolerance threshold: only a track's override can be
    /// removed (ADR-0028 §1).
    ProjectToleranceRequired,
    /// A DAW move with a grid step of zero or less (ADR-0028 §3, Q4): there is no "no grid".
    GridNotPositive,
    /// A DAW move beyond the threshold would make a piece of the note cross a barline (ADR-0028
    /// §3, ADR-0028 Q6): a DAW move never makes a bar overfull.
    MoveCrossesBarline,
    /// A DAW move beyond the threshold of a grace note (ADR-0028 §3, Q9): grace notes are moved
    /// only by offsets, their place belongs to their main event.
    MoveOfGraceNote,
    /// A DAW move beyond the threshold of a notehead that a tie leads into (ADR-0028 §10, T1):
    /// moving it alone could leave the tie pointing backwards.
    MoveOfTiedNote,
    /// A DAW move or resize beyond the threshold of a tuplet note (ADR-0028 §10, T2; §11, U6): on
    /// the DAW grid it would leave its group.
    MoveOfTupletNote,
    /// A DAW resize whose scale of the playback rules is zero or negative (ADR-0028 §11, U1).
    ScaleNotPositive,
    /// A position or a derived index disagreed with the content: a bug of the model, reported
    /// instead of panicking. Never expected. Build it with [`DocumentError::inconsistent`].
    Inconsistent {
        /// Where in the source code the inconsistency was detected (file, line and column), so
        /// that a bug report names the broken check instead of a bare "inconsistent". Filled in
        /// by the compiler through `#[track_caller]`: no allocation, a pointer to static data.
        at: &'static Location<'static>,
    },
}

impl DocumentError {
    /// The [`DocumentError::Inconsistent`] error, located at the **caller's** line
    /// (`#[track_caller]`): write `.ok_or(DocumentError::inconsistent())` at the check itself.
    ///
    /// Pass it as a value (`ok_or`), not as a function (`ok_or_else(DocumentError::inconsistent)`):
    /// called through a function pointer, the location would be the standard library's, not the
    /// check's. Building it eagerly costs nothing measurable: it is one pointer to static data.
    ///
    /// # Complexity
    /// O(1).
    #[track_caller]
    #[must_use]
    pub const fn inconsistent() -> Self {
        Self::Inconsistent {
            at: Location::caller(),
        }
    }
}

impl fmt::Display for DocumentError {
    /// Writes a short English description of the error, for developers (see the type's docs).
    #[allow(
        clippy::too_many_lines,
        reason = "one short arm per error: splitting the match would only scatter it"
    )]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdsExhausted => f.write_str("the document's ID counter is exhausted"),
            Self::NoStaves => f.write_str("an instrument needs at least one staff"),
            Self::TooManyStaves => write!(
                f,
                "an instrument can have at most {MAX_STAVES_PER_INSTRUMENT} staves"
            ),
            Self::TooManyVoices => write!(
                f,
                "an instrument can have at most {MAX_VOICES_PER_INSTRUMENT} voices"
            ),
            Self::TooManyBars => f.write_str("the document has the most bars it can address"),
            Self::TooManyTracks => f.write_str("the document has the most tracks it can hold"),
            Self::ZeroId => f.write_str("a saved ID is 0, which is never an ID"),
            Self::DuplicateId(raw) => write!(f, "the saved ID {raw} names two objects"),
            Self::IdNotIssued(raw) => write!(
                f,
                "the saved ID {raw} was never handed out by the document's counter"
            ),
            Self::NotCanonical => f.write_str(
                "the saved document lists its objects in an order the document would not keep",
            ),
            Self::UnknownStaff(id) => write!(f, "no staff has the ID {}", id.get()),
            Self::UnknownBar(id) => write!(f, "no bar has the ID {}", id.get()),
            Self::UnknownVoice(id) => write!(f, "no voice has the ID {}", id.get()),
            Self::UnknownInstrument(id) => write!(f, "no instrument has the ID {}", id.get()),
            Self::UnknownEvent(id) => write!(f, "no event has the ID {}", id.get()),
            Self::UnknownNote(id) => write!(f, "no notehead has the ID {}", id.get()),
            Self::UnknownTuplet(id) => {
                write!(f, "the voice has no tuplet with the ID {}", id.get())
            }
            Self::UnknownSlur(id) => write!(f, "no slur has the ID {}", id.get()),
            Self::StaffOfAnotherInstrument(id) => write!(
                f,
                "the staff {} belongs to another instrument than the voice",
                id.get()
            ),
            Self::VoiceOfAnotherInstrument(id) => write!(
                f,
                "the voice {} belongs to another instrument than the dynamic",
                id.get()
            ),
            Self::NegativeOffset => f.write_str("an offset in a bar cannot be negative"),
            Self::EmptyChord => f.write_str("a chord needs at least one notehead"),
            Self::WholeBarRestInTuplet => f.write_str("a whole-bar rest cannot be in a tuplet"),
            Self::GraceInTuplet => f.write_str("a grace chord cannot be in a tuplet"),
            Self::NotAChord(id) => write!(
                f,
                "event {} is a rest: only a chord carries articulations, ornaments and arpeggios",
                id.get()
            ),
            Self::TupletTooDeep => {
                write!(f, "tuplets can be nested at most {MAX_TUPLET_DEPTH} deep")
            }
            Self::TieToItself => f.write_str("a notehead cannot be tied to itself"),
            Self::SlurToItself => f.write_str("a slur cannot start and end on the same event"),
            Self::DuplicateMark => f.write_str("there is already a change at that point"),
            Self::EmptyText => f.write_str("a text cannot be empty"),
            Self::EmptyTempoMark => f.write_str("a tempo mark needs a text or a tempo change"),
            Self::InitialTempoNotImmediate => {
                f.write_str("the initial tempo mark must set a tempo at once")
            }
            Self::HairpinNotForward => {
                f.write_str("a hairpin must end after it starts in score order")
            }
            Self::PassOutOfRange(p) => write!(f, "pass {p} is outside 1 to {MAX_PLAYS}"),
            Self::PlaysOutOfRange(n) => {
                write!(f, "a section is played 2 to {MAX_PLAYS} times, not {n}")
            }
            Self::EmptyPassSet => f.write_str("a volta needs at least one pass"),
            Self::VoltaNotForward => f.write_str("a volta's last bar cannot come before its first"),
            Self::UnknownMarker(id) => write!(f, "no marker has the ID {}", id.get()),
            Self::BarRepeatCount => {
                f.write_str("a bar repeat covers a positive multiple of its span of bars")
            }
            Self::PassRuleNotForward => {
                f.write_str("a pass rule must end after it starts in score order")
            }
            Self::ClefOutOfRange => {
                f.write_str("a clef sits on lines 1 to 5 with an octave shift of at most 2")
            }
            Self::NoteValue(e) => write!(f, "note value: {e}"),
            Self::Pitch(e) => write!(f, "pitch: {e}"),
            Self::Arithmetic(e) => write!(f, "arithmetic: {e}"),
            Self::MeterMap(e) => write!(f, "meter map: {e}"),
            Self::TempoMap(e) => write!(f, "tempo map: {e}"),
            Self::Invalid(v) => write!(f, "invalid document: {v}"),
            Self::DuplicateObject(o) => write!(f, "{o:?} is already in use or was never issued"),
            Self::UnknownObject(o) => write!(f, "the document has no {o:?}"),
            Self::NotRemovable(o) => write!(f, "{o:?} cannot be removed, only set or moved"),
            Self::MarkKindMismatch(o) => {
                write!(f, "{o:?} cannot become a mark of another kind")
            }
            Self::NotAMark(o) => write!(f, "{o:?} is not a mark: it has commands of its own"),
            Self::TupletInUse(id) => write!(f, "tuplet {} is still in use", id.get()),
            Self::PlaceOutOfOrder => f.write_str("that place would break the order of the chunk"),
            Self::BarSpanMismatch => {
                f.write_str("the meter data of the new bars covers another number of bars")
            }
            Self::BarsNotEmpty(id) => {
                write!(
                    f,
                    "bar {} still holds something and cannot be removed",
                    id.get()
                )
            }
            Self::PastLastBar => f.write_str("the event would reach past the last bar"),
            Self::StartsPastBarline => f.write_str("the event would start after its bar ends"),
            Self::NotWritable => f.write_str("no written note values add up to that length"),
            Self::InsideBarRepeat(id) => write!(
                f,
                "bars cannot be inserted inside bar repeat {} and its source",
                id.get()
            ),
            Self::NoBarsLeft => f.write_str("at least one bar must remain"),
            Self::UnknownTrack(id) => write!(f, "no track has the ID {}", id.get()),
            Self::ToleranceNotPositive => {
                f.write_str("a tolerance threshold must be strictly positive")
            }
            Self::ProjectToleranceRequired => {
                f.write_str("the project's tolerance threshold cannot be removed")
            }
            Self::GridNotPositive => f.write_str("the grid step of a move must be positive"),
            Self::MoveCrossesBarline => {
                f.write_str("the moved note would cross a barline (a DAW move never splits it)")
            }
            Self::MoveOfGraceNote => {
                f.write_str("a grace note is moved only by a performance offset")
            }
            Self::MoveOfTiedNote => {
                f.write_str("a notehead that a tie leads into cannot be moved alone")
            }
            Self::MoveOfTupletNote => {
                f.write_str("a tuplet note is moved or resized only by a performance offset")
            }
            Self::ScaleNotPositive => {
                f.write_str("the scale of the playback rules must be strictly positive")
            }
            Self::Inconsistent { at } => write!(
                f,
                "the document is inconsistent (a bug of the model, detected at {}:{})",
                at.file(),
                at.line()
            ),
        }
    }
}

impl Error for DocumentError {
    /// The underlying error, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NoteValue(e) => Some(e),
            Self::Pitch(e) => Some(e),
            Self::Arithmetic(e) => Some(e),
            Self::MeterMap(e) => Some(e),
            Self::TempoMap(e) => Some(e),
            Self::Invalid(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MeterMapError> for DocumentError {
    /// Wraps a meter map error, so that `?` works on `core-time` lookups.
    fn from(e: MeterMapError) -> Self {
        Self::MeterMap(e)
    }
}

impl From<RationalError> for DocumentError {
    /// Wraps an arithmetic error, so that `?` works on rational operations.
    fn from(e: RationalError) -> Self {
        Self::Arithmetic(e)
    }
}

impl From<PitchError> for DocumentError {
    /// Wraps a pitch error, so that `?` works on pitch operations.
    fn from(e: PitchError) -> Self {
        Self::Pitch(e)
    }
}

impl From<NoteValueError> for DocumentError {
    /// Wraps a note value error, so that `?` works on note value and tuplet constructors.
    fn from(e: NoteValueError) -> Self {
        Self::NoteValue(e)
    }
}
