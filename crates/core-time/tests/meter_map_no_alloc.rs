// SPDX-License-Identifier: GPL-3.0-or-later

//! Every [`MeterMap`] **lookup** is allocation-free (ADR-0018 §4, ADR-0007 rule 1), on its success
//! and on its error paths: the audio thread will call them for the metronome (F3). Editing the map
//! does allocate, by design (it rebuilds the segments on the editing path), so the map is built
//! before the measured block.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). Inputs go
//! through `black_box` so that the compiler cannot precompute the results.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_time::{
    BarIndex, BarKind, BarPosition, BeatGrouping, BeatPosition, Duration, IrregularBar, Meter,
    MeterChange, MeterMap, Position, Rational,
};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// A duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// A map with every kind of bar: pickup, meter changes, additive and grouped bars, a free bar,
/// numbering rules.
fn busy_map() -> MeterMap {
    let mut map = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    map.set_irregular_bar(
        BarIndex::FIRST,
        IrregularBar {
            length: d(1, 4),
            kind: BarKind::AlignEnd,
        },
    )
    .unwrap();
    map.set_meter_change(
        BarIndex::new(3),
        MeterChange::new(Meter::new(&[3, 2, 2], 8).unwrap()),
    )
    .unwrap();
    map.set_meter_change(
        BarIndex::new(6),
        MeterChange::with_grouping(
            Meter::new(&[9], 8).unwrap(),
            BeatGrouping::new(&[3, 2, 2, 2], 8).unwrap(),
        ),
    )
    .unwrap();
    map.set_irregular_bar(
        BarIndex::new(8),
        IrregularBar {
            length: d(7, 2),
            kind: BarKind::Free,
        },
    )
    .unwrap();
    map.set_uncounted(BarIndex::new(8), true);
    map.set_numbering_restart(BarIndex::new(10), Some(1));
    map
}

/// Every lookup on many bars and points, valid and invalid (before the start, outside the bar,
/// beat in a free bar, index overflow).
#[test]
fn lookups_do_not_allocate() {
    let map = busy_map();
    let positions: [Position; 6] = [
        Position::ZERO,
        Position::from_rational(Rational::new(13, 8).unwrap()),
        Position::from_rational(Rational::new(9, 1).unwrap()),
        Position::from_rational(Rational::new(-1, 4).unwrap()),
        Position::from_rational(Rational::new(1 << 40, 1).unwrap()),
        Position::from_rational(Rational::new(100, 3).unwrap()),
    ];
    let bars = [0_u32, 1, 3, 5, 6, 8, 9, 10, 500, u32::MAX].map(BarIndex::new);
    let offsets = [Duration::ZERO, d(3, 8), d(1, 1), d(-1, 8)];
    let (map, positions, bars, offsets) = black_box((&map, positions, bars, offsets));
    assert_no_allocations(|| {
        for p in positions {
            black_box(map.locate(p)).ok();
        }
        for bar in bars {
            black_box(map.bar_start(bar)).ok();
            black_box(map.bar_length(bar)).ok();
            black_box(map.meter_at(bar)).ok();
            black_box(map.is_free(bar)).ok();
            black_box(map.beat_range(bar)).ok();
            black_box(map.display_number(bar)).ok();
            for offset in offsets {
                let at = BarPosition { bar, offset };
                black_box(map.position_of(at)).ok();
                black_box(map.beat_of(at)).ok();
                for beat in [0_u32, 2, 7] {
                    black_box(map.beat_position(bar, BeatPosition { beat, offset })).ok();
                }
            }
        }
    });
}
