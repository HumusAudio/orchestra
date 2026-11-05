// SPDX-License-Identifier: GPL-3.0-or-later

//! The incremental update of the played notes (ADR-0027 §9 and §14, H11 and H12; SCALETTA F1.13c
//! part 2): after a transaction of the editor, only what the change set touches is recomputed.
//!
//! # What recomputes what
//! - **The form or the bars changed** (`form_changed`, `meter_from`): the path itself was rebuilt
//!   (F1.12, P7), so everything is recomputed.
//! - **The tempo changed**: nothing. The played notes are in musical time (ADR-0027 §1) and their
//!   attack profiles in seconds (H14): neither depends on the tempo map.
//! - **The dynamics or hairpins of an instrument changed** (`dynamics_changed`, H11): the
//!   timelines of its voices are rebuilt and the **velocities** of its notes recomputed from the
//!   inputs every note keeps ([`Basis`](super::Basis)); starts, ends and pitches stay.
//! - **A cell changed** (voice, written bar): every play of that bar on the path is a changed
//!   **played bar** k, and a window of played bars around it is recomputed with the same code as
//!   the full computation (`perform_range`), keeping the middle of the result.
//!
//! # The window of a changed played bar k (H12)
//! The rules link a note to its neighbours in three ways only: a **tie chain** is one note made
//! of noteheads in several bars; an **acciaccatura** takes its time from the previous note (which
//! may be in the bar before) and is bounded by its length when it touches it; an **`After`
//! grace** takes its time from the note it follows. So a window is four bars c ≤ a ≤ k ≤ b ≤ e:
//! - **kept** `[a, b]`: the notes (by the played bar where they start) that are replaced;
//!   - `a ≤ k − 1`, for the notes the acciaccaturas of k shorten; and no note from before `a`
//!     reaches into `a` (the *cover* below), so every chain touching the bar before k starts
//!     inside the window;
//!   - every note starting in `[a, b)` reaches at most to `b − 1`, so the graces of `b` (the
//!     first bar after them) are recomputed too; a note starting in `b` itself is unchanged,
//!     because nothing it depends on is (its bar and everything after are as before);
//!   - no note starting in `(a, b]` starts before `a` (a very long run of acciaccaturas);
//! - **computed** `[c, e]`: `c` is the first bar of the chains reaching the bar before `a`, so the
//!   previous note of the graces of `a` is complete; `e` is past the last bar every note of
//!   `[c, b]` reaches, so no chain is cut and the graces after it are there.
//!
//! The conditions are checked on the notes before the change (the part's own notes) and on the
//! notes the window computes; whenever one fails the window grows and is computed again. Windows
//! whose computed ranges overlap are merged, so every window reads only notes no other window
//! replaces. Their bounds only ever move outwards, within the path, so the loop ends.
//!
//! # The cover
//! `cover[j]` is the first played bar of the notes reaching bar j (j itself if none comes from
//! before). Following `cover` until it stops moving gives a bar no note from before reaches into:
//! a clean place to start a window.
//!
//! # Complexity
//! O(n) per window and round for the n notes of the instrument (the conditions scan them), plus
//! the computation of the window's bars, plus O(n + w log w) to splice the w new notes into the
//! sorted list. A note entered on the heavy score recomputes three or four bars of one instrument.

use orchestra_core_model::{ChangeSet, Document, DocumentError, Instrument};
use orchestra_core_time::Position;

use super::{Extra, Part, PerformError, Performance, PlayedNote, last_played, order_key};
use crate::loudness::{PathPoint, Profile, Timeline};
use crate::unroll::Unrolled;

