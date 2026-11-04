// SPDX-License-Identifier: GPL-3.0-or-later

//! What a voice is made of: events (chords and rests), their written note values, tuplets and
//! ties (ADR-0024 §1–§4, SCALETTA F1.09b), with the markings written on them (ADR-0024 §5, §6,
//! §10; SCALETTA F1.09c).
//!
//! # What lives here
//! - [`BaseValue`] and [`NoteValue`]: the written form of a duration (a quarter, a dotted eighth).
//!   It is **the only stored form**: the metric duration in whole notes is always derived from it
//!   (ADR-0024 §2), so the figure and the number can never disagree.
//! - [`RestValue`]: a rest with a note value, or a **whole-bar rest** that lasts whatever its bar
//!   lasts.
//! - [`Tuplet`] and [`TupletDisplay`]: "`actual` notes of `unit` in the time of `normal`". Tuplets
//!   live in a table of their voice; every event records the innermost tuplet that contains it
//!   (ADR-0024 §3, choice C2).
//! - [`Tie`]: a forward link from a notehead to the notehead it is tied to (ADR-0024 §4).
//! - [`Note`], [`Chord`], [`Rest`], [`EventBody`], [`Event`], [`PlacedEvent`]: the events of a
//!   voice and their offset in the bar (ADR-0022 §4).
//!
//! The values written on these events live in `crate::marking` and are fields here: a chord may
//! be a grace chord ([`Grace`]) and carries its [`ArticulationSet`], its [`Ornament`], its
//! [`Fermata`] and its [`Arpeggio`]; a rest may carry a fermata; every notehead carries its
//! [`PerformanceOffsets`] inline (ADR-0024 C7).
//!
//! Everything here is a plain value: no type in this module knows the document. The document
//! (`crate::document`) creates the events with their IDs and keeps the indices; the voice
//! (`crate::content`) keeps the tuplet table and computes metric durations.

use std::num::NonZeroU8;

use orchestra_core_time::{Duration, Rational, RationalError};

use crate::id::{EventId, NoteId, StaffId, TupletId};
use crate::marking::{Arpeggio, ArticulationSet, Fermata, Grace, Ornament, PerformanceOffsets};
use crate::pitch::Pitch;

/// The most augmentation dots a note value can have (ADR-0024 §2). Four dots already appear only
/// in a handful of real scores; the bound keeps every derived length small (§ [`NoteValue`]).
pub const MAX_DOTS: u8 = 4;

/// The deepest tuplet nesting (ADR-0024 §3, choice C3): a tuplet inside a tuplet inside a tuplet
/// inside a tuplet. With terms of at most 255 the product of four ratios has a numerator and a
/// denominator below 255⁴ ≈ 4.2 × 10⁹, and the metric duration of any member stays far inside an
/// `i64` fraction. Raising it is a constant and a test, not a file format change.
pub const MAX_TUPLET_DEPTH: usize = 4;

// ------------------------------------------------------------------------------------------------
// Note values
// ------------------------------------------------------------------------------------------------

/// The undotted written value of a note or a rest, from the maxima (8 whole notes) to the 1024th
/// note (ADR-0024 §2).
///
/// The names are the American ones (en-US is the source language of the code); the interface
/// shows the localised names (crotchet, semiminima, noire, Viertel, negra…) from the glossary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BaseValue {
    /// Maxima: 8 whole notes (mensural notation, kept because MusicXML has it).
    Maxima,
    /// Longa: 4 whole notes.
    Longa,
    /// Breve (double whole note): 2 whole notes.
    Breve,
    /// Whole note (semibreve): the unit of every `Position` and `Duration` (ADR-0018).
    Whole,
    /// Half note (minim): 1/2.
    Half,
    /// Quarter note (crotchet): 1/4.
    Quarter,
    /// Eighth note (quaver): 1/8.
    Eighth,
    /// Sixteenth note: 1/16.
    Sixteenth,
    /// Thirty-second note: 1/32.
    ThirtySecond,
    /// Sixty-fourth note: 1/64.
    SixtyFourth,
    /// 128th note: 1/128.
    OneHundredTwentyEighth,
    /// 256th note: 1/256.
    TwoHundredFiftySixth,
    /// 512th note: 1/512.
    FiveHundredTwelfth,
    /// 1024th note: 1/1024, the shortest value MusicXML can name.
    OneThousandTwentyFourth,
}

