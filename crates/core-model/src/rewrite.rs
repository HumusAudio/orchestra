// SPDX-License-Identifier: GPL-3.0-or-later

//! The meter change that rewrites the music: `MeterMode::Rewrite` (ADR-0026 §7, §11 R1–R11;
//! SCALETTA F1.11c, part 2b).
//!
//! # What it does
//! The music keeps its place in time and the barlines move. From the bar of the change up to the
//! next meter change, the next free bar or the end of the score (the **region**, R1), the old bars
//! are replaced by a new **grid** of bars (R2): an optional special first bar (a pickup or free bar
//! kept, or the bar `SetIrregularBar` asks for: R3, R4), then bars of the meter in force, then,
//! if the region does not end on a barline of the grid, an irregular last bar with what is left,
//! so that nothing after the region moves (at the end of the score the last bar keeps its full
//! length instead). Every event and every mark is placed by its position into the new bars.
//!
//! # The steps, in order
//! 1. The region and its old bars (start and length of each, relative to the region's start).
//! 2. R10: refused if a bar repeat touches the region.
//! 3. The grid, then the **splits** of R9: a form marking on an old barline that is no barline
//!    of the grid splits the new bar holding it into irregular bars (ADR-0025 §1), so the form
//!    and the played path do not change.
//! 4. Bars added at the end of the region if the grid has more bars than the region had.
//! 5. The events (R5, R6, R8), voice by voice and bar by bar in score order, each moved with one
//!    `Replace` to its new bar and offset; one that now crosses a barline of the grid becomes a
//!    tie chain (the first part keeps its IDs, the continuations get new ones and no performance
//!    offsets, ADR-0026 §7 rule 2), written with `split_values` inside its tuplet (Z3, Z5); its
//!    own ties go on from the last part, and the slurs that ended on it end on the last part.
//!    Whole-bar rests become rests of written values (R6).
//! 6. The marks with a point (R7) and the form markings (R9): removed, then inserted again at
//!    their new places. Two phases, because a mark moved into a place another mark has not left
//!    yet would be refused as a duplicate.
//! 7. The meter map of the region in one bulk edit (Z1), and the bars the grid no longer needs
//!    removed at the end of the region (they are empty by then).
//! 8. R12: if the region ended at a free bar, the old meter is set again at the first bar after
//!    the free bars, so that the change does not reach past them.
//!
//! The local check of the editor then verifies every cell and every mark the command touched.
//!
//! # Complexity
//! O(e + k) edits for the e events and k marks of the region (one `Replace` per event, two edits
//! per mark), each O(chunk) or O(list); the meter map is rebuilt once; never O(score) outside the
//! region. The history holds one old event per event moved (R11).

use orchestra_core_time::{
    BarIndex, BarKind, BarSpan, Duration, IrregularBar, MeterChange, Rational,
};

use crate::chain::{chain_ids, push_parts, tie_to_next};
use crate::command::replace_event_at;
use crate::document::{Document, DocumentError, is_whole_bar_rest};
use crate::edit::{Edit, Object};
use crate::editor::{CommandError, Transaction};
use crate::event::{Chord, Event, EventBody, Note, NoteValue, PlacedEvent, Rest, RestValue};
use crate::id::{BarId, VoiceId};
use crate::marking::GraceAttach;
use crate::marks::{Mark, ScorePoint};
use crate::tempo_mark::TempoMarkChange;

/// What the meter change entry at the first bar of the region becomes.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FirstMeter {
    /// `ChangeMeter`: this entry (`None` removes it).
    Set(Option<MeterChange>),
    /// `SetIrregularBar`: the entry the bar already has, if any, stays.
    Keep,
}

/// The new bars of the region: where each starts (relative to the region's start), how long it
/// is, whether it is irregular and whether it is counted.
#[derive(Debug, Clone, Default)]
struct Grid {
    /// The start of each new bar, relative to the region's start; strictly increasing, from 0.
    starts: Vec<Duration>,
    /// The length of each new bar.
    lengths: Vec<Duration>,
    /// The irregular setting of each new bar, `None` for a regular bar of the meter.
    irregular: Vec<Option<IrregularBar>>,
    /// Whether each new bar is left out of the displayed numbering (the second part of a split).
    uncounted: Vec<bool>,
}

impl Grid {
    /// The number of new bars. O(1).
    const fn len(&self) -> usize {
        self.starts.len()
    }

