// SPDX-License-Identifier: GPL-3.0-or-later

//! The content of one voice: one chunk of events per bar, and the voice's tuplet table
//! (ADR-0022 §4 and §7, ADR-0024 §3, SCALETTA F1.09b).
//!
//! # Layout
//! A voice keeps a vector of [`Chunk`]s **aligned with the bar list**: chunk `i` holds the voice's
//! events in the bar with index `i`. Inserting a bar (F1.11) inserts one empty chunk per voice —
//! O(B) pointer moves, no event touched (ADR-0022 §4, candidate P3 of `spikes/model/`).
//!
//! A chunk is `None` while the voice has nothing in that bar (no allocation: most voices are
//! silent in most bars of an orchestral score), and otherwise an `Arc` of a vector of
//! [`PlacedEvent`]s sorted by (offset, slot), events with the same key keeping their insertion
//! order. The slot (ADR-0024 §5, F1.09c) puts at one offset first the `After` grace chords of the
//! event that ends there, then the `Before` grace chords of the event that starts there, then that
//! event: grace chords last zero, so the offset alone could not order them.
//! The `Arc` is what makes snapshots cheap (ADR-0022 §7): a background reader shares the chunks,
//! and an edit copies only the chunk it touches (`Arc::make_mut`, copy-on-write).
//!
//! The tuplet table is separate from the chunks because a tuplet may span several bars
//! (ADR-0024 §3, choice C4): it is keyed by `TupletId` and holds a handful of entries per bar at
//! most.

use std::sync::Arc;

use orchestra_core_time::Duration;

use crate::document::DocumentError;
use crate::event::{EventBody, MAX_TUPLET_DEPTH, PlacedEvent, RestValue, Tuplet};
use crate::id::{EventId, IdMap, TupletId};

/// The events of one voice in one bar (ADR-0022 §4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chunk {
    /// `None` for an empty chunk (no allocation), otherwise the events sorted by (offset, slot).
    events: Option<Arc<Vec<PlacedEvent>>>,
}

impl Chunk {
    /// An empty chunk. Does not allocate. O(1).
    pub(crate) const EMPTY: Self = Self { events: None };

    /// The events, sorted by (offset, slot) (insertion order among equal keys). O(1).
    #[must_use]
    pub fn events(&self) -> &[PlacedEvent] {
        self.events.as_deref().map_or(&[], Vec::as_slice)
    }

    /// Inserts an event after every event whose (offset, slot) key is not later than its own, so
    /// that events with the same key (three `Before` graces in a row, two main events at one
    /// offset in an invalid voice) keep the order they were entered in. O(log k) to find the
    /// place, O(k) to shift the later events, plus a copy of the chunk if a snapshot still shares
    /// it.
    pub(crate) fn insert(&mut self, placed: PlacedEvent) {
        let events = Arc::make_mut(self.events.get_or_insert_with(Arc::default));
        let key = placed.order_key();
        let at = events.partition_point(|e| e.order_key() <= key);
        events.insert(at, placed);
    }

    /// The entry of this event, mutably (copying the chunk first if a snapshot shares it).
    /// O(k): a linear scan, since chunks hold a handful of events.
    pub(crate) fn get_mut(&mut self, id: EventId) -> Option<&mut PlacedEvent> {
        // Look first without copying: a miss must not copy a shared chunk for nothing.
        let present = self.events().iter().any(|e| e.event().id() == id);
        if !present {
            return None;
        }
        self.events
            .as_mut()
            .map(Arc::make_mut)
            .and_then(|events| events.iter_mut().find(|e| e.event().id() == id))
    }
}

/// The content of a voice: its chunks and its tuplets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct VoiceContent {
    /// One chunk per bar of the bar list, in the same order.
    pub(crate) chunks: Vec<Chunk>,
    /// The voice's tuplets.
    pub(crate) tuplets: IdMap<TupletId, Tuplet>,
}

impl VoiceContent {
    /// The content of a new voice in a document of `bars` bars: all chunks empty. O(bars), one
    /// allocation for the vector of (empty) chunk pointers.
    pub(crate) fn with_bars(bars: usize) -> Self {
        Self {
            chunks: vec![Chunk::EMPTY; bars],
            tuplets: IdMap::default(),
        }
    }

    /// How deep a tuplet sits: 1 for a top-level tuplet, 2 for one nested in it, … O(depth).
    ///
    /// Bounded by [`MAX_TUPLET_DEPTH`] + 1 iterations even if the table were corrupt (a cycle
    /// cannot be built: a parent must exist before its child, so the loop guard is only a
    /// safety net against an infinite loop).
    pub(crate) fn depth_of(&self, tuplet: TupletId) -> usize {
        let mut depth = 0_usize;
        let mut next = Some(tuplet);
        while let Some(id) = next {
            depth = depth.saturating_add(1);
            if depth > MAX_TUPLET_DEPTH {
                break;
            }
            next = self.tuplets.get(&id).and_then(|t| t.parent());
        }
        depth
    }
}

/// The metric duration of an event of a voice (ADR-0024 §2): the written length times the ratio
/// of every tuplet that contains it, the bar's length for a whole-bar rest, and **zero** for a
/// grace chord (ADR-0024 §5: it keeps its written value but does not move the following events).
///
/// `bar_length` is only read for a whole-bar rest; callers that walk a bar pass the length they
/// already have, so the meter map is not asked again for every event.
///
/// # Complexity
/// O(depth) ≤ [`MAX_TUPLET_DEPTH`] hash lookups and rational multiplications; never allocates.
///
/// # Errors
/// [`DocumentError::UnknownTuplet`] if the event names a tuplet missing from the table (the
/// document never builds one), [`DocumentError::Arithmetic`] on overflow (impossible within the
/// bounds of ADR-0024 C3, checked anyway).
pub(crate) fn metric_duration(
    tuplets: &IdMap<TupletId, Tuplet>,
    body: &EventBody,
    bar_length: Duration,
) -> Result<Duration, DocumentError> {
    let (value, mut tuplet) = match body {
        // A grace chord takes no metric time; how much it takes from its neighbours when played
        // is a playback rule (F1.13).
        EventBody::Chord(c) if c.grace().is_some() => return Ok(Duration::ZERO),
        EventBody::Chord(c) => (c.value(), c.tuplet()),
        EventBody::Rest(r) => match r.value() {
            // A whole-bar rest lasts its bar, and is never in a tuplet.
            RestValue::WholeBar => return Ok(bar_length),
            RestValue::Value(v) => (v, r.tuplet()),
        },
    };
    let mut duration = value.length()?;
    // Walk up the nesting chain, innermost first, multiplying the ratios. The guard bounds the
    // loop by the maximum depth, whatever the table holds.
    for _ in 0..MAX_TUPLET_DEPTH {
        let Some(id) = tuplet else { break };
        let t = tuplets.get(&id).ok_or(DocumentError::UnknownTuplet(id))?;
        duration = duration.checked_mul(t.ratio()?)?;
        tuplet = t.parent();
    }
    Ok(duration)
}
