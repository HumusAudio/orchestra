// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written edge cases of [`Rational`] (ADR-0015, "Consequences"): `i64::MAX`, `i64::MIN`,
//! zero, negative values, results that fit only after reduction, and the overflows that must be
//! reported. Each case is written out so that a reader can check it by hand; the systematic
//! coverage is in `rational_oracle.rs`.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{Rational, RationalError};

/// Shorthand: builds `n/d`, which every caller here knows to be valid.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// Shorthand: numerator and denominator of a value.
fn parts(x: Rational) -> (i64, i64) {
    (x.numerator(), x.denominator())
}

/// `new` reduces and moves the sign to the numerator; zero is always `0/1`.
#[test]
fn construction_is_canonical() {
    assert_eq!(parts(r(6, 8)), (3, 4));
    assert_eq!(parts(r(6, -8)), (-3, 4));
    assert_eq!(parts(r(-6, -8)), (3, 4));
    assert_eq!(parts(r(0, -7)), (0, 1));
    assert_eq!(parts(r(0, i64::MAX)), (0, 1));
    assert_eq!(r(0, 5), Rational::ZERO);
    assert_eq!(r(9, 9), Rational::ONE);
    assert_eq!(parts(Rational::ZERO), (0, 1));
    assert_eq!(parts(Rational::ONE), (1, 1));
}

/// `i64::MIN` is accepted as an input when the reduced value fits, and rejected otherwise.
#[test]
fn i64_min_as_input() {
    // −2⁶³ / 2 = −2⁶²: fits.
    assert_eq!(parts(r(i64::MIN, 2)), (-(1 << 62), 1));
    // −2⁶³ / −2⁶³ = 1.
    assert_eq!(r(i64::MIN, i64::MIN), Rational::ONE);
    // 2 / −2⁶³ = −1/2⁶²: fits.
    assert_eq!(parts(r(2, i64::MIN)), (-1, 1 << 62));
    // −2⁶³ / 1: the numerator would be i64::MIN (invariant 3).
    assert_eq!(Rational::new(i64::MIN, 1), Err(RationalError::Overflow));
    // −2⁶³ / −1 = 2⁶³: does not fit.
    assert_eq!(Rational::new(i64::MIN, -1), Err(RationalError::Overflow));
    // 1 / −2⁶³: the denominator would be 2⁶³.
    assert_eq!(Rational::new(1, i64::MIN), Err(RationalError::Overflow));
    assert_eq!(
        Rational::from_integer(i64::MIN),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        parts(Rational::from_integer(-i64::MAX).unwrap()),
        (-i64::MAX, 1)
    );
}

/// Zero denominators and divisions by zero have their own error.
#[test]
fn division_by_zero() {
    assert_eq!(Rational::new(1, 0), Err(RationalError::DivisionByZero));
    assert_eq!(Rational::new(0, 0), Err(RationalError::DivisionByZero));
    assert_eq!(
        Rational::ZERO.checked_recip(),
        Err(RationalError::DivisionByZero)
    );
    assert_eq!(
        Rational::ONE.checked_div(Rational::ZERO),
        Err(RationalError::DivisionByZero)
    );
}

/// The musical motivation of the whole type: tuplets add up exactly.
#[test]
fn tuplets_add_up_exactly() {
    // Three triplet eighths make a quarter note.
    let triplet_eighth = r(1, 12);
    let mut sum = Rational::ZERO;
    for _ in 0..3 {
        sum = sum.checked_add(triplet_eighth).unwrap();
    }
    assert_eq!(sum, r(1, 4));
    // A 7:4 inside a 5:4 inside a 3:2 sixteenth (ADR-0015's example): 1/16 · 2/3 · 4/5 · 4/7.
    let nested = r(1, 16)
        .checked_mul(r(2, 3))
        .and_then(|x| x.checked_mul(r(4, 5)))
        .and_then(|x| x.checked_mul(r(4, 7)))
        .unwrap();
    // 1/16 · 2/3 = 1/24, · 4/5 = 1/30, · 4/7 = 2/105 of a whole note.
    assert_eq!(parts(nested), (2, 105));
    // 105 of them make exactly two whole notes, with no drift.
    let mut total = Rational::ZERO;
    for _ in 0..105 {
        total = total.checked_add(nested).unwrap();
    }
    assert_eq!(total, Rational::from_integer(2).unwrap());
}

