// SPDX-License-Identifier: GPL-3.0-or-later

//! The commands that change how long bars are: meter changes and irregular, pickup and free bars
//! (ADR-0026 §7, §11 Y1–Y6; SCALETTA F1.11c, part 1).
//!
//! # Two modes, one shape (Y1)
//! A meter change ([`Command::ChangeMeter`](crate::Command::ChangeMeter)) and a change of one
//! bar's length ([`Command::SetIrregularBar`](crate::Command::SetIrregularBar)) both take a
//! [`MeterMode`]: what happens to the music already written. ADR-0026 §7 names two:
//! - **`KeepBars`** (this part): every bar keeps its contents and offsets, only the bar lengths
//!   change; a shortened bar may become overfull (a valid state, ADR-0022 §4) and the music after
//!   it moves in time. O(1) in the voices: one edit of the meter map, plus the per-bar groupings
//!   that no longer fit (Y3).
//! - **`Rewrite`** (part 2, the default the interface will offer): the music keeps its place in
//!   time and the barlines move. It is added to [`MeterMode`] then, which is `#[non_exhaustive]`
//!   for that reason.
//!
//! # The decisions written down here (owner, 2026-11-05, ADR-0026 §11)
//! - **Y2**: `KeepBars` changes one thing in the meter map; events, marks and the form stay in
//!   their bars at their offsets. Marks past a shortened barline are fine: the tempo map already
//!   clamps a tempo mark to its barline (F1.09c), and the other marks follow their bar like the
//!   events of an overfull bar.
//! - **Y3**: the per-bar beat groupings of the section that no longer add up to the new meter are
//!   removed in the same command (they described how the old meter was counted).
//! - **Y4**: a pickup longer than the new meter, or a section grouping that does not add up to it,
//!   is refused with the meter map's typed error: the request contradicts itself.
//! - **Y5**: inside a bar repeat the change is refused ([`DocumentError::InsideBarRepeat`]): it
//!   would make a repeated bar differ in length from its source.
//! - **Y7** (owner, 2026-11-05, found by the property test): a change that leaves a tied or
//!   slurred note past a shortened barline, so that its tie or slur no longer goes forward in score
//!   order (which is by position, ADR-0024), is **refused** with the violation; `Rewrite`, which
//!   moves the barlines instead, has no such case.
//! - **Y6**: no bulk edit of the meter map here: `KeepBars` makes edits in sequence, each valid on
//!   its own. The bulk edit comes with `Rewrite`.

use orchestra_core_time::{BarIndex, BarKind, IrregularBar, MeterChange};

use crate::document::{Document, DocumentError};
use crate::edit::{Edit, MeterEdit};
use crate::editor::{CommandError, Transaction};
use crate::id::BarId;
use crate::rewrite::FirstMeter;

/// What a change of bar lengths does to the music already written (ADR-0026 §7, Y1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MeterMode {
    /// The music keeps its place in time and the barlines move, up to the next meter change, the
    /// next free bar or the end of the score (ADR-0026 §7, R1–R11; `crate::rewrite`). What the
    /// interface offers by default, as Dorico, `MuseScore` and Sibelius do.
    Rewrite,
    /// Every bar keeps its contents and offsets; only the bar lengths change, and the music after
    /// the change moves in time (Dorico's Insert mode counterpart).
    KeepBars,
}

/// The meter change in force at `at`, with its section grouping: the last entry at or before
/// it. O(m) for the m meter changes.
///
/// # Errors
/// [`DocumentError::Inconsistent`] if there is none (the map always has one at bar 0).
fn change_in_force(doc: &Document, at: BarIndex) -> Result<MeterChange, DocumentError> {
    doc.meter_map()
        .meter_changes()
        .iter()
        .rev()
        .find(|(b, _)| *b <= at)
        .map(|(_, c)| *c)
        .ok_or(DocumentError::Inconsistent)
}

