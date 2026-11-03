// SPDX-License-Identifier: GPL-3.0-or-later

//! What is written **on** an event or a notehead: grace notes, articulations, ornaments,
//! fermatas, arpeggios, and the performance offsets of each notehead (ADR-0024 §5, §6, §10;
//! SCALETTA F1.09c).
//!
//! # What lives here
//! - [`Grace`]: makes a chord a grace chord, an acciaccatura or an appoggiatura, sitting
//!   **before** the event it ornaments or **after** the event it follows (the *Nachschlag*). A
//!   grace chord has zero metric duration (ADR-0024 §5).
//! - [`Articulation`] and [`ArticulationSet`]: accents, staccato, tenuto…, several of which can
//!   be combined on one chord, stored as a bit set in one `u16`.
//! - [`Ornament`] and [`TremoloStrokes`]: trills, turns, mordents, single-note tremolos.
//! - [`Fermata`] (on a chord or a rest) and [`Arpeggio`] (on a chord).
//! - [`PerformanceOffsets`]: the explicit departures of one notehead's performance from what the
//!   playback rules derive (ADR-0004): onset and length in musical time, velocity on the 16-bit
//!   scale.
//!
//! Everything here is a plain value, like `crate::event`: the document stores these values in its
//! chords, rests and noteheads, and the builder (later the commands of F1.11) sets them.
//!
//! # Written intent, not performance
//! Except for [`PerformanceOffsets`], every type here is **written intent**: what the score shows.
//! How long a staccato note sounds, how fast a trill alternates, how much time a grace note takes
//! from its neighbours, all of that is derived by the playback rules of F1.13; an ornament expanded
//! into several sounding notes produces *performed* notes that are not written notes (ADR-0004).

use std::fmt;
use std::num::NonZeroU8;

use orchestra_core_time::Duration;

use crate::pitch::Alteration;

// ------------------------------------------------------------------------------------------------
// Grace notes
// ------------------------------------------------------------------------------------------------

/// The kind of a grace note (ADR-0024 §5): how it is drawn and, by convention, how it is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraceKind {
    /// Drawn with a slash through the stem: played as short as possible, "crushed" against its
    /// main note.
    Acciaccatura,
    /// Drawn without a slash: takes part of the main note's time (how much is a playback rule).
    Appoggiatura,
}

/// Which side of the main event a grace chord is on (ADR-0024 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraceAttach {
    /// Before the event it ornaments: the grace sits at that event's offset. The usual case.
    Before,
    /// After the event it follows (the *Nachschlag*, MEI `pre`, the closing notes of a trill):
    /// the grace sits at the offset where that event **ends**.
    After,
}

/// Makes a chord a grace chord (ADR-0024 §5).
///
/// A grace chord keeps its written value (the eighth that is drawn) but has **zero metric
/// duration**: it does not move the events that follow it in its voice. Its place is its offset
/// and its *slot* (see [`slot`](Self::slot)): at one offset, a chunk holds first the `After`
/// graces of the event that ends there, then the `Before` graces of the event that starts there,
/// then that event. A grace chord is never a tuplet member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Grace {
    /// Acciaccatura or appoggiatura.
    pub kind: GraceKind,
    /// Before or after its main event.
    pub attach: GraceAttach,
}

impl Grace {
    /// The slot of an `After` grace in the order of events at one offset: first.
    pub(crate) const SLOT_AFTER: u8 = 0;
    /// The slot of a `Before` grace: after the `After` graces, before the main event.
    pub(crate) const SLOT_BEFORE: u8 = 1;
    /// The slot of every event that is not a grace: last at its offset.
    pub(crate) const SLOT_MAIN: u8 = 2;

