// SPDX-License-Identifier: GPL-3.0-or-later

//! Memory, allocations and wall-clock times of the three candidates (complement to the
//! instruction counts of `benches/model.rs`).
//!
//! - **Memory**: bytes still allocated after building a model from a generated score, minus the
//!   bytes of the score itself, measured by a counting global allocator; plus the number of live
//!   allocations (each one costs allocator metadata and a cache miss when scanned).
//! - **Times**: median of 31 runs of each operation on a fresh clone of the model (the clone is
//!   not timed), to compare with the millisecond budgets of ADR-0007 (re-layout after a note
//!   ≤ 16 ms, opening 100 bars ≤ 1 s). Wall-clock times are noisy; the decision rests on the
//!   instruction counts, these only say "far from the budget" or "close to it".
//!
//! Run with `cargo run --release -p model --bin report`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use model::p1::P1;
use model::p2::P2;
use model::p3::P3;
use model::*;
use orchestra_core_time::MeterMap;

/// Counts live bytes and live allocations; forwards to the system allocator.
struct Counting;

static BYTES: AtomicIsize = AtomicIsize::new(0);
static BLOCKS: AtomicIsize = AtomicIsize::new(0);

// SAFETY: every method forwards to `System` with the caller's arguments unchanged, so the
// `GlobalAlloc` contract is the system allocator's; the counters are only statistics.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        BLOCKS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        BLOCKS.fetch_sub(1, Ordering::Relaxed);
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        BYTES.fetch_add(
            new_size as isize - layout.size() as isize,
            Ordering::Relaxed,
        );
        // SAFETY: forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Median wall time in microseconds of `op` run on 31 fresh clones of `model`.
fn time<M: Clone>(model: &M, mut op: impl FnMut(&mut M)) -> f64 {
    let mut samples = Vec::new();
    for _ in 0..31 {
        let mut m = model.clone();
        let start = Instant::now();
        op(&mut m);
        samples.push(start.elapsed().as_secs_f64() * 1e6);
        black_box(&m);
    }
    samples.sort_by(f64::total_cmp);
    samples[15]
}

/// One row of the report for candidate `M` on score `spec`.
fn row<M: Model + Clone>(name: &str, spec: &Spec) {
    let probes = probes(spec);
    let m0 = &spec.meter;
    let maps: [MeterMap; 4] = [
        after_insert_bar(m0),
        after_lengthen(m0),
        after_keep_bars(m0),
        after_rewrite(m0),
    ];

    // Memory: live bytes and blocks of the model alone.
    let (b0, k0) = (
        BYTES.load(Ordering::Relaxed),
        BLOCKS.load(Ordering::Relaxed),
    );
    let start = Instant::now();
    let model = M::build(spec);
    let build_ms = start.elapsed().as_secs_f64() * 1e3;
    let bytes = BYTES.load(Ordering::Relaxed) - b0;
    let blocks = BLOCKS.load(Ordering::Relaxed) - k0;
    let (_, notes) = spec.counts();

    let insert = time(&model, |m| {
        for (v, at, e) in &probes.inserts {
            m.insert(m0, *v, *at, *e);
        }
    }) / BATCH as f64;
    let delete = time(&model, |m| {
        for id in &probes.ids {
            black_box(m.delete(*id));
        }
    }) / BATCH as f64;
    let find = time(&model, |m| {
        for id in &probes.ids {
            black_box(m.find(m0, *id));
        }
    }) / BATCH as f64;
    let collect = time(&model, |m| {
        let mut out = Vec::new();
        m.collect(m0, COLLECT, &mut out);
        black_box(out);
    });
    let insert_bar = time(&model, |m| m.insert_bar(m0, &maps[0], INSERT_BAR));
    let lengthen = time(&model, |m| m.lengthen_bar(m0, &maps[1], FREE_BAR));
    let keep = time(&model, |m| m.meter_keep_bars(m0, &maps[2], METER_BAR));
    let rewrite = time(&model, |m| {
        m.meter_rewrite(m0, &maps[3], METER_BAR, METER_UNTIL, METER_UNTIL_REWRITTEN)
    });
    let snapshot = time(&model, |m| {
        black_box(m.snapshot());
    });
    let iterate = time(&model, |m| {
        let mut sum = 0_u64;
        m.for_each(m0, |v, p, e| {
            black_box(p);
            sum = sum.wrapping_add(e.id.0 ^ v as u64);
        });
        black_box(sum);
    });

    println!(
        "| {name} | {:.1} MB | {} | {:.0} B | {build_ms:.1} ms | {insert:.2} | {delete:.2} | {find:.3} | {collect:.0} | {insert_bar:.0} | {lengthen:.0} | {keep:.0} | {rewrite:.0} | {snapshot:.0} | {iterate:.0} |",
        bytes as f64 / 1e6,
        blocks,
        bytes as f64 / notes as f64,
    );
}

fn main() {
    for size in [Size::Normal, Size::Heavy] {
        let spec = generate(size);
        let (events, notes) = spec.counts();
        println!(
            "\n### {size:?}: {} bars, {} voices, {events} events, {notes} notes\n",
            spec.bars,
            spec.voices.len()
        );
        println!("Times in µs (insert/delete/find: per operation).\n");
        println!(
            "| | memory | allocations | per note | build | insert | delete | find | collect 40–60 | insert bar | lengthen free bar | meter keep | meter rewrite | snapshot |"
        );
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
        row::<P1>("P1", &spec);
        row::<P2>("P2", &spec);
        row::<P3>("P3", &spec);
    }
}
