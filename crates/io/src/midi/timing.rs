// SPDX-License-Identifier: GPL-3.0-or-later

//! Time in the MIDI file: positions to ticks (ADR-0032 E3) and the tempo events that make the
//! ticks sound at the right seconds (E4).
//!
//! # Ticks (E3)
//! A position `p` on the unrolled timeline (whole notes) is at tick `round(p × 3 840)` (960 per
//! quarter), rounded to the nearest, halves away from zero, plus the lead-in (E5). **Absolute**
//! positions are rounded, never durations, so a septuplet is off by at most half a tick at each
//! note and never drifts.
//!
//! # Tempo (E4): rebuilt from the seconds, not from the tempo marks
//! MIDI time changes only in steps; ours changes continuously inside gradual changes, and jumps
//! from bar to bar where the path repeats (the shift of F1.12). So the export does not translate
//! the tempo marks: it takes **breakpoints** on the timeline, asks the path for their time in
//! seconds, and writes, for every stretch between two breakpoints, the tempo that makes the
//! stretch last exactly what it lasts.
//!
//! ## Breakpoints
//! The start of the file (position 0, or the lead-in's start), the end of the music, every tempo
//! change of the tempo map each time it is played, the start of every played bar whose shift
//! differs from the previous bar's, and, **only inside gradual changes**, a grid of one 32nd note
//! (120 ticks) from the start of each played bar plus every note start and end there.
//!
//! ## Exact stretches, in units of 1/960 µs
//! A tempo of `T` microseconds per quarter note lasts `T / 960` µs per tick. Counting time in
//! **units of 1/960 µs**, a tick at tempo `T` lasts exactly `T` units, and a stretch of `Δ` ticks
//! that must last `N` units gets `T = N div Δ` for its first `Δ − k` ticks and `T + 1` for its last
//! `k = N mod Δ` ticks: `(Δ − k)·T + k·(T + 1) = N`, **exactly**. So every breakpoint falls on its
//! time to within half a unit (≈ 0.0005 µs, the rounding of its time to units), whatever the
//! tempo, at the cost of at most two tempo events per stretch; a tempo event is written only where
//! the value changes, so constant-tempo music has one per tempo mark (or two, when its
//! microseconds per quarter are not an integer). `N` is measured from the time the reader has
//! actually reached, so nothing accumulates even when a tempo is clamped.
//!
//! ## Exactness of the seconds
//! The path's time is exact in constant-tempo segments and an `f64` approximation inside gradual
//! changes (ADR-0021). The approximation is acceptable here because a file export is an
//! **output**: nothing flows back into the model (ADR-0015 §8).

use orchestra_core_model::Document;
use orchestra_core_time::invariant::convert_or;
use orchestra_core_time::{Duration, Position, Rational, RationalError, TempoChange, TimeAt};
use orchestra_playback::{PlayedNote, UnrollError, Unrolled};

use super::MidiError;

/// Ticks per whole note: 4 × 960 (ADR-0032 E3).
pub(crate) const TICKS_PER_WHOLE: i64 = 3_840;

/// Units of time per second: one unit is 1/960 of a microsecond, the time one tick lasts at a
/// tempo of one microsecond per quarter note (see the module documentation).
pub(crate) const UNITS_PER_SECOND: i64 = 960_000_000;

/// The smallest tempo a Standard MIDI File can write: 1 µs per quarter note.
const MIN_TEMPO: i64 = 1;

/// The largest tempo a Standard MIDI File can write: 2²⁴ − 1 µs per quarter note (about 3.6
/// quarter notes per minute).
const MAX_TEMPO: i64 = 16_777_215;

/// The step of the grid inside gradual changes: one 32nd note, in whole notes (checked at compile
/// time, see `orchestra_core_time::rational!`).
const GRID: Rational = orchestra_core_time::rational!(1, 32);

/// `num / den` rounded to the nearest integer, halves away from zero; `den` must be positive.
/// O(1).
pub(crate) const fn round_div(num: i128, den: i128) -> i128 {
    if num >= 0 {
        (2 * num + den) / (2 * den)
    } else {
        -((-2 * num + den) / (2 * den))
    }
}