    /// Appends a bar. O(1) amortised.
    fn push(
        &mut self,
        length: Duration,
        irregular: Option<IrregularBar>,
        uncounted: bool,
    ) -> Result<(), DocumentError> {
        let start = match (self.starts.last(), self.lengths.last()) {
            (Some(s), Some(l)) => s.checked_add(*l)?,
            _ => Duration::ZERO,
        };
        self.starts.push(start);
        self.lengths.push(length);
        self.irregular.push(irregular);
        self.uncounted.push(uncounted);
        Ok(())
    }

    /// The end of the last bar, relative to the region's start. O(1).
    fn end(&self) -> Result<Duration, DocumentError> {
        match (self.starts.last(), self.lengths.last()) {
            (Some(s), Some(l)) => Ok(s.checked_add(*l)?),
            _ => Ok(Duration::ZERO),
        }
    }

    /// The new bar holding the relative position `p` and the offset of `p` in it. A position on a
    /// barline belongs to the bar starting there, or, with `at_end`, to the bar ending there (an
    /// `After` grace note, R5). A position past the last bar stays in the last bar, overfull (R8).
    /// O(log n).
    fn locate(&self, p: Duration, at_end: bool) -> Result<(usize, Duration), DocumentError> {
        let after = if at_end {
            self.starts.partition_point(|s| *s < p)
        } else {
            self.starts.partition_point(|s| *s <= p)
        };
        let k = after.saturating_sub(1);
        let start = self.starts.get(k).copied().unwrap_or(Duration::ZERO);
        Ok((k, p.checked_sub(start)?))
    }

    /// The new bar starting exactly at `p`, if any. O(log n).
    fn starting_at(&self, p: Duration) -> Option<usize> {
        self.starts.binary_search(&p).ok()
    }

    /// The new bar ending exactly at `p`, if any. O(log n).
    fn ending_at(&self, p: Duration) -> Result<Option<usize>, DocumentError> {
        if p == self.end()? {
            return Ok(self.len().checked_sub(1));
        }
        Ok(self.starting_at(p).and_then(|k| k.checked_sub(1)))
    }

    /// Whether `p` is the start of a new bar or the end of the last one. O(log n).
    fn is_barline(&self, p: Duration) -> Result<bool, DocumentError> {
        Ok(self.starting_at(p).is_some() || p == self.end()?)
    }

    /// Splits the new bars at the given positions, which lie strictly inside bars (R9). The parts
    /// of a regular or `AlignStart` bar are: the first `AlignStart`, the last `AlignEnd` (it ends
    /// a nominal bar), any middle one `AlignStart`; a pickup's first part stays `AlignEnd`; a free
    /// bar's parts stay free. Every part after the first is uncounted (ADR-0025 §1). O(n + c log c)
    /// for c cuts.
    fn split(&self, cuts: &mut Vec<Duration>) -> Result<Self, DocumentError> {
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Self::default();
        let mut next = cuts.iter().peekable();
        for k in 0..self.len() {
            let (Some(&start), Some(&length), Some(&irregular), Some(&uncounted)) = (
                self.starts.get(k),
                self.lengths.get(k),
                self.irregular.get(k),
                self.uncounted.get(k),
            ) else {
                return Err(DocumentError::Inconsistent);
            };
            let end = start.checked_add(length)?;
            let mut inside = Vec::new();
            while let Some(&&c) = next.peek() {
                if c >= end {
                    break;
                }
                if c > start {
                    inside.push(c);
                }
                next.next();
            }
            if inside.is_empty() {
                out.push(length, irregular, uncounted)?;
                continue;
            }
            let kind = irregular.map_or(BarKind::AlignStart, |i| i.kind);
            let mut edges = vec![start];
            edges.extend(inside);
            edges.push(end);
            let pieces = edges.len().saturating_sub(1);
            for (j, pair) in edges.windows(2).enumerate() {
                let [a, b] = pair else {
                    return Err(DocumentError::Inconsistent);
                };
                let piece_kind = match kind {
                    BarKind::Free => BarKind::Free,
                    _ if j == 0 && kind == BarKind::AlignEnd => BarKind::AlignEnd,
                    _ if j.saturating_add(1) == pieces => BarKind::AlignEnd,
                    _ => BarKind::AlignStart,
                };
                out.push(
                    b.checked_sub(*a)?,
                    Some(IrregularBar {
                        length: b.checked_sub(*a)?,
                        kind: piece_kind,
                    }),
                    uncounted || j > 0,
                )?;
            }
        }
        Ok(out)
    }
}

/// The region of a rewrite: its old bars and the new grid.
struct Region {
    /// The index of the first bar of the region.
    first: u32,
    /// The index of the first bar after the region (the number of bars at the end of the score).
    end: u32,
    /// The start of each old bar of the region, relative to the region's start.
    old_starts: Vec<Duration>,
    /// The length of each old bar of the region.
    old_lengths: Vec<Duration>,
    /// The total length of the old bars.
    total: Duration,
    /// The new bars.
    grid: Grid,
    /// The IDs of the new bars: the region's own, in order, then new ones.
    ids: Vec<BarId>,
}

