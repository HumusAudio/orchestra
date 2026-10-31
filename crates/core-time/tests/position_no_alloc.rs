// SPDX-License-Identifier: GPL-3.0-or-later

//! Every [`Position`] and [`Duration`] operation is allocation-free (ADR-0018 §1, ADR-0007
//! rule 1), on its success and on its error paths: the audio thread will move and measure
//! positions when it converts them to samples (F1.05).
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). The
//! inputs go through `black_box` so that the compiler cannot evaluate the `const fn` operations at
//! compile time and leave nothing to measure.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_time::{Duration, Position, Rational};
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// Values that exercise every path: zero, musical values, negatives and the extremes of the range
/// (so that overflow errors are produced too).
fn samples() -> [Rational; 6] {
    let max = Rational::from_integer(i64::MAX).unwrap();
    [
        Rational::ZERO,
        Rational::new(-3, 8).unwrap(),
        Rational::new(7, 12).unwrap(),
        Rational::new(1, i64::MAX).unwrap(),
        max,
        -max,
    ]
}

/// Every method of both types, on every pair of samples: values, overflows and divisions by zero
/// all occur among these pairs.
#[test]
fn position_and_duration_do_not_allocate() {
    let values = black_box(samples());
    assert_no_allocations(|| {
        for a in values {
            for b in values {
                let (p, q) = (Position::from_rational(a), Position::from_rational(b));
                let (x, y) = (Duration::from_rational(a), Duration::from_rational(b));
                black_box(p.checked_add(y)).ok();
                black_box(p.checked_sub(y)).ok();
                black_box(p.checked_duration_since(q)).ok();
                black_box(x.checked_add(y)).ok();
                black_box(x.checked_sub(y)).ok();
                black_box(x.checked_mul(b)).ok();
                black_box(x.checked_div(b)).ok();
                black_box(x.checked_ratio(y)).ok();
                black_box((-x, x.abs(), x.signum(), x.is_zero(), p < q, x < y));
            }
        }
    });
}