    /// The slot of this grace in the order of events at one offset (ADR-0024 §5). O(1).
    ///
    /// The slot is derived from `attach`, never stored: the sort key of a chunk is
    /// (offset, slot, insertion order), so the `After` graces of the previous event come first,
    /// then the `Before` graces, then the main event ([`Self::SLOT_MAIN`]).
    #[must_use]
    pub(crate) const fn slot(self) -> u8 {
        match self.attach {
            GraceAttach::After => Self::SLOT_AFTER,
            GraceAttach::Before => Self::SLOT_BEFORE,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Articulations
// ------------------------------------------------------------------------------------------------

/// One articulation mark (ADR-0024 §6).
///
/// The names follow SMuFL and MusicXML. Each has its own symbol, and some that look like
/// combinations are **not** combinations: portato (a dot under a slur, or a tenuto dash with a
/// dot as one symbol) is its own mark, not staccato + tenuto, because composers and engravers
/// treat it as one articulation with one meaning (detached legato).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Articulation {
    /// Accent (`>`).
    Accent,
    /// Marcato, the strong accent (`^`).
    Marcato,
    /// Staccato (a dot).
    Staccato,
    /// Staccatissimo (a wedge or a stroke over the note).
    Staccatissimo,
    /// Spiccato (MusicXML's `spiccato`, a vertical stroke).
    Spiccato,
    /// Tenuto (a dash).
    Tenuto,
    /// Portato, the detached legato: its own symbol, not staccato + tenuto.
    Portato,
    /// Stress (a small `u`-like sign of 20th-century notation).
    Stress,
    /// Unstress (the opposite of stress).
    Unstress,
    /// Soft accent (a "hairpin" accent `<>` over the note).
    SoftAccent,
}

impl Articulation {
    /// Every articulation, in the order of their bits: handy for tests, for iterating a set, and
    /// for the interface's palette.
    pub const ALL: [Self; 10] = [
        Self::Accent,
        Self::Marcato,
        Self::Staccato,
        Self::Staccatissimo,
        Self::Spiccato,
        Self::Tenuto,
        Self::Portato,
        Self::Stress,
        Self::Unstress,
        Self::SoftAccent,
    ];

    /// The bit of this articulation in an [`ArticulationSet`]. O(1).
    ///
    /// The bit number is the position of the variant in [`Self::ALL`]; ten variants fit in a
    /// `u16` with room for six more. The bit numbers are **not** the file format: F1.15 decides
    /// how a set is written, so reordering variants here could only break unsaved data.
    const fn bit(self) -> u16 {
        // `as u16` of a fieldless enum's discriminant (0..=9) is lossless.
        1 << (self as u16)
    }
}

/// The articulations of a chord: a set of [`Articulation`]s, as one `u16` bit set (ADR-0024 §6).
///
/// A bit set rather than a list because a chord may carry several articulations at once (an
/// accented staccato), their order on the page is the engraver's choice (F2B) and not part of the
/// intent, and two bytes per chord cost nothing. Every operation is O(1) and never allocates.
///
/// # Example
/// ```
/// use orchestra_core_model::{Articulation, ArticulationSet};
///
/// let set = ArticulationSet::EMPTY
///     .with(Articulation::Accent)
///     .with(Articulation::Staccato);
/// assert!(set.contains(Articulation::Staccato));
/// assert_eq!(set.len(), 2);
/// // Portato is its own mark: it is not staccato + tenuto.
/// let portato = ArticulationSet::EMPTY.with(Articulation::Portato);
/// assert!(!portato.contains(Articulation::Staccato));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ArticulationSet(u16);

impl ArticulationSet {
    /// No articulation.
    pub const EMPTY: Self = Self(0);

    /// The same set with `articulation` added (no change if it was there). O(1).
    #[must_use]
    pub const fn with(self, articulation: Articulation) -> Self {
        Self(self.0 | articulation.bit())
    }

    /// The same set without `articulation` (no change if it was absent). O(1).
    #[must_use]
    pub const fn without(self, articulation: Articulation) -> Self {
        Self(self.0 & !articulation.bit())
    }

    /// Whether `articulation` is in the set. O(1).
    #[must_use]
    pub const fn contains(self, articulation: Articulation) -> bool {
        self.0 & articulation.bit() != 0
    }

    /// Whether the set is empty. O(1).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many articulations the set holds. O(1) (one population count).
    #[must_use]
    pub const fn len(self) -> usize {
        // At most 16 bits are set: the count always fits.
        self.0.count_ones() as usize
    }

    /// The articulations of the set, in the order of [`Articulation::ALL`]. O(10) to exhaust;
    /// never allocates.
    pub fn iter(self) -> impl Iterator<Item = Articulation> {
        Articulation::ALL
            .into_iter()
            .filter(move |&a| self.contains(a))
    }
}

impl FromIterator<Articulation> for ArticulationSet {
    /// The set of the given articulations (duplicates are harmless). O(n).
    fn from_iter<I: IntoIterator<Item = Articulation>>(iter: I) -> Self {
        iter.into_iter().fold(Self::EMPTY, Self::with)
    }
}

// ------------------------------------------------------------------------------------------------
// Ornaments
// ------------------------------------------------------------------------------------------------

/// The number of strokes of a single-note tremolo: 1 to 4 (ADR-0024 §6).
///
/// One stroke divides the note into eighths, two into sixteenths, three into thirty-seconds
/// (usually read as "unmeasured"), four into sixty-fourths. Five strokes do not occur in
/// practice and are refused, so a corrupt file cannot ask for absurd subdivisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TremoloStrokes(NonZeroU8);

impl TremoloStrokes {
    /// The most strokes a tremolo can have.
    pub const MAX: u8 = 4;

    /// A tremolo of `strokes` strokes. O(1).
    ///
    /// # Errors
    /// [`OrnamentError::TremoloStrokesOutOfRange`] outside 1..=[`Self::MAX`].
    pub const fn new(strokes: u8) -> Result<Self, OrnamentError> {
        match NonZeroU8::new(strokes) {
            Some(n) if strokes <= Self::MAX => Ok(Self(n)),
            _ => Err(OrnamentError::TremoloStrokesOutOfRange),
        }
    }

    /// The number of strokes, 1 to 4. O(1).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// An ornament written on a chord (ADR-0024 §6).
///
/// The mordents are named after SMuFL (`ornamentMordent`, the lower mordent with the stroke, and
/// `ornamentShortTrill`, the upper mordent without it) rather than after MusicXML's
/// "mordent"/"inverted-mordent", which importers are known to swap. Two-note tremolos change the
/// written durations of both notes and come with a later step (ADR-0024 C9).
///
/// How an ornament is played (how many notes, how fast, starting from which note) is a playback
/// rule (F1.13); the notes it produces are performed notes, not written ones (ADR-0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ornament {
    /// A trill, with the accidental of the auxiliary note when one is written above the sign
    /// (`None`: the auxiliary follows the key).
    Trill {
        /// The written alteration of the upper (auxiliary) note, if shown.
        auxiliary: Option<Alteration>,
    },
    /// A turn (upper note, main, lower, main).
    Turn,
    /// An inverted turn (lower note first).
    InvertedTurn,
    /// The (lower) mordent: main, lower, main. SMuFL `ornamentMordent`, drawn with a stroke.
    Mordent,
    /// The upper mordent (short trill): main, upper, main. SMuFL `ornamentShortTrill`.
    UpperMordent,
    /// A single-note tremolo: strokes through the stem.
    Tremolo(TremoloStrokes),
}

/// Why an ornament could not be built.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrnamentError {
    /// A tremolo with 0 strokes or more than [`TremoloStrokes::MAX`].
    TremoloStrokesOutOfRange,
}

impl fmt::Display for OrnamentError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TremoloStrokesOutOfRange => write!(
                f,
                "a tremolo has between 1 and {} strokes",
                TremoloStrokes::MAX
            ),
        }
    }
}

