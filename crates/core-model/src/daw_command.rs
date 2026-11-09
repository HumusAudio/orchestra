// SPDX-License-Identifier: GPL-3.0-or-later

//! The DAW move of a note: tolerance threshold, performance-only moves, pass-specific offsets
//! (ADR-0004, ADR-0028; SCALETTA F1.14b).
//!
//! # The rule (ADR-0028 §2–§3)
//! The DAW view knows where a note sounds on the unrolled timeline; the model knows where it is
//! written. When the user drags a played note, the DAW side computes the **onset offset** the user
//! wants, measured from the note's written start, with whatever the playback rules add (arpeggios,
//! acciaccaturas) already taken out (ADR-0028 Q1). Then [`MovePlayed`] decides:
//!
//! - `|onset| ≤ threshold` of the note's track ([`crate::Tolerance`]): the onset offset of the
//!   notehead becomes `onset`, and **the score does not change**;
//! - beyond it, the **written position** moves to the grid point nearest to `written + onset`
//!   (the grid counted from the start of the bar it falls in, ADR-0028 Q4), and the rest becomes
//!   the new onset offset. If the grid brings the note back to its own place, only the offset is
//!   stored.
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
//! - A notehead of a chord with more than one notehead, anywhere along its written tie chain
//!   (ADR-0028 Q5): moving the event would drag the other noteheads along, and splitting a chord is
//!   an editorial choice the command does not make.
//!
//! # When a written move is refused (ADR-0028 Q6, ADR-0028 Q9, ADR-0028 T1, ADR-0028 T2)
//! Beyond the threshold the move is refused, changing nothing, for a grace note
//! ([`DocumentError::MoveOfGraceNote`]), a notehead some tie leads into
//! ([`DocumentError::MoveOfTiedNote`]), a tuplet note ([`DocumentError::MoveOfTupletNote`]), a
//! piece that would cross a barline ([`DocumentError::MoveCrossesBarline`]) or go past the last bar
//! ([`DocumentError::PastLastBar`]), and, through the local check that follows every command, a
//! destination already taken by another event of the voice ([`crate::Violation::Overlap`],
//! ADR-0026 V2).
//!
//! # Tie chains (ADR-0028 Q7)
//! A written move takes the whole written chain **forward** from the dragged notehead (it is the
//! first one: ADR-0028 T1 refuses the others), every event by the same amount, each with its grace
//! notes, as long as every piece still fits inside one bar.
//!
//! # Durations (ADR-0028 §6 and §11, U1–U9; SCALETTA F1.14c)
//! [`ResizePlayed`] is the same rule for the end of a note: the DAW side gives the **length
//! offset** it wants (from what the playback rules give) and the factor the rules scale the
//! written length by (`scale`: ½ for a staccato, 1 without shortening articulations, ADR-0028 U1).
//! Within the threshold it is the chain's length offset; beyond it the written end goes to the
//! nearest grid point (at least one grid step after the start, ADR-0028 U2), the chain is
//! **rewritten** with its new length, and the rest, `length − Δ × scale`, stays as the offset: the
//! played end lands exactly where the user dropped it.
//!
//! # Rewriting a tie chain (ADR-0028 U3, ADR-0028 U4, ADR-0028 U9)
//! A new length, or a move with [`MoveMode::SplitAtBarlines`] (the "move anyway" of ADR-0028 U5),
//! rewrites the written chain from a start with a total length: cut at every barline and written
//! with the longest values first (ADR-0026 V3), the pieces tied. The existing pieces' IDs are
//! **reused in order** (the first keeps its markings, offsets and the tie leading into it); only
//! extra pieces get new IDs and surplus ones are deleted, so a drag back and forth creates no ID at
//! every mouse movement. The other pieces are plain, with no offsets, so the residual on the first
//! notehead is the whole length offset of the chain (ADR-0027 N8). Slurs ending on the old last
//! piece end on the new one; `After` grace notes follow the new end, `Before` ones the first piece;
//! grace notes of the pieces in between are deleted. A length past a barline is allowed
//! (ADR-0028 U4); an occupied space is refused by the local check (ADR-0026 V2).
//!
//! # Inside a gesture
//! A drag sends [`MovePlayed`] at every mouse movement inside a gesture (ADR-0026 §4): the
//! compaction keeps one old value per event and per pass entry, and each update starts from the
//! state the previous one left. Crossing the threshold back and forth simply moves the written
//! note back and forth.

