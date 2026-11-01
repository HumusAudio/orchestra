// SPDX-License-Identifier: GPL-3.0-or-later

//! Every [`TempoMap`] **lookup** is allocation-free (ADR-0020 §6, ADR-0007 rule 1), on its success
//! and on its error paths, including the slow path of the grid rounding (the gcd reduction of
//! ADR-0021 §4): the audio thread will convert positions and times through them. Editing the map
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
    Duration, FrameRate, GradualChange, Position, Rational, SampleRate, Seconds, Tempo,
    TempoChange, TempoMap,
};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// The rational `n/d`.
fn q(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// A tempo of `bpm` beats of `unit` whole notes.
fn tempo(unit: Rational, bpm: Rational) -> Tempo {
    Tempo::new(Duration::from_rational(unit), bpm).unwrap()
}

/// A map with every kind of segment: constant, gradual with and without a starting tempo,
/// back-to-back gradual changes, an implicit constant segment, boundaries off the grid.
fn busy_map() -> TempoMap {
    let at = |n, d| Position::from_rational(q(n, d));
    let mut map = TempoMap::new(tempo(q(1, 4), q(77, 1))).unwrap();
    map.set_change(
        at(2, 1),
        TempoChange::Gradual(GradualChange {
            from: None,
            to: tempo(q(1, 4), q(131, 1)),
            end: at(5, 1),
        }),
    )
    .unwrap();
    map.set_change(
        at(5, 1),
        TempoChange::Gradual(GradualChange {
            from: Some(tempo(q(3, 8), q(185, 2))),
            to: tempo(q(1, 8), q(211, 1)),
            end: at(25, 3),
        }),
    )
    .unwrap();
    map.set_change(at(12, 1), TempoChange::Immediate(tempo(q(1, 2), q(63, 1))))
        .unwrap();
    map
}

/// Both conversions on many points, valid and failing (overflow, foreign denominators).
#[test]
fn lookups_do_not_allocate() {
    let map = busy_map();
    let positions: [Position; 8] = [
        Position::ZERO,
        Position::from_rational(q(-3, 4)),
        Position::from_rational(q(2, 1)),
        Position::from_rational(q(7, 2)),
        Position::from_rational(q(6, 1)),
        Position::from_rational(q(10, 1)),
        Position::from_rational(q(100, 7)),
        Position::from_rational(q(i64::MAX, 1)),
    ];
    let times: [Seconds; 8] = [
        Seconds::ZERO,
        Seconds::from_rational(q(-1, 3)),
        Seconds::from_rational(q(312_345, 48_000)),
        Seconds::from_rational(q(700_001, 44_100)),
        Seconds::from_rational(q(1001 * 400, 30_000)),
        Seconds::from_rational(q(66_298, 3_001)),
        Seconds::from_rational(q(1_000, 1)),
        Seconds::from_rational(q(i64::MAX, 1)),
    ];
    let rates = [44_100_u32, 48_000, 384_000].map(|hz| SampleRate::new(hz).unwrap());
    let fps = [q(24, 1), q(30_000, 1001)].map(|f| FrameRate::new(f).unwrap());
    let (map, positions, times, rates, fps) = black_box((&map, positions, times, rates, fps));
    assert_no_allocations(|| {
        for p in positions {
            black_box(map.time_at(p)).ok();
        }
        for t in times {
            black_box(map.position_at(t)).ok();
        }
        // Sample and frame indices (with the exact check inside gradual changes) and their
        // inverses, on every point and rate.
        for p in positions {
            for rate in rates {
                black_box(map.sample_at(p, rate)).ok();
            }
            for f in fps {
                black_box(map.frame_at(p, f)).ok();
            }
        }
        for n in [0_i64, 123_457, 1_000_003, -48_000, i64::MAX] {
            for rate in rates {
                black_box(map.position_of_sample(n, rate)).ok();
            }
            for f in fps {
                black_box(map.position_of_frame(n, f)).ok();
            }
        }
        // Many sample instants across every segment, including the gradual ones.
        for n in (0..3_000_000_i64).step_by(10_007) {
            black_box(map.position_at(Seconds::from_rational(q(n, 48_000)))).ok();
        }
    });
}
