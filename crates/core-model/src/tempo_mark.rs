// SPDX-License-Identifier: GPL-3.0-or-later

//! Tempo marks: the written tempo indications, from which the tempo map is derived (ADR-0024 §11,
//! choice C5 = T2; SCALETTA F1.09c).
//!
//! # One source for what is printed and what is played
//! "Allegro ♩ = 132", "poco a poco accel.", "a tempo" are written at (bar, offset) points, in one
//! list of the document. The [`TempoMap`] of ADR-0020/0021, which turns positions into seconds,
//! is **derived** from them and never edited directly: so the score cannot say 120 and play 90,
//! and a bar inserted before a tempo change moves the change with it by construction (the marks
//! are bar-relative), which is why ADR-0024 §11 replaces ADR-0022 §5.
//!
//! # The rules approved by the owner (2026-11-02, choices E1–E3)
//! - **E1:** the initial tempo is a mark at the very start of the score, with an ID (it is printed
//!   text that can be selected), which cannot be removed and must give a tempo: some tempo is
//!   always in force. It is kept apart from the list of changes, like the initial clef and key,
//!   because a document is created before it has any bar to point at.
//! - **E2:** a mark with only text ("dolce", "con moto" without a number) changes nothing in the
//!   tempo map: the text is for the reader.
//! - **E3:** a gradual change has an explicit end point, where its target tempo is reached.
//!
//! # Deriving the map (`Document::derive_tempo_map`)
//! The marks are read in score order (bar index, offset). Two rules are checked **in score order**
//! (they are what the user wrote, so they are judged on what the user sees):
//! - a gradual change ends strictly after it starts;
//! - no other mark that changes the tempo starts strictly inside a gradual change (one that
//!   starts exactly where it ends is fine: "rit. … a tempo"). Text-only marks inside are fine.
//!
//! Then every point becomes a written position: the start of its bar plus its offset, **clamped
//! to the barline** (owner's decision of 2026-11-03). An overfull bar (ADR-0022 §4) is a valid
//! state that a shorter meter can create, and a mark at offset 5/4 of a 4/4 bar would otherwise
//! land after a mark at the start of the next bar, inverting their order. Clamping keeps the order
//! (the positions never decrease along the score order), and two cases follow from it:
//! - two marks may land on the same position: the **later one in score order wins**; the earlier
//!   one stays written in the score but does not change the performance;
//! - a gradual change may shrink to nothing (its end clamped onto its start): it then acts as an
//!   immediate change to its target tempo, the only reading that keeps the tempo after it right.
//!
//! The result goes through [`TempoMap::from_changes`], the bulk edit of ADR-0022 §8: O(t) for t
//! marks, ≈ 0.6 ms for 250 segments (F1.05b numbers).

use orchestra_core_time::{Duration, GradualChange, Position, Tempo, TempoChange, TempoMap};

use crate::document::{Document, DocumentError};
use crate::marks::{ScorePoint, point_key};
use crate::validate::Violation;

// ------------------------------------------------------------------------------------------------
// The marks
// ------------------------------------------------------------------------------------------------

/// A metronome mark: a tempo, and whether it is printed (ADR-0024 §11).
///
/// A hidden metronome is how a DAW user's tempo automation looks in the score: it changes the
/// tempo map but prints nothing (or only the text, if there is one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Metronome {
    /// The tempo (beat unit and beats per minute, ADR-0020).
    pub tempo: Tempo,
    /// Whether "♩ = 132" is printed.
    pub shown: bool,
}

impl Metronome {
    /// A printed metronome mark. O(1).
    #[must_use]
    pub const fn shown(tempo: Tempo) -> Self {
        Self { tempo, shown: true }
    }

    /// A metronome mark that changes the tempo but is not printed. O(1).
    #[must_use]
    pub const fn hidden(tempo: Tempo) -> Self {
        Self {
            tempo,
            shown: false,
        }
    }
}