impl BaseValue {
    /// Every base value, longest first: handy for tests and for the interface's value palette.
    pub const ALL: [Self; 14] = [
        Self::Maxima,
        Self::Longa,
        Self::Breve,
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::SixtyFourth,
        Self::OneHundredTwentyEighth,
        Self::TwoHundredFiftySixth,
        Self::FiveHundredTwelfth,
        Self::OneThousandTwentyFourth,
    ];

    /// The base-2 logarithm of the value's **denominator** in whole notes: 0 for the whole note,
    /// 2 for the quarter, −3 for the maxima (its length is 2³ = 8 wholes). O(1).
    ///
    /// The length of the value is `2^(−exponent)` whole notes; keeping it as an exponent lets
    /// [`NoteValue::length`] build the exact fraction with shifts, without any multiplication that
    /// could overflow.
    #[must_use]
    pub const fn exponent(self) -> i8 {
        match self {
            Self::Maxima => -3,
            Self::Longa => -2,
            Self::Breve => -1,
            Self::Whole => 0,
            Self::Half => 1,
            Self::Quarter => 2,
            Self::Eighth => 3,
            Self::Sixteenth => 4,
            Self::ThirtySecond => 5,
            Self::SixtyFourth => 6,
            Self::OneHundredTwentyEighth => 7,
            Self::TwoHundredFiftySixth => 8,
            Self::FiveHundredTwelfth => 9,
            Self::OneThousandTwentyFourth => 10,
        }
    }
}

/// A written note value: a base value and 0 to [`MAX_DOTS`] augmentation dots (ADR-0024 §2).
///
/// This is what the score shows and what the file stores. The **length** it stands for (before
/// any tuplet) is derived by [`length`](Self::length); the metric duration inside tuplets by the
/// voice. Storing a number next to the figure would be two copies of one fact (ADR-0003).
///
/// # Example
/// ```
/// use orchestra_core_model::{BaseValue, NoteValue};
/// use orchestra_core_time::{Duration, Rational};
///
/// // A dotted quarter lasts 3/8 of a whole note.
/// let dotted_quarter = NoteValue::new(BaseValue::Quarter, 1)?;
/// assert_eq!(dotted_quarter.length()?, Duration::from_rational(Rational::new(3, 8)?));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteValue {
    /// The undotted value.
    base: BaseValue,
    /// The number of dots, 0..=[`MAX_DOTS`] by construction.
    dots: u8,
}

impl NoteValue {
    /// A value with `dots` augmentation dots. O(1).
    ///
    /// # Errors
    /// [`NoteValueError::TooManyDots`] above [`MAX_DOTS`].
    pub const fn new(base: BaseValue, dots: u8) -> Result<Self, NoteValueError> {
        if dots > MAX_DOTS {
            return Err(NoteValueError::TooManyDots);
        }
        Ok(Self { base, dots })
    }

    /// An undotted value. O(1), infallible.
    #[must_use]
    pub const fn plain(base: BaseValue) -> Self {
        Self { base, dots: 0 }
    }

    /// The undotted value. O(1).
    #[must_use]
    pub const fn base(self) -> BaseValue {
        self.base
    }

    /// The number of dots. O(1).
    #[must_use]
    pub const fn dots(self) -> u8 {
        self.dots
    }

    /// The length this value stands for outside any tuplet, in whole notes. O(1).
    ///
    /// # Formula
    /// A base value of `2^(−e)` wholes with `d` dots lasts `2^(−e) × (2 − 2^(−d))`, which is
    /// `(2^(d+1) − 1) / 2^d` times the base: a dot adds half, a second dot a quarter, and so on.
    /// The fraction is built from shifts: the numerator is at most `(2⁵ − 1) × 2³ = 248` (a maxima
    /// with four dots) and the denominator at most `2⁴ × 2¹⁰ = 16 384` (a 1024th with four
    /// dots), so nothing can overflow; [`Rational::new`] reduces it to lowest terms.
    ///
    /// # Errors
    /// Never in practice (the bounds above); the `Result` is [`Rational::new`]'s, kept rather than
    /// unwrapped because library code never panics (CLAUDE.md §4).
    pub const fn length(self) -> Result<Duration, RationalError> {
        let exponent = self.base.exponent();
        // (2^(d+1) − 1): 1 for no dot, 3 for one dot, 7 for two…, 31 for four.
        let dots_numerator = (1_i64 << (self.dots + 1)) - 1;
        // 2^d: the denominator the dots bring.
        let dots_denominator = 1_i64 << self.dots;
        // A negative exponent (maxima, longa, breve) multiplies the numerator, a positive one the
        // denominator. `unsigned_abs` of an `i8` in −3..=10 is at most 10: the shifts are tiny.
        let (numerator, denominator) = if exponent < 0 {
            (dots_numerator << exponent.unsigned_abs(), dots_denominator)
        } else {
            (dots_numerator, dots_denominator << exponent.unsigned_abs())
        };
        match Rational::new(numerator, denominator) {
            Ok(r) => Ok(Duration::from_rational(r)),
            Err(e) => Err(e),
        }
    }
}