impl Region {
    /// The old bar of the region with this ID, by its place in the region. O(1) average.
    fn old_index(&self, doc: &Document, bar: BarId) -> Option<usize> {
        let index = doc.bars().index_of(bar)?.get();
        (self.first..self.end)
            .contains(&index)
            .then(|| usize::try_from(index.saturating_sub(self.first)).ok())
            .flatten()
    }

    /// The position of a point of an old bar of the region, relative to the region's start, or
    /// `None` if the point is outside the region. O(1) average.
    fn position(
        &self,
        doc: &Document,
        point: ScorePoint,
    ) -> Result<Option<Duration>, DocumentError> {
        let Some(k) = self.old_index(doc, point.bar) else {
            return Ok(None);
        };
        let start = self.old_starts.get(k).ok_or(DocumentError::Inconsistent)?;
        Ok(Some(start.checked_add(point.offset)?))
    }

    /// A point moved into the new bars (R7): unchanged outside the region. O(log n).
    fn moved(&self, doc: &Document, point: ScorePoint) -> Result<ScorePoint, DocumentError> {
        let Some(p) = self.position(doc, point)? else {
            return Ok(point);
        };
        let (k, offset) = self.grid.locate(p, false)?;
        Ok(ScorePoint::new(self.bar(k)?, offset))
    }

    /// The ID of the new bar `k`. O(1).
    fn bar(&self, k: usize) -> Result<BarId, DocumentError> {
        self.ids.get(k).copied().ok_or(DocumentError::Inconsistent)
    }

    /// The start and the end of an old bar, relative to the region's start. O(1).
    fn old_bar(&self, k: usize) -> Result<(Duration, Duration), DocumentError> {
        let (Some(&s), Some(&l)) = (self.old_starts.get(k), self.old_lengths.get(k)) else {
            return Err(DocumentError::Inconsistent);
        };
        Ok((s, s.checked_add(l)?))
    }
}

/// Executes a rewrite from the bar at `at` (see the module documentation): `first` is the special
/// first bar of the grid (R3, R4), `meter_entry` what the meter change entry at `at` becomes, and
/// the grid's bars follow `change`'s meter, the one in force at `at` after the command.
///
/// # Errors
/// [`DocumentError::InsideBarRepeat`] (R10); [`DocumentError::NotWritable`] for a part of a split
/// no value can write; the meter map's errors (a pickup longer than the meter, R3); the edits'.
pub(crate) fn rewrite(
    tx: &mut Transaction<'_>,
    at: BarIndex,
    first: Option<IrregularBar>,
    meter_entry: FirstMeter,
    change: MeterChange,
) -> Result<(), CommandError> {
    let mut region = plan(tx.document(), at, first, change)?;
    // R12: the meter that must still be in force after the free bars that end the region.
    let resume = meter_after_free_bars(tx.document(), &region)?;
    let old_count = region.end.saturating_sub(region.first);
    let new_count = u32::try_from(region.grid.len()).map_err(|_| DocumentError::TooManyBars)?;

    // 4. New bars at the end of the region, if the grid needs more.
    if new_count > old_count {
        let extra = new_count.saturating_sub(old_count);
        let mut ids = Vec::with_capacity(usize::try_from(extra).unwrap_or(0));
        for _ in 0..extra {
            ids.push(BarId::from_counter(tx.new_id()?));
        }
        region.ids.extend_from_slice(&ids);
        tx.apply(Edit::InsertBars {
            at: BarIndex::new(region.end),
            ids,
            meter: BarSpan::empty(extra),
        })?;
    }

    // 5. The events.
    move_events(tx, &region)?;
    // 6. The marks and the form.
    move_marks(tx, &region)?;
    move_form(tx, &region)?;

    // 7. The meter map, then the bars no longer needed.
    let doc = tx.document();
    let span = new_span(doc, &region, meter_entry, old_count.max(new_count))?;
    let first_bar = region.bar(0)?;
    tx.apply(Edit::Meter(crate::edit::MeterEdit::Span {
        bar: first_bar,
        span,
    }))?;
    if new_count < old_count {
        tx.apply(Edit::RemoveBars {
            at: BarIndex::new(region.first.saturating_add(new_count)),
            count: old_count.saturating_sub(new_count),
        })?;
    }

    // 8. R12: if the region ended at a free bar, the meter after the free bars is put back as it
    //    was, with an explicit meter change, so that nothing after the region moves.
    if let Some((bar, old)) = resume {
        let doc = tx.document();
        let index = doc
            .bars()
            .index_of(bar)
            .ok_or(DocumentError::UnknownBar(bar))?;
        if meter_change_in_force(doc, index) != Some(old) {
            tx.apply(Edit::Meter(crate::edit::MeterEdit::MeterChange {
                bar,
                change: Some(old),
            }))?;
        }
    }
    Ok(())
}