/// The tick of a position on the unrolled timeline, **without** the lead-in: `round(p × 3 840)`.
///
/// # Errors
/// [`RationalError::Overflow`] if the tick does not fit an `i64` (a position of about 2⁵¹ whole
/// notes: never in a real score).
///
/// # Complexity
/// O(1).
pub(crate) fn ticks_of(position: Position) -> Result<i64, RationalError> {
    scaled_round(position.as_rational(), TICKS_PER_WHOLE)
}

/// `round(r × factor)`, halves away from zero (`round_div`), as an `i64`: the conversion of an
/// exact value into whole ticks or time units that the encoder makes everywhere.
///
/// The product is computed in `i128`: `|numerator| < 2⁶³` and `factor < 2³⁰` here, so it cannot
/// overflow. Shared by `ticks_of`, `units_of` and `notes::units_of_seconds`.
///
/// # Errors
/// [`RationalError::Overflow`] if the rounded result does not fit an `i64`.
///
/// # Complexity
/// O(1).
pub(crate) fn scaled_round(r: Rational, factor: i64) -> Result<i64, RationalError> {
    let rounded = round_div(
        i128::from(r.numerator()) * i128::from(factor),
        i128::from(r.denominator()),
    );
    i64::try_from(rounded).map_err(|_| RationalError::Overflow)
}

/// The time of the path in units (see the module documentation), rounded to the nearest unit.
///
/// # Errors
/// [`RationalError::Overflow`] if the time does not fit an `i64` of units (about 300 years) or an
/// approximate time is not finite.
fn units_of(time: TimeAt) -> Result<i64, RationalError> {
    match time {
        TimeAt::Exact(seconds) => scaled_round(seconds.as_rational(), UNITS_PER_SECOND),
        TimeAt::Approximate(seconds) => {
            // Inside a gradual change (see the module documentation). An `f64` has 53 bits of
            // mantissa, so an hour (3.5 × 10¹² units) keeps a resolution far below one unit.
            #[allow(
                clippy::cast_precision_loss,
                reason = "UNITS_PER_SECOND is below 2^53: exact in an f64"
            )]
            let units = (seconds * UNITS_PER_SECOND as f64).round();
            // The bound is checked in `f64` before converting: `as` saturates silently.
            #[allow(
                clippy::cast_precision_loss,
                reason = "the bound only needs to be approximately i64::MAX"
            )]
            let bound = i64::MAX as f64;
            if !units.is_finite() || units.abs() >= bound {
                return Err(RationalError::Overflow);
            }
            #[allow(
                clippy::cast_possible_truncation,
                reason = "finite, integral and within the i64 range, checked above"
            )]
            Ok(units as i64)
        }
    }
}

/// The time in units of a point of the unrolled timeline, from the path: the played bar that
/// holds it (the last one starting at or before it; the first one for a point before 0, an
/// acciaccatura before the first note) and the offset into it.
///
/// # Errors
/// The path's and the arithmetic errors.
///
/// # Complexity
/// O(log path) to find the bar, plus the path's O(log t) lookup.
pub(crate) fn time_of(doc: &Document, path: &Unrolled, at: Position) -> Result<i64, MidiError> {
    let played = path.played();
    let index = played.partition_point(|b| b.start <= at).saturating_sub(1);
    let start = played.get(index).map_or(Position::ZERO, |b| b.start);
    let offset = at.checked_duration_since(start)?;
    Ok(units_of(path.time_at(doc, index, offset)?)?)
}

