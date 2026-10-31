// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the domain types [`Position`] and [`Duration`] (ADR-0018 §1).
//!
//! # What there is to test
//! The types add no arithmetic: every method forwards to a [`Rational`] operation, which is
//! already checked against an exact oracle in `rational_oracle.rs`. So the tests here check:
//! 1. **forwarding**: each method returns exactly what the corresponding `Rational` operation
//!    returns, errors included (a wrapper calling `checked_add` instead of `checked_sub` would be
//!    caught here);
//! 2. **the algebra** that motivates the types: `(p + d) − p = d`, `a − b = −(b − a)`, …;
//! 3. hand-written cases a reader can check: musical values, error propagation, `Debug`.
//!
//! The rejected expressions (`position + position`, `position × number`) are checked by the
//! `compile_fail` doctests in the module documentation.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_time::{Duration, Position, Rational, RationalError};
use proptest::prelude::*;

/// Shorthand: the rational `n/d`, which every caller knows to be valid.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// Shorthand: the position `n/d` whole notes from the start.
fn pos(n: i64, d: i64) -> Position {
    Position::from_rational(r(n, d))
}

/// Shorthand: the duration of `n/d` whole notes.
fn dur(n: i64, d: i64) -> Duration {
    Duration::from_rational(r(n, d))
}

// ------------------------------------------------------------------------------------------------
// Input generators
// ------------------------------------------------------------------------------------------------

/// A rational that is either musical (small numerator and denominator, where the algebraic laws
/// must hold without overflow) or extreme (close to `i64::MAX`, where overflow errors appear and
/// must be forwarded unchanged).
fn rational() -> impl Strategy<Value = Rational> {
    prop_oneof![
        // Musical values: bars, beats, tuplets of tuplets.
        (-100_000_i64..=100_000, 1_i64..=1_680).prop_map(|(n, d)| r(n, d)),
        // Extreme values: their sums and products overflow.
        (0_i64..=16, 1_i64..=3, any::<bool>()).prop_map(|(k, d, neg)| {
            let x = r(i64::MAX - k, d);
            if neg { -x } else { x }
        }),
    ]
}

/// A small rational, for which none of the operations below can overflow: the algebraic laws
/// are only meaningful when every step succeeds.
fn small() -> impl Strategy<Value = Rational> {
    (-100_000_i64..=100_000, 1_i64..=1_680).prop_map(|(n, d)| r(n, d))
}

// ------------------------------------------------------------------------------------------------
// Forwarding: each method is exactly its `Rational` operation
// ------------------------------------------------------------------------------------------------

proptest! {
    // 1024 cases: the methods are trivial forwards, but the extreme inputs must come up often
    // enough to exercise the error paths.
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Position ± duration and position − position forward to `Rational`'s sum and difference.
    #[test]
    fn position_methods_forward(a in rational(), b in rational()) {
        let p = Position::from_rational(a);
        prop_assert_eq!(
            p.checked_add(Duration::from_rational(b)).map(Position::as_rational),
            a.checked_add(b)
        );
        prop_assert_eq!(
            p.checked_sub(Duration::from_rational(b)).map(Position::as_rational),
            a.checked_sub(b)
        );
        prop_assert_eq!(
            p.checked_duration_since(Position::from_rational(b)).map(Duration::as_rational),
            a.checked_sub(b)
        );
    }

    /// Every fallible `Duration` method forwards to the corresponding `Rational` operation.
    #[test]
    fn duration_methods_forward(a in rational(), b in rational()) {
        let (x, y) = (Duration::from_rational(a), Duration::from_rational(b));
        prop_assert_eq!(x.checked_add(y).map(Duration::as_rational), a.checked_add(b));
        prop_assert_eq!(x.checked_sub(y).map(Duration::as_rational), a.checked_sub(b));
        prop_assert_eq!(x.checked_mul(b).map(Duration::as_rational), a.checked_mul(b));
        prop_assert_eq!(x.checked_div(b).map(Duration::as_rational), a.checked_div(b));
        prop_assert_eq!(x.checked_ratio(y), a.checked_div(b));
    }

    /// The infallible `Duration` methods and the comparisons agree with `Rational`.
    #[test]
    fn duration_infallible_methods_forward(a in rational(), b in rational()) {
        let (x, y) = (Duration::from_rational(a), Duration::from_rational(b));
        prop_assert_eq!((-x).as_rational(), -a);
        prop_assert_eq!(x.abs().as_rational(), a.abs());
        prop_assert_eq!(x.signum(), a.signum());
        prop_assert_eq!(x.is_zero(), a.is_zero());
        prop_assert_eq!(x.is_positive(), a.is_positive());
        prop_assert_eq!(x.is_negative(), a.is_negative());
        prop_assert_eq!(x.cmp(&y), a.cmp(&b));
        prop_assert_eq!(
            Position::from_rational(a).cmp(&Position::from_rational(b)),
            a.cmp(&b)
        );
    }

    // --------------------------------------------------------------------------------------------
    // The algebra
    // --------------------------------------------------------------------------------------------

    /// Moving by a duration and measuring back gives the same duration: `(p + d) − p = d`.
    #[test]
    fn move_then_measure(p in small(), d in small()) {
        let p = Position::from_rational(p);
        let d = Duration::from_rational(d);
        let moved = p.checked_add(d).unwrap();
        prop_assert_eq!(moved.checked_duration_since(p).unwrap(), d);
        // And moving back lands on the start again: `(p + d) − d = p`.
        prop_assert_eq!(moved.checked_sub(d).unwrap(), p);
    }

    /// Measuring in the other direction flips the sign: `a − b = −(b − a)`.
    #[test]
    fn duration_since_is_antisymmetric(a in small(), b in small()) {
        let (a, b) = (Position::from_rational(a), Position::from_rational(b));
        prop_assert_eq!(
            a.checked_duration_since(b).unwrap(),
            -b.checked_duration_since(a).unwrap()
        );
    }

    /// The ratio of two durations, multiplied back by the unit, gives the duration again.
    #[test]
    fn ratio_times_unit(a in small(), b in small()) {
        prop_assume!(!b.is_zero());
        let (x, unit) = (Duration::from_rational(a), Duration::from_rational(b));
        let ratio = x.checked_ratio(unit).unwrap();
        prop_assert_eq!(unit.checked_mul(ratio).unwrap(), x);
    }
}