/// Why a note value, a tuplet or a clef could not be built.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteValueError {
    /// More than [`MAX_DOTS`] dots.
    TooManyDots,
    /// A tuplet term (`actual` or `normal`) of zero.
    ZeroTupletTerm,
}

impl std::fmt::Display for NoteValueError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyDots => write!(f, "a note value has at most {MAX_DOTS} dots"),
            Self::ZeroTupletTerm => f.write_str("a tuplet's terms must be between 1 and 255"),
        }
    }
}

impl std::error::Error for NoteValueError {}

/// The written value of a rest (ADR-0024 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RestValue {
    /// A rest of this note value.
    Value(NoteValue),
    /// A **whole-bar rest**: drawn as a whole rest centred in the bar, lasting exactly its bar's
    /// length from the meter map, whatever that is. It stays right when the meter changes and in
    /// a pickup bar, the ambiguity MusicXML readers disagree on (w3c/musicxml#478). It is valid
    /// only alone in its voice and bar, and never in a tuplet.
    WholeBar,
}

// ------------------------------------------------------------------------------------------------
// Tuplets
// ------------------------------------------------------------------------------------------------

/// Whether a tuplet's bracket is drawn (ADR-0024 §3). Engraving decides `Auto` (F2B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TupletBracket {
    /// The engraver decides (usually: no bracket when the notes are beamed together).
    #[default]
    Auto,
    /// Always drawn.
    Shown,
    /// Never drawn.
    Hidden,
}

/// What a tuplet's number says (ADR-0024 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TupletNumber {
    /// Only the actual number: "3".
    #[default]
    Actual,
    /// The ratio: "3:2", and with the unit when the engraver needs it ("3:2♪").
    Ratio,
    /// No number.
    Hidden,
}

/// How a tuplet is drawn: display only, it never changes a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TupletDisplay {
    /// The bracket.
    pub bracket: TupletBracket,
    /// The number.
    pub number: TupletNumber,
}

/// A tuplet: `actual` notes of `unit` in the time of `normal` notes of `unit` (ADR-0024 §3).
///
/// A triplet of eighths is `3:2` with unit eighth; its members last `normal / actual = 2/3` of
/// their written value. The unit is stored because the display ("3:2♪") and the check "is the
/// tuplet complete" need it: the ratio alone would lose it.
///
/// Tuplets live in a table of their voice ([`crate::Voice::tuplet`]), and every member event
/// stores its innermost tuplet; `parent` makes the nesting a chain of at most
/// [`MAX_TUPLET_DEPTH`] links. A tuplet whose members do not add up to its total is a valid,
/// *incomplete* state (it occurs while typing); the score shows it as an error (F2B).
///
/// # Example
/// ```
/// use orchestra_core_model::{BaseValue, NoteValue, Tuplet};
/// use orchestra_core_time::Rational;
///
/// let triplet = Tuplet::new(3, 2, NoteValue::plain(BaseValue::Eighth))?;
/// assert_eq!(triplet.ratio()?, Rational::new(2, 3)?);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tuplet {
    /// How many notes of `unit` are played…
    actual: NonZeroU8,
    /// …in the time of how many notes of `unit`.
    normal: NonZeroU8,
    /// The value the two numbers count.
    unit: NoteValue,
    /// The tuplet this one is nested in, if any (set by the document when the tuplet is added).
    parent: Option<TupletId>,
    /// How it is drawn.
    display: TupletDisplay,
}