/// Refuses a change of bar lengths inside a bar repeat (Y5).
///
/// A bar repeat covers its source bars (the `span` bars before its first repeated bar) and its
/// repeated bars, `[source, end)`. Changing the length of one bar (`whole_range` false: an
/// irregular bar) breaks it anywhere in that range. A meter change at a bar (`whole_range` true)
/// changes that bar and the ones after it, so it breaks the repeat only strictly after the first
/// source bar: at the first source bar every bar of the range changes alike.
///
/// # Complexity
/// O(r) for the r bar repeats of the score (few).
///
/// # Errors
/// [`DocumentError::InsideBarRepeat`] naming the first repeat concerned.
fn check_bar_repeats(doc: &Document, at: BarIndex, whole_range: bool) -> Result<(), DocumentError> {
    let at = at.get();
    for repeat in doc
        .instruments()
        .flat_map(crate::document::Instrument::bar_repeats)
    {
        let first = crate::form::index(doc.bars(), repeat.first());
        let source = first.saturating_sub(repeat.span().bars());
        let end = first.saturating_add(repeat.count());
        let inside = if whole_range {
            source < at && at < end
        } else {
            source <= at && at < end
        };
        if inside {
            return Err(DocumentError::InsideBarRepeat(repeat.id()));
        }
    }
    Ok(())
}

/// Executes [`Command::ChangeMeter`](crate::Command::ChangeMeter): sets the meter change at
/// `bar` (or removes it with `None`, never at bar 0) in the given mode.
///
/// # How (`KeepBars`)
/// 1. Y5: refused inside a bar repeat.
/// 2. Y3: the per-bar groupings of the section (from `bar` up to the next meter change, which
///    stays) that do not add up to the meter now in force there are removed, one edit each.
/// 3. The meter map edit itself; the map refuses what contradicts it (Y4), and the command is then
///    reverted as a whole.
///
/// The bar starts after `bar` move, so the tempo map is derived again when the transaction ends.
///
/// # Complexity
/// O(r) for the bar repeats, O(g) for the g per-bar groupings, then one meter map rebuild per edit
/// (O(m log m) for its m entries). Never O(voices).
///
/// # Errors
/// [`DocumentError::UnknownBar`], [`DocumentError::InsideBarRepeat`], the meter map's (wrapped in
/// [`DocumentError::MeterMap`]: no meter at the start, a pickup longer than its meter, a grouping
/// that does not add up).
pub(crate) fn change_meter(
    tx: &mut Transaction<'_>,
    bar: BarId,
    change: Option<MeterChange>,
    mode: MeterMode,
) -> Result<(), CommandError> {
    let doc = tx.document();
    let at = doc.bar_index(bar)?;
    if mode == MeterMode::Rewrite {
        // The meter of the grid: the new one, or (removing the change) the one before it.
        let meter = if let Some(change) = change {
            change
        } else {
            let before = at.get().checked_sub(1).ok_or(DocumentError::MeterMap(
                orchestra_core_time::MeterMapError::NoMeterAtStart,
            ))?;
            change_in_force(doc, BarIndex::new(before))?
        };
        // R3: a pickup or a free bar at the change stays as the grid's first bar.
        let first = doc
            .meter_map()
            .irregular_bars()
            .iter()
            .find(|(b, i)| *b == at && matches!(i.kind, BarKind::AlignEnd | BarKind::Free))
            .map(|(_, i)| *i);
        return crate::rewrite::rewrite(tx, at, first, FirstMeter::Set(change), meter);
    }
    check_bar_repeats(doc, at, true)?;
    let map = doc.meter_map();
    // The meter that will be in force at `at`: the new one, or (when removing) the one before.
    let meter = if let Some(change) = change {
        change.meter
    } else {
        let before = at.get().checked_sub(1).ok_or(DocumentError::MeterMap(
            orchestra_core_time::MeterMapError::NoMeterAtStart,
        ))?;
        map.meter_at(BarIndex::new(before))?
    };
    // The section: up to the next meter change after `at`, which keeps its own meter.
    let next = map
        .meter_changes()
        .iter()
        .map(|(b, _)| b.get())
        .find(|&b| b > at.get())
        .unwrap_or(u32::MAX);
    let nominal = meter.nominal_length()?;
    let mut stale = Vec::new();
    for &(b, grouping) in map.bar_groupings() {
        if (at.get()..next).contains(&b.get()) && grouping.total()? != nominal {
            stale.push(doc.bars().id_at(b).ok_or(DocumentError::Inconsistent)?);
        }
    }
    for stale_bar in stale {
        tx.apply(Edit::Meter(MeterEdit::BarGrouping {
            bar: stale_bar,
            grouping: None,
        }))?;
    }
    tx.apply(Edit::Meter(MeterEdit::MeterChange { bar, change }))?;
    Ok(check_overfull(tx.document(), at.get(), next)?)
}