impl std::error::Error for OrnamentError {}

// ------------------------------------------------------------------------------------------------
// Fermatas and arpeggios
// ------------------------------------------------------------------------------------------------

/// A fermata on a chord or a rest (ADR-0024 §6). How long it holds is a playback rule (F1.13).
///
/// A fermata over a **barline** (the end of a section) belongs to the form (F1.10), not to an
/// event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fermata {
    /// The short (angular) fermata.
    Short,
    /// The normal (round) fermata.
    Normal,
    /// The long (square) fermata.
    Long,
    /// The very long fermata (a double square).
    VeryLong,
}

/// An arpeggio sign on a chord (ADR-0024 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arpeggio {
    /// A plain wavy line: rolled upwards by convention, direction not written.
    Plain,
    /// A wavy line with an arrow up: rolled from the lowest note.
    Up,
    /// A wavy line with an arrow down: rolled from the highest note.
    Down,
    /// The non-arpeggio bracket: the notes must sound together, explicitly not rolled.
    NonArpeggio,
}

// ------------------------------------------------------------------------------------------------
// Performance offsets
// ------------------------------------------------------------------------------------------------

/// The explicit departures of one notehead's performance from what the playback rules derive
/// (ADR-0004, ADR-0024 §10). All zero by default: the note is played as the rules say.
///
/// # Units (ADR-0024 C8)
/// - `onset` and `length` are **signed durations in musical time** (whole notes, ADR-0018 §1): a
///   note played a sixty-fourth early has `onset = −1/64`, one held an eighth longer has
///   `length = +1/8`. Being in musical time, the human feel scales with the tempo when the tempo
///   changes, and the one tempo map stays the only bridge to seconds (ADR-0005). The tolerance
///   threshold in milliseconds (ADR-0004) is converted through the tempo map at the note's
///   position; it never changes what is stored here.
/// - `velocity` is a signed **delta on the 16-bit velocity scale of MIDI 2.0** (0..=65 535):
///   MIDI 1.0's 7 bits are a projection of it, so high-resolution input loses nothing.
///
/// # What the values may be
/// The types bound nothing beyond themselves: an offset may push a note before the start of the
/// score, a length offset may make the derived length zero or negative, a velocity delta may go
/// beyond the scale. These are performance details, not structural errors, so the model stores
/// them as given and the playback rules (F1.13) clamp the **result** (a note never starts before
/// 0, lasts less than a minimum, or leaves the velocity scale). Clamping on input instead would
/// lose information when a later edit of the rules or the dynamics moves the base values.
///
/// # Storage (ADR-0024 C7)
/// Inline in every notehead, so it costs no allocation and is copied with its chunk by
/// copy-on-write: about 40 bytes per notehead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerformanceOffsets {
    /// How much earlier (negative) or later (positive) the note starts, in whole notes.
    pub onset: Duration,
    /// How much shorter (negative) or longer (positive) the note sounds, in whole notes.
    pub length: Duration,
    /// How much softer (negative) or louder (positive) the note is, in steps of the 16-bit
    /// velocity scale.
    pub velocity: i32,
}

