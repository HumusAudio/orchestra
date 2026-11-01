// SPDX-License-Identifier: GPL-3.0-or-later

//! Candidate P2 (B3 + D2 + C1): one chunk per bar per voice, offsets **relative to the bar**, bars
//! named by their index.
//!
//! - Insert/delete: one meter map lookup to find the bar (O(log m)), then a small sorted `Vec`
//!   (O(k), k = events of that bar in that voice).
//! - Find by ID: the index keeps `(voice, bar)`; a scan of that one chunk, plus a bar start.
//! - Range of bars: one bar start per bar, then one rational addition per event (the cost D1 does
//!   not pay).
//! - Lengthen a free bar or change a meter keeping the bars: **nothing** to do.
//! - Insert a bar: one empty chunk inserted per voice (O(B) pointer moves), but the index names
//!   bars by number, so every later event's index entry changes: O(n_after) hash updates (C1's
//!   price, which C2 avoids).
//! - Rewrite meter change: the events of the region are re-chunked; later index entries shift.

use std::ops::Range;

use orchestra_core_time::{BarIndex, Duration, MeterMap, Position};

use crate::{Event, EventId, IdMap, Model, Spec};

/// One bar of one voice: `(offset in bar, event)`, sorted by offset.
type Chunk = Vec<(Duration, Event)>;

/// The P2 document.
#[derive(Clone)]
pub struct P2 {
    /// For every voice, one chunk per bar of the document (empty chunks do not allocate).
    voices: Vec<Vec<Chunk>>,
    /// ID → (voice, bar index). Derived, rebuilt at load.
    index: IdMap<EventId, (u32, u32)>,
}

/// The starts of `bars` in `meter`: one O(log m) lookup each.
pub(crate) fn bar_starts(meter: &MeterMap, bars: Range<u32>) -> Vec<Position> {
    bars.map(|b| meter.bar_start(BarIndex::new(b)).unwrap())
        .collect()
}

impl Model for P2 {
    type Snapshot = Vec<Vec<Chunk>>;

    fn build(spec: &Spec) -> Self {
        let mut index = IdMap::default();
        let mut voices = Vec::with_capacity(spec.voices.len());
        for (v, events) in spec.voices.iter().enumerate() {
            let mut chunks = vec![Chunk::new(); spec.bars as usize];
            for (p, event) in events {
                let at = spec.meter.locate(*p).unwrap();
                chunks[at.bar.get() as usize].push((at.offset, *event));
                index.insert(event.id, (v as u32, at.bar.get()));
            }
            voices.push(chunks);
        }
        Self { voices, index }
    }

    fn insert(&mut self, meter: &MeterMap, voice: usize, at: Position, event: Event) {
        let at = meter.locate(at).unwrap();
        let chunk = &mut self.voices[voice][at.bar.get() as usize];
        let i = chunk.partition_point(|(o, _)| *o <= at.offset);
        chunk.insert(i, (at.offset, event));
        self.index.insert(event.id, (voice as u32, at.bar.get()));
    }

    fn delete(&mut self, id: EventId) -> Option<Event> {
        let (voice, bar) = self.index.remove(&id)?;
        let chunk = &mut self.voices[voice as usize][bar as usize];
        let i = chunk.iter().position(|(_, e)| e.id == id)?;
        Some(chunk.remove(i).1)
    }

    fn find(&self, meter: &MeterMap, id: EventId) -> Option<Position> {
        let &(voice, bar) = self.index.get(&id)?;
        let chunk = &self.voices[voice as usize][bar as usize];
        let (offset, _) = chunk.iter().find(|(_, e)| e.id == id)?;
        let start = meter.bar_start(BarIndex::new(bar)).unwrap();
        Some(start.checked_add(*offset).unwrap())
    }

    fn collect(&self, meter: &MeterMap, bars: Range<u32>, out: &mut Vec<(Position, EventId)>) {
        let starts = bar_starts(meter, bars.clone());
        for voice in &self.voices {
            for (chunk, start) in voice[bars.start as usize..bars.end as usize]
                .iter()
                .zip(&starts)
            {
                out.extend(
                    chunk
                        .iter()
                        .map(|(o, e)| (start.checked_add(*o).unwrap(), e.id)),
                );
            }
        }
    }

    fn insert_bar(&mut self, _before: &MeterMap, _after: &MeterMap, bar: u32) {
        for (v, voice) in self.voices.iter_mut().enumerate() {
            voice.insert(bar as usize, Chunk::new());
            for (b, chunk) in voice.iter().enumerate().skip(bar as usize + 1) {
                for (_, event) in chunk {
                    self.index.insert(event.id, (v as u32, b as u32));
                }
            }
        }
    }

    fn lengthen_bar(&mut self, _: &MeterMap, _: &MeterMap, _: u32) {
        // Nothing: offsets are relative to their bar, and only the meter map changed.
    }

    fn meter_keep_bars(&mut self, _: &MeterMap, _: &MeterMap, _: u32) {
        // Nothing: every bar keeps its contents; a bar that is now too short is overfull, which
        // is a representable state (its events keep their offsets) for the score to flag.
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
        let shift = until_after - until_before;
        let mut region = Vec::new();
        for (v, voice) in self.voices.iter_mut().enumerate() {
            // Take the region's events out with their absolute positions…
            region.clear();
            for (chunk, start) in voice[bar as usize..until_before as usize]
                .iter_mut()
                .zip(&starts)
            {
                region.extend(
                    chunk
                        .drain(..)
                        .map(|(o, e)| (start.checked_add(o).unwrap(), e)),
                );
            }
            // …give the region its new number of (empty) bars…
            voice.splice(
                until_before as usize..until_before as usize,
                std::iter::repeat_with(Chunk::new).take(shift as usize),
            );
            // …and put them back across the new barlines (already in order: push is enough).
            for (p, event) in region.drain(..) {
                let at = after.locate(p).unwrap();
                voice[at.bar.get() as usize].push((at.offset, event));
                self.index.insert(event.id, (v as u32, at.bar.get()));
            }
            // The later bars kept their contents but changed number.
            for (b, chunk) in voice.iter().enumerate().skip(until_after as usize) {
                for (_, event) in chunk {
                    self.index.insert(event.id, (v as u32, b as u32));
                }
            }
        }
    }

    fn snapshot(&self) -> Self::Snapshot {
        self.voices.clone()
    }

    fn for_each(&self, meter: &MeterMap, mut f: impl FnMut(usize, Position, &Event)) {
        let bars = self.voices.first().map_or(0, Vec::len) as u32;
        let starts = bar_starts(meter, 0..bars);
        for (v, voice) in self.voices.iter().enumerate() {
            for (chunk, start) in voice.iter().zip(&starts) {
                for (o, event) in chunk {
                    f(v, start.checked_add(*o).unwrap(), event);
                }
            }
        }
    }
}
