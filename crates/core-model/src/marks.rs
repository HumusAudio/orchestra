// SPDX-License-Identifier: GPL-3.0-or-later

//! Clefs and key signatures: marks that change along the score at (bar, offset) points, kept
//! **per staff** (clefs) and **per document** with per-instrument overrides (keys), never inside a
//! voice (ADR-0024 §9, SCALETTA F1.09b).
//!
//! # Why not in the voices?
//! A staff carries several voices and a voice may move between staves (ADR-0022 §2): if a clef
//! were an event of a voice, the clef in force on a staff would depend on which voice is asked.
//!
//! # What lives here
//! - [`Clef`] and [`ClefSign`]: a sign on a staff line, with an optional octave shift (the treble
//!   clef with an 8 below of tenors and guitars).
//! - [`KeySignature`] and [`KeyOverride`]: what a staff shows (a key, or no signature at all), and
//!   how an instrument departs from the document's key.
//! - [`ScorePoint`]: a (bar, offset) point, where every position-anchored thing sits.
//! - [`Mark`] and [`MarkList`]: a list of changes sorted by (bar, offset), with an O(log c)
//!   lookup of the mark in force at a point. The *initial* clef of a staff and the initial key of
//!   the document are fields of their owners, not marks (owner's decision of 2026-11-02): so a
//!   staff without a clef cannot be built, and the lists only hold the changes.

use orchestra_core_time::{BarIndex, Duration};

use crate::document::{BarList, DocumentError};
use crate::id::BarId;
use crate::pitch::Key;

// ------------------------------------------------------------------------------------------------
// Clefs
// ------------------------------------------------------------------------------------------------

/// The sign of a clef (ADR-0024 §9). Tablature "clefs" come with tablature staves (SCALETTA,
/// "Beyond").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClefSign {
    /// The G clef: its curl marks the G above middle C.
    G,
    /// The F clef: its dots surround the F below middle C.
    F,
    /// The C clef: its centre marks middle C (alto on line 3, tenor on line 4…).
    C,
    /// The neutral clef of unpitched percussion: no pitch is implied.
    Percussion,
}

/// A clef: a sign, the staff line it sits on, and an octave shift (ADR-0024 §9).
///
/// Lines are numbered from the bottom, 1 to 5, as in MusicXML: the treble clef is G on line 2,
/// the bass clef F on line 4. `octave` is the small 8 or 15 above (positive) or below
/// (negative): the tenor's treble clef is G, line 2, octave −1.
///
/// A clef never changes a pitch: pitches are stored as spelled at concert pitch (ADR-0004), and
/// the clef only tells the engraving which staff line a pitch is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Clef {
    /// The sign.
    sign: ClefSign,
    /// The staff line, 1 (bottom) to 5 (top).
    line: u8,
    /// The octave shift, −2 to +2.
    octave: i8,
}

impl Clef {
    /// The treble clef: G on line 2.
    pub const TREBLE: Self = Self {
        sign: ClefSign::G,
        line: 2,
        octave: 0,
    };
    /// The treble clef with an 8 below (tenor voice, guitar): G on line 2, an octave down.
    pub const TREBLE_8VB: Self = Self {
        sign: ClefSign::G,
        line: 2,
        octave: -1,
    };
    /// The bass clef: F on line 4.
    pub const BASS: Self = Self {
        sign: ClefSign::F,
        line: 4,
        octave: 0,
    };
    /// The alto clef: C on line 3 (viola).
    pub const ALTO: Self = Self {
        sign: ClefSign::C,
        line: 3,
        octave: 0,
    };
    /// The tenor clef: C on line 4 (cello, bassoon, trombone in their high range).
    pub const TENOR: Self = Self {
        sign: ClefSign::C,
        line: 4,
        octave: 0,
    };
    /// The percussion clef, conventionally centred on line 3.
    pub const PERCUSSION: Self = Self {
        sign: ClefSign::Percussion,
        line: 3,
        octave: 0,
    };

    /// A clef with this sign on this line and this octave shift. O(1).
    ///
    /// # Errors
    /// [`DocumentError::ClefOutOfRange`] for a line outside 1–5 or an octave shift outside ±2.
    pub const fn new(sign: ClefSign, line: u8, octave: i8) -> Result<Self, DocumentError> {
        if line < 1 || line > 5 || octave < -2 || octave > 2 {
            return Err(DocumentError::ClefOutOfRange);
        }
        Ok(Self { sign, line, octave })
    }