/// R12 (owner's choice of 2026-11-06): where the meter must resume after the free bars that end
/// a region, and which meter, read **before** the rewrite.
///
/// # Why
/// A free bar suspends the meter without ending it (ADR-0018): after it, the meter in force goes
/// on. A region that ends at a free bar (R1) is rewritten alone, but the meter-map edit of the
/// change still reaches the bars after the free bar, up to the next meter change; without this
/// step they would take the new meter while keeping their music, as under `KeepBars`: overfull
/// bars, music no longer where it sounded (against R2, "nothing after the region moves"), and a
/// tie across one of their barlines could point backwards (the command property test found it).
///
/// Returns the first bar that is not free after the free bars at the region's end, with the meter
/// change in force there before the command; `None` if the region does not end at a free bar, if
/// the free bars reach the end of the score, or if that bar has a meter change of its own (it
/// then sets its meter itself). O(free bars + m) for the m meter changes.
fn meter_after_free_bars(
    doc: &Document,
    region: &Region,
) -> Result<Option<(BarId, MeterChange)>, DocumentError> {
    let map = doc.meter_map();
    let bars = u32::try_from(doc.bars().len()).map_err(|_| DocumentError::TooManyBars)?;
    let mut r = region.end;
    if r >= bars || !map.is_free(BarIndex::new(r))? {
        return Ok(None);
    }
    while r < bars && map.is_free(BarIndex::new(r))? {
        r = r.saturating_add(1);
    }
    if r >= bars || map.meter_changes().iter().any(|(b, _)| b.get() == r) {
        return Ok(None);
    }
    let index = BarIndex::new(r);
    let bar = doc.bars().id_at(index).ok_or(DocumentError::Inconsistent)?;
    Ok(meter_change_in_force(doc, index).map(|old| (bar, old)))
}

/// The meter change in force at a bar: the last entry of the meter map at or before it (with its
/// beat grouping). Always `Some` in a valid document, whose map has an entry at bar 0. O(m).
fn meter_change_in_force(doc: &Document, index: BarIndex) -> Option<MeterChange> {
    doc.meter_map()
        .meter_changes()
        .iter()
        .rev()
        .find(|(b, _)| *b <= index)
        .map(|(_, change)| *change)
}