impl Tuplet {
    /// A top-level tuplet with the default display. O(1).
    ///
    /// `actual == normal` is allowed: a 1:1 "tuplet" is how Dorico users write a value held
    /// across a barline without ties, and it changes no duration.
    ///
    /// # Errors
    /// [`NoteValueError::ZeroTupletTerm`] if either term is 0 (the type `u8` already bounds them
    /// at 255, ADR-0024 choice C3).
    pub const fn new(actual: u8, normal: u8, unit: NoteValue) -> Result<Self, NoteValueError> {
        match (NonZeroU8::new(actual), NonZeroU8::new(normal)) {
            (Some(actual), Some(normal)) => Ok(Self {
                actual,
                normal,
                unit,
                parent: None,
                display: TupletDisplay {
                    bracket: TupletBracket::Auto,
                    number: TupletNumber::Actual,
                },
            }),
            _ => Err(NoteValueError::ZeroTupletTerm),
        }
    }

    /// The same tuplet with another display. O(1).
    #[must_use]
    pub const fn with_display(mut self, display: TupletDisplay) -> Self {
        self.display = display;
        self
    }

    /// How many notes of the unit are played. O(1).
    #[must_use]
    pub const fn actual(self) -> u8 {
        self.actual.get()
    }

    /// In the time of how many notes of the unit. O(1).
    #[must_use]
    pub const fn normal(self) -> u8 {
        self.normal.get()
    }

    /// The value the two numbers count. O(1).
    #[must_use]
    pub const fn unit(self) -> NoteValue {
        self.unit
    }

    /// The tuplet this one is nested in. O(1).
    #[must_use]
    pub const fn parent(self) -> Option<TupletId> {
        self.parent
    }

    /// How the tuplet is drawn. O(1).
    #[must_use]
    pub const fn display(self) -> TupletDisplay {
        self.display
    }

    /// The factor applied to the written length of every member: `normal / actual` (2/3 for a
    /// triplet). Nested tuplets multiply their factors. O(1).
    ///
    /// # Errors
    /// Never in practice (both terms are 1..=255); the `Result` is [`Rational::new`]'s.
    pub const fn ratio(self) -> Result<Rational, RationalError> {
        // `as i64` from `u8` is lossless; `From` is not usable in a `const fn`.
        Rational::new(self.normal.get() as i64, self.actual.get() as i64)
    }

    /// The total the members should add up to when the tuplet is complete: `normal × unit`, in
    /// whole notes. Used by the "incomplete tuplet" check of the engraving (F2B). O(1).
    ///
    /// # Errors
    /// Never in practice (at most 255 × 31/2 whole notes).
    pub const fn total(self) -> Result<Duration, RationalError> {
        // `?` is not usable in a `const fn`, hence the explicit matches.
        match (
            self.unit.length(),
            Rational::from_integer(self.normal.get() as i64),
        ) {
            (Ok(unit), Ok(normal)) => unit.checked_mul(normal),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    }

    /// Sets the parent: only the document does it, after checking that the parent belongs to
    /// the same voice and that the nesting stays within [`MAX_TUPLET_DEPTH`]. O(1).
    pub(crate) const fn set_parent(&mut self, parent: Option<TupletId>) {
        self.parent = parent;
    }
}

// ------------------------------------------------------------------------------------------------
// Ties and notes
// ------------------------------------------------------------------------------------------------

/// The tie of a notehead (ADR-0024 §4).
///
/// A tie is stored once, on the notehead it **starts** from, as a link to the notehead it goes
/// to. The backward direction ("which note ties into this one") is a derived question. The model
/// checks (`Document::validate`) that the target sounds at the same pitch (F♯ to G♭ is legal),
/// starts later, and belongs to the same instrument (another voice is fine: piano writing does
/// it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tie {
    /// Not tied.
    #[default]
    None,
    /// Tied to this notehead.
    To(NoteId),
    /// A laissez-vibrer tie: drawn, tied to nothing, the note rings on.
    LetRing,
}

/// One notehead of a chord (ADR-0022 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Note {
    /// The notehead's identity.
    id: NoteId,
    /// Its pitch at concert pitch, with its spelling (ADR-0004, ADR-0023).
    pitch: Pitch,
    /// Its tie, if any.
    tie: Tie,
    /// How its performance departs from what the playback rules derive (ADR-0024 §10): inline,
    /// all zero by default.
    offsets: PerformanceOffsets,
}