impl PerformanceOffsets {
    /// No departure: the note is played exactly as the playback rules derive it.
    pub const NONE: Self = Self {
        onset: Duration::ZERO,
        length: Duration::ZERO,
        velocity: 0,
    };

    /// Whether every offset is zero. O(1).
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

impl Default for PerformanceOffsets {
    /// [`PerformanceOffsets::NONE`]. Written by hand because `Duration` has no `Default` (a zero
    /// duration is rarely what a default should mean, so `core-time` does not offer one).
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every articulation has its own bit: adding all of them sets ten distinct bits.
    #[test]
    fn articulation_bits_are_distinct() {
        let all: ArticulationSet = Articulation::ALL.into_iter().collect();
        assert_eq!(all.len(), Articulation::ALL.len());
        for a in Articulation::ALL {
            assert_eq!(ArticulationSet::EMPTY.with(a).len(), 1);
        }
    }

    /// The slots order `After` < `Before` < main.
    #[test]
    fn grace_slots_are_ordered() {
        let after = Grace {
            kind: GraceKind::Acciaccatura,
            attach: GraceAttach::After,
        };
        let before = Grace {
            kind: GraceKind::Appoggiatura,
            attach: GraceAttach::Before,
        };
        assert!(after.slot() < before.slot());
        assert!(before.slot() < Grace::SLOT_MAIN);
    }

    /// The default offsets are the "none" offsets.
    #[test]
    fn default_offsets_are_none() {
        assert!(PerformanceOffsets::default().is_none());
        assert_eq!(PerformanceOffsets::default(), PerformanceOffsets::NONE);
    }
}
