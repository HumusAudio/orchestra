// SPDX-License-Identifier: GPL-3.0-or-later

//! Instruction counts (Gungraun, Callgrind) of every operation of ANALYSIS.md §7 on the three
//! candidates and the two reference scores.
//!
//! Each benchmark function is one operation; its `#[bench::…]` cases are the six combinations
//! `p1|p2|p3 × normal|heavy`. The score, the model, the meter maps and the probes are built in the
//! case's argument expression, which Gungraun evaluates outside the measured region; the result is
//! returned so that its drop is not measured either. The small operations run in batches of 64
//! (ADR-0017 §4); divide by 64 for the cost of one.
//!
//! The candidates are wrapped in one enum so that every case shares one function; the `match`
//! costs a couple of instructions per call, the same for all.

// Some operations only read the model; the shared case macro still takes the input as `mut`.
#![allow(unused_mut)]

use std::hint::black_box;

use gungraun::prelude::*;
use model::p1::P1;
use model::p2::P2;
use model::p3::P3;
use model::*;
use orchestra_core_time::MeterMap;

// One of the three candidates.
enum Any {
    P1(P1),
    P2(P2),
    P3(P3),
}

// Runs `$body` with `$m` bound to the candidate inside `$any`, whatever its type.
macro_rules! with {
    ($any:expr, $m:ident => $body:expr) => {
        match $any {
            Any::P1($m) => $body,
            Any::P2($m) => $body,
            Any::P3($m) => $body,
        }
    };
}

// Which candidate a case measures.
#[derive(Clone, Copy)]
enum Kind {
    P1,
    P2,
    P3,
}

// Everything an operation needs.
struct Input {
    model: Any,
    before: MeterMap,
    after: MeterMap,
    probes: Probes,
}

// The setup of a case: the score of `size`, the model `kind`, the meter map after the edit given
// by `after` (identity for the operations that do not change it).
fn setup(kind: Kind, size: Size, after: fn(&MeterMap) -> MeterMap) -> Input {
    let spec = generate(size);
    let model = match kind {
        Kind::P1 => Any::P1(P1::build(&spec)),
        Kind::P2 => Any::P2(P2::build(&spec)),
        Kind::P3 => Any::P3(P3::build(&spec)),
    };
    Input {
        model,
        after: after(&spec.meter),
        probes: probes(&spec),
        before: spec.meter,
    }
}

fn same(map: &MeterMap) -> MeterMap {
    map.clone()
}

// The case attributes are the same for every operation but the meter map edit; this macro writes
// them once. `$after` is the function giving the meter map after the edit.
macro_rules! cases {
    ($after:ident, $(#[$m:meta])* fn $name:ident($input:ident: Input) -> $ret:ty $body:block) => {
        #[library_benchmark]
        #[bench::p1_normal(setup(Kind::P1, Size::Normal, $after))]
        #[bench::p2_normal(setup(Kind::P2, Size::Normal, $after))]
        #[bench::p3_normal(setup(Kind::P3, Size::Normal, $after))]
        #[bench::p1_heavy(setup(Kind::P1, Size::Heavy, $after))]
        #[bench::p2_heavy(setup(Kind::P2, Size::Heavy, $after))]
        #[bench::p3_heavy(setup(Kind::P3, Size::Heavy, $after))]
        fn $name(mut $input: Input) -> $ret $body
    };
}

// 64 inserts, spread over voices and bars, each at the position of an existing event.
cases!(
    same,
    fn insert(input: Input) -> Input {
        let inserts = std::mem::take(&mut input.probes.inserts);
        with!(&mut input.model, m => for (v, at, e) in black_box(&inserts) {
            m.insert(&input.before, *v, *at, *e);
        });
        input.probes.inserts = inserts;
        input
    }
);

// 64 deletes by ID.
cases!(
    same,
    fn delete(input: Input) -> Input {
        with!(&mut input.model, m => for id in black_box(&input.probes.ids) {
            black_box(m.delete(*id));
        });
        input
    }
);

// 64 lookups by ID, returning the absolute position.
cases!(
    same,
    fn find(input: Input) -> Input {
        with!(&input.model, m => for id in black_box(&input.probes.ids) {
            black_box(m.find(&input.before, *id));
        });
        input
    }
);

// Every event of bars 40–60 of every voice, with absolute positions (one page of layout).
cases!(
    same,
    fn collect(input: Input) -> (Input, Vec<(orchestra_core_time::Position, EventId)>) {
        let mut out = Vec::with_capacity(1 << 16);
        with!(&input.model, m => m.collect(&input.before, COLLECT, &mut out));
        (input, black_box(out))
    }
);

// A regular bar inserted at bar 30.
cases!(
    after_insert_bar,
    fn insert_bar(input: Input) -> Input {
        with!(&mut input.model, m => m.insert_bar(&input.before, &input.after, INSERT_BAR));
        input
    }
);

// The free bar 10 lengthened by a quarter (a note entered past its end).
cases!(
    after_lengthen,
    fn lengthen_free_bar(input: Input) -> Input {
        with!(&mut input.model, m => m.lengthen_bar(&input.before, &input.after, FREE_BAR));
        input
    }
);

// 3/4 from bar 20 to 31, every bar keeping its contents (behaviour (i)).
cases!(
    after_keep_bars,
    fn meter_keep_bars(input: Input) -> Input {
        with!(&mut input.model, m => m.meter_keep_bars(&input.before, &input.after, METER_BAR));
        input
    }
);

// 3/4 from bar 20, the 12 bars rewritten into 16 (behaviour (ii)).
cases!(
    after_rewrite,
    fn meter_rewrite(input: Input) -> Input {
        with!(&mut input.model, m => m.meter_rewrite(
        &input.before, &input.after, METER_BAR, METER_UNTIL, METER_UNTIL_REWRITTEN));
        input
    }
);

// A snapshot for a background reader, then 64 inserts while the snapshot is alive (P3 pays the
// copy of each chunk it touches here).
cases!(
    same,
    fn snapshot_then_insert(input: Input) -> (Input, usize) {
        let inserts = std::mem::take(&mut input.probes.inserts);
        let kept = with!(&mut input.model, m => {
            let snapshot = black_box(m.snapshot());
            for (v, at, e) in black_box(&inserts) {
                m.insert(&input.before, *v, *at, *e);
            }
            // Keep the snapshot alive until after the inserts; its drop is measured too, as it would
            // happen on the reader's thread in the real program.
            std::mem::size_of_val(&snapshot)
        });
        input.probes.inserts = inserts;
        (input, kept)
    }
);

// The whole score in order with absolute positions (export, full playback compile).
cases!(
    same,
    fn iterate(input: Input) -> (Input, u64) {
        let mut sum = 0_u64;
        // Every visited position goes through `black_box`: a real consumer uses it, and otherwise
        // the optimiser may drop the additions that compute it (the cost P2/P3 pay over P1).
        with!(&input.model, m => m.for_each(&input.before, |v, p, e| {
            black_box(p);
            sum = sum.wrapping_add(e.id.0 ^ v as u64);
        }));
        (input, black_box(sum))
    }
);

library_benchmark_group!(
    name = storage,
    benchmarks = [
        insert,
        delete,
        find,
        collect,
        insert_bar,
        lengthen_free_bar,
        meter_keep_bars,
        meter_rewrite,
        snapshot_then_insert,
        iterate
    ]
);

main!(library_benchmark_groups = storage);
