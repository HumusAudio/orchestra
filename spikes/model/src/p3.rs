// SPDX-License-Identifier: GPL-3.0-or-later

//! Candidate P3 (B3 + D2 + C2 + E3): like P2, but bars have **stable IDs** and every non-empty
//! chunk sits behind an `Arc` (copy-on-write).
//!
//! - The ID index names bars by `BarId`, so inserting a bar touches no index entry of the events:
//!   one ID inserted in the bar list, the slots of the later bars updated (O(B)), one `None`
//!   inserted per voice (O(B) pointer moves).
//! - A snapshot copies only pointers (one per non-empty bar per voice) and bumps reference counts;
//!   the first edit of a chunk shared with a snapshot copies that one chunk (`Arc::make_mut`).
//! - The price: one extra hash lookup (BarId → slot) on find and delete, and one indirection per
//!   chunk on scans.

use std::ops::Range;
use std::sync::Arc;

use orchestra_core_time::{BarIndex, Duration, MeterMap, Position};

use crate::p2::bar_starts;
use crate::{Event, EventId, IdMap, Model, Spec};

/// One bar of one voice: `(offset in bar, event)`, sorted by offset.
type Chunk = Vec<(Duration, Event)>;

/// A voice: one optional shared chunk per bar, aligned with the document's bar list.
type Voice = Vec<Option<Arc<Chunk>>>;

/// The P3 document.
#[derive(Clone)]
pub struct P3 {
    /// The bar list (C2): the stable ID of every bar, in order. Lengths stay in the meter map.
    bar_ids: Vec<u64>,
    /// BarId → its current index in `bar_ids`. Derived.
    slots: IdMap<u64, u32>,
    /// The next free bar ID.
    next_bar: u64,
    /// Every voice.
    voices: Vec<Voice>,
    /// Event ID → (voice, bar ID). Derived, rebuilt at load.
    index: IdMap<EventId, (u32, u64)>,
}

impl P3 {
    /// Inserts `count` new bar IDs at index `at` and renumbers the slots of the bars after them.
    fn insert_bar_ids(&mut self, at: u32, count: u32) {
        let ids = (0..count).map(|k| self.next_bar + u64::from(k));
        self.bar_ids.splice(at as usize..at as usize, ids);
        self.next_bar += u64::from(count);
        for (slot, id) in self.bar_ids.iter().enumerate().skip(at as usize) {
            self.slots.insert(*id, slot as u32);
        }
    }

    /// The chunk of `voice` at `bar`, created if empty, unshared if a snapshot holds it.
    fn chunk_mut(&mut self, voice: usize, bar: usize) -> &mut Chunk {
        Arc::make_mut(self.voices[voice][bar].get_or_insert_with(Default::default))
    }
}

impl Model for P3 {
    type Snapshot = Vec<Voice>;

    fn build(spec: &Spec) -> Self {
        let mut model = Self {
            bar_ids: Vec::new(),
            slots: IdMap::default(),
            next_bar: 1,
            voices: vec![vec![None; spec.bars as usize]; spec.voices.len()],
            index: IdMap::default(),
        };
        model.insert_bar_ids(0, spec.bars);
        for (v, events) in spec.voices.iter().enumerate() {
            for (p, event) in events {
                let at = spec.meter.locate(*p).unwrap();
                let bar = at.bar.get() as usize;
                model.chunk_mut(v, bar).push((at.offset, *event));
                let id = model.bar_ids[bar];
                model.index.insert(event.id, (v as u32, id));
            }
        }
        model
    }

    fn insert(&mut self, meter: &MeterMap, voice: usize, at: Position, event: Event) {
        let at = meter.locate(at).unwrap();
        let bar = at.bar.get() as usize;
        let chunk = self.chunk_mut(voice, bar);
        let i = chunk.partition_point(|(o, _)| *o <= at.offset);
        chunk.insert(i, (at.offset, event));
        let id = self.bar_ids[bar];
        self.index.insert(event.id, (voice as u32, id));
    }