/// The unrolled intervals that lie inside a gradual tempo change, sorted and disjoint: for every
/// played bar, its intersection with each gradual change of the tempo map, moved to the bar's
/// place on the unrolled timeline.
///
/// # Complexity
/// O(path × (log g + k)) for g gradual changes and k of them meeting a bar (k is at most a few).
fn gradual_intervals(
    doc: &Document,
    path: &Unrolled,
) -> Result<Vec<(Position, Position)>, MidiError> {
    // The gradual changes as written ranges, sorted by start (the tempo map's order); they do not
    // overlap (ADR-0020 §3), so their ends are sorted as well.
    let ranges: Vec<(Position, Position)> = doc
        .tempo_map()
        .changes()
        .iter()
        .filter_map(|(at, change)| match change {
            TempoChange::Gradual(g) => Some((*at, g.end)),
            TempoChange::Immediate(_) => None,
        })
        .collect();
    let mut out = Vec::new();
    if ranges.is_empty() {
        return Ok(out);
    }
    for (index, bar) in path.played().iter().enumerate() {
        let length = bar_length(path, index)?;
        let written_end = bar.written_start.checked_add(length)?;
        // The first range that ends after the bar's written start, then every following one that
        // starts before its written end.
        let first = ranges.partition_point(|(_, end)| *end <= bar.written_start);
        for (from, to) in ranges.iter().skip(first) {
            if *from >= written_end {
                break;
            }
            let a = (*from).max(bar.written_start);
            let b = (*to).min(written_end);
            let ua = bar
                .start
                .checked_add(a.checked_duration_since(bar.written_start)?)?;
            let ub = bar
                .start
                .checked_add(b.checked_duration_since(bar.written_start)?)?;
            out.push((ua, ub));
        }
    }
    Ok(out)
}

/// The length of the played bar at `index`: up to the next played bar's start, or to the end of
/// the path for the last one. O(1).
///
/// # Errors
/// The arithmetic error; [`UnrollError::Endless`] past the end of the path.
pub(crate) fn bar_length(path: &Unrolled, index: usize) -> Result<Duration, MidiError> {
    let played = path.played();
    let bar = played.get(index).ok_or(UnrollError::Endless)?;
    let end = played.get(index + 1).map_or(path.end(), |b| b.start);
    Ok(end.checked_duration_since(bar.start)?)
}

/// One stretch of constant tempo in the written file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    /// Its first tick (with the lead-in).
    tick: u64,
    /// The reader's time at that tick, in units from the start of the file.
    units: i64,
    /// The tempo from that tick on, in µs per quarter (= units per tick).
    tempo: i64,
}

/// The tempo of the file (see the module documentation): the tempo events to write and the
/// reader's clock they define, to place the controller curves of ADR-0032 E8 in seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TempoTrack {
    /// The stretches of constant tempo, by tick; the first at tick 0. Consecutive stretches may
    /// share a tempo (the events are written only where it changes).
    segments: Vec<Segment>,
    /// The ticks where a tempo had to be clamped to the format's range.
    pub(crate) clamped: Vec<u64>,
}

