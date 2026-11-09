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

use std::error::Error;
use std::fmt;
use std::hash::Hash;
use std::num::NonZeroU64;
use std::panic::Location;

use orchestra_core_time::{
    BarIndex, Duration, MeterMap, MeterMapError, Position, RationalError, TempoMap, TempoMapError,
};

use crate::bar_repeat::{BarRepeat, BarRepeatSpan, PassRule};
use crate::change::ChangeSet;
use crate::command::chord_from;
use crate::content::{Chunk, VoiceContent, metric_duration};
use crate::dynamic::{Dynamic, DynamicMark, DynamicPlacement, DynamicScope, Hairpin, HairpinShape};
use crate::edit::{Object, ObjectRef};
use crate::event::{
    Chord, Event, EventBody, MAX_TUPLET_DEPTH, Note, NoteValue, NoteValueError, PlacedEvent, Rest,
    RestValue, Tie, Tuplet,
};
use crate::form::{BarNumbering, Form, MAX_PLAYS, PassSet, RepeatEnd, RepeatStart, Volta};
use crate::id::{
    BarId, BarRepeatId, ClefId, DynamicId, EventId, HairpinId, IdCounter, IdMap, IdRefs,
    InstrumentId, JumpId, KeyChangeId, MarkerId, NoteId, PassRuleId, RepeatId, SlurId, StaffId,
    TempoMarkId, TrackId, TupletId, VoiceId, VoltaId,
};
use crate::jump::{Jump, JumpSpec, Marker, MarkerKind};
use crate::mark_command::check_text;
use crate::marking::{Arpeggio, ArticulationSet, Fermata, Grace, Ornament, PerformanceOffsets};
use crate::marks::{Clef, KeyOverride, KeySignature, Mark, MarkList, ScorePoint};
use crate::pass_offsets::PassOffsets;
use crate::pitch::{Interval, Key, Pitch, PitchError};
use crate::slur::{Slur, SlurPlacement, SlurRefs};
use crate::tempo_mark::{TempoMark, TempoMarkChange};
use crate::tolerance::{Tolerance, ToleranceSettings};
use crate::validate::Violation;

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

// ------------------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------------------

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

// ------------------------------------------------------------------------------------------------
// Staves, voices, instruments, tracks
// ------------------------------------------------------------------------------------------------

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
    fn new() -> Self {
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
    pub fn index_of(&self, id: BarId) -> Option<BarIndex> {
        self.index.get(&id).copied().map(BarIndex::new)
    }

    /// Appends a bar at the end. O(1) amortised: the new bar's index is the old length, and no
    /// other bar moves, so the inverse index only gains one entry.
    ///
    /// # Errors
    /// [`DocumentError::TooManyBars`] when the new index would not fit a [`BarIndex`] with the
    /// count still fitting a `u32` (so at most 2³² − 1 bars).
    fn push(&mut self, id: BarId) -> Result<(), DocumentError> {
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

// ------------------------------------------------------------------------------------------------
// The builder
// ------------------------------------------------------------------------------------------------

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
    use crate::pitch::Mode;
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