/// Steps 1–3: the region, the refusal of R10, the grid with its splits.
///
/// # Complexity
/// O(bars of the region × log m) for the old bars, O(r) for the bar repeats, O(f) for the form
/// markings, O(n) for the grid.
fn plan(
    doc: &Document,
    at: BarIndex,
    first: Option<IrregularBar>,
    change: MeterChange,
) -> Result<Region, DocumentError> {
    let map = doc.meter_map();
    let bars = u32::try_from(doc.bars().len()).map_err(|_| DocumentError::TooManyBars)?;
    let i = at.get();
    // 1. The region: up to the next meter change or the next free bar after `at` (R1). Under
    //    `ChangeMeter`, a meter change at `at` itself is the one being replaced.
    let mut end = i.saturating_add(1);
    while end < bars {
        let has_change = map.meter_changes().iter().any(|(b, _)| b.get() == end);
        if has_change || map.is_free(BarIndex::new(end))? {
            break;
        }
        end = end.saturating_add(1);
    }
    // 2. R10: a bar repeat whose source or repeated bars meet the region.
    for repeat in doc
        .instruments()
        .flat_map(crate::document::Instrument::bar_repeats)
    {
        let (source, _, stop) = repeat.covered(doc.bars());
        if source < end && stop > i {
            return Err(DocumentError::InsideBarRepeat(repeat.id()));
        }
    }
    let start = map.bar_start(at)?;
    let mut old_starts = Vec::new();
    let mut old_lengths = Vec::new();
    for b in i..end {
        let index = BarIndex::new(b);
        old_starts.push(map.bar_start(index)?.checked_duration_since(start)?);
        old_lengths.push(map.bar_length(index)?);
    }
    let total = match (old_starts.last(), old_lengths.last()) {
        (Some(s), Some(l)) => s.checked_add(*l)?,
        _ => return Err(DocumentError::Inconsistent),
    };
    let at_score_end = end == bars;

    // 3. The grid (R2, R3, R4): the special first bar, whole bars of the meter, the rest.
    let nominal = change.meter.nominal_length()?;
    let mut grid = Grid::default();
    if let Some(first) = first {
        grid.push(first.length, Some(first), false)?;
    }
    loop {
        let covered = grid.end()?;
        if covered >= total {
            break;
        }
        let left = total.checked_sub(covered)?;
        if left >= nominal || at_score_end {
            grid.push(nominal, None, false)?;
        } else {
            grid.push(
                left,
                Some(IrregularBar {
                    length: left,
                    kind: BarKind::AlignStart,
                }),
                false,
            )?;
        }
    }
    // R9: the old barlines that carry a form marking must stay barlines.
    let region_ids: Vec<BarId> = doc
        .bars()
        .ids()
        .get(usize::try_from(i).unwrap_or(0)..usize::try_from(end).unwrap_or(0))
        .unwrap_or(&[])
        .to_vec();
    let mut region = Region {
        first: i,
        end,
        old_starts,
        old_lengths,
        total,
        grid: Grid::default(),
        ids: region_ids,
    };
    let mut cuts = Vec::new();
    for p in form_positions(doc, &region)? {
        // A form marking at the very end of the score, where the last bar keeps its full length,
        // ends the last bar there instead: no empty bar is left after the final barline.
        if at_score_end && p == total && grid.end()? > total {
            if let (Some(last), Some(start)) = (grid.lengths.last_mut(), grid.starts.last()) {
                let length = total.checked_sub(*start)?;
                *last = length;
                if let Some(irregular) = grid.irregular.last_mut() {
                    *irregular = Some(IrregularBar {
                        length,
                        kind: BarKind::AlignStart,
                    });
                }
            }
            continue;
        }
        if !grid.is_barline(p)? {
            cuts.push(p);
        }
    }
    region.grid = grid.split(&mut cuts)?;
    Ok(region)
}

/// The positions, relative to the region's start, of the old barlines of the region that carry a
/// form marking: the start of a bar for a start repeat, the first bar of a volta and a marker on
/// the start side; the end of a bar for an end repeat, the last bar of a volta, a marker on the end
/// side and a jump (R9). O(f) for the f form markings.
fn form_positions(doc: &Document, region: &Region) -> Result<Vec<Duration>, DocumentError> {
    let form = doc.form();
    let mut out = Vec::new();
    let mut at = |bar: BarId, end: bool| -> Result<(), DocumentError> {
        if let Some(k) = region.old_index(doc, bar) {
            let (s, e) = region.old_bar(k)?;
            out.push(if end { e } else { s });
        }
        Ok(())
    };
    for r in form.repeat_starts() {
        at(r.bar(), false)?;
    }
    for r in form.repeat_ends() {
        at(r.bar(), true)?;
    }
    for v in form.voltas() {
        at(v.first(), false)?;
        at(v.last(), true)?;
    }
    for m in form.markers() {
        at(m.bar(), !m.kind().at_start())?;
    }
    for j in form.jumps() {
        at(j.bar(), true)?;
    }
    Ok(out)
}

/// Step 5: every event of the region, moved into its new bar; tie chains where it now crosses a
/// barline of the grid; whole-bar rests rewritten (R5, R6, R8).
///
/// # Complexity
/// O(e) edits for the e events of the region, plus one edit per new part.
fn move_events(tx: &mut Transaction<'_>, region: &Region) -> Result<(), CommandError> {
    // Every event of the region is read before anything moves: the new bars reuse the region's
    // bar IDs, so reading an old bar after the moves into it began would read moved events again.
    let doc = tx.document();
    let mut all: Vec<(VoiceId, PlacedEvent, Duration, Duration)> = Vec::new();
    for voice in doc
        .instruments()
        .flat_map(crate::document::Instrument::voices)
    {
        for k in 0..region.old_starts.len() {
            let old_bar = BarIndex::new(
                region
                    .first
                    .saturating_add(u32::try_from(k).map_err(|_| DocumentError::TooManyBars)?),
            );
            let (bar_start, _) = region.old_bar(k)?;
            let bar_length = region
                .old_lengths
                .get(k)
                .copied()
                .ok_or(DocumentError::Inconsistent)?;
            for placed in voice.events_at(old_bar) {
                let p = bar_start.checked_add(placed.offset())?;
                all.push((voice.id(), placed.clone(), p, bar_length));
            }
        }
    }
    for (voice, placed, p, bar_length) in all {
        move_event(tx, region, voice, &placed, p, bar_length)?;
    }
    Ok(())
}

