// SPDX-License-Identifier: GPL-3.0-or-later

//! The DAW move of a note: tolerance threshold, performance-only moves, pass-specific offsets
//! (ADR-0004, ADR-0028; SCALETTA F1.14b).
//!
//! # The rule (ADR-0028 §2–§3)
//! The DAW view knows where a note sounds on the unrolled timeline; the model knows where it is
//! written. When the user drags a played note, the DAW side computes the **onset offset** the user
//! wants, measured from the note's written start, with whatever the playback rules add (arpeggios,
//! acciaccaturas) already taken out (Q1). Then [`MovePlayed`] decides:
//!
//! - `|onset| ≤ threshold` of the note's track ([`crate::Tolerance`]): the onset offset of the
//!   notehead becomes `onset`, and **the score does not change**;
//! - beyond it, the **written position** moves to the grid point nearest to `written + onset`
//!   (the grid counted from the start of the bar it falls in, Q4), and the rest becomes the new
//!   onset offset. If the grid brings the note back to its own place, only the offset is stored.
//!
//! So the played start, `written + offset`, is always exactly where the user dropped the note;
//! only *how it is written down* changes.
//!
//! # When the score never changes
//! - [`MoveMode::PerformanceOnly`]: the user's **conscious choice** (owner, 2026-11-06, ADR-0028
//!   §4) to move only the performance, by any amount. Never a silent fallback: a move the rule
//!   cannot write is refused with a typed error, and the interface offers this mode instead.
//! - A move of **one pass** (`pass: Some(p)`): always performance-only (ADR-0028 §5), stored in
//!   the instrument's pass-offset table (`crate::pass_offsets`) as the difference from the inline
//!   offset, since playback adds the two.
//! - A notehead of a chord with more than one notehead, anywhere along its written tie chain (Q5):
//!   moving the event would drag the other noteheads along, and splitting a chord is an editorial
//!   choice the command does not make.
//!
//! # When a written move is refused (Q6, Q9, T1, T2)
//! Beyond the threshold the move is refused, changing nothing, for a grace note
//! ([`DocumentError::MoveOfGraceNote`]), a notehead some tie leads into
//! ([`DocumentError::MoveOfTiedNote`]), a tuplet note ([`DocumentError::MoveOfTupletNote`]), a
//! piece that would cross a barline ([`DocumentError::MoveCrossesBarline`]) or go past the last bar
//! ([`DocumentError::PastLastBar`]), and, through the local check that follows every command, a
//! destination already taken by another event of the voice ([`crate::Violation::Overlap`], V2).
//!
//! # Tie chains (Q7)
//! A written move takes the whole written chain **forward** from the dragged notehead (it is the
//! first one: T1 refuses the others), every event by the same amount, each with its grace notes,
//! as long as every piece still fits inside one bar.
//!
//! # Inside a gesture
//! A drag sends [`MovePlayed`] at every mouse movement inside a gesture (ADR-0026 §4): the
//! compaction keeps one old value per event and per pass entry, and each update starts from the
//! state the previous one left. Crossing the threshold back and forth simply moves the written
//! note back and forth.

use orchestra_core_time::{BarIndex, Duration, Position};

use crate::command::{self, EventChange};
use crate::document::{Document, DocumentError};
use crate::edit::Edit;
use crate::editor::{CommandError, Transaction};
use crate::event::Tie;
use crate::id::{BarId, EventId, NoteId, TrackId};
use crate::tolerance::Tolerance;

/// How a DAW move treats the score (ADR-0028 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveMode {
    /// The default rule: within the threshold an offset, beyond it a written move.
    ByThreshold,
    /// The user's conscious choice: always an offset, whatever its size; the score never changes.
    PerformanceOnly,
}

/// A note dragged in the DAW view (the input of [`crate::Command::MovePlayed`], ADR-0028 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MovePlayed {
    /// The notehead the played note is keyed by: the first notehead of its played tie chain, as
    /// `PlayedNote::note` reports it.
    pub note: NoteId,
    /// `Some(p)` to move it only in pass `p` (always performance-only); `None` for every pass.
    pub pass: Option<u32>,
    /// The onset offset the user wants, in whole notes from the written start of the note
    /// (negative = earlier), with what the playback rules add already taken out (Q1).
    pub onset: Duration,
    /// The step of the DAW's active grid, in whole notes, counted from the start of each bar; it
    /// must be positive when the move may become a written one (Q4).
    pub grid: Duration,
    /// The default rule or the performance-only choice.
    pub mode: MoveMode,
}