    /// The sign. O(1).
    #[must_use]
    pub const fn sign(self) -> ClefSign {
        self.sign
    }

    /// The staff line, 1 (bottom) to 5 (top). O(1).
    #[must_use]
    pub const fn line(self) -> u8 {
        self.line
    }

    /// The octave shift, −2 to +2. O(1).
    #[must_use]
    pub const fn octave(self) -> i8 {
        self.octave
    }
}

// ------------------------------------------------------------------------------------------------
// Key signatures
// ------------------------------------------------------------------------------------------------

/// What a staff shows at the start of a line: a key signature, or none at all (ADR-0024 §9).
///
/// `NoSignature` is not the same as C major: a horn part written without signature has no
/// signature in its **written** part either, whereas C major at concert pitch becomes D major for
/// a B♭ clarinet. That is why it is its own case (owner's decision of 2026-11-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeySignature {
    /// This key (at concert pitch where it is stored; written pitch where it is derived).
    Key(Key),
    /// No signature: every alteration is written as an accidental.
    NoSignature,
}

/// How an instrument departs from the document's key from a point on (ADR-0024 §9, K1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyOverride {
    /// From here on, the instrument shows this (polytonal writing, an instrument without
    /// signature), whatever the document's key does, until another override.
    Signature(KeySignature),
    /// From here on, the instrument follows the document's key again.
    FollowScore,
}

// ------------------------------------------------------------------------------------------------
// Points of the score
// ------------------------------------------------------------------------------------------------

/// A point of the score: a bar and a distance from its start (ADR-0022 §6). Position-anchored
/// things (clef, key and tempo changes, dynamics, the ends of hairpins and of gradual tempo
/// changes) sit at one, so that they follow their bar when bars are inserted or deleted before it.
///
/// Plain data with public fields: whether the bar exists and the offset is not negative is
/// checked by the document when something is placed there, as for every other ID it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScorePoint {
    /// The bar.
    pub bar: BarId,
    /// The distance from the start of the bar (≥ 0 once placed in a document). It may reach past
    /// the end of an overfull bar (ADR-0022 §4).
    pub offset: Duration,
}

impl ScorePoint {
    /// The point `offset` after the start of `bar`. O(1).
    #[must_use]
    pub const fn new(bar: BarId, offset: Duration) -> Self {
        Self { bar, offset }
    }

    /// The point at the very start of `bar`. O(1).
    #[must_use]
    pub const fn start_of(bar: BarId) -> Self {
        Self::new(bar, Duration::ZERO)
    }
}

/// The sort key of a point in **score order** of points: its bar's index, then its offset.
/// O(1) on average (one `BarId → index` lookup).
///
/// Every point stored in the document has a bar of the document (checked on insertion, and bars
/// are never removed without what sits on them, F1.11). Should one be missing anyway, it sorts
/// after everything, which keeps binary searches well-defined instead of panicking.
pub(crate) fn point_key(bars: &BarList, point: ScorePoint) -> (u32, Duration) {
    (bars.key(point.bar), point.offset)
}

// ------------------------------------------------------------------------------------------------
// Sorted lists of marks
// ------------------------------------------------------------------------------------------------

/// A change at a point of the score: an ID, a (bar, offset) point and a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mark<I, T> {
    /// The change's identity.
    id: I,
    /// The bar it is in.
    bar: BarId,
    /// The distance from the start of the bar (≥ 0).
    offset: Duration,
    /// What changes there.
    value: T,
}

impl<I: Copy, T> Mark<I, T> {
    /// A mark with its ID at (bar, offset). O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// ADR-0029 L1 of 2026-11-06): the value only *describes* an object. It enters a document only
    /// through [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as
    /// the builder checks what it creates, so no invalid document can come from it.
    pub const fn new(id: I, bar: BarId, offset: Duration, value: T) -> Self {
        Self {
            id,
            bar,
            offset,
            value,
        }
    }