// ------------------------------------------------------------------------------------------------
// Hand-written cases
// ------------------------------------------------------------------------------------------------

/// Musical values: four quarters make a 4/4 bar, a triplet fits exactly, a dotted quarter is
/// three eighths.
#[test]
fn musical_values() {
    let quarter = dur(1, 4);
    let bar = quarter.checked_mul(r(4, 1)).unwrap();
    assert_eq!(bar, dur(1, 1));
    // Bar 3 of a 4/4 piece starts two whole notes after the start.
    let bar_three = Position::ZERO
        .checked_add(bar)
        .unwrap()
        .checked_add(bar)
        .unwrap();
    assert_eq!(bar_three, pos(2, 1));
    // A quarter inside a 3:2 triplet is 1/6; three of them make a half note exactly.
    let triplet_quarter = quarter.checked_mul(r(2, 3)).unwrap();
    assert_eq!(triplet_quarter, dur(1, 6));
    assert_eq!(triplet_quarter.checked_mul(r(3, 1)).unwrap(), dur(1, 2));
    // A dotted quarter is three eighths, and a 6/8 bar holds two of them.
    let dotted = quarter.checked_mul(r(3, 2)).unwrap();
    assert_eq!(dotted, dur(3, 8));
    assert_eq!(dur(6, 8).checked_ratio(dotted).unwrap(), r(2, 1));
}

/// A position before the start and a negative duration are both valid values (ADR-0018 §1).
#[test]
fn negative_values_are_allowed() {
    // A first note played a 32nd early by a microtiming offset sounds before zero.
    let early = Position::ZERO.checked_add(dur(-1, 32)).unwrap();
    assert!(early < Position::ZERO);
    assert_eq!(early, pos(-1, 32));
    // Measuring backwards gives a negative duration, whose absolute value is the distance.
    let back = Position::ZERO.checked_duration_since(pos(3, 4)).unwrap();
    assert!(back.is_negative());
    assert_eq!(back.signum(), -1);
    assert_eq!(back.abs(), dur(3, 4));
    assert_eq!(-back, dur(3, 4));
}

/// Errors of the underlying arithmetic reach the caller unchanged.
#[test]
fn errors_are_forwarded() {
    let max = Rational::from_integer(i64::MAX).unwrap();
    let far = Position::from_rational(max);
    assert_eq!(far.checked_add(dur(1, 1)), Err(RationalError::Overflow));
    assert_eq!(
        Position::from_rational(-max).checked_duration_since(pos(2, 1)),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        Duration::from_rational(max).checked_mul(r(2, 1)),
        Err(RationalError::Overflow)
    );
    assert_eq!(
        dur(1, 4).checked_div(Rational::ZERO),
        Err(RationalError::DivisionByZero)
    );
    assert_eq!(
        dur(1, 4).checked_ratio(Duration::ZERO),
        Err(RationalError::DivisionByZero)
    );
}

/// The constants are zero, and the explicit conversions are inverse of each other.
#[test]
fn constants_and_conversions() {
    assert_eq!(Position::ZERO.as_rational(), Rational::ZERO);
    assert_eq!(Duration::ZERO.as_rational(), Rational::ZERO);
    assert!(Duration::ZERO.is_zero());
    let x = r(-7, 12);
    assert_eq!(Position::from_rational(x).as_rational(), x);
    assert_eq!(Duration::from_rational(x).as_rational(), x);
}

/// The types are as small as `Rational`: the wrapping costs nothing (ADR-0018 §1).
#[test]
fn same_size_as_rational() {
    assert_eq!(size_of::<Position>(), size_of::<Rational>());
    assert_eq!(size_of::<Duration>(), size_of::<Rational>());
}

/// `Debug` names the type and writes the fraction, or the integer alone.
#[test]
fn debug_format() {
    assert_eq!(format!("{:?}", pos(3, 8)), "Position(3/8)");
    assert_eq!(format!("{:?}", pos(2, 1)), "Position(2)");
    assert_eq!(format!("{:?}", dur(-1, 4)), "Duration(-1/4)");
    assert_eq!(format!("{:?}", Duration::ZERO), "Duration(0)");
}

/// The methods are `const fn`: fixed musical values can be computed at compile time.
#[test]
fn usable_in_const_context() {
    /// A quarter note, computed by the compiler.
    const QUARTER: Duration = match Rational::new(1, 4) {
        Ok(q) => Duration::from_rational(q),
        Err(_) => panic!("1/4 is a valid rational"),
    };
    /// Beat 2 of a 4/4 bar, computed by the compiler.
    const BEAT_TWO: Position = match Position::ZERO.checked_add(QUARTER) {
        Ok(p) => p,
        Err(_) => panic!("0 + 1/4 cannot overflow"),
    };
    assert_eq!(BEAT_TWO, pos(1, 4));
}
