// SPDX-License-Identifier: GPL-3.0-or-later

//! The tolerance threshold of the DAW move (ADR-0004, ADR-0028 §1–§2; SCALETTA F1.14b).
//!
//! # What it decides
//! When a note is moved in the DAW view, the onset offset the user asks for is compared with the
//! threshold of the note's track: within it the move stays a **performance offset** and the score
//! does not change; beyond it the written position changes (`crate::daw_command`). The threshold
//! is part of the project, saved in the file and changed by an undoable command, because two
//! people opening the same project must see the same behaviour.
//!
//! # Two units (Q2)
//! - [`Tolerance::Musical`]: a fraction of a whole note, the default (a 1/32 note, D09). The feel
//!   follows tempo edits, as the offsets themselves do (ADR-0024 C8).
//! - [`Tolerance::Millis`]: a number of milliseconds, for tempo-independent behaviour. It is
//!   converted at the note's written start through the one tempo map (ADR-0005): it never changes
//!   what is stored, only the comparison.
//!
//! # Where it lives (Q3)
//! [`ToleranceSettings`]: one project value plus optional per-track overrides, kept sorted by
//! track ID so that the list is a pure function of its entries (the same rule as the marks,
//! `crate::edit`), and a lookup is a binary search.

use std::cmp::Ordering;

use orchestra_core_time::{Duration, Position, Rational, TempoMap, TimeAt};

use crate::document::DocumentError;
use crate::id::TrackId;

/// The width of the zone around a note's written start inside which a DAW move stays a
/// performance offset (ADR-0028 §1). Both variants are strictly positive: a zero threshold would
/// make every nudge rewrite the score, which is what [`MoveMode`](crate::MoveMode) already offers
/// the other way round, and would only be a trap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tolerance {
    /// A musical length, in whole notes (`1/32` is a thirty-second note).
    Musical(Duration),
    /// A real-time length, in milliseconds, converted at the note through the tempo map.
    Millis(u32),
}

impl Tolerance {
    /// The default of a new project: a thirty-second note (ADR-0004's proposal, approved as D09
    /// on 2026-11-06). At 120 quarter notes per minute that is 62.5 ms. O(1).
    ///
    /// # Errors
    /// Never in practice: `1/32` is a valid rational. Typed rather than unwrapped (no panics in
    /// library code, CLAUDE.md §4).
    pub fn default_musical() -> Result<Self, DocumentError> {
        Ok(Self::Musical(Duration::from_rational(Rational::new(
            1, 32,
        )?)))
    }

    /// Checks that the threshold is strictly positive. O(1).
    ///
    /// # Errors
    /// [`DocumentError::ToleranceNotPositive`] for a zero or negative musical length or zero
    /// milliseconds.
    pub fn check(self) -> Result<(), DocumentError> {
        let positive = match self {
            Self::Musical(length) => length.is_positive(),
            Self::Millis(ms) => ms > 0,
        };
        if positive {
            Ok(())
        } else {
            Err(DocumentError::ToleranceNotPositive)
        }
    }

    /// Whether an onset offset of `onset` from the written start `at` stays within this threshold
    /// (inclusive: an offset exactly as wide as the threshold is still an offset, ADR-0028 §2).
    ///
    /// # How the milliseconds compare (T6)
    /// The distance is the time between `at` and `at + onset` on the tempo map. In a constant
    /// segment both times are exact rationals, and the comparison with `ms / 1000` seconds is
    /// exact. Inside a gradual change the tempo map only gives an `f64` approximation of a time
    /// that is irrational anyway (ADR-0021 §3), so the comparison is made in `f64` there; a note
    /// sitting on the threshold to a few nanoseconds may then go either way, which no user can
    /// hear or aim for.
    ///
    /// # Complexity
    /// O(1) for a musical threshold; O(log t) for milliseconds (two tempo map lookups).
    ///
    /// # Errors
    /// [`DocumentError::Arithmetic`] or [`DocumentError::TempoMap`] on overflow (absurd values).
    pub fn allows(
        self,
        tempo: &TempoMap,
        at: Position,
        onset: Duration,
    ) -> Result<bool, DocumentError> {
        match self {
            Self::Musical(width) => Ok(onset.abs() <= width),
            Self::Millis(ms) => {
                let from = tempo.time_at(at).map_err(DocumentError::TempoMap)?;
                let to = tempo
                    .time_at(at.checked_add(onset)?)
                    .map_err(DocumentError::TempoMap)?;
                let limit = Rational::new(i64::from(ms), 1000)?;
                match (from, to) {
                    (TimeAt::Exact(a), TimeAt::Exact(b)) => {
                        let distance = b.as_rational().checked_sub(a.as_rational())?.abs();
                        Ok(distance.cmp(&limit) != Ordering::Greater)
                    }
                    // At least one end inside a gradual change: compare the approximations.
                    (a, b) => Ok((b.to_f64() - a.to_f64()).abs() <= limit.to_f64()),
                }
            }
        }
    }
}

/// The thresholds of a project (ADR-0028 §1, Q3): one project value, always present, and
/// optional per-track overrides.
///
/// # Invariants
/// - Every threshold is strictly positive ([`Tolerance::check`]).
/// - `per_track` is sorted by track ID with at most one entry per track; every track exists in the
///   document (`Document::validate` checks it; tracks are never removed today, and when they are,
///   the command that removes one removes its override too, ADR-0028 §1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToleranceSettings {
    /// The value of every track without an override.
    project: Tolerance,
    /// The overrides, sorted by track ID. Usually empty: a sorted vector costs nothing then and is
    /// searched in O(log n) otherwise.
    per_track: Vec<(TrackId, Tolerance)>,
}