    fn delete(&mut self, id: EventId) -> Option<Event> {
        let (voice, bar_id) = self.index.remove(&id)?;
        let slot = *self.slots.get(&bar_id)? as usize;
        let chunk = self.chunk_mut(voice as usize, slot);
        let i = chunk.iter().position(|(_, e)| e.id == id)?;
        Some(chunk.remove(i).1)
    }

    fn find(&self, meter: &MeterMap, id: EventId) -> Option<Position> {
        let &(voice, bar_id) = self.index.get(&id)?;
        let slot = *self.slots.get(&bar_id)?;
        let chunk = self.voices[voice as usize][slot as usize].as_deref()?;
        let (offset, _) = chunk.iter().find(|(_, e)| e.id == id)?;
        let start = meter.bar_start(BarIndex::new(slot)).unwrap();
        Some(start.checked_add(*offset).unwrap())
    }

    fn collect(&self, meter: &MeterMap, bars: Range<u32>, out: &mut Vec<(Position, EventId)>) {
        let starts = bar_starts(meter, bars.clone());
        for voice in &self.voices {
            for (chunk, start) in voice[bars.start as usize..bars.end as usize]
                .iter()
                .zip(&starts)
            {
                if let Some(chunk) = chunk {
                    out.extend(
                        chunk
                            .iter()
                            .map(|(o, e)| (start.checked_add(*o).unwrap(), e.id)),
                    );
                }
            }
        }
    }

    fn insert_bar(&mut self, _before: &MeterMap, _after: &MeterMap, bar: u32) {
        self.insert_bar_ids(bar, 1);
        for voice in &mut self.voices {
            voice.insert(bar as usize, None);
        }
    }

    fn lengthen_bar(&mut self, _: &MeterMap, _: &MeterMap, _: u32) {
        // Nothing, as in P2.
    }

    fn meter_keep_bars(&mut self, _: &MeterMap, _: &MeterMap, _: u32) {
        // Nothing, as in P2.
    }

    fn meter_rewrite(
        &mut self,
        before: &MeterMap,
        after: &MeterMap,
        bar: u32,
        until_before: u32,
        until_after: u32,
    ) {
        let starts = bar_starts(before, bar..until_before);
        // The region keeps its bar IDs and gets new ones at its end; later bars keep theirs.
        self.insert_bar_ids(until_before, until_after - until_before);
        let mut region = Vec::new();
        for v in 0..self.voices.len() {
            let voice = &mut self.voices[v];
            region.clear();
            for (chunk, start) in voice[bar as usize..until_before as usize]
                .iter_mut()
                .zip(&starts)
            {
                if let Some(chunk) = chunk.take() {
                    // Reuse the chunk's memory if no snapshot holds it.
                    let chunk = Arc::unwrap_or_clone(chunk);
                    region.extend(
                        chunk
                            .into_iter()
                            .map(|(o, e)| (start.checked_add(o).unwrap(), e)),
                    );
                }
            }
            voice.splice(
                until_before as usize..until_before as usize,
                std::iter::repeat_n(None, (until_after - until_before) as usize),
            );
            for (p, event) in region.drain(..) {
                let at = after.locate(p).unwrap();
                let b = at.bar.get() as usize;
                self.chunk_mut(v, b).push((at.offset, event));
                let id = self.bar_ids[b];
                self.index.insert(event.id, (v as u32, id));
            }
        }
    }

    fn snapshot(&self) -> Self::Snapshot {
        self.voices.clone()
    }

    fn for_each(&self, meter: &MeterMap, mut f: impl FnMut(usize, Position, &Event)) {
        let starts = bar_starts(meter, 0..self.bar_ids.len() as u32);
        for (v, voice) in self.voices.iter().enumerate() {
            for (chunk, start) in voice.iter().zip(&starts) {
                for (o, event) in chunk.iter().flat_map(|c| c.iter()) {
                    f(v, start.checked_add(*o).unwrap(), event);
                }
            }
        }
    }
}