/// Results that fit only **after** reduction succeed (ADR-0015 §4): the intermediate values are
/// far beyond `i64`, the final ones are not.
#[test]
fn results_that_fit_only_after_reduction() {
    let max = i64::MAX;
    // MAX/2 + MAX/2: the intermediate numerator 2·MAX does not fit, the reduced MAX/1 does.
    let half_max = r(max, 2);
    assert_eq!(parts(half_max.checked_add(half_max).unwrap()), (max, 1));
    // MAX/3 · 3/MAX = 1, through cross-reduction.
    assert_eq!(r(max, 3).checked_mul(r(3, max)).unwrap(), Rational::ONE);
    // (MAX−1)/MAX − (MAX−2)/MAX = 1/MAX (equal denominators, fast path).
    assert_eq!(
        parts(r(max - 1, max).checked_sub(r(max - 2, max)).unwrap()),
        (1, max)
    );
    // 1/MAX ÷ 1/MAX = 1.
    assert_eq!(r(1, max).checked_div(r(1, max)).unwrap(), Rational::ONE);
    // Different large denominators whose lcm fits: 1/(2·3·p) + 1/(2·5·p) with p large.
    let p = 1_000_000_007_i64;
    let sum = r(1, 6 * p).checked_add(r(1, 10 * p)).unwrap();
    // 1/6p + 1/10p = (5 + 3)/30p = 4/15p.
    assert_eq!(parts(sum), (4, 15 * p));
}

/// Results that do not fit are reported as `Overflow`, never wrapped or saturated.
#[test]
fn overflows_are_reported() {
    let max = Rational::from_integer(i64::MAX).unwrap();
    assert_eq!(max.checked_add(Rational::ONE), Err(RationalError::Overflow));
    assert_eq!(
        max.checked_mul(Rational::from_integer(2).unwrap()),
        Err(RationalError::Overflow)
    );
    // −MAX − 1 would be exactly i64::MIN: excluded by invariant 3.
    assert_eq!(
        (-max).checked_sub(Rational::ONE),
        Err(RationalError::Overflow)
    );
    // 1/MAX · 1/2: the denominator 2·MAX does not fit.
    assert_eq!(
        r(1, i64::MAX).checked_mul(r(1, 2)),
        Err(RationalError::Overflow)
    );
    // 1/MAX + 1/(MAX−1): coprime denominators, their product does not fit.
    assert_eq!(
        r(1, i64::MAX).checked_add(r(1, i64::MAX - 1)),
        Err(RationalError::Overflow)
    );
    // Dividing by a tiny value.
    assert_eq!(max.checked_div(r(1, 2)), Err(RationalError::Overflow));
}

/// Negation, absolute value and sign at the extremes of the range.
#[test]
fn negation_at_the_extremes() {
    let max = Rational::from_integer(i64::MAX).unwrap();
    assert_eq!(parts(-max), (-i64::MAX, 1));
    assert_eq!((-max).abs(), max);
    assert_eq!(-Rational::ZERO, Rational::ZERO);
    assert_eq!(parts(-r(3, 4)), (-3, 4));
    assert_eq!(r(-3, 4).signum(), -1);
    assert_eq!(Rational::ZERO.signum(), 0);
    assert_eq!(r(3, 4).signum(), 1);
}

/// The reciprocal swaps the parts and keeps the sign in the numerator.
#[test]
fn reciprocal() {
    assert_eq!(parts(r(3, 4).checked_recip().unwrap()), (4, 3));
    assert_eq!(parts(r(-3, 4).checked_recip().unwrap()), (-4, 3));
    assert_eq!(
        parts(r(1, i64::MAX).checked_recip().unwrap()),
        (i64::MAX, 1)
    );
    assert_eq!(
        parts(r(-i64::MAX, 1).checked_recip().unwrap()),
        (-1, i64::MAX)
    );
}