impl ToleranceSettings {
    /// The settings of a new project: the default threshold ([`Tolerance::default_musical`]) and
    /// no override. O(1); does not allocate.
    ///
    /// # Errors
    /// Never in practice (see [`Tolerance::default_musical`]).
    pub fn new() -> Result<Self, DocumentError> {
        Ok(Self {
            project: Tolerance::default_musical()?,
            per_track: Vec::new(),
        })
    }

    /// The project value. O(1).
    #[must_use]
    pub const fn project(&self) -> Tolerance {
        self.project
    }

    /// The overrides, sorted by track ID. O(1).
    #[must_use]
    pub fn per_track(&self) -> &[(TrackId, Tolerance)] {
        &self.per_track
    }

    /// The override of one track, if it has one. O(log n).
    #[must_use]
    pub fn track_override(&self, track: TrackId) -> Option<Tolerance> {
        self.per_track
            .binary_search_by_key(&track, |(t, _)| *t)
            .ok()
            .and_then(|i| self.per_track.get(i))
            .map(|(_, tolerance)| *tolerance)
    }

    /// The threshold in force for a track: its override, or the project value. O(log n).
    #[must_use]
    pub fn for_track(&self, track: TrackId) -> Tolerance {
        self.track_override(track).unwrap_or(self.project)
    }

    /// Sets the project value (`track: None`) or a track's override, or removes the override with
    /// `tolerance: None`, and returns the previous value in the same form: what the edit's inverse
    /// needs (`crate::edit`). The project value cannot be removed. On error nothing changed.
    ///
    /// # Complexity
    /// O(log n) to find the entry, O(n) to shift the vector on an insertion or a removal.
    ///
    /// # Errors
    /// [`DocumentError::ToleranceNotPositive`]; [`DocumentError::ProjectToleranceRequired`] for
    /// `(None, None)`.
    pub fn set(
        &mut self,
        track: Option<TrackId>,
        tolerance: Option<Tolerance>,
    ) -> Result<Option<Tolerance>, DocumentError> {
        if let Some(value) = tolerance {
            value.check()?;
        }
        match (track, tolerance) {
            (None, None) => Err(DocumentError::ProjectToleranceRequired),
            (None, Some(value)) => Ok(Some(std::mem::replace(&mut self.project, value))),
            (Some(track), value) => {
                let found = self.per_track.binary_search_by_key(&track, |(t, _)| *t);
                Ok(match (found, value) {
                    (Ok(i), Some(value)) => self
                        .per_track
                        .get_mut(i)
                        .map(|entry| std::mem::replace(&mut entry.1, value)),
                    (Ok(i), None) => Some(self.per_track.remove(i).1),
                    (Err(i), Some(value)) => {
                        self.per_track.insert(i, (track, value));
                        None
                    }
                    (Err(_), None) => None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a musical threshold of `n/d` whole notes.
    fn musical(n: i64, d: i64) -> Tolerance {
        Tolerance::Musical(Duration::from_rational(Rational::new(n, d).unwrap()))
    }

    /// The default is a thirty-second note, and an offset exactly as wide is still within it.
    #[test]
    fn default_is_a_thirty_second_and_inclusive() {
        let tolerance = Tolerance::default_musical().unwrap();
        assert_eq!(tolerance, musical(1, 32));
        // A quarter note at 120 per minute.
        let quarter = Duration::from_rational(Rational::new(1, 4).unwrap());
        let tempo = TempoMap::new(
            orchestra_core_time::Tempo::new(quarter, Rational::from_integer(120).unwrap()).unwrap(),
        )
        .unwrap();
        let at = Position::ZERO;
        let edge = Duration::from_rational(Rational::new(-1, 32).unwrap());
        let past = Duration::from_rational(Rational::new(3, 64).unwrap());
        assert!(tolerance.allows(&tempo, at, edge).unwrap());
        assert!(!tolerance.allows(&tempo, at, past).unwrap());
    }

    /// Zero thresholds are refused, and the project value cannot be removed.
    #[test]
    fn zero_and_missing_project_values_are_refused() {
        assert_eq!(
            Tolerance::Millis(0).check(),
            Err(DocumentError::ToleranceNotPositive)
        );
        assert_eq!(
            musical(0, 1).check(),
            Err(DocumentError::ToleranceNotPositive)
        );
        let mut settings = ToleranceSettings::new().unwrap();
        assert_eq!(
            settings.set(None, None),
            Err(DocumentError::ProjectToleranceRequired)
        );
    }

    /// Overrides stay sorted, and each `set` returns exactly what undoes it.
    #[test]
    fn overrides_are_sorted_and_set_returns_the_inverse() {
        let mut settings = ToleranceSettings::new().unwrap();
        let track = |raw| TrackId::from_counter(std::num::NonZeroU64::new(raw).unwrap());
        let (a, b) = (track(7), track(3));
        assert_eq!(settings.set(Some(a), Some(Tolerance::Millis(30))), Ok(None));
        assert_eq!(settings.set(Some(b), Some(musical(1, 16))), Ok(None));
        assert_eq!(
            settings
                .per_track()
                .iter()
                .map(|(t, _)| *t)
                .collect::<Vec<_>>(),
            vec![b, a]
        );
        assert_eq!(settings.for_track(a), Tolerance::Millis(30));
        assert_eq!(settings.set(Some(a), None), Ok(Some(Tolerance::Millis(30))));
        assert_eq!(settings.for_track(a), musical(1, 32));
        assert_eq!(
            settings.set(None, Some(Tolerance::Millis(20))),
            Ok(Some(musical(1, 32)))
        );
    }
}