impl Performance {
    /// Brings the performance up to date after a transaction of the editor, given the path
    /// **already updated** with [`Unrolled::update`] and the change set of the transaction (see the
    /// module documentation of `perform::update`). Returns whether anything was recomputed.
    ///
    /// The result is the same as [`Performance::new`] on the new document (the property test of
    /// ADR-0027 §11 checks it); only the cost differs.
    ///
    /// # Errors
    /// As [`Performance::new`]. On error `self` is unchanged.
    pub fn update(
        &mut self,
        doc: &Document,
        path: &Unrolled,
        changes: &ChangeSet,
    ) -> Result<bool, PerformError> {
        if changes.form_changed()
            || changes.meter_from().is_some()
            || self.parts.len() != doc.tracks().len()
        {
            *self = Self::new(doc, path)?;
            return Ok(true);
        }
        // The parts touched: (part index, dynamics changed, changed played bars).
        let mut touched: Vec<(usize, bool, Vec<u32>)> = Vec::new();
        let entry = |touched: &mut Vec<(usize, bool, Vec<u32>)>, i: usize| -> usize {
            if let Some(k) = touched.iter().position(|t| t.0 == i) {
                k
            } else {
                touched.push((i, false, Vec::new()));
                touched.len().saturating_sub(1)
            }
        };
        for &instrument in changes.dynamics_changed() {
            let Some(i) = self.parts.iter().position(|p| p.id == instrument) else {
                *self = Self::new(doc, path)?;
                return Ok(true);
            };
            let k = entry(&mut touched, i);
            if let Some(t) = touched.get_mut(k) {
                t.1 = true;
            }
        }
        for &(voice, bar) in changes.cells() {
            let Some((instrument, _)) = doc.voice(voice) else {
                *self = Self::new(doc, path)?;
                return Ok(true);
            };
            let Some(i) = self.parts.iter().position(|p| p.id == instrument.id()) else {
                *self = Self::new(doc, path)?;
                return Ok(true);
            };
            // A bar no longer in the document was removed with the bars, which set `meter_from`.
            let Some(index) = doc.bars().index_of(bar) else {
                continue;
            };
            let k = entry(&mut touched, i);
            if let Some(t) = touched.get_mut(k) {
                t.2.extend_from_slice(path.plays_of(index));
            }
        }
        if touched.is_empty() {
            return Ok(false);
        }
        // Everything is computed before anything is replaced, so that an error changes nothing.
        let mut changes: Vec<PartChange> = Vec::with_capacity(touched.len());
        for (i, dynamics, mut ks) in touched {
            let old = self
                .parts
                .get(i)
                .ok_or(crate::unroll::UnrollError::Endless)?;
            let instrument = doc
                .instrument(old.id)
                .ok_or(DocumentError::UnknownInstrument(old.id))?;
            ks.sort_unstable();
            ks.dedup();
            changes.push(old.change(i, doc, path, instrument, (dynamics, &ks))?);
        }
        for change in changes {
            if let Some(part) = self.parts.get_mut(change.index) {
                part.apply(change);
            }
        }
        Ok(true)
    }
}

/// A window of the update, in played bars (see the module documentation): kept `[a, b]`,
/// computed `[c, e]`, with `c ≤ a ≤ b ≤ e`. The bounds only move outwards while it is settled and
/// grown, within the path, so the loop that does it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    /// The first bar computed.
    c: u32,
    /// The first bar kept.
    a: u32,
    /// The last bar kept.
    b: u32,
    /// The last bar computed.
    e: u32,
}

/// What changes in one part, computed before anything is replaced (see [`Performance::update`]).
struct PartChange {
    /// The part's index.
    index: usize,
    /// The new timelines, if the dynamics changed (H11).
    timelines: Option<Vec<Timeline>>,
    /// The stretch of the sorted lists replaced, if cells changed (H12).
    splice: Option<Splice>,
    /// The notes (by index in the new list) whose loudness must be read again, with it.
    relevel: Vec<(usize, Profile)>,
}

/// A stretch `from..to` of a part's sorted lists and the notes and extras that replace it (see
/// [`Part::splice`]). Applied in place with `Vec::splice`: only the tail of the lists moves, and no
/// new list of the whole instrument is allocated (measured: allocating and filling a fresh list of
/// the first piano of the heavy score made a note's update twice as slow in real time as its
/// instruction count said).
struct Splice {
    /// The first index replaced.
    from: usize,
    /// The index after the last one replaced.
    to: usize,
    /// The new notes of the stretch, sorted.
    notes: Vec<PlayedNote>,
    /// Their extras.
    extras: Vec<Extra>,
}

/// What the old notes of a part say about each played bar, gathered in one pass over them so that
/// a window is settled by looking at bars, not at notes.
struct Summary {
    /// For each played bar, the furthest played bar a note starting there reaches (the bar
    /// itself if no note starts there).
    reach: Vec<u32>,
    /// For each played bar, the earliest start of a note starting there.
    earliest: Vec<Option<Position>>,
    /// For each played bar, the latest start of a note starting there.
    latest: Vec<Option<Position>>,
    /// The cover (see the module documentation).
    cover: Vec<u32>,
}

