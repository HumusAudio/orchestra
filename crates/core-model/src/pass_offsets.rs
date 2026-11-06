// SPDX-License-Identifier: GPL-3.0-or-later

//! Pass-specific performance offsets (ADR-0004, ADR-0024 §10, ADR-0025 §12 P6, ADR-0028 §5;
//! SCALETTA F1.14b).
//!
//! # What they are
//! A note inside a repeat is written once and played several times. Its inline offsets
//! ([`PerformanceOffsets`] on the notehead) apply to every pass; an entry of this table applies to
//! **one** pass only, and is **added** to the inline offsets before the bounds of ADR-0027 §8. It
//! is what a DAW edit of "only the second time through" writes: always a performance-only change,
//! never a change of the score (a written change would change every pass, ADR-0028 §5).
//!
//! # The key
//! `(NoteId, pass)`, where `pass` is the pass index of the note's notes as the path reports it
//! (`StaffContent::pass` of F1.12): how many times *those notes* were played before. On a staff
//! with bar repeats it counts the copies of the source bar too, so the original and each % copy
//! have distinct keys (T4). The note is the first notehead of the played tie chain for the onset
//! and the velocity, and every notehead of the chain contributes its length (T3, like N8 for the
//! inline offsets).
//!
//! # Lifetime
//! - An entry equal to [`PerformanceOffsets::NONE`] is never stored: setting it removes the entry,
//!   so the table holds only real deviations and two equal tables mean the same performance.
//! - When a note leaves the document, the editor removes its entries in the same command (T5);
//!   the undo puts them back.
//! - When the form changes and a pass no longer exists, the entry **stays**, ignored by playback:
//!   deleting it would lose it on the undo of the form change (ADR-0028 §5).
//!
//! # Storage
//! One table per instrument (= per track: a track plays exactly one instrument), a vector sorted
//! by key. Lookups are O(log n), an insertion or removal O(n) to shift. Pass-specific edits are
//! rare compared with notes (most notes are played once), so the vector is usually empty and costs
//! nothing; a recording with per-pass deviations on every note of a long repeat would be the case
//! to measure if it ever becomes common.

use crate::id::NoteId;
use crate::marking::PerformanceOffsets;

/// One entry of a [`PassOffsets`] table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassOffset {
    /// The notehead.
    pub note: NoteId,
    /// The pass index of its notes (see the module documentation).
    pub pass: u32,
    /// The offsets added in that pass; never [`PerformanceOffsets::NONE`].
    pub offsets: PerformanceOffsets,
}

/// The pass-specific offsets of one instrument (see the module documentation).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PassOffsets {
    /// The entries, sorted by (note, pass), unique, none equal to `NONE`.
    entries: Vec<PassOffset>,
}

impl PassOffsets {
    /// The position of `(note, pass)` in the sorted vector, or where it would go. O(log n).
    fn search(&self, note: NoteId, pass: u32) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|e| (e.note, e.pass).cmp(&(note, pass)))
    }

    /// The offsets of `note` in `pass`, or [`PerformanceOffsets::NONE`] without an entry. O(log n).
    #[must_use]
    pub fn get(&self, note: NoteId, pass: u32) -> PerformanceOffsets {
        self.search(note, pass)
            .ok()
            .and_then(|i| self.entries.get(i))
            .map_or(PerformanceOffsets::NONE, |e| e.offsets)
    }

    /// Sets the offsets of `note` in `pass` and returns the previous ones (`NONE` if there were
    /// none): exactly what undoes it. Setting `NONE` removes the entry. O(log n + n).
    pub fn set(
        &mut self,
        note: NoteId,
        pass: u32,
        offsets: PerformanceOffsets,
    ) -> PerformanceOffsets {
        match (self.search(note, pass), offsets.is_none()) {
            (Ok(i), true) => self.entries.remove(i).offsets,
            (Ok(i), false) => self
                .entries
                .get_mut(i)
                .map_or(PerformanceOffsets::NONE, |e| {
                    std::mem::replace(&mut e.offsets, offsets)
                }),
            (Err(i), false) => {
                self.entries.insert(
                    i,
                    PassOffset {
                        note,
                        pass,
                        offsets,
                    },
                );
                PerformanceOffsets::NONE
            }
            (Err(_), true) => PerformanceOffsets::NONE,
        }
    }

    /// The entries of one note, in pass order: a slice of the sorted vector, found with two binary
    /// searches. O(log n); never allocates.
    #[must_use]
    pub fn of_note(&self, note: NoteId) -> &[PassOffset] {
        let start = self.entries.partition_point(|e| e.note < note);
        let end = self.entries.partition_point(|e| e.note <= note);
        self.entries.get(start..end).unwrap_or(&[])
    }

    /// Every entry, sorted by (note, pass). O(1).
    #[must_use]
    pub fn entries(&self) -> &[PassOffset] {
        &self.entries
    }

    /// Whether the table holds no entry. O(1).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use orchestra_core_time::{Duration, Rational};

    use super::*;

    /// A notehead ID for the tests.
    fn note(raw: u64) -> NoteId {
        NoteId::from_counter(NonZeroU64::new(raw).unwrap())
    }

    /// Offsets with only an onset of `n/d` whole notes.
    fn onset(n: i64, d: i64) -> PerformanceOffsets {
        PerformanceOffsets {
            onset: Duration::from_rational(Rational::new(n, d).unwrap()),
            ..PerformanceOffsets::NONE
        }
    }

    /// Setting returns the previous value, `NONE` removes, and the order is (note, pass) whatever
    /// the order of the insertions.
    #[test]
    fn set_returns_the_inverse_and_keeps_the_order() {
        let mut table = PassOffsets::default();
        assert_eq!(
            table.set(note(5), 1, onset(1, 64)),
            PerformanceOffsets::NONE
        );
        assert_eq!(
            table.set(note(2), 3, onset(-1, 64)),
            PerformanceOffsets::NONE
        );
        assert_eq!(
            table.set(note(5), 0, onset(1, 32)),
            PerformanceOffsets::NONE
        );
        let keys: Vec<_> = table.entries().iter().map(|e| (e.note, e.pass)).collect();
        assert_eq!(keys, vec![(note(2), 3), (note(5), 0), (note(5), 1)]);
        assert_eq!(table.of_note(note(5)).len(), 2);
        assert_eq!(table.set(note(5), 1, onset(1, 16)), onset(1, 64));
        assert_eq!(
            table.set(note(5), 1, PerformanceOffsets::NONE),
            onset(1, 16)
        );
        assert_eq!(table.get(note(5), 1), PerformanceOffsets::NONE);
        assert_eq!(
            table.set(note(9), 0, PerformanceOffsets::NONE),
            PerformanceOffsets::NONE
        );
        assert_eq!(table.entries().len(), 2);
    }
}
