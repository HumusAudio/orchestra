// SPDX-License-Identifier: GPL-3.0-or-later

//! Candidate P1 (B1 + D1): one `Vec` per voice, sorted by **absolute** position.
//!
//! - Insert/delete: binary search, then a `memmove` of the tail of the voice: O(log n + n).
//! - Find by ID: the index keeps `(voice, position)`, so O(1) without touching the voice.
//! - Range of bars: two binary searches per voice, then a contiguous slice: O(log n + r).
//! - Any edit that moves bars (insert a bar, lengthen a free bar, keep-bars meter change) rewrites
//!   the position of **every later event of every voice** and its index entry: O(n).
//! - A rewrite meter change costs nothing: absolute positions do not care where barlines are.
//!
//! Limitation found while writing it: an **overfull bar** (behaviour (i) with a shorter meter)
//! cannot be represented. Its last events get positions inside the next bar's time, so they
//! interleave with that bar's events and nothing records which bar they belong to. Here the voice
//! is re-sorted to keep the binary searches valid; the cost is counted, the meaning is lost.

use std::ops::Range;

use orchestra_core_time::{BarIndex, MeterMap, Position};

use crate::{Event, EventId, IdMap, Model, Spec};

/// The P1 document: voices of `(absolute position, event)` and the ID index.
#[derive(Clone)]
pub struct P1 {
    /// Every voice, sorted by position; events at the same position keep insertion order.
    voices: Vec<Vec<(Position, Event)>>,
    /// ID → (voice, position). Derived (a cache, ADR-0003 §5): rebuilt at load.
    index: IdMap<EventId, (u32, Position)>,
}

impl P1 {
    /// Moves every event of every voice from bar `from` on (bars of `before`) to the same offset
    /// inside bar `map_bar(bar)` of `after`, updating the index.
    ///
    /// Bar starts are computed once per bar crossed (a cursor walks the sorted voice), so the cost
    /// is O(n_after) rational additions plus O(bars × log m) meter map lookups per voice.
    fn remap(
        &mut self,
        before: &MeterMap,
        after: &MeterMap,
        from: u32,
        map_bar: impl Fn(u32) -> u32,
    ) {
        let start = before.bar_start(BarIndex::new(from)).unwrap();
        for (v, voice) in self.voices.iter_mut().enumerate() {
            let first = voice.partition_point(|(p, _)| *p < start);
            let mut bar = from;
            let mut bar_start = start;
            let mut next_start = before.bar_start(BarIndex::new(bar + 1)).unwrap();
            let mut new_start = after.bar_start(BarIndex::new(map_bar(bar))).unwrap();
            for (p, event) in &mut voice[first..] {
                while *p >= next_start {
                    bar += 1;
                    bar_start = next_start;
                    next_start = before.bar_start(BarIndex::new(bar + 1)).unwrap();
                    new_start = after.bar_start(BarIndex::new(map_bar(bar))).unwrap();
                }
                let offset = p.checked_duration_since(bar_start).unwrap();
                *p = new_start.checked_add(offset).unwrap();
                self.index.insert(event.id, (v as u32, *p));
            }
            // Only an overfull bar can break the order; the check is O(n) and the sort runs only
            // when needed (a stable sort keeps the order of equal positions).
            if !voice.is_sorted_by_key(|(p, _)| *p) {
                voice.sort_by_key(|(p, _)| *p);
            }
        }
    }
}

impl Model for P1 {
    type Snapshot = Vec<Vec<(Position, Event)>>;

    fn build(spec: &Spec) -> Self {
        let mut index = IdMap::default();
        for (v, voice) in spec.voices.iter().enumerate() {
            for (p, event) in voice {
                index.insert(event.id, (v as u32, *p));
            }
        }
        Self {
            voices: spec.voices.clone(),
            index,
        }
    }

    fn insert(&mut self, _meter: &MeterMap, voice: usize, at: Position, event: Event) {
        let events = &mut self.voices[voice];
        let i = events.partition_point(|(p, _)| *p <= at);
        events.insert(i, (at, event));
        self.index.insert(event.id, (voice as u32, at));
    }

    fn delete(&mut self, id: EventId) -> Option<Event> {
        let (voice, at) = self.index.remove(&id)?;
        let events = &mut self.voices[voice as usize];
        let first = events.partition_point(|(p, _)| *p < at);
        let i = first + events[first..].iter().position(|(_, e)| e.id == id)?;
        Some(events.remove(i).1)
    }

    fn find(&self, _meter: &MeterMap, id: EventId) -> Option<Position> {
        self.index.get(&id).map(|&(_, p)| p)
    }

    fn collect(&self, meter: &MeterMap, bars: Range<u32>, out: &mut Vec<(Position, EventId)>) {
        let from = meter.bar_start(BarIndex::new(bars.start)).unwrap();
        let to = meter.bar_start(BarIndex::new(bars.end)).unwrap();
        for voice in &self.voices {
            let lo = voice.partition_point(|(p, _)| *p < from);
            let hi = voice.partition_point(|(p, _)| *p < to);
            out.extend(voice[lo..hi].iter().map(|(p, e)| (*p, e.id)));
        }
    }

    fn insert_bar(&mut self, before: &MeterMap, after: &MeterMap, bar: u32) {
        self.remap(before, after, bar, |b| b + 1);
    }

    fn lengthen_bar(&mut self, before: &MeterMap, after: &MeterMap, bar: u32) {
        // The free bar itself does not move: only what comes after it.
        self.remap(before, after, bar + 1, |b| b);
    }

    fn meter_keep_bars(&mut self, before: &MeterMap, after: &MeterMap, bar: u32) {
        self.remap(before, after, bar, |b| b);
    }

    fn meter_rewrite(&mut self, _: &MeterMap, _: &MeterMap, _: u32, _: u32, _: u32) {
        // Nothing to do: absolute positions are independent of the barlines.
    }

    fn snapshot(&self) -> Self::Snapshot {
        self.voices.clone()
    }

    fn for_each(&self, _meter: &MeterMap, mut f: impl FnMut(usize, Position, &Event)) {
        for (v, voice) in self.voices.iter().enumerate() {
            for (p, event) in voice {
                f(v, *p, event);
            }
        }
    }
}
