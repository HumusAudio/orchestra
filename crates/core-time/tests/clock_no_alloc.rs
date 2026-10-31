// SPDX-License-Identifier: GPL-3.0-or-later

//! The rounding and constant-tempo conversions of [`SampleRate`], [`FrameRate`] and [`Tempo`]
//! are allocation-free (ADR-0020 §6, ADR-0007 rule 1): the audio thread will call them for every
//! event it schedules.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5).

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_time::{Duration, FrameRate, Rational, SampleRate, Seconds, Tempo};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// Every conversion on a few times, including an overflowing one (error path).
#[test]
fn conversions_do_not_allocate() {
    let rate = SampleRate::new(48_000).unwrap();
    let fps = FrameRate::new(Rational::new(30_000, 1001).unwrap()).unwrap();
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(3, 8).unwrap()),
        Rational::new(185, 2).unwrap(),
    )
    .unwrap();
    let times = [
        Seconds::ZERO,
        Seconds::from_rational(Rational::new(1, 96_000).unwrap()),
        Seconds::from_rational(Rational::new(-7, 3).unwrap()),
        Seconds::from_rational(Rational::from_integer(i64::MAX).unwrap()),
    ];
    let (rate, fps, tempo, times) = black_box((rate, fps, tempo, times));
    assert_no_allocations(|| {
        for t in times {
            black_box(rate.sample_at(t)).ok();
            black_box(fps.frame_at(t)).ok();
            black_box(tempo.duration_in(t)).ok();
            black_box(tempo.seconds_for(Duration::from_rational(t.as_rational()))).ok();
        }
        for n in [0_i64, 1, -1, 172_800_000, i64::MIN] {
            black_box(rate.time_of_sample(n)).ok();
            black_box(fps.frame_start(n)).ok();
        }
        black_box(tempo.speed()).ok();
    });
}