impl Summary {
    /// The summary of a part's notes on a path of `len` played bars. O(n + len + spans).
    fn new(part: &Part, len: usize) -> Self {
        let mut reach: Vec<u32> = (0..u32::try_from(len).unwrap_or(u32::MAX)).collect();
        let mut earliest: Vec<Option<Position>> = vec![None; len];
        let mut latest: Vec<Option<Position>> = vec![None; len];
        for (note, extra) in part.notes.iter().zip(&part.extras) {
            let Ok(p) = usize::try_from(note.played) else {
                continue;
            };
            if let Some(r) = reach.get_mut(p) {
                *r = (*r).max(extra.reach);
            }
            // The notes are sorted by start: the first one met in a bar starts earliest, the last
            // one latest, so no comparison of positions is needed (they cost a 128-bit product).
            if let Some(e) = earliest.get_mut(p)
                && e.is_none()
            {
                *e = Some(note.start);
            }
            if let Some(l) = latest.get_mut(p) {
                *l = Some(note.start);
            }
        }
        // cover[j]: the first bar of the notes reaching j, swept bar by bar.
        let mut cover: Vec<u32> = (0..u32::try_from(len).unwrap_or(u32::MAX)).collect();
        for (p, &r) in reach.iter().enumerate() {
            let from = u32::try_from(p).unwrap_or(u32::MAX);
            for j in from.saturating_add(1)..=r {
                if let Some(c) = usize::try_from(j).ok().and_then(|j| cover.get_mut(j)) {
                    *c = (*c).min(from);
                }
            }
        }
        Self {
            reach,
            earliest,
            latest,
            cover,
        }
    }

    /// The furthest bar reached by the notes starting in played bar `p`. O(1).
    fn reach(&self, p: u32) -> u32 {
        usize::try_from(p)
            .ok()
            .and_then(|i| self.reach.get(i))
            .copied()
            .unwrap_or(p)
    }

    /// The earliest and latest start of the notes starting in played bar `p`, if any. O(1).
    fn starts(&self, p: u32) -> Option<(Position, Position)> {
        let i = usize::try_from(p).ok()?;
        Some(((*self.earliest.get(i)?)?, (*self.latest.get(i)?)?))
    }
}

impl Part {
    /// What a change does to this part: new timelines if the dynamics changed (H11), the windows
    /// of the changed played bars `ks` recomputed (H12), and the notes whose loudness must be read
    /// again: those of the stretch where a voice's timeline really changed. `self` is only read.
    ///
    /// # Errors
    /// As [`Performance::new`].
    fn change(
        &self,
        index: usize,
        doc: &Document,
        path: &Unrolled,
        instrument: &Instrument,
        (dynamics, ks): (bool, &[u32]),
    ) -> Result<PartChange, PerformError> {
        let timelines = if dynamics {
            Some(Timeline::all(doc, path, instrument)?)
        } else {
            None
        };
        let current = timelines.as_deref().unwrap_or(&self.timelines);
        let splice = if ks.is_empty() {
            None
        } else {
            Some(self.recompute(doc, path, instrument, current, ks)?)
        };
        let mut relevel = Vec::new();
        if let Some(new) = &timelines {
            // H11: only the notes read where a timeline differs, and only their velocities (the
            // order of the list does not depend on them).
            let everything = Some(((0, orchestra_core_time::Duration::ZERO), None));
            let stretches: Vec<Option<(PathPoint, Option<PathPoint>)>> = new
                .iter()
                .enumerate()
                .map(|(v, t)| {
                    self.timelines
                        .get(v)
                        .map_or(everything, |old| old.changed(t))
                })
                .collect();
            // The extras of the lists as they will be once the splice is applied.
            let (from, to, middle) = splice
                .as_ref()
                .map_or((0, 0, &[][..]), |s| (s.from, s.to, s.extras.as_slice()));
            let extras = self
                .extras
                .get(..from)
                .unwrap_or(&[])
                .iter()
                .chain(middle)
                .chain(self.extras.get(to..).unwrap_or(&[]));
            for (i, extra) in extras.enumerate() {
                let basis = &extra.basis;
                let inside =
                    stretches
                        .get(basis.voice)
                        .copied()
                        .flatten()
                        .is_some_and(|(lo, hi)| {
                            lo <= basis.point && hi.is_none_or(|hi| basis.point < hi)
                        });
                if inside {
                    relevel.push((i, basis.profile(new)?));
                }
            }
        }
        Ok(PartChange {
            index,
            timelines,
            splice,
            relevel,
        })
    }