/// Executes [`crate::Command::MovePlayed`] (see the module documentation).
///
/// # Complexity
/// O(1) average for the note, O(tracks + overrides) for the threshold, O(log t) for a threshold in
/// milliseconds; a written move costs, per event of the tie chain, a meter-map lookup (O(log m))
/// and the move itself (O(voices + k + noteheads)), plus the local check of the cells touched.
///
/// # Errors
/// [`DocumentError::UnknownNote`] and the other lookups; [`DocumentError::GridNotPositive`]; the
/// refusals listed in the module documentation; the arithmetic errors (absurd values only).
pub(crate) fn move_played(
    tx: &mut Transaction<'_>,
    order: &MovePlayed,
) -> Result<(), CommandError> {
    let doc = tx.document();
    let (found, head) = doc
        .note(order.note)
        .ok_or(DocumentError::UnknownNote(order.note))?;
    let event = found.event().id();
    let is_grace = found.event().grace().is_some();
    let voice = found.voice();
    let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
    let instrument = instrument.id();

    // One pass only: always an offset, stored as the difference from the inline offset (playback
    // adds the two, ADR-0028 §5). The entry's length and velocity stay as they were.
    if let Some(pass) = order.pass {
        let table = doc
            .instrument(instrument)
            .ok_or(DocumentError::UnknownInstrument(instrument))?
            .pass_offsets();
        let mut entry = table.get(order.note, pass);
        entry.onset = order.onset.checked_sub(head.offsets().onset)?;
        return tx.apply(Edit::SetPassOffsets {
            instrument,
            note: order.note,
            pass,
            offsets: entry,
        });
    }

    let chain = written_chain(doc, order.note)?;
    // Q5: a chord with other noteheads anywhere along the chain keeps its written place.
    let lone_noteheads = chain.iter().all(|&(e, _)| {
        doc.event(e)
            .and_then(|found| found.event().chord().map(|c| c.notes().count() == 1))
            .unwrap_or(false)
    });
    let performance_only = order.mode == MoveMode::PerformanceOnly || !lone_noteheads;
    let written = doc.event_position(event)?;
    if performance_only
        || threshold_of(doc, voice)?.allows(doc.tempo_map(), written, order.onset)?
    {
        return set_onset(tx, event, order.note, order.onset);
    }

    // Beyond the threshold: the refusals that do not depend on the destination.
    if !order.grid.is_positive() {
        return Err(DocumentError::GridNotPositive.into());
    }
    if is_grace {
        return Err(DocumentError::MoveOfGraceNote.into());
    }
    if !doc.ties_into(order.note).is_empty() {
        return Err(DocumentError::MoveOfTiedNote.into());
    }
    if chain.iter().any(|&(e, _)| {
        doc.event(e)
            .is_some_and(|found| found.event().tuplet().is_some())
    }) {
        return Err(DocumentError::MoveOfTupletNote.into());
    }

    let new_start = quantise(doc, written, order.onset, order.grid)?;
    let delta = new_start.checked_duration_since(written)?;
    if delta.is_zero() {
        // The grid brought it back to its own place: only the offset changes.
        return set_onset(tx, event, order.note, order.onset);
    }
    // Plan every move first, so that a refusal changes nothing (the editor would revert anyway,
    // but planning keeps the errors in a predictable order: the first piece that does not fit).
    let mut moves: Vec<(EventId, BarId, Duration)> = Vec::with_capacity(chain.len());
    for &(e, _) in &chain {
        let to = doc.event_position(e)?.checked_add(delta)?;
        let length = doc.metric_duration(e)?;
        let (bar, offset) = place_in_bar(doc, to, length)?;
        moves.push((e, bar, offset));
    }
    for (e, bar, offset) in moves {
        command::change_event(tx, e, EventChange::Move { bar, offset })?;
    }
    // The residual: the played start stays exactly where the user dropped the note.
    let residual = order.onset.checked_sub(delta)?;
    set_onset(tx, event, order.note, residual)
}

/// Executes [`crate::Command::SetTolerance`]: the project's threshold (`track: None`) or a
/// track's override, removed with `tolerance: None` (ADR-0028 §1). O(tracks + overrides).
///
/// # Errors
/// [`DocumentError::UnknownTrack`], [`DocumentError::ToleranceNotPositive`],
/// [`DocumentError::ProjectToleranceRequired`].
pub(crate) fn set_tolerance(
    tx: &mut Transaction<'_>,
    track: Option<TrackId>,
    tolerance: Option<Tolerance>,
) -> Result<(), CommandError> {
    tx.apply(Edit::SetTolerance { track, tolerance })
}

/// The threshold in force for the track that plays `voice`. O(voices + tracks + overrides).
///
/// # Errors
/// [`DocumentError::UnknownVoice`] or [`DocumentError::Inconsistent`] (a voice without a track: a
/// bug of the model).
fn threshold_of(doc: &Document, voice: crate::id::VoiceId) -> Result<Tolerance, DocumentError> {
    let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
    let track = doc
        .track_of(instrument.id())
        .ok_or(DocumentError::Inconsistent)?;
    Ok(doc.tolerance().for_track(track.id()))
}