/// Executes [`Command::SetIrregularBar`](crate::Command::SetIrregularBar): makes `bar` irregular
/// (a bar of another length, a pickup or a free bar), or regular again with `None`, in the given
/// mode.
///
/// # How (`KeepBars`)
/// 1. Y5: refused inside a bar repeat, its source included.
/// 2. Y3: a free bar has no beats, so its per-bar grouping, if any, is removed first. (A per-bar
///    grouping adds up to the meter, not to the bar's own length, so no other case breaks one.)
/// 3. The meter map edit; the map refuses a non-positive length or a pickup longer than its meter
///    (Y4).
/// 4. Y7, as for a meter change ([`check_overfull`]).
///
/// # Complexity
/// O(r) for the bar repeats, then one or two meter map rebuilds. Never O(voices).
///
/// # Errors
/// [`DocumentError::UnknownBar`], [`DocumentError::InsideBarRepeat`], the meter map's.
pub(crate) fn set_irregular_bar(
    tx: &mut Transaction<'_>,
    bar: BarId,
    irregular: Option<IrregularBar>,
    mode: MeterMode,
) -> Result<(), CommandError> {
    let doc = tx.document();
    let at = doc.bar_index(bar)?;
    if mode == MeterMode::Rewrite {
        // R4: the grid starts with the bar asked for (or a regular one), in the meter in force.
        let meter = change_in_force(doc, at)?;
        return crate::rewrite::rewrite(tx, at, irregular, FirstMeter::Keep, meter);
    }
    check_bar_repeats(doc, at, false)?;
    let has_grouping = doc
        .meter_map()
        .bar_groupings()
        .iter()
        .any(|(b, _)| *b == at);
    if has_grouping && irregular.is_some_and(|i| i.kind == BarKind::Free) {
        tx.apply(Edit::Meter(MeterEdit::BarGrouping {
            bar,
            grouping: None,
        }))?;
    }
    tx.apply(Edit::Meter(MeterEdit::IrregularBar { bar, irregular }))?;
    Ok(check_overfull(
        tx.document(),
        at.get(),
        at.get().saturating_add(1),
    )?)
}

/// Checks the ties and slurs of the events that now lie past the end of their bar, in the bars
/// `[from, to)` whose length changed (Y7).
///
/// # Why
/// Score order, which a tie and a slur must go forward in (rules 3 and 5), is by **position**
/// (ADR-0024). `KeepBars` leaves every event at its offset, so an event past the new end of a
/// shortened bar now starts at or after the start of the next bar: a tie from it into the next
/// bar no longer goes forward. Only such events can change order with others (every other event
/// keeps its place relative to its neighbours), so the local check of `check_cells` runs on the
/// cells holding one, and refuses the change with the violation.
///
/// # Complexity
/// O(bars × voices) to look at the last event of each chunk of the range (chunks are sorted by
/// offset), plus the local check on the few overfull cells. Never O(events).
///
/// # Errors
/// [`DocumentError::Invalid`] with the first violation found.
fn check_overfull(doc: &Document, from: u32, to: u32) -> Result<(), DocumentError> {
    let mut cells = crate::change::ChangeSet::default();
    let end = to.min(u32::try_from(doc.bars().len()).map_err(|_| DocumentError::TooManyBars)?);
    for index in from..end {
        let at = BarIndex::new(index);
        let length = doc.meter_map().bar_length(at)?;
        let bar = doc.bars().id_at(at).ok_or(DocumentError::Inconsistent)?;
        for voice in doc
            .instruments()
            .flat_map(crate::document::Instrument::voices)
        {
            if voice
                .events_at(at)
                .last()
                .is_some_and(|placed| placed.offset() >= length)
            {
                cells.cell(voice.id(), bar);
            }
        }
    }
    doc.check_cells(&cells)
}