    /// Applies a change computed by [`change`](Self::change). Cannot fail. O(changed notes).
    fn apply(&mut self, change: PartChange) {
        if let Some(timelines) = change.timelines {
            self.timelines = timelines;
        }
        if let Some(splice) = change.splice {
            // Both ranges are within the lists: `splice` computed them on these very lists.
            let to = splice.to.min(self.notes.len());
            let from = splice.from.min(to);
            self.notes.splice(from..to, splice.notes);
            self.extras.splice(from..to, splice.extras);
        }
        for (i, loud) in change.relevel {
            if let Some(note) = self.notes.get_mut(i) {
                note.velocity = loud.velocity;
                note.hold = loud.hold;
                note.settle_in = loud.settle_in;
                note.settle_to = loud.settle_to;
            }
        }
    }

    /// The notes and extras after recomputing the windows of the changed played bars `ks`
    /// (sorted, unique) with `timelines` (see the module documentation). O(see there).
    ///
    /// # Errors
    /// As [`Performance::new`].
    fn recompute(
        &self,
        doc: &Document,
        path: &Unrolled,
        instrument: &Instrument,
        timelines: &[Timeline],
        ks: &[u32],
    ) -> Result<Splice, PerformError> {
        let Some(last) = last_played(path) else {
            return Ok(Splice {
                from: 0,
                to: self.notes.len(),
                notes: Vec::new(),
                extras: Vec::new(),
            });
        };
        let summary = Summary::new(self, path.played().len());
        let mut windows: Vec<Window> = ks
            .iter()
            .map(|&k| Window {
                c: k,
                a: k.saturating_sub(1),
                b: k.saturating_add(1).min(last),
                e: 0,
            })
            .collect();
        let mut results: Vec<Vec<(PlayedNote, Extra)>>;
        loop {
            for w in &mut windows {
                settle(w, &summary, path, last);
            }
            let count = windows.len();
            windows = merge(windows);
            if windows.len() != count {
                // Merged windows are settled again, then computed.
                continue;
            }
            results = Vec::with_capacity(windows.len());
            let mut grown = false;
            for w in &mut windows {
                let slots = perform_window(doc, path, instrument, timelines, *w)?;
                grown |= grow(w, &slots, &summary.cover, path, last);
                results.push(slots);
            }
            if !grown {
                break;
            }
        }
        Ok(self.splice(&windows, results, &summary))
    }

    /// The splice that replaces the notes of the kept bars with the new ones (see
    /// [`recompute`](Self::recompute)).
    ///
    /// # How
    /// The replaced notes start between the earliest and the latest start of their bars (the
    /// summary), the new ones between their own: outside that stretch of the sorted lists nothing
    /// changes, so only the stretch is rebuilt, merging the old notes it keeps with the new ones.
    /// O(log n + m log m) for the m notes of the stretch.
    fn splice(
        &self,
        windows: &[Window],
        results: Vec<Vec<(PlayedNote, Extra)>>,
        summary: &Summary,
    ) -> Splice {
        let kept = |played: u32| windows.iter().any(|w| w.a <= played && played <= w.b);
        let mut new: Vec<(PlayedNote, Extra)> = results
            .into_iter()
            .zip(windows)
            .flat_map(|(slots, w)| {
                slots
                    .into_iter()
                    .filter(move |(n, _)| w.a <= n.played && n.played <= w.b)
            })
            .collect();
        // The stretch of starts touched: the replaced notes' and the new ones'.
        let mut range: Option<(Position, Position)> = None;
        let mut widen = |lo: Position, hi: Position| {
            range = Some(range.map_or((lo, hi), |(l, h)| (l.min(lo), h.max(hi))));
        };
        for w in windows {
            for p in w.a..=w.b {
                if let Some((lo, hi)) = summary.starts(p) {
                    widen(lo, hi);
                }
            }
        }
        for (n, _) in &new {
            widen(n.start, n.start);
        }
        let Some((lo, hi)) = range else {
            return Splice {
                from: 0,
                to: 0,
                notes: Vec::new(),
                extras: Vec::new(),
            };
        };
        let i0 = self.notes.partition_point(|n| n.start < lo);
        let i1 = self.notes.partition_point(|n| n.start <= hi);
        let middle = self
            .notes
            .get(i0..i1)
            .unwrap_or(&[])
            .iter()
            .zip(self.extras.get(i0..i1).unwrap_or(&[]))
            .filter(|(n, _)| !kept(n.played))
            .map(|(n, x)| (*n, *x));
        new.extend(middle);
        new.sort_unstable_by_key(|x| order_key(&x.0));
        let (notes, extras) = new.into_iter().unzip();
        Splice {
            from: i0,
            to: i1,
            notes,
            extras,
        }
    }
}