impl Note {
    /// A new untied notehead without performance offsets: only the document creates them, with
    /// IDs from its counter. O(1).
    pub(crate) const fn new(id: NoteId, pitch: Pitch) -> Self {
        Self {
            id,
            pitch,
            tie: Tie::None,
            offsets: PerformanceOffsets::NONE,
        }
    }

    /// The notehead's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> NoteId {
        self.id
    }

    /// The pitch, at concert pitch, with its spelling. O(1).
    #[must_use]
    pub const fn pitch(&self) -> Pitch {
        self.pitch
    }

    /// The tie that starts from this notehead. O(1).
    #[must_use]
    pub const fn tie(&self) -> Tie {
        self.tie
    }

    /// Changes the tie: only the document does it, after checking the target exists. O(1).
    pub(crate) const fn set_tie(&mut self, tie: Tie) {
        self.tie = tie;
    }

    /// The performance offsets of this notehead (all zero unless set). O(1).
    #[must_use]
    pub const fn offsets(&self) -> PerformanceOffsets {
        self.offsets
    }

    /// Changes the performance offsets: only the document does it. O(1).
    pub(crate) const fn set_offsets(&mut self, offsets: PerformanceOffsets) {
        self.offsets = offsets;
    }
}

// ------------------------------------------------------------------------------------------------
// Events
// ------------------------------------------------------------------------------------------------

/// A chord: one or more noteheads sharing a stem, a note value and a tuplet (ADR-0022 §4,
/// ADR-0024 §1). A single note is a chord of one notehead.
///
/// # Markings (ADR-0024 §5–§6)
/// A chord may be a **grace chord** (zero metric duration, never in a tuplet), and carries the
/// articulations, the ornament, the fermata and the arpeggio written on it. They belong to the
/// chord, not to each notehead, because they are drawn once per stem and mean one thing for the
/// whole chord; what differs per notehead (spelling, tie, performance offsets) is in [`Note`].
/// One ornament per chord: a trill and a turn on the same chord do not occur in the repertoire
/// the model targets, and a second one can become a set later without changing the file format's
/// meaning of the first.
///
/// # Notehead order
/// The noteheads are kept sorted by **sounding pitch, lowest first**; two noteheads sounding the
/// same (F♯4 and G♭4) are ordered by their letter on the staff (F before G). So two chords with
/// the same pitches compare equal however they were entered, and layout never has to sort.
///
/// # Storage
/// The lowest notehead is inline and the others in a boxed slice. An empty boxed slice does not
/// allocate, so a single note (the vast majority of events) costs no heap allocation of its own;
/// a chord costs one. A chord without noteheads cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The written value.
    value: NoteValue,
    /// The innermost tuplet containing the chord, if any (ADR-0024 §3). Always `None` for a grace
    /// chord.
    tuplet: Option<TupletId>,
    /// `Some` if this is a grace chord (ADR-0024 §5).
    grace: Option<Grace>,
    /// The articulations written on the chord.
    articulations: ArticulationSet,
    /// The ornament written on the chord, if any.
    ornament: Option<Ornament>,
    /// The fermata over (or under) the chord, if any.
    fermata: Option<Fermata>,
    /// The arpeggio sign before the chord, if any.
    arpeggio: Option<Arpeggio>,
    /// The lowest notehead.
    lowest: Note,
    /// The other noteheads, lowest to highest (often empty: no allocation then).
    upper: Box<[Note]>,
}

impl Chord {
    /// A chord without markings from noteheads already sorted by the document (lowest first).
    /// The document refuses a grace chord in a tuplet before calling this. O(1) moves.
    pub(crate) const fn from_sorted(
        value: NoteValue,
        tuplet: Option<TupletId>,
        grace: Option<Grace>,
        lowest: Note,
        upper: Box<[Note]>,
    ) -> Self {
        Self {
            value,
            tuplet,
            grace,
            articulations: ArticulationSet::EMPTY,
            ornament: None,
            fermata: None,
            arpeggio: None,
            lowest,
            upper,
        }
    }

    /// The written value. O(1).
    #[must_use]
    pub const fn value(&self) -> NoteValue {
        self.value
    }

