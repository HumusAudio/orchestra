// SPDX-License-Identifier: GPL-3.0-or-later

//! Tie chains: the parts of a note that does not fit where it is written (ADR-0024 V3, ADR-0026,
//! ADR-0028).
//!
//! # Why a module of its own?
//! Three commands write a duration as a chain of tied events: entering a note that crosses a
//! barline (`command::enter_event`), rewriting the bars after a meter change (`rewrite`, Z3–Z5)
//! and moving or resizing a note from the DAW side beyond the tolerance threshold
//! (`daw_command::rewrite_chain`). Each used to cut the duration into written values, hand out
//! the IDs of the new events and noteheads, and tie every notehead to the one of the next part,
//! with its own copy of the same three loops (review point A3). They differ only in what they
//! keep: a new note keeps nothing, a rewritten event keeps its own ID for the first part, a
//! rewritten DAW chain keeps the IDs of the chain it replaces for as many parts as it had. The
//! three steps are here, once:
//!
//! 1. [`push_parts`]: one stretch of a bar written with the longest values first, inside a tuplet
//!    chain whose ratio scales every value;
//! 2. [`chain_ids`]: the IDs of every part, the kept ones first, then new ones from the
//!    transaction's counter in a fixed order (an event's, then one per notehead, part by part), so
//!    that the same command always produces the same IDs;
//! 3. [`tie_to_next`]: the tie of the `k`-th notehead of a part, to the `k`-th notehead of the
//!    next part, or the given tie for the last part.

use orchestra_core_time::{Duration, Rational};

use crate::command::split_values;
use crate::document::DocumentError;
use crate::editor::{CommandError, Transaction};
use crate::event::{NoteValue, Tie};
use crate::id::{EventId, NoteId};

/// The IDs of one part of a chain: its event's and its noteheads', in the order of the chord's
/// noteheads as the caller gives them.
pub(crate) type PartIds = (EventId, Vec<NoteId>);

/// Appends to `out` the written values of a stretch of `length` starting at `start` in `bar`,
/// inside a tuplet chain whose ratio is `ratio` (1 outside any tuplet): each value as
/// `(bar, offset, value)`, the offsets advancing by the value's length times the ratio (Z3).
///
/// `bar` is whatever names a bar for the caller (a `BarId`, or an index into a grid being built);
/// it is only copied into the output.
///
/// # Errors
/// [`DocumentError::NotWritable`] if the stretch is no sum of written values (see
/// `command::split_values`); the arithmetic errors (absurd values only).
///
/// # Complexity
/// O(values), at most `MAX_VALUES_PER_PART` of them.
pub(crate) fn push_parts<B: Copy>(
    out: &mut Vec<(B, Duration, NoteValue)>,
    bar: B,
    start: Duration,
    length: Duration,
    ratio: Rational,
) -> Result<(), DocumentError> {
    let mut offset = start;
    for value in split_values(length, ratio)? {
        out.push((bar, offset, value));
        // The value's length in the bar: its written length scaled by the tuplet chain.
        offset = offset.checked_add(value.length()?.checked_mul(ratio)?)?;
    }
    Ok(())
}

/// The IDs of the `parts` parts of a chain whose events have `noteheads` noteheads each (0 for
/// rests): the entries of `keep` first, as they are, then new IDs from the transaction for the
/// remaining parts, the event's first and then one per notehead. Extra entries of `keep` (a chain
/// that becomes shorter) are ignored; the caller removes those events.
///
/// # Why every ID before any event?
/// Each notehead is tied to the notehead of the next part, so the next part's IDs must be known
/// when a part is built.
///
/// # Errors
/// [`DocumentError::IdsExhausted`] through the transaction.
///
/// # Complexity
/// O(parts × noteheads).
pub(crate) fn chain_ids(
    tx: &mut Transaction<'_>,
    parts: usize,
    noteheads: usize,
    keep: &[PartIds],
) -> Result<Vec<PartIds>, CommandError> {
    let mut ids: Vec<PartIds> = Vec::with_capacity(parts);
    for j in 0..parts {
        if let Some(kept) = keep.get(j) {
            ids.push(kept.clone());
            continue;
        }
        let event = EventId::from_counter(tx.new_id()?);
        let mut notes = Vec::with_capacity(noteheads);
        for _ in 0..noteheads {
            notes.push(NoteId::from_counter(tx.new_id()?));
        }
        ids.push((event, notes));
    }
    Ok(ids)
}

/// The tie of the `k`-th notehead of part `j`: to the `k`-th notehead of part `j + 1`, or `last`
/// when part `j` is the last one (the tie the chain had at its end, or none for a new note).
/// O(1).
pub(crate) fn tie_to_next(ids: &[PartIds], j: usize, k: usize, last: Tie) -> Tie {
    ids.get(j.saturating_add(1))
        .and_then(|(_, notes)| notes.get(k))
        .map_or(last, |&next| Tie::To(next))
}