/// Grows a window until the conditions of the module documentation hold on the old notes (read in
/// the summary, bar by bar), then sets its computed range. O(bars of the window) per round.
fn settle(w: &mut Window, summary: &Summary, path: &Unrolled, last: u32) {
    let cover = &summary.cover;
    loop {
        let before = *w;
        w.a = closure(cover, w.a);
        let start_a = bar_start(path, w.a);
        // Every note starting in [a, b) reaches at most b − 1 (b grows while it is walked).
        let mut p = w.a;
        while p < w.b {
            w.b = w.b.max(summary.reach(p).saturating_add(1)).min(last);
            p = p.saturating_add(1);
        }
        // No note starting in (a, b] starts before a.
        for p in w.a.saturating_add(1)..=w.b {
            if let Some((earliest, _)) = summary.starts(p)
                && earliest < start_a
            {
                w.a = w.a.min(closure(cover, bar_at(path, earliest)));
            }
        }
        if *w == before {
            break;
        }
    }
    // The four bounds only ever move outwards (c and a down, b and e up): `grow` may have pushed
    // `e` past what the old notes ask for, and settling must not pull it back, or the two would
    // chase each other for ever (a mutation test found it).
    let c = if w.a == 0 {
        0
    } else {
        closure(cover, w.a.saturating_sub(1))
    };
    w.c = w.c.min(c);
    w.e = w.e.max(w.b.saturating_add(1)).min(last);
    for p in w.c..=w.b {
        w.e = w.e.max(summary.reach(p).saturating_add(1)).min(last);
    }
}

/// The notes a window computes: `perform_range` on `[c, e]`. O(the bars of the window).
///
/// # Errors
/// As [`Performance::new`].
fn perform_window(
    doc: &Document,
    path: &Unrolled,
    instrument: &Instrument,
    timelines: &[Timeline],
    w: Window,
) -> Result<Vec<(PlayedNote, Extra)>, PerformError> {
    super::perform_range(doc, path, instrument, timelines, w.c, w.e)
}

/// Grows a window if the notes it computed break a condition of the module documentation (a
/// chain longer than before, a note reaching past the computed range, graces starting before the
/// window). Returns whether it grew. O(notes of the window).
fn grow(
    w: &mut Window,
    slots: &[(PlayedNote, Extra)],
    cover: &[u32],
    path: &Unrolled,
    last: u32,
) -> bool {
    let before = *w;
    let start_a = bar_start(path, w.a);
    for (note, extra) in slots {
        let p = note.played;
        let after = extra.reach.saturating_add(1).min(last);
        if w.a <= p && p < w.b {
            w.b = w.b.max(after);
        }
        if w.c <= p && p <= w.b {
            w.e = w.e.max(after);
        }
        if w.a < p && p <= w.b && note.start < start_a {
            w.a = w.a.min(closure(cover, bar_at(path, note.start)));
        }
    }
    *w != before
}

/// Merges the windows whose computed ranges overlap or touch, sorted by their start. O(w log w).
fn merge(mut windows: Vec<Window>) -> Vec<Window> {
    windows.sort_unstable_by_key(|w| (w.c, w.a));
    let mut out: Vec<Window> = Vec::with_capacity(windows.len());
    for w in windows {
        match out.last_mut() {
            Some(prev) if w.c <= prev.e.saturating_add(1) => {
                prev.c = prev.c.min(w.c);
                prev.a = prev.a.min(w.a);
                prev.b = prev.b.max(w.b);
                prev.e = prev.e.max(w.e);
            }
            _ => out.push(w),
        }
    }
    out
}

/// Follows the cover from bar `j` until no note from before reaches the bar (see the module
/// documentation). O(steps); each step goes strictly back.
fn closure(cover: &[u32], mut j: u32) -> u32 {
    while let Some(&c) = usize::try_from(j).ok().and_then(|i| cover.get(i)) {
        if c >= j {
            break;
        }
        j = c;
    }
    j
}

/// Where the played bar `k` starts on the unrolled timeline (the start of the performance if
/// there is no such bar). O(1).
fn bar_start(path: &Unrolled, k: u32) -> Position {
    usize::try_from(k)
        .ok()
        .and_then(|k| path.played().get(k))
        .map_or(Position::ZERO, |p| p.start)
}

/// The played bar holding the point `at` of the unrolled timeline: the last one starting at or
/// before it, 0 if none. O(log path).
fn bar_at(path: &Unrolled, at: Position) -> u32 {
    let i = path.played().partition_point(|p| p.start <= at);
    u32::try_from(i.saturating_sub(1)).unwrap_or(0)
}