    /// The innermost tuplet containing the chord. O(1).
    #[must_use]
    pub const fn tuplet(&self) -> Option<TupletId> {
        self.tuplet
    }

    /// `Some` if this is a grace chord, with its kind and side. O(1).
    #[must_use]
    pub const fn grace(&self) -> Option<Grace> {
        self.grace
    }

    /// The articulations written on the chord. O(1).
    #[must_use]
    pub const fn articulations(&self) -> ArticulationSet {
        self.articulations
    }

    /// The ornament written on the chord. O(1).
    #[must_use]
    pub const fn ornament(&self) -> Option<Ornament> {
        self.ornament
    }

    /// The fermata on the chord. O(1).
    #[must_use]
    pub const fn fermata(&self) -> Option<Fermata> {
        self.fermata
    }

    /// The arpeggio sign of the chord. O(1).
    #[must_use]
    pub const fn arpeggio(&self) -> Option<Arpeggio> {
        self.arpeggio
    }

    /// Changes the articulations: only the document does it. O(1).
    pub(crate) const fn set_articulations(&mut self, articulations: ArticulationSet) {
        self.articulations = articulations;
    }

    /// Changes the ornament: only the document does it. O(1).
    pub(crate) const fn set_ornament(&mut self, ornament: Option<Ornament>) {
        self.ornament = ornament;
    }

    /// Changes the arpeggio sign: only the document does it. O(1).
    pub(crate) const fn set_arpeggio(&mut self, arpeggio: Option<Arpeggio>) {
        self.arpeggio = arpeggio;
    }

    /// Changes the fermata: only the document and the commands do it (an event's fermata is
    /// changed through [`Event::set_fermata`], which also covers rests). O(1).
    pub(crate) const fn set_fermata(&mut self, fermata: Option<Fermata>) {
        self.fermata = fermata;
    }

    /// The noteheads, lowest first. O(1) to create, O(noteheads) to exhaust; never allocates.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        std::iter::once(&self.lowest).chain(self.upper.iter())
    }

    /// How many noteheads the chord has (at least 1). O(1).
    #[must_use]
    pub const fn len(&self) -> usize {
        // The boxed slice holds at most `MAX` usize elements, and the chord one more: the `+ 1`
        // cannot overflow because a slice of `usize::MAX` noteheads cannot exist in memory.
        self.upper.len().saturating_add(1)
    }

    /// Always false: a chord has at least one notehead. Present because clippy expects `is_empty`
    /// next to `len`. O(1).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The notehead with this ID, mutably: the document changes a tie through it. O(noteheads).
    pub(crate) fn note_mut(&mut self, id: NoteId) -> Option<&mut Note> {
        std::iter::once(&mut self.lowest)
            .chain(self.upper.iter_mut())
            .find(|n| n.id == id)
    }
}

/// A rest (ADR-0022 §4: an event without noteheads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rest {
    /// The written value.
    value: RestValue,
    /// The innermost tuplet containing the rest (never set on a whole-bar rest).
    tuplet: Option<TupletId>,
    /// The fermata over the rest, if any (a general pause is a whole-bar rest with a fermata).
    fermata: Option<Fermata>,
}

impl Rest {
    /// A rest without fermata: only the document creates them, after checking the tuplet. O(1).
    pub(crate) const fn new(value: RestValue, tuplet: Option<TupletId>) -> Self {
        Self {
            value,
            tuplet,
            fermata: None,
        }
    }

    /// The written value. O(1).
    #[must_use]
    pub const fn value(&self) -> RestValue {
        self.value
    }

    /// The innermost tuplet containing the rest. O(1).
    #[must_use]
    pub const fn tuplet(&self) -> Option<TupletId> {
        self.tuplet
    }

    /// The fermata on the rest. O(1).
    #[must_use]
    pub const fn fermata(&self) -> Option<Fermata> {
        self.fermata
    }

    /// Changes the fermata: only the document and the commands do it. O(1).
    pub(crate) const fn set_fermata(&mut self, fermata: Option<Fermata>) {
        self.fermata = fermata;
    }
}

/// What an event is: a chord or a rest.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventBody {
    /// A chord (a single note is a chord of one notehead).
    Chord(Chord),
    /// A rest.
    Rest(Rest),
}