/// A gradual change written in the score: accelerando or ritardando to a tempo, up to a point
/// (ADR-0024 §11, E3; linear in time, ADR-0020 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GradualMark {
    /// The tempo to jump to first ("Meno mosso ♩ = 80, poco a poco accel.", ADR-0021 §6), with
    /// its printed metronome; `None` starts from the tempo in force.
    pub from: Option<Metronome>,
    /// The tempo reached at `end`. It is not printed here: a printed target is a mark of its own
    /// at the end ("a tempo ♩ = 132").
    pub to: Tempo,
    /// Where `to` is reached, strictly after the mark in score order.
    pub end: ScorePoint,
}

/// How a tempo mark changes the tempo map (ADR-0024 §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TempoMarkChange {
    /// From the mark on, this tempo.
    Immediate(Metronome),
    /// From the mark to its end, gradually.
    Gradual(GradualMark),
}

/// A written tempo indication: printed text, a tempo change, or both (ADR-0024 §11).
///
/// At least one of the two is present: a mark with neither would print nothing and play nothing.
/// The text is never empty, for the same reason ([`TempoMark::new`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TempoMark {
    /// The printed words ("Allegro", "poco a poco accel."), the user's own text, shown as written.
    /// `Box<str>`: it never grows after it is made.
    text: Option<Box<str>>,
    /// What it does to the tempo map; `None` for text only (E2).
    change: Option<TempoMarkChange>,
}

impl TempoMark {
    /// A tempo mark. O(length of `text`); allocates its copy of the text, if any.
    ///
    /// # Errors
    /// [`DocumentError::EmptyText`] if `text` is empty or only white space;
    /// [`DocumentError::EmptyTempoMark`] if there is neither text nor change.
    pub fn new(text: Option<&str>, change: Option<TempoMarkChange>) -> Result<Self, DocumentError> {
        if text.is_some_and(|t| t.trim().is_empty()) {
            return Err(DocumentError::EmptyText);
        }
        if text.is_none() && change.is_none() {
            return Err(DocumentError::EmptyTempoMark);
        }
        Ok(Self {
            text: text.map(Box::from),
            change,
        })
    }

    /// This mark with its gradual change ending at `end` instead; a mark without a gradual change
    /// is returned unchanged. O(text length).
    ///
    /// For the commands that move or clip the end of a *rit.* or an *accel.* with the bars around
    /// it (review point B11: they rebuilt the mark from its text and a modified copy of its
    /// change, three times).
    ///
    /// # Errors
    /// Those of [`TempoMark::new`], which cannot happen for a mark that was valid already: the
    /// text and the presence of a change are kept.
    pub(crate) fn with_gradual_end(&self, end: ScorePoint) -> Result<Self, DocumentError> {
        match self.change {
            Some(TempoMarkChange::Gradual(mut gradual)) => {
                gradual.end = end;
                Self::new(self.text(), Some(TempoMarkChange::Gradual(gradual)))
            }
            _ => Ok(self.clone()),
        }
    }

    /// A mark that sets a tempo, with optional text: the usual initial mark. O(length of `text`).
    ///
    /// # Errors
    /// [`DocumentError::EmptyText`] if `text` is empty or only white space.
    pub fn immediate(text: Option<&str>, metronome: Metronome) -> Result<Self, DocumentError> {
        Self::new(text, Some(TempoMarkChange::Immediate(metronome)))
    }

    /// The printed words, if any. O(1).
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// What the mark does to the tempo map, if anything. O(1).
    #[must_use]
    pub const fn change(&self) -> Option<TempoMarkChange> {
        self.change
    }
}

// ------------------------------------------------------------------------------------------------
// Deriving the tempo map
// ------------------------------------------------------------------------------------------------

impl Document {
    /// The written position of a point, clamped to its barline (see the module documentation).
    /// O(log m) for m meter changes.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`]; meter map errors (absurd meters only); arithmetic errors.
    fn clamped_position(&self, point: ScorePoint) -> Result<Position, DocumentError> {
        let index = self.bar_index(point.bar)?;
        let start = self.meter_map().bar_start(index)?;
        let length = self.meter_map().bar_length(index)?;
        Ok(start.checked_add(point.offset.min(length))?)
    }