impl TempoTrack {
    /// Builds the tempo of the file for the path and the played notes (see the module
    /// documentation). `origin` is the position of tick 0 (0, or the lead-in's negative start)
    /// and `lead` the ticks of the lead-in.
    ///
    /// # Errors
    /// The path's and the arithmetic errors.
    ///
    /// # Complexity
    /// O(path × (log g + k) + n log g + B log B) for n notes and B breakpoints (B is about the
    /// number of tempo marks and shifts, plus 32 per whole note inside gradual changes, plus the
    /// notes there); plus O(log path) per breakpoint for its time.
    pub(crate) fn build<'a>(
        doc: &Document,
        path: &Unrolled,
        notes: impl Iterator<Item = &'a PlayedNote>,
        origin: Position,
        lead: i64,
    ) -> Result<Self, MidiError> {
        let gradual = gradual_intervals(doc, path)?;
        let mut points: Vec<Position> = vec![origin];
        // The end of the music: the path's end or the last note's end, whichever is later (an
        // arpeggio or an offset may reach past the last barline).
        let mut end = path.end();
        // Notes inside gradual changes, and the end.
        let inside = |p: Position| {
            let i = gradual.partition_point(|(_, b)| *b <= p);
            gradual.get(i).is_some_and(|(a, _)| *a <= p)
        };
        for note in notes {
            end = end.max(note.end);
            for p in [note.start, note.end] {
                if inside(p) {
                    points.push(p);
                }
            }
        }
        points.push(end);
        // Tempo changes and shifts, bar by bar.
        let changes = doc.tempo_map().changes();
        let mut previous_shift = None;
        for (index, bar) in path.played().iter().enumerate() {
            if previous_shift != Some(bar.shift) {
                points.push(bar.start);
            }
            previous_shift = Some(bar.shift);
            let written_end = bar.written_start.checked_add(bar_length(path, index)?)?;
            let first = changes.partition_point(|(at, _)| *at < bar.written_start);
            for (at, _) in changes.iter().skip(first) {
                if *at >= written_end {
                    break;
                }
                points.push(
                    bar.start
                        .checked_add(at.checked_duration_since(bar.written_start)?)?,
                );
            }
        }
        // The grid inside gradual changes, from the start of each played bar.
        for (index, bar) in path.played().iter().enumerate() {
            let length = bar_length(path, index)?;
            let bar_end = bar.start.checked_add(length)?;
            let first = gradual.partition_point(|(_, b)| *b <= bar.start);
            for (a, b) in gradual.iter().skip(first) {
                if *a >= bar_end {
                    break;
                }
                points.push(*a);
                points.push(*b);
                // The grid points strictly inside (a, b): bar start + m/32.
                let from = a
                    .checked_duration_since(bar.start)?
                    .as_rational()
                    .checked_div(GRID)?
                    .floor()
                    .saturating_add(1);
                let mut m = from.max(0);
                loop {
                    let p = bar.start.checked_add(Duration::from_rational(
                        GRID.checked_mul(Rational::from_integer(m)?)?,
                    ))?;
                    if p >= *b {
                        break;
                    }
                    points.push(p);
                    m = m.saturating_add(1);
                }
            }
        }
        points.retain(|p| *p >= origin);
        points.sort_unstable();
        points.dedup();
        Self::from_points(doc, path, &points, lead)
    }

    /// The stretches between sorted, distinct breakpoints (see the module documentation, "Exact
    /// stretches").
    fn from_points(
        doc: &Document,
        path: &Unrolled,
        points: &[Position],
        lead: i64,
    ) -> Result<Self, MidiError> {
        // (tick, units) of each breakpoint, ticks from the start of the file, units from the time
        // of the first breakpoint; two breakpoints on the same tick keep the first.
        let mut marks: Vec<(u64, i64)> = Vec::with_capacity(points.len());
        let mut base = None;
        for p in points {
            // The lead-in makes every tick non-negative (see `lead`): a negative one is a bug.
            let tick = convert_or(ticks_of(*p)?.saturating_add(lead), 0_u64);
            let time = time_of(doc, path, *p)?;
            let base_time = *base.get_or_insert(time);
            if marks.last().is_some_and(|(t, _)| *t == tick) {
                continue;
            }
            marks.push((tick, time.saturating_sub(base_time)));
        }
        let mut segments: Vec<Segment> = Vec::new();
        let mut clamped = Vec::new();
        // The reader's time at the current breakpoint, in units: where the tempo written so far
        // has actually brought it.
        let mut reached: i64 = 0;
        for pair in marks.windows(2) {
            let [(a, _), (b, target)] = *pair else {
                continue;
            };
            let delta = i64::try_from(b - a).map_err(|_| RationalError::Overflow)?;
            // `delta > 0`: the marks have distinct, increasing ticks.
            let wanted = target.saturating_sub(reached);
            let (mut low, k) = (wanted.div_euclid(delta), wanted.rem_euclid(delta));
            let mut high = low.saturating_add(1);
            if low < MIN_TEMPO || high > MAX_TEMPO {
                // Out of the format's range: one clamped tempo for the whole stretch. `reached`
                // below then follows what is really written, so the next stretch makes up for it.
                clamped.push(a);
                low = low.clamp(MIN_TEMPO, MAX_TEMPO);
                high = low;
            }
            let split = if low == high { 0 } else { k };
            segments.push(Segment {
                tick: a,
                units: reached,
                tempo: low,
            });
            reached = reached.saturating_add(low.saturating_mul(delta - split));
            if split > 0 {
                let tick = b - u64::try_from(split).map_err(|_| RationalError::Overflow)?;
                segments.push(Segment {
                    tick,
                    units: reached,
                    tempo: high,
                });
                reached = reached.saturating_add(high.saturating_mul(split));
            }
        }
        if segments.is_empty() {
            // A single breakpoint (an empty document): the first tempo of the score, from the
            // time a quarter note takes at position 0.
            let quarter = Position::from_rational(Rational::new(1, 4)?);
            let units =
                time_of(doc, path, quarter)?.saturating_sub(time_of(doc, path, Position::ZERO)?);
            segments.push(Segment {
                tick: 0,
                units: 0,
                tempo: units.clamp(MIN_TEMPO, MAX_TEMPO),
            });
        }
        // The tick of the first segment is that of the first breakpoint, 0 by construction (the
        // origin is the first point and maps to tick 0).
        Ok(Self { segments, clamped })
    }

    /// The tempo events to write: (tick, µs per quarter), only where the value changes. O(s).
    pub(crate) fn events(&self) -> Vec<(u64, u32)> {
        let mut out: Vec<(u64, u32)> = Vec::new();
        for s in &self.segments {
            // `tempo` is within 1 ..= 2²⁴ − 1 by construction.
            let tempo = convert_or(s.tempo, u32::MAX);
            if out.last().is_none_or(|(_, t)| *t != tempo) {
                out.push((s.tick, tempo));
            }
        }
        out
    }

    /// The reader's time at `tick`, in units from the start of the file. O(log s).
    pub(crate) fn units_at(&self, tick: u64) -> i64 {
        let i = self
            .segments
            .partition_point(|s| s.tick <= tick)
            .saturating_sub(1);
        self.segments.get(i).map_or(0, |s| {
            let ticks = i64::try_from(tick.saturating_sub(s.tick)).unwrap_or(i64::MAX);
            s.units.saturating_add(ticks.saturating_mul(s.tempo))
        })
    }

    /// The tick the reader reaches at `units` (from the start of the file), rounded to the
    /// nearest. O(log s).
    pub(crate) fn tick_at(&self, units: i64) -> u64 {
        let i = self
            .segments
            .partition_point(|s| s.units <= units)
            .saturating_sub(1);
        self.segments.get(i).map_or(0, |s| {
            let ticks = round_div(
                i128::from(units.saturating_sub(s.units)),
                i128::from(s.tempo.max(1)),
            );
            let ticks = u64::try_from(ticks.max(0)).unwrap_or(u64::MAX);
            s.tick.saturating_add(ticks)
        })
    }
}