/// Moves one event of the region (see [`move_events`]). `start` is its position relative to the
/// region's start, `bar_length` the length of its old bar (for a whole-bar rest).
fn move_event(
    tx: &mut Transaction<'_>,
    region: &Region,
    voice: VoiceId,
    placed: &PlacedEvent,
    start: Duration,
    bar_length: Duration,
) -> Result<(), CommandError> {
    let event = placed.event();
    let grid = &region.grid;
    // Grace notes last zero and are never split: a `Before` grace goes where its main event
    // starts, an `After` grace where it ends (the end of a bar, if that is a barline).
    if let Some(grace) = event.grace() {
        let (bar, offset) = grid.locate(start, grace.attach == GraceAttach::After)?;
        return replace_event_at(tx, voice, region.bar(bar)?, offset, event.clone());
    }
    let doc = tx.document();
    let (_, content) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
    let whole = is_whole_bar_rest(event);
    let length = if whole {
        bar_length
    } else {
        content.metric_duration(event, bar_length)?
    };
    let end = start.checked_add(length)?;
    // The barlines of the grid strictly inside the event, before the end of the region: past
    // the region's end it stays overfull in the last bar (R8).
    // The starts are sorted: the ones strictly inside the event are one run, found by two binary
    // searches (O(log n) per event, instead of a scan of every new bar).
    let limit = end.min(region.total);
    let from = grid.starts.partition_point(|&c| c <= start);
    let to = grid.starts.partition_point(|&c| c < limit);
    let cuts: Vec<Duration> = grid.starts.get(from..to).unwrap_or(&[]).to_vec();
    let (bar, offset) = grid.locate(start, false)?;
    if cuts.is_empty() && !whole {
        return replace_event_at(tx, voice, region.bar(bar)?, offset, event.clone());
    }
    // The parts: (start, length) of each piece between the cuts.
    let mut edges = vec![start];
    edges.extend(cuts);
    edges.push(end);
    // The ratio of the tuplet chain: a written value lasts `ratio` times its length (Z3). A
    // whole-bar rest is in no tuplet.
    let ratio = if whole {
        Rational::ONE
    } else {
        let value = match event.body() {
            EventBody::Chord(c) => c.value(),
            EventBody::Rest(r) => match r.value() {
                RestValue::Value(value) => value,
                RestValue::WholeBar => NoteValue::plain(crate::event::BaseValue::Whole),
            },
        };
        length.checked_ratio(value.length()?)?
    };
    let mut parts: Vec<(usize, Duration, NoteValue)> = Vec::new();
    for pair in edges.windows(2) {
        let [from, to] = pair else {
            return Err(DocumentError::Inconsistent.into());
        };
        let (bar, offset) = grid.locate(*from, false)?;
        push_parts(&mut parts, bar, offset, to.checked_sub(*from)?, ratio)?;
    }
    write_parts(tx, region, voice, event, &parts)
}

/// Writes the parts of a split event (R5, R6): the first keeps the event's IDs, its noteheads,
/// their performance offsets and the event's markings; the others get new IDs, plain noteheads and
/// no markings (ADR-0026 §7 rule 2). Each notehead is tied to the same pitch of the next part;
/// the last part's noteheads take the original ties. Rests are not tied. The slurs that ended on
/// the event end on the last part.
///
/// # Complexity
/// O(parts × noteheads) edits and IDs, plus O(slurs of the event).
fn write_parts(
    tx: &mut Transaction<'_>,
    region: &Region,
    voice: VoiceId,
    event: &Event,
    parts: &[(usize, Duration, NoteValue)],
) -> Result<(), CommandError> {
    let originals: Vec<Note> = event
        .chord()
        .map(|c| c.notes().copied().collect())
        .unwrap_or_default();
    // Every ID first: the first part keeps the event's own IDs, the others are new.
    let own = (event.id(), originals.iter().map(Note::id).collect());
    let ids = chain_ids(tx, parts.len(), originals.len(), &[own])?;
    for (j, (&(bar, offset, value), (id, notes))) in parts.iter().zip(&ids).enumerate() {
        let body = match event.body() {
            EventBody::Chord(chord) => {
                let mut made = Vec::with_capacity(originals.len());
                for (n, (original, &note_id)) in originals.iter().zip(notes).enumerate() {
                    let mut note = if j == 0 {
                        *original
                    } else {
                        Note::new(note_id, original.pitch())
                    };
                    // To the next part; the last part keeps the tie the event had.
                    note.set_tie(tie_to_next(&ids, j, n, original.tie()));
                    made.push(note);
                }
                let like: Option<&Chord> = (j == 0).then_some(chord);
                EventBody::Chord(crate::command::chord_from(
                    value,
                    chord.tuplet(),
                    None,
                    made,
                    like,
                )?)
            }
            EventBody::Rest(rest) => {
                let tuplet = if is_whole_bar_rest(event) {
                    None
                } else {
                    rest.tuplet()
                };
                let mut made = Rest::new(RestValue::Value(value), tuplet);
                if j == 0 {
                    made.set_fermata(rest.fermata());
                }
                EventBody::Rest(made)
            }
        };
        let made = Event::new(*id, event.staff(), body);
        if j == 0 {
            replace_event_at(tx, voice, region.bar(bar)?, offset, made)?;
        } else {
            tx.apply(Edit::Insert(Object::Event {
                voice,
                bar: region.bar(bar)?,
                placed: PlacedEvent::new(offset, made),
                place: None,
            }))?;
        }
    }
    // The slurs ending on the event end on its last part.
    if let Some((last_id, _)) = ids.last()
        && *last_id != event.id()
    {
        let doc = tx.document();
        let mut edits = Vec::new();
        for &slur in doc.slurs_at(event.id()) {
            let (instrument, found) = doc.slur(slur).ok_or(DocumentError::Inconsistent)?;
            if found.end() == event.id() {
                edits.push(Edit::Replace(Object::Slur {
                    instrument: instrument.id(),
                    slur: found.with_end(*last_id),
                }));
            }
        }
        for edit in edits {
            tx.apply(edit)?;
        }
    }
    Ok(())
}