    /// Derives the tempo map from the tempo marks (see the module documentation).
    ///
    /// The builder calls it after every tempo mark it adds; the commands of F1.11 will call it
    /// after every edit that touches the marks or where bars start.
    ///
    /// # Complexity
    /// [`walk_tempo_marks`](Self::walk_tempo_marks), then [`TempoMap::from_changes`], O(t).
    /// Allocates the list of changes and the segments.
    ///
    /// # Errors
    /// Those of [`walk_tempo_marks`](Self::walk_tempo_marks); [`DocumentError::TempoMap`] for what
    /// the tempo map itself rejects (absurd tempi only, once the rules of the marks hold).
    pub(crate) fn derive_tempo_map(&self) -> Result<TempoMap, DocumentError> {
        let mut changes: Vec<(Position, TempoChange)> =
            Vec::with_capacity(self.tempo_marks().len().saturating_add(1));
        self.walk_tempo_marks(|at, change| {
            // Same position as the previous change (only possible through the clamping): the
            // later mark in score order wins.
            match changes.last_mut() {
                Some(last) if last.0 == at => *last = (at, change),
                _ => changes.push((at, change)),
            }
        })?;
        TempoMap::from_changes(changes).map_err(DocumentError::TempoMap)
    }

    /// Checks the rules of the tempo marks and hands each change of the tempo map to `emit`, in
    /// order, at its clamped position (see the module documentation): first the initial tempo at
    /// position 0, then one change per mark that has one. Text-only marks emit nothing.
    ///
    /// # Why a walk with a callback?
    /// [`Document::validate`] needs the checks without the map (rule 7, and without allocating:
    /// the builder already derived the map when each mark was added), the derivation needs both.
    /// One walk serves the two, so the rules cannot drift apart.
    ///
    /// # Complexity
    /// O(t log m) for t marks and m meter changes (a bar start and a bar length per point), plus
    /// whatever `emit` costs. Never allocates by itself.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with [`Violation::GradualTempoNotForward`] or
    /// [`Violation::TempoChangeInsideGradual`]; [`DocumentError::UnknownBar`] for a point in a bar
    /// the document does not have; [`DocumentError::InitialTempoNotImmediate`] (unreachable: the
    /// builder refuses such an initial mark); meter map and arithmetic errors (absurd meters only).
    pub(crate) fn walk_tempo_marks(
        &self,
        mut emit: impl FnMut(Position, TempoChange),
    ) -> Result<(), DocumentError> {
        let (_, initial) = self.initial_tempo();
        let Some(TempoMarkChange::Immediate(start)) = initial.change() else {
            return Err(DocumentError::InitialTempoNotImmediate);
        };
        emit(Position::ZERO, TempoChange::Immediate(start.tempo));

        // The end (as a score-order key) of the last gradual change seen: the next mark with a
        // change must not start before it. Only the next one needs checking, because the marks
        // are sorted, so every later mark starts at or after that one.
        let mut open: Option<(u32, Duration)> = None;
        for mark in self.tempo_marks().as_slice() {
            let Some(change) = mark.value().change() else {
                // Text only (E2): nothing to play.
                continue;
            };
            let key = point_key(self.bars(), mark.point());
            if let Some(end) = open.take()
                && key < end
            {
                // Reported on the mark that falls inside, the one the user just placed wrongly.
                return Err(DocumentError::Invalid(Violation::TempoChangeInsideGradual(
                    mark.id(),
                )));
            }
            let at = self.clamped_position(mark.point())?;
            let derived = match change {
                TempoMarkChange::Immediate(metronome) => TempoChange::Immediate(metronome.tempo),
                TempoMarkChange::Gradual(gradual) => {
                    // The end's bar must exist before its key means anything.
                    self.bar_index(gradual.end.bar)?;
                    let end_key = point_key(self.bars(), gradual.end);
                    if end_key <= key {
                        return Err(DocumentError::Invalid(Violation::GradualTempoNotForward(
                            mark.id(),
                        )));
                    }
                    open = Some(end_key);
                    let end = self.clamped_position(gradual.end)?;
                    if end <= at {
                        // Shrunk to nothing by the clamping: an immediate change to the target.
                        TempoChange::Immediate(gradual.to)
                    } else {
                        TempoChange::Gradual(GradualChange {
                            from: gradual.from.map(|m| m.tempo),
                            to: gradual.to,
                            end,
                        })
                    }
                }
            };
            emit(at, derived);
        }
        Ok(())
    }
}