use orchestra_core_time::{BarIndex, Duration, Position, Rational};

use crate::chain::{PartIds, chain_ids, push_parts, tie_to_next};
use crate::command::{self, EventChange};
use crate::document::{Document, DocumentError};
use crate::edit::{Edit, Object, ObjectRef};
use crate::editor::{CommandError, Transaction};
use crate::event::{Event, EventBody, Note, NoteValue, PlacedEvent, Tie};
use crate::id::{BarId, EventId, InstrumentId, NoteId, TrackId, VoiceId};
use crate::marking::{GraceAttach, PerformanceOffsets};
use crate::tolerance::Tolerance;

/// How a DAW move treats the score (ADR-0028 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveMode {
    /// The default rule: within the threshold an offset, beyond it a written move.
    ByThreshold,
    /// The user's conscious choice: always an offset, whatever its size; the score never changes.
    PerformanceOnly,
    /// "Move anyway" (ADR-0028 U5): as [`ByThreshold`](Self::ByThreshold), but a written move whose
    /// piece would cross a barline rewrites the chain as tied notes instead of being refused. For
    /// [`ResizePlayed`] it is the same as `ByThreshold`, which always splits at barlines
    /// (ADR-0028 U4).
    SplitAtBarlines,
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
    /// (negative = earlier), with what the playback rules add already taken out (ADR-0028 Q1).
    pub onset: Duration,
    /// The step of the DAW's active grid, in whole notes, counted from the start of each bar; it
    /// must be positive when the move may become a written one (ADR-0028 Q4).
    pub grid: Duration,
    /// The default rule or the performance-only choice.
    pub mode: MoveMode,
}