/// The written tie chain forward from `note`: each event and its notehead, following
/// [`Tie::To`] links. A [`Tie::LetRing`] or no tie ends it. `validate` keeps ties strictly forward
/// in score order, so the chain cannot loop; the length is bounded by the number of noteheads
/// anyway, as a defence against a bug. O(chain).
///
/// # Errors
/// [`DocumentError::UnknownNote`] for a link to a notehead that does not exist.
fn written_chain(doc: &Document, note: NoteId) -> Result<Vec<(EventId, NoteId)>, DocumentError> {
    let mut chain = Vec::new();
    let mut current = note;
    loop {
        let (found, head) = doc
            .note(current)
            .ok_or(DocumentError::UnknownNote(current))?;
        chain.push((found.event().id(), current));
        match head.tie() {
            Tie::To(next) if chain.len() <= doc.notes.len() => current = next,
            Tie::To(_) => return Err(DocumentError::Inconsistent),
            Tie::None | Tie::LetRing => return Ok(chain),
        }
    }
}

/// The grid point nearest to `written + onset` (ADR-0028 §3): the grid counted from the start of
/// the bar the point falls in, its end (the next bar's start) included, so a bar whose length is
/// not a multiple of the grid still has a point on its barline. A tie goes to the point **farther**
/// from the written position, so that rounding never undoes a move. A point before the start of
/// the score is taken at the start.
///
/// # Complexity
/// O(log m) for the meter-map lookups, O(1) arithmetic.
///
/// # Errors
/// [`DocumentError::PastLastBar`] for a point past the document's last bar; the arithmetic and
/// meter-map errors (absurd values only).
fn quantise(
    doc: &Document,
    written: Position,
    onset: Duration,
    grid: Duration,
) -> Result<Position, DocumentError> {
    let target = written.checked_add(onset)?.max(Position::ZERO);
    let at = doc.meter_map().locate(target)?;
    check_bar_exists(doc, at.bar)?;
    let bar_start = doc.meter_map().bar_start(at.bar)?;
    let bar_length = doc.meter_map().bar_length(at.bar)?;
    // floor(offset / grid) steps from the bar start, and the next point, clamped to the barline.
    let steps = at.offset.checked_ratio(grid)?.floor();
    let lower = grid.checked_mul(orchestra_core_time::Rational::from_integer(steps)?)?;
    let upper = lower.checked_add(grid)?.min(bar_length);
    let below = at.offset.checked_sub(lower)?;
    let above = upper.checked_sub(at.offset)?;
    let lower_point = bar_start.checked_add(lower)?;
    let upper_point = bar_start.checked_add(upper)?;
    Ok(match below.cmp(&above) {
        std::cmp::Ordering::Less => lower_point,
        std::cmp::Ordering::Greater => upper_point,
        // Halfway: the point farther from where the note is written.
        std::cmp::Ordering::Equal => {
            let from_lower = written.checked_duration_since(lower_point)?.abs();
            let from_upper = written.checked_duration_since(upper_point)?.abs();
            if from_upper >= from_lower {
                upper_point
            } else {
                lower_point
            }
        }
    })
}

/// The bar and offset where an event of `length` starting at `at` goes, if it fits inside one
/// bar of the document (Q6: a DAW move never makes a bar overfull). O(log m).
///
/// # Errors
/// [`DocumentError::PastLastBar`] past the last bar; [`DocumentError::MoveCrossesBarline`] if it
/// would reach past its bar's end; the meter-map errors.
fn place_in_bar(
    doc: &Document,
    at: Position,
    length: Duration,
) -> Result<(BarId, Duration), DocumentError> {
    let found = doc.meter_map().locate(at)?;
    let bar = check_bar_exists(doc, found.bar)?;
    let bar_length = doc.meter_map().bar_length(found.bar)?;
    if found.offset.checked_add(length)? > bar_length {
        return Err(DocumentError::MoveCrossesBarline);
    }
    Ok((bar, found.offset))
}

/// The ID of the bar at `index`, or [`DocumentError::PastLastBar`] if the document has no such
/// bar (the meter map itself covers infinitely many). O(1).
fn check_bar_exists(doc: &Document, index: BarIndex) -> Result<BarId, DocumentError> {
    doc.bars().id_at(index).ok_or(DocumentError::PastLastBar)
}

/// Sets the onset offset of one notehead, keeping its length and velocity offsets; nothing if it
/// already has that onset (no history for a mouse movement that changes nothing). O(event lookup)
/// + the replacement.
///
/// # Errors
/// The lookups' and the edit's.
fn set_onset(
    tx: &mut Transaction<'_>,
    event: EventId,
    note: NoteId,
    onset: Duration,
) -> Result<(), CommandError> {
    let (voice, bar, mut placed) = command::current(tx.document(), event)?;
    let chord = command::chord_mut(&mut placed, event)?;
    let Some(head) = chord.note_mut(note) else {
        return Err(DocumentError::UnknownNote(note).into());
    };
    let mut offsets = head.offsets();
    if offsets.onset == onset {
        return Ok(());
    }
    offsets.onset = onset;
    head.set_offsets(offsets);
    command::replace_event(tx, voice, bar, placed)
}