/// Rounding to integers, on both sides of zero and at the extremes.
#[test]
fn rounding() {
    assert_eq!(
        (r(7, 2).floor(), r(7, 2).ceil(), r(7, 2).trunc()),
        (3, 4, 3)
    );
    assert_eq!(
        (r(-7, 2).floor(), r(-7, 2).ceil(), r(-7, 2).trunc()),
        (-4, -3, -3)
    );
    assert_eq!(
        (r(6, 2).floor(), r(6, 2).ceil(), r(6, 2).trunc()),
        (3, 3, 3)
    );
    assert_eq!((Rational::ZERO.floor(), Rational::ZERO.ceil()), (0, 0));
    assert_eq!(r(i64::MAX, 1).ceil(), i64::MAX);
    assert_eq!(r(-i64::MAX, 1).floor(), -i64::MAX);
    assert_eq!(r(-i64::MAX, 2).floor(), -(i64::MAX / 2) - 1);
    assert_eq!(r(i64::MAX, 2).ceil(), i64::MAX / 2 + 1);
    assert_eq!(r(1, i64::MAX).ceil(), 1);
    assert_eq!(r(-1, i64::MAX).floor(), -1);
}

/// Comparisons across different denominators, including values that differ by one part in
/// 2⁶³ (where any float comparison would fail).
#[test]
fn comparisons() {
    assert!(r(1, 3) < r(1, 2));
    assert!(r(-1, 2) < r(-1, 3));
    assert_eq!(r(2, 4), r(1, 2));
    let max = i64::MAX;
    // (MAX−1)/MAX < (MAX−2)/(MAX−1): as f64 both are 1.0.
    assert!(r(max - 2, max - 1) < r(max - 1, max));
    assert_eq!(r(max - 2, max - 1).to_f64(), r(max - 1, max).to_f64());
    assert_eq!(r(1, 3).max(r(1, 2)), r(1, 2));
}

/// `to_f64` is the plain approximation, exact for musical values.
#[test]
fn approximation_for_display() {
    assert_eq!(r(1, 4).to_f64(), 0.25);
    assert_eq!(r(-3, 8).to_f64(), -0.375);
    assert_eq!(Rational::ZERO.to_f64(), 0.0);
}

/// The `Debug` form (there is deliberately no `Display` until F1.15).
#[test]
fn debug_format() {
    assert_eq!(format!("{:?}", r(3, 8)), "Rational(3/8)");
    assert_eq!(format!("{:?}", r(-3, 8)), "Rational(-3/8)");
    assert_eq!(format!("{:?}", r(4, 2)), "Rational(2)");
    assert_eq!(format!("{:?}", Rational::ZERO), "Rational(0)");
}

/// The developer-facing error messages (see `RationalError`: never shown to users as they are).
#[test]
fn error_messages() {
    assert!(RationalError::Overflow.to_string().contains("overflow"));
    assert!(RationalError::DivisionByZero.to_string().contains("zero"));
    // It is a real `std::error::Error`, so it can be boxed and propagated with `?`.
    let boxed: Box<dyn std::error::Error> = Box::new(RationalError::Overflow);
    assert!(boxed.source().is_none());
}

/// Constructors and arithmetic are `const fn`: values can be computed and checked at compile
/// time. If one of these failed, the build itself would fail.
#[test]
fn const_evaluation() {
    /// A quarter note, built at compile time.
    const QUARTER: Rational = match Rational::new(1, 4) {
        Ok(x) => x,
        Err(_) => panic!("1/4 is valid"),
    };
    /// A dotted quarter, computed at compile time from `QUARTER`.
    const DOTTED_QUARTER: Rational = match Rational::new(1, 8) {
        Ok(eighth) => match QUARTER.checked_add(eighth) {
            Ok(x) => x,
            Err(_) => panic!("3/8 fits"),
        },
        Err(_) => panic!("1/8 is valid"),
    };
    assert_eq!(parts(DOTTED_QUARTER), (3, 8));
}
