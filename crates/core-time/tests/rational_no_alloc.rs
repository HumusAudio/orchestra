// SPDX-License-Identifier: GPL-3.0-or-later

//! Every [`Rational`] operation is allocation-free (ADR-0015 §9, ADR-0007 rule 1), on its
//! success **and** on its error paths: the audio thread will compare and convert positions, and
//! an error must not be a hidden reason to touch the heap either.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). The
//! inputs go through `black_box` so that the compiler cannot evaluate the operations at compile
//! time (they are `const fn`) and leave nothing to measure.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_time::Rational;
use orchestra_test_support::assert_no_allocations;

orchestra_test_support::install_counting_allocator!();

/// Values that exercise every path: zero, one, negatives, equal and different denominators, the
/// extremes of the range (so that overflow errors are produced too).
fn samples() -> [Rational; 8] {
    let max = Rational::from_integer(i64::MAX).unwrap();
    [
        Rational::ZERO,
        Rational::ONE,
        Rational::new(-3, 8).unwrap(),
        Rational::new(5, 8).unwrap(),
        Rational::new(7, 12).unwrap(),
        Rational::new(1, i64::MAX).unwrap(),
        max,
        -max,
    ]
}

/// Construction, including its error paths (zero denominator, `i64::MIN`).
#[test]
fn construction_does_not_allocate() {
    let inputs = black_box([(6_i64, -8_i64), (1, 0), (i64::MIN, 1), (i64::MIN, 2)]);
    assert_no_allocations(|| {
        for (n, d) in inputs {
            black_box(Rational::new(black_box(n), black_box(d))).ok();
            black_box(Rational::from_integer(black_box(n))).ok();
        }
    });
}

/// The four checked operations and the reciprocal, on every pair of samples: values, overflows
/// and divisions by zero all occur among these pairs.
#[test]
fn arithmetic_does_not_allocate() {
    let values = black_box(samples());
    assert_no_allocations(|| {
        for x in values {
            black_box(x.checked_recip()).ok();
            for y in values {
                black_box(x.checked_add(y)).ok();
                black_box(x.checked_sub(y)).ok();
                black_box(x.checked_mul(y)).ok();
                black_box(x.checked_div(y)).ok();
            }
        }
    });
}

/// Comparisons, sign, rounding, negation and the float approximation.
#[test]
fn infallible_operations_do_not_allocate() {
    let values = black_box(samples());
    assert_no_allocations(|| {
        for x in values {
            black_box(-x);
            black_box(x.abs());
            black_box((x.signum(), x.is_zero(), x.is_positive(), x.is_negative()));
            black_box((x.is_integer(), x.floor(), x.ceil(), x.trunc()));
            black_box(x.to_f64());
            black_box((x.numerator(), x.denominator()));
            for y in values {
                black_box(x.cmp(&y));
                black_box(x == y);
            }
        }
    });
}