/// Step 6 for the marks with a point (R7): every mark with a point (or an end) in the region is
/// removed, then inserted again at its moved points. O(k) edits for the k marks concerned, each
/// O(list).
#[allow(
    clippy::too_many_lines,
    reason = "one short loop per kind of mark: splitting them would only scatter the rule"
)]
fn move_marks(tx: &mut Transaction<'_>, region: &Region) -> Result<(), CommandError> {
    let doc = tx.document();
    let touches = |point: ScorePoint| region.old_index(doc, point.bar).is_some();
    let mut moved: Vec<Object> = Vec::new();
    for instrument in doc.instruments() {
        let owner = instrument.id();
        for d in instrument.dynamics().iter().filter(|d| touches(d.at())) {
            moved.push(Object::Dynamic {
                instrument: owner,
                dynamic: d.with_point(region.moved(doc, d.at())?),
            });
        }
        for h in instrument
            .hairpins()
            .iter()
            .filter(|h| touches(h.start()) || touches(h.end()))
        {
            moved.push(Object::Hairpin {
                instrument: owner,
                hairpin: h.with_points(region.moved(doc, h.start())?, region.moved(doc, h.end())?),
            });
        }
        for r in instrument
            .pass_rules()
            .iter()
            .filter(|r| touches(r.start()) || touches(r.end()))
        {
            moved.push(Object::PassRule {
                instrument: owner,
                rule: r.with_points(region.moved(doc, r.start())?, region.moved(doc, r.end())?),
            });
        }
        for m in instrument
            .key_overrides()
            .as_slice()
            .iter()
            .filter(|m| touches(m.point()))
        {
            moved.push(Object::KeyOverride {
                instrument: owner,
                mark: m.moved_to(region.moved(doc, m.point())?),
            });
        }
        for staff in instrument.staves() {
            for m in staff
                .clef_changes()
                .as_slice()
                .iter()
                .filter(|m| touches(m.point()))
            {
                moved.push(Object::ClefChange {
                    staff: staff.id(),
                    mark: m.moved_to(region.moved(doc, m.point())?),
                });
            }
        }
    }
    for m in doc
        .key_changes()
        .as_slice()
        .iter()
        .filter(|m| touches(m.point()))
    {
        moved.push(Object::KeyChange(m.moved_to(region.moved(doc, m.point())?)));
    }
    for m in doc.tempo_marks().as_slice() {
        let gradual_end = match m.value().change() {
            Some(TempoMarkChange::Gradual(g)) => Some(g),
            _ => None,
        };
        if !touches(m.point()) && !gradual_end.is_some_and(|g| touches(g.end)) {
            continue;
        }
        let value = match gradual_end {
            Some(g) => m.value().with_gradual_end(region.moved(doc, g.end)?)?,
            None => m.value().clone(),
        };
        let to = region.moved(doc, m.point())?;
        moved.push(Object::TempoMark(Mark::new(
            m.id(),
            to.bar,
            to.offset,
            value,
        )));
    }
    reinsert(tx, moved)
}