    /// This mark at another point, with a copy of its value. O(1) plus the copy of the value.
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): one method instead of calling `new` with every
    /// other field copied by hand, so a field added later cannot be forgotten by one of them.
    #[must_use]
    pub(crate) fn moved_to(&self, point: ScorePoint) -> Self
    where
        T: Clone,
    {
        Self::new(self.id, point.bar, point.offset, self.value.clone())
    }

    /// The change's identity. O(1).
    pub const fn id(&self) -> I {
        self.id
    }

    /// The bar it is in. O(1).
    pub const fn bar(&self) -> BarId {
        self.bar
    }

    /// The distance from the start of the bar. O(1).
    pub const fn offset(&self) -> Duration {
        self.offset
    }

    /// What changes there. O(1).
    ///
    /// A reference rather than a copy: most values are small `Copy` types (clefs, keys), but a
    /// tempo mark carries its printed text (ADR-0024 §11), which is not.
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The (bar, offset) point of the mark. O(1).
    pub const fn point(&self) -> ScorePoint {
        ScorePoint::new(self.bar, self.offset)
    }
}

/// The changes of one kind of mark, sorted by (bar index, offset), at most one per point.
///
/// # Why a sorted vector?
/// Clef and key changes are few (tens per staff, a handful per score), and the hot question is
/// "which one is in force here?", asked once per event by layout and by the DAW's spelling. A
/// sorted vector answers it with a binary search: O(log c) comparisons, each costing one O(1)
/// `BarId → index` lookup. Insertions are O(c). Inserting a bar elsewhere (F1.11) does not
/// reorder the marks, because they are keyed by `BarId` and bars keep their relative order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MarkList<I, T> {
    /// The marks, sorted by (bar index, offset), no two at the same point.
    marks: Vec<Mark<I, T>>,
}

impl<I, T> Default for MarkList<I, T> {
    /// An empty list. Does not allocate.
    fn default() -> Self {
        Self { marks: Vec::new() }
    }
}

impl<I: Copy + PartialEq, T> MarkList<I, T> {
    /// The marks in score order. O(1).
    pub fn as_slice(&self) -> &[Mark<I, T>] {
        &self.marks
    }

    /// How many changes there are. O(1).
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// Whether there are no changes. O(1).
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// The sort key of a mark: its bar's index and its offset ([`point_key`]). O(1) on average.
    fn key(bars: &BarList, bar: BarId, offset: Duration) -> (u32, Duration) {
        point_key(bars, ScorePoint::new(bar, offset))
    }

    /// Inserts a mark at its place. O(log c) to find it, O(c) to shift the later ones.
    ///
    /// # Errors
    /// [`DocumentError::DuplicateMark`] if a mark of this list is already at that point: two clefs
    /// at the same place of a staff are a contradiction, not two facts.
    pub(crate) fn insert(&mut self, mark: Mark<I, T>, bars: &BarList) -> Result<(), DocumentError> {
        let key = Self::key(bars, mark.bar, mark.offset);
        let at = self
            .marks
            .partition_point(|m| Self::key(bars, m.bar, m.offset) < key);
        if self
            .marks
            .get(at)
            .is_some_and(|m| Self::key(bars, m.bar, m.offset) == key)
        {
            return Err(DocumentError::DuplicateMark);
        }
        self.marks.insert(at, mark);
        Ok(())
    }

    /// Removes the mark with this ID and returns it. O(c): a linear search (marks are sorted by
    /// point, not by ID) and the shift of the later ones. The builder uses it to take back a tempo
    /// mark whose derived tempo map turned out invalid (all or nothing).
    pub(crate) fn remove(&mut self, id: I) -> Option<Mark<I, T>> {
        let at = self.marks.iter().position(|m| m.id == id)?;
        Some(self.marks.remove(at))
    }

    /// The last mark at or before (`bar`, `offset`), if any: the one in force there. O(log c)
    /// comparisons, each an O(1) average hash lookup; never allocates.
    ///
    /// A mark at exactly the point is in force at the point (a clef at offset 1/4 applies to the
    /// note at offset 1/4).
    pub fn in_force_at(
        &self,
        bars: &BarList,
        bar: BarIndex,
        offset: Duration,
    ) -> Option<&Mark<I, T>> {
        let key = (bar.get(), offset);
        // `partition_point` gives the first mark strictly after the point; the one before it, if
        // any, is the last one at or before the point.
        let after = self
            .marks
            .partition_point(|m| Self::key(bars, m.bar, m.offset) <= key);
        after.checked_sub(1).and_then(|i| self.marks.get(i))
    }
}