/// The end of a note dragged in the DAW view (the input of [`crate::Command::ResizePlayed`],
/// ADR-0028 §11, U8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResizePlayed {
    /// The notehead the played note is keyed by (as in [`MovePlayed`]).
    pub note: NoteId,
    /// `Some(p)` to change it only in pass `p` (always performance-only); `None` for every pass.
    pub pass: Option<u32>,
    /// The length offset the user wants for the whole played note, in whole notes: the played
    /// length minus what the playback rules give (negative = shorter).
    pub length: Duration,
    /// The factor the playback rules scale the written length by for this note (½ for a staccato,
    /// 1 without shortening articulations, ADR-0028 U1); strictly positive. A written change of Δ
    /// changes the played length by Δ × `scale`.
    pub scale: Rational,
    /// The step of the DAW's active grid, as in [`MovePlayed`].
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
    let Target {
        event,
        is_grace,
        voice,
        instrument,
        head,
    } = target(doc, order.note)?;

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
    // ADR-0028 Q5: a chord with other noteheads anywhere along the chain keeps its written place.
    let lone_noteheads = lone_noteheads(doc, &chain);
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
    let mut crosses = false;
    for &(e, _) in &chain {
        let to = doc.event_position(e)?.checked_add(delta)?;
        let length = doc.metric_duration(e)?;
        match place_in_bar(doc, to, length) {
            Ok((bar, offset)) => moves.push((e, bar, offset)),
            // "Move anyway" (ADR-0028 U5): the whole chain is rewritten as ties from the new start.
            Err(DocumentError::MoveCrossesBarline) if order.mode == MoveMode::SplitAtBarlines => {
                crosses = true;
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    if crosses {
        let total = chain_end(doc, &chain)?.checked_duration_since(written)?;
        rewrite_chain(tx, &chain, new_start, total)?;
    } else {
        for (e, bar, offset) in moves {
            command::change_event(tx, e, EventChange::Move { bar, offset })?;
        }
    }
    // The residual: the played start stays exactly where the user dropped the note.
    let residual = order.onset.checked_sub(delta)?;
    set_onset(tx, event, order.note, residual)
}

/// Executes [`crate::Command::ResizePlayed`] (see the module documentation).
///
/// # Complexity
/// As [`move_played`] for the lookups and the threshold (measured at the written end); a written
/// change rewrites the chain: O(pieces) edits, each O(voices + k), plus the local check.
///
/// # Errors
/// [`DocumentError::UnknownNote`] and the other lookups; [`DocumentError::GridNotPositive`],
/// [`DocumentError::ScaleNotPositive`]; [`DocumentError::MoveOfTupletNote`] beyond the threshold
/// (ADR-0028 U6); [`DocumentError::PastLastBar`]; [`DocumentError::NotWritable`] for a length no
/// sum of written values expresses (a grid of triplets); an overlap through the local check.
pub(crate) fn resize_played(
    tx: &mut Transaction<'_>,
    order: &ResizePlayed,
) -> Result<(), CommandError> {
    let doc = tx.document();
    let Target {
        event,
        is_grace,
        voice,
        instrument,
        head,
    } = target(doc, order.note)?;
    let chain = written_chain(doc, order.note)?;
    // The inline length offsets of the noteheads after the first: ADR-0027 N8 adds them all, so the
    // first notehead's share is what is left.
    let mut others = Duration::ZERO;
    for &(_, n) in chain.iter().skip(1) {
        let (_, head) = doc.note(n).ok_or(DocumentError::UnknownNote(n))?;
        others = others.checked_add(head.offsets().length)?;
    }

    // One pass only: the entry of the first notehead takes what the inline offsets and the other
    // noteheads' entries of that pass do not already give.
    if let Some(pass) = order.pass {
        let table = doc
            .instrument(instrument)
            .ok_or(DocumentError::UnknownInstrument(instrument))?
            .pass_offsets();
        let mut taken = others;
        for &(_, n) in &chain {
            if n != order.note {
                taken = taken.checked_add(table.get(n, pass).length)?;
            }
        }
        taken = taken.checked_add(head.offsets().length)?;
        let mut entry = table.get(order.note, pass);
        entry.length = order.length.checked_sub(taken)?;
        return tx.apply(Edit::SetPassOffsets {
            instrument,
            note: order.note,
            pass,
            offsets: entry,
        });
    }

    let lone_noteheads = lone_noteheads(doc, &chain);
    let performance_only = order.mode == MoveMode::PerformanceOnly || !lone_noteheads || is_grace;
    let start = doc.event_position(event)?;
    let end = chain_end(doc, &chain)?;
    if performance_only || threshold_of(doc, voice)?.allows(doc.tempo_map(), end, order.length)? {
        return set_length(tx, event, order.note, order.length.checked_sub(others)?);
    }

    // Beyond the threshold.
    if !order.grid.is_positive() {
        return Err(DocumentError::GridNotPositive.into());
    }
    if !order.scale.is_positive() {
        return Err(DocumentError::ScaleNotPositive.into());
    }
    if chain.iter().any(|&(e, _)| {
        doc.event(e)
            .is_some_and(|found| found.event().tuplet().is_some())
    }) {
        return Err(DocumentError::MoveOfTupletNote.into());
    }
    // The written change that would give the whole offset, then the grid (ADR-0028 U1,
    // ADR-0028 U2).
    let wanted = order.length.checked_div(order.scale)?;
    let mut new_end = quantise_end(doc, end, wanted, order.grid)?;
    let shortest = start.checked_add(order.grid)?;
    if new_end < shortest {
        new_end = shortest;
    }
    let delta = new_end.checked_duration_since(end)?;
    if delta.is_zero() {
        return set_length(tx, event, order.note, order.length.checked_sub(others)?);
    }
    rewrite_chain(tx, &chain, start, new_end.checked_duration_since(start)?)?;
    // The rewritten pieces after the first are plain: the first notehead carries the whole
    // residual (ADR-0028 U3).
    let residual = order.length.checked_sub(delta.checked_mul(order.scale)?)?;
    set_length(tx, event, order.note, residual)
}

/// What a DAW move or resize needs to know about the notehead it starts from.
#[derive(Debug, Clone, Copy)]
struct Target {
    /// The event holding the notehead.
    event: EventId,
    /// Whether that event is a grace chord (moved only by a performance offset).
    is_grace: bool,
    /// Its voice.
    voice: VoiceId,
    /// The instrument owning the voice (its pass offsets, its track's threshold).
    instrument: InstrumentId,
    /// The notehead itself, with its inline offsets.
    head: Note,
}

/// The notehead `note` and where it lives. O(event lookup) + O(voices).
///
/// The preamble shared by `move_played` and `resize_played`.
///
/// # Errors
/// [`DocumentError::UnknownNote`], or [`DocumentError::UnknownVoice`] for an inconsistent index.
fn target(doc: &Document, note: NoteId) -> Result<Target, DocumentError> {
    let (found, head) = doc.note(note).ok_or(DocumentError::UnknownNote(note))?;
    let voice = found.voice();
    let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
    Ok(Target {
        event: found.event().id(),
        is_grace: found.event().grace().is_some(),
        voice,
        instrument: instrument.id(),
        head: *head,
    })
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
fn threshold_of(doc: &Document, voice: VoiceId) -> Result<Tolerance, DocumentError> {
    let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
    let track = doc
        .track_of(instrument.id())
        .ok_or(DocumentError::inconsistent())?;
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
            Tie::To(_) => return Err(DocumentError::inconsistent()),
            Tie::None | Tie::LetRing => return Ok(chain),
        }
    }
}

/// Whether every event of a chain is a chord of one notehead (ADR-0028 Q5: otherwise the score
/// keeps it). O(chain).
fn lone_noteheads(doc: &Document, chain: &[(EventId, NoteId)]) -> bool {
    chain.iter().all(|&(e, _)| {
        doc.event(e)
            .and_then(|found| found.event().chord().map(|c| c.notes().count() == 1))
            .unwrap_or(false)
    })
}

/// Where the written chain ends: the end of its last piece. O(chain's last event lookup).
///
/// # Errors
/// [`DocumentError::Inconsistent`] for an empty chain (never: it holds at least the dragged
/// note); the lookups' and the arithmetic errors.
fn chain_end(doc: &Document, chain: &[(EventId, NoteId)]) -> Result<Position, DocumentError> {
    let &(last, _) = chain.last().ok_or(DocumentError::inconsistent())?;
    Ok(doc
        .event_position(last)?
        .checked_add(doc.metric_duration(last)?)?)
}

/// The end of the score: the end of the document's last bar. O(log m).
///
/// # Errors
/// [`DocumentError::NoBarsLeft`] for a document without bars (never: one always remains); the
/// meter-map errors.
fn score_end(doc: &Document) -> Result<Position, DocumentError> {
    let count = u32::try_from(doc.bars().len()).map_err(|_| DocumentError::TooManyBars)?;
    let last = BarIndex::new(count.checked_sub(1).ok_or(DocumentError::NoBarsLeft)?);
    Ok(doc
        .meter_map()
        .bar_start(last)?
        .checked_add(doc.meter_map().bar_length(last)?)?)
}

/// The grid point nearest to `end + wanted` for the **end** of a note (ADR-0028 U2): as
/// [`quantise`], except that the end of the score is a valid point (a note may end exactly on the
/// last barline), and a point past it is taken at it.
///
/// # Errors
/// The meter-map and arithmetic errors.
fn quantise_end(
    doc: &Document,
    end: Position,
    wanted: Duration,
    grid: Duration,
) -> Result<Position, DocumentError> {
    let last = score_end(doc)?;
    let target = end.checked_add(wanted)?;
    if target >= last {
        return Ok(last);
    }
    quantise(doc, end, wanted, grid)
}

/// The pieces of a chain written from `start` for `total` (ADR-0028 U3, ADR-0028 U4): cut at every
/// barline, each part written with the longest values first (`command::split_values`, ADR-0026 V3),
/// as (bar, offset, value).
///
/// # Complexity
/// O(bars crossed × values per part), each a meter-map lookup O(log m).
///
/// # Errors
/// [`DocumentError::PastLastBar`] if it would end past the last bar; [`DocumentError::NotWritable`]
/// for a part no sum of written values expresses; the meter-map and arithmetic errors.
fn chain_parts(
    doc: &Document,
    start: Position,
    total: Duration,
) -> Result<Vec<(BarId, Duration, NoteValue)>, DocumentError> {
    let end = start.checked_add(total)?;
    if end > score_end(doc)? {
        return Err(DocumentError::PastLastBar);
    }
    let mut parts = Vec::new();
    let mut at = start;
    while at < end {
        let found = doc.meter_map().locate(at)?;
        let bar = check_bar_exists(doc, found.bar)?;
        let room = doc
            .meter_map()
            .bar_length(found.bar)?
            .checked_sub(found.offset)?;
        let left = end.checked_duration_since(at)?;
        let piece = left.min(room);
        push_parts(&mut parts, bar, found.offset, piece, Rational::ONE)?;
        at = at.checked_add(piece)?;
    }
    Ok(parts)
}

/// Rewrites a written tie chain (single-notehead chords, no tuplet, checked by the callers) so that
/// it starts at `start` and lasts `total`, reusing its pieces' IDs in order (see the module
/// documentation, ADR-0028 U3, ADR-0028 U4, ADR-0028 U9).
///
/// # How
/// 1. The new pieces ([`chain_parts`]) and their IDs: the existing ones in order, new ones for
///    extra pieces.
/// 2. The grace notes: the first piece's `Before` graces and the last piece's `After` graces are
///    kept; any other grace of the chain is deleted (with its slurs, `command::delete_events`).
/// 3. The slurs ending on the old last piece end on the new last piece; the other slurs touching a
///    surplus piece are removed. Then the surplus pieces are removed.
/// 4. Every piece is written in its place: the first keeps its notehead (pitch, offsets) and its
///    chord's markings, the others are plain; each is tied to the next, the last takes the old last
///    piece's tie (none, or "let ring").
/// 5. The kept graces go to the new start and the new end.
///
/// # Complexity
/// O(pieces + graces + slurs of the chain) edits, each O(voices + k).
///
/// # Errors
/// As [`chain_parts`]; the lookups' and the edits'.
#[allow(
    clippy::too_many_lines,
    reason = "the five steps of the How list, in order: splitting them would scatter one rewrite"
)]
fn rewrite_chain(
    tx: &mut Transaction<'_>,
    chain: &[(EventId, NoteId)],
    start: Position,
    total: Duration,
) -> Result<(), CommandError> {
    let doc = tx.document();
    let &(first_event, first_note) = chain.first().ok_or(DocumentError::inconsistent())?;
    let &(last_event, last_note) = chain.last().ok_or(DocumentError::inconsistent())?;
    let found = doc
        .event(first_event)
        .ok_or(DocumentError::UnknownEvent(first_event))?;
    let voice: VoiceId = found.voice();
    let staff = found.event().staff();
    let first_chord = found
        .event()
        .chord()
        .ok_or(DocumentError::NotAChord(first_event))?
        .clone();
    let head = *doc
        .note(first_note)
        .ok_or(DocumentError::UnknownNote(first_note))?
        .1;
    let last_tie = doc
        .note(last_note)
        .ok_or(DocumentError::UnknownNote(last_note))?
        .1
        .tie();
    let parts = chain_parts(doc, start, total)?;
    if parts.is_empty() {
        return Err(DocumentError::inconsistent().into());
    }

    // 2. The grace notes of the chain, sorted into kept and deleted.
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut stray = Vec::new();
    for (k, &(e, _)) in chain.iter().enumerate() {
        for g in command::graces_of(doc, e)? {
            let attach = doc
                .event(g)
                .and_then(|x| x.event().grace())
                .map(|x| x.attach);
            match attach {
                Some(GraceAttach::Before) if k == 0 => before.push(g),
                Some(GraceAttach::After) if k.saturating_add(1) == chain.len() => after.push(g),
                _ => stray.push(g),
            }
        }
    }

    // 1. The IDs of the new pieces: the chain's own for as many parts as it had (one notehead
    // each: a DAW move only rewrites chains of lone noteheads), new ones after.
    let keep: Vec<PartIds> = chain.iter().map(|&(e, n)| (e, vec![n])).collect();
    let ids = chain_ids(tx, parts.len(), 1, &keep)?;
    let surplus: Vec<EventId> = chain.iter().skip(parts.len()).map(|&(e, _)| e).collect();
    let new_last = ids.last().ok_or(DocumentError::inconsistent())?.0;

    if !stray.is_empty() {
        command::delete_events(tx, &stray)?;
    }

    // 3. The slurs on the old last piece and on the surplus pieces.
    let doc = tx.document();
    let mut slur_edits = Vec::new();
    let mut touched: Vec<crate::id::SlurId> = Vec::new();
    for &e in surplus.iter().chain(std::iter::once(&last_event)) {
        for &id in doc.slurs_at(e) {
            if touched.contains(&id) {
                continue;
            }
            touched.push(id);
            let (instrument, slur) = doc.slur(id).ok_or(DocumentError::inconsistent())?;
            let start_gone = surplus.contains(&slur.start());
            if slur.end() == last_event && !start_gone && slur.start() != new_last {
                if new_last != last_event {
                    slur_edits.push(Edit::Replace(Object::Slur {
                        instrument: instrument.id(),
                        slur: slur.with_end(new_last),
                    }));
                }
            } else if start_gone || surplus.contains(&slur.end()) || slur.start() == new_last {
                slur_edits.push(Edit::Remove(ObjectRef::Slur(id)));
            }
        }
    }
    for edit in slur_edits {
        tx.apply(edit)?;
    }
    for &e in &surplus {
        tx.apply(Edit::Remove(ObjectRef::Event(e)))?;
    }

    // 4. The pieces.
    for (j, (&(bar, offset, value), (event_id, notes))) in parts.iter().zip(&ids).enumerate() {
        let event_id = *event_id;
        let &note_id = notes.first().ok_or(DocumentError::inconsistent())?;
        let mut note = if j == 0 {
            head
        } else {
            Note::new(note_id, head.pitch())
        };
        // To the next piece; the last piece keeps the tie the chain ended with.
        note.set_tie(tie_to_next(&ids, j, 0, last_tie));
        let like = (j == 0).then_some(&first_chord);
        let chord = command::chord_from(value, None, None, vec![note], like)?;
        let made = Event::new(event_id, staff, EventBody::Chord(chord));
        if j < chain.len() {
            command::replace_event_at(tx, voice, bar, offset, made)?;
        } else {
            tx.apply(Edit::Insert(Object::Event {
                voice,
                bar,
                placed: PlacedEvent::new(offset, made),
                place: None,
            }))?;
        }
    }

    // 5. The kept graces: `Before` ones at the new start, `After` ones at the new end (the end of
    // the last piece's bar if it ends on a barline).
    let &(first_bar, first_offset, _) = parts.first().ok_or(DocumentError::inconsistent())?;
    let &(last_bar, last_offset, last_value) = parts.last().ok_or(DocumentError::inconsistent())?;
    let end_offset = last_offset.checked_add(last_value.length()?)?;
    for (graces, bar, offset) in [
        (before, first_bar, first_offset),
        (after, last_bar, end_offset),
    ] {
        for g in graces {
            let (v, _, placed) = command::current(tx.document(), g)?;
            command::replace_event_at(tx, v, bar, offset, placed.event().clone())?;
        }
    }
    Ok(())
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
    let lower = grid.checked_mul(Rational::from_integer(steps)?)?;
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
/// bar of the document (ADR-0028 Q6: a DAW move never makes a bar overfull). O(log m).
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

/// Sets the length offset of one notehead, keeping its onset and velocity offsets; nothing if it
/// already has that length offset. O(event lookup) + the replacement.
///
/// # Errors
/// The lookups' and the edit's.
fn set_length(
    tx: &mut Transaction<'_>,
    event: EventId,
    note: NoteId,
    length: Duration,
) -> Result<(), CommandError> {
    set_offsets_with(tx, event, note, |offsets| {
        let changed = offsets.length != length;
        offsets.length = length;
        changed
    })
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
    set_offsets_with(tx, event, note, |offsets| {
        let changed = offsets.onset != onset;
        offsets.onset = onset;
        changed
    })
}

/// Changes the performance offsets of one notehead with `change`, which edits them in place and
/// says whether anything changed; the event is replaced only then, so an unchanged value leaves
/// no history step. O(event lookup) + the replacement.
///
/// The body shared by `set_length` and `set_onset`.
///
/// # Errors
/// [`DocumentError::UnknownNote`] if the event has no such notehead; the lookups' and the edit's.
fn set_offsets_with(
    tx: &mut Transaction<'_>,
    event: EventId,
    note: NoteId,
    change: impl FnOnce(&mut PerformanceOffsets) -> bool,
) -> Result<(), CommandError> {
    let (voice, bar, mut placed) = command::current(tx.document(), event)?;
    let chord = command::chord_mut(&mut placed, event)?;
    let Some(head) = chord.note_mut(note) else {
        return Err(DocumentError::UnknownNote(note).into());
    };
    let mut offsets = head.offsets();
    if !change(&mut offsets) {
        return Ok(());
    }
    head.set_offsets(offsets);
    command::replace_event(tx, voice, bar, placed)
}