/// Step 6 for the form markings (R9): each on an old bar of the region is removed, then inserted
/// again on the new bar that starts or ends where its old barline was (the splits of the grid
/// made sure there is one). O(f) edits.
fn move_form(tx: &mut Transaction<'_>, region: &Region) -> Result<(), CommandError> {
    let doc = tx.document();
    let grid = &region.grid;
    // The new bar starting (or ending) where the old bar started (or ended).
    let starting = |bar: BarId| -> Result<Option<BarId>, DocumentError> {
        let Some(k) = region.old_index(doc, bar) else {
            return Ok(None);
        };
        let (s, _) = region.old_bar(k)?;
        let new = grid.starting_at(s).ok_or(DocumentError::Inconsistent)?;
        Ok(Some(region.bar(new)?))
    };
    let ending = |bar: BarId| -> Result<Option<BarId>, DocumentError> {
        let Some(k) = region.old_index(doc, bar) else {
            return Ok(None);
        };
        let (_, e) = region.old_bar(k)?;
        let new = grid.ending_at(e)?.ok_or(DocumentError::Inconsistent)?;
        Ok(Some(region.bar(new)?))
    };
    let form = doc.form();
    let mut moved = Vec::new();
    for r in form.repeat_starts() {
        if let Some(bar) = starting(r.bar())? {
            moved.push(Object::RepeatStart(r.with_bar(bar)));
        }
    }
    for r in form.repeat_ends() {
        if let Some(bar) = ending(r.bar())? {
            moved.push(Object::RepeatEnd(r.with_bar(bar)));
        }
    }
    for v in form.voltas() {
        let (first, last) = (starting(v.first())?, ending(v.last())?);
        if first.is_some() || last.is_some() {
            moved.push(Object::Volta(
                v.with_bars(first.unwrap_or(v.first()), last.unwrap_or(v.last())),
            ));
        }
    }
    for m in form.markers() {
        let bar = if m.kind().at_start() {
            starting(m.bar())?
        } else {
            ending(m.bar())?
        };
        if let Some(bar) = bar {
            moved.push(Object::Marker(m.with_bar(bar)));
        }
    }
    for j in form.jumps() {
        if let Some(bar) = ending(j.bar())? {
            moved.push(Object::Jump(j.with_bar(bar)));
        }
    }
    reinsert(tx, moved)
}

/// Removes every object, then inserts each again with its new content (see step 6 of the module
/// documentation for why two phases). O(objects) edits.
fn reinsert(tx: &mut Transaction<'_>, objects: Vec<Object>) -> Result<(), CommandError> {
    for object in &objects {
        tx.apply(Edit::Remove(object.reference()))?;
    }
    for object in objects {
        tx.apply(Edit::Insert(object))?;
    }
    Ok(())
}

/// Step 7: the meter map data of the region's bars, `count` of them (the grid's, then empty bars
/// that are about to be removed): the meter change entry at the first bar, the irregular and the
/// uncounted bars of the grid, the numbering restarts moved to the new bar where their old bar
/// started (R9). Groupings and uncounted marks of the old bars are not kept (R1). O(n + r log n).
fn new_span(
    doc: &Document,
    region: &Region,
    meter_entry: FirstMeter,
    count: u32,
) -> Result<BarSpan, DocumentError> {
    let map = doc.meter_map();
    let mut span = BarSpan::empty(count);
    let entry = match meter_entry {
        FirstMeter::Set(change) => change,
        FirstMeter::Keep => map
            .meter_changes()
            .iter()
            .find(|(b, _)| b.get() == region.first)
            .map(|(_, c)| *c),
    };
    if let Some(change) = entry {
        span.set_meter_change(BarIndex::FIRST, change);
    }
    for (k, (irregular, uncounted)) in region
        .grid
        .irregular
        .iter()
        .zip(&region.grid.uncounted)
        .enumerate()
    {
        let bar = BarIndex::new(u32::try_from(k).map_err(|_| DocumentError::TooManyBars)?);
        if let Some(irregular) = irregular {
            span.set_irregular_bar(bar, *irregular);
        }
        if *uncounted {
            span.set_uncounted(bar, true);
        }
    }
    for &(b, number) in map.numbering_restarts() {
        if (region.first..region.end).contains(&b.get()) {
            let k = usize::try_from(b.get().saturating_sub(region.first))
                .map_err(|_| DocumentError::TooManyBars)?;
            let (start, _) = region.old_bar(k)?;
            let (new, _) = region.grid.locate(start, false)?;
            let bar = BarIndex::new(u32::try_from(new).map_err(|_| DocumentError::TooManyBars)?);
            span.set_numbering_restart(bar, Some(number));
        }
    }
    Ok(span)
}