#[cfg(test)]
mod tests {
    //! The arithmetic of the module on hand-computed values.

    use super::*;

    /// Rounding halves away from zero, both signs.
    #[test]
    fn rounding_is_half_away_from_zero() {
        assert_eq!(round_div(5, 2), 3);
        assert_eq!(round_div(-5, 2), -3);
        assert_eq!(round_div(4, 3), 1);
        assert_eq!(round_div(-4, 3), -1);
        assert_eq!(round_div(0, 7), 0);
    }

    /// Ticks of exact positions: binary values, triplets, a septuplet rounded, a negative one.
    #[test]
    fn ticks_of_positions() -> Result<(), RationalError> {
        let p = |n, d| Rational::new(n, d).map(Position::from_rational);
        assert_eq!(ticks_of(p(1, 4)?)?, 960);
        assert_eq!(ticks_of(p(1, 12)?)?, 320, "an eighth-note triplet");
        assert_eq!(ticks_of(p(1, 28)?)?, 137, "3840/28 = 137.14");
        assert_eq!(ticks_of(p(-1, 16)?)?, -240);
        Ok(())
    }

    /// The reader's clock and its inverse on two hand-made segments.
    #[test]
    fn clock_and_inverse() {
        let track = TempoTrack {
            segments: vec![
                Segment {
                    tick: 0,
                    units: 0,
                    tempo: 500_000,
                },
                Segment {
                    tick: 960,
                    units: 480_000_000,
                    tempo: 250_000,
                },
            ],
            clamped: Vec::new(),
        };
        assert_eq!(track.units_at(960), 480_000_000, "half a second");
        assert_eq!(track.units_at(1_920), 720_000_000);
        assert_eq!(track.tick_at(720_000_000), 1_920);
        assert_eq!(track.tick_at(240_000_000), 480);
        assert_eq!(track.events(), vec![(0, 500_000), (960, 250_000)]);
    }
}