/// An event of a voice (ADR-0024 §1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Event {
    /// The event's identity.
    id: EventId,
    /// The staff it is drawn on: by default its voice's home staff, always a staff of the same
    /// instrument (ADR-0022 §2).
    staff: StaffId,
    /// The chord or the rest.
    body: EventBody,
}

impl Event {
    /// An event: only the document creates them. O(1).
    pub(crate) const fn new(id: EventId, staff: StaffId, body: EventBody) -> Self {
        Self { id, staff, body }
    }

    /// The event's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }

    /// The staff the event is drawn on. O(1).
    #[must_use]
    pub const fn staff(&self) -> StaffId {
        self.staff
    }

    /// The chord or the rest. O(1).
    #[must_use]
    pub const fn body(&self) -> &EventBody {
        &self.body
    }

    /// The chord, if the event is one. O(1).
    #[must_use]
    pub const fn chord(&self) -> Option<&Chord> {
        match &self.body {
            EventBody::Chord(c) => Some(c),
            EventBody::Rest(_) => None,
        }
    }

    /// The innermost tuplet containing the event, chord or rest. O(1).
    #[must_use]
    pub const fn tuplet(&self) -> Option<TupletId> {
        match &self.body {
            EventBody::Chord(c) => c.tuplet,
            EventBody::Rest(r) => r.tuplet,
        }
    }

    /// The grace of the event: `Some` only for a grace chord. O(1).
    #[must_use]
    pub const fn grace(&self) -> Option<Grace> {
        match &self.body {
            EventBody::Chord(c) => c.grace,
            EventBody::Rest(_) => None,
        }
    }

    /// The fermata on the event, chord or rest. O(1).
    #[must_use]
    pub const fn fermata(&self) -> Option<Fermata> {
        match &self.body {
            EventBody::Chord(c) => c.fermata,
            EventBody::Rest(r) => r.fermata,
        }
    }

    /// The slot of the event in the order of events at one offset (ADR-0024 §5): the `After`
    /// graces first, then the `Before` graces, then every other event. O(1).
    #[must_use]
    pub(crate) const fn slot(&self) -> u8 {
        match self.grace() {
            Some(grace) => grace.slot(),
            None => Grace::SLOT_MAIN,
        }
    }

    /// Changes the fermata, on a chord or a rest: only the document does it. O(1).
    pub(crate) const fn set_fermata(&mut self, fermata: Option<Fermata>) {
        match &mut self.body {
            EventBody::Chord(c) => c.fermata = fermata,
            EventBody::Rest(r) => r.fermata = fermata,
        }
    }

    /// The body, mutably: the document changes ties and markings through it. O(1).
    pub(crate) const fn body_mut(&mut self) -> &mut EventBody {
        &mut self.body
    }
}

/// An event with its offset from the start of its bar: one entry of a chunk (ADR-0022 §4).
///
/// The offset is a non-negative `Duration` in whole notes. It may reach past the bar's length:
/// that is an overfull bar, a valid state the score shows as an error (ADR-0022 §4).
///
/// A chunk is sorted by [`order_key`](Self::order_key), (offset, slot), with the insertion order
/// kept among equal keys: grace chords have zero duration, so the offset alone would not say
/// whether a grace comes before or after the main event at the same offset (ADR-0024 §5).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlacedEvent {
    /// The distance from the start of the bar (≥ 0).
    offset: Duration,
    /// The event.
    event: Event,
}

impl PlacedEvent {
    /// An entry: only the document creates them, after checking the offset. O(1).
    pub(crate) const fn new(offset: Duration, event: Event) -> Self {
        Self { offset, event }
    }

    /// The distance from the start of the bar, in whole notes. O(1).
    #[must_use]
    pub const fn offset(&self) -> Duration {
        self.offset
    }

    /// The event. O(1).
    #[must_use]
    pub const fn event(&self) -> &Event {
        &self.event
    }

    /// The event, mutably. O(1).
    pub(crate) const fn event_mut(&mut self) -> &mut Event {
        &mut self.event
    }

    /// The key a chunk is sorted by: the offset, then the slot (`After` graces, `Before` graces,
    /// main events). O(1).
    pub(crate) const fn order_key(&self) -> (Duration, u8) {
        (self.offset, self.event.slot())
    }
}
