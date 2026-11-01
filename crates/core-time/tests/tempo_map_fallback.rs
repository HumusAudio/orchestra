// SPDX-License-Identifier: GPL-3.0-or-later

//! How often the exact check of a sample or frame index inside a gradual change cannot be
//! completed and the `f64` estimate is returned as [`IndexAt::Estimated`] (ADR-0020 §5, ADR-0021
//! §5: "F1.05b measures how often it happens").
//!
//! # The score
//! A long, deliberately unfriendly score: about two and a half hours of music (3000 whole notes)
//! with a tempo change every 4 whole notes, two thirds of them gradual, cycling through metronome
//! marks with large prime factors (♩ = 41, 59, 97, 131, 211…), fractional marks (92.5) and several
//! beat units. Far from the start the start times have large numerators and every boundary is off
//! the grid, which is the worst case for the exact check.
//!
//! # The queries
//! Points every 1/7 + 1/11 of a whole note (a stride that keeps hitting new fractions), at the
//! common sample rates, at 384 kHz (which does not divide the grid) and at three frame rates. The
//! test prints the counts and fails if any rate dividing the grid needs the estimate.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{
    Duration, FrameRate, GradualChange, IndexAt, Position, Rational, SampleRate, Tempo,
    TempoChange, TempoMap,
};

/// The rational `n/d`.
fn q(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// The unfriendly score of the module documentation.
fn long_score() -> TempoMap {
    let units = [q(1, 4), q(3, 8), q(1, 8), q(1, 2)];
    let marks = [
        q(41, 1),
        q(59, 1),
        q(97, 1),
        q(131, 1),
        q(211, 1),
        q(185, 2),
        q(73, 1),
        q(167, 1),
        q(53, 1),
    ];
    let tempo = |i: usize| {
        Tempo::new(
            Duration::from_rational(units[i % units.len()]),
            marks[i % marks.len()],
        )
        .unwrap()
    };
    let mut map = TempoMap::new(tempo(0)).unwrap();
    for i in 1..750_usize {
        let start = Position::from_rational(q(i64::try_from(i).unwrap() * 4, 1));
        let change = if i % 3 == 0 {
            TempoChange::Immediate(tempo(i))
        } else {
            TempoChange::Gradual(GradualChange {
                from: (i % 5 == 0).then(|| tempo(i + 3)),
                to: tempo(i),
                end: start.checked_add(Duration::from_rational(q(7, 2))).unwrap(),
            })
        };
        map.set_change(start, change).unwrap();
    }
    map
}

/// Counts the estimated indices at each rate and fails if a rate dividing the grid needed one.
#[test]
fn estimates_are_rare_and_only_off_the_grid() {
    let map = long_score();
    let samples = [44_100_u32, 48_000, 96_000, 192_000, 384_000];
    let frames = [(24, 1), (25, 1), (30_000, 1001)];
    let stride = q(1, 7).checked_add(q(1, 11)).unwrap();
    let mut points = Vec::new();
    let mut p = Position::ZERO;
    while p < Position::from_rational(q(3000, 1)) {
        points.push(p);
        p = p.checked_add(Duration::from_rational(stride)).unwrap();
    }
    for hz in samples {
        let rate = SampleRate::new(hz).unwrap();
        let estimated = points
            .iter()
            .filter(|&&p| matches!(map.sample_at(p, rate).unwrap(), IndexAt::Estimated(_)))
            .count();
        eprintln!("{hz} Hz: {estimated} estimated of {}", points.len());
        assert!(
            hz == 384_000 || estimated == 0,
            "{hz} Hz needed {estimated} estimates"
        );
    }
    for (n, d) in frames {
        let fps = FrameRate::new(q(n, d)).unwrap();
        let estimated = points
            .iter()
            .filter(|&&p| matches!(map.frame_at(p, fps).unwrap(), IndexAt::Estimated(_)))
            .count();
        eprintln!("{n}/{d} fps: {estimated} estimated of {}", points.len());
        assert_eq!(estimated, 0, "{n}/{d} fps needed {estimated} estimates");
    }
    let end = map.time_at(Position::from_rational(q(3000, 1))).unwrap();
    eprintln!("score length: {:.0} s", end.to_f64());
}
