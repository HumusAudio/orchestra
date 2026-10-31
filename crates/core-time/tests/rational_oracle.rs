// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of [`Rational`] against an independent exact oracle (ADR-0015, ADR-0016 §4).
//!
//! # The oracle
//! Every operation is recomputed here in `i128` by the **textbook** formulas, with no shortcut:
//! `a/b + c/d = (a·d + c·b) / (b·d)`, `a/b · c/d = (a·c) / (b·d)`, and only then reduced with
//! Euclid's algorithm (`%`). The library uses different algorithms (division by the gcd of the
//! denominators before adding, cross-reduction before multiplying, Stein's gcd), so a bug in one
//! of the two shows up as a disagreement instead of being reproduced by both.
//!
//! The textbook formulas cannot overflow `i128`: numerators and denominators of valid values are
//! at most `2⁶³ − 1` in magnitude (invariants 1 and 3), so each product is below 2¹²⁶ and a sum
//! of two products below 2¹²⁷, which `i128` holds. The oracle uses plain operators on purpose: if
//! that reasoning were wrong, the test would panic in debug builds rather than pass silently.
//!
//! The oracle then decides what the library must return: the reduced value if it fits the
//! invariants (`|num| ≤ i64::MAX`, `1 ≤ den ≤ i64::MAX`), `Overflow` otherwise.
//!
//! # The inputs
//! Uniformly random `i64` almost never hit the interesting cases (`i64::MAX`, `i64::MIN`, big
//! common factors), so [`component`] mixes several generators that favour them, and [`pair`]
//! builds pairs of related values (equal, opposite, reciprocal, sharing a large factor), where
//! intermediate results are large but the reduced result often fits: exactly the case where
//! ADR-0015 is stricter than `num-rational`.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]
// The oracle mirrors the textbook formulas `a/b + c/d = (a·d + c·b)/(b·d)`: the single letters ARE
// the clearest names here, because they let a reader compare the code with the formula.
#![allow(
    clippy::many_single_char_names,
    reason = "names mirror the textbook formulas"
)]

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use orchestra_core_time::{Rational, RationalError};
use proptest::prelude::*;

// ------------------------------------------------------------------------------------------------
// The oracle
// ------------------------------------------------------------------------------------------------

/// Greatest common divisor by Euclid's algorithm on magnitudes: deliberately a different
/// algorithm from the library's binary (Stein) gcd.
fn euclid(a: i128, b: i128) -> u128 {
    let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// What the library must return for the exact fraction `num/den`, as `(numerator, denominator)`.
///
/// - `den == 0` → `DivisionByZero`;
/// - otherwise reduce (sign to the numerator) and check the invariants of ADR-0015 §2:
///   `|num| ≤ i64::MAX` (which also excludes `i64::MIN`) and `den ≤ i64::MAX` → value, else
///   `Overflow`.
fn expected(num: i128, den: i128) -> Result<(i64, i64), RationalError> {
    if den == 0 {
        return Err(RationalError::DivisionByZero);
    }
    let g = euclid(num, den);
    // `g ≥ 1` because `den ≠ 0`. Magnitudes are at most 2¹²⁷ (the magnitude of `i128::MIN`),
    // and `u128` holds them.
    let n = num.unsigned_abs() / g;
    let d = den.unsigned_abs() / g;
    let max = u128::try_from(i64::MAX).unwrap();
    if n > max || d > max {
        return Err(RationalError::Overflow);
    }
    let n = i64::try_from(n).unwrap();
    let d = i64::try_from(d).unwrap();
    // The sign of the value is negative when exactly one part is negative (and it is not zero).
    let negative = (num < 0) != (den < 0);
    Ok((if negative { -n } else { n }, d))
}

/// The parts of a library result, in the same shape as [`expected`].
fn parts(r: Result<Rational, RationalError>) -> Result<(i64, i64), RationalError> {
    r.map(|r| (r.numerator(), r.denominator()))
}

/// Numerator and denominator of `r`, widened to `i128` for the oracle's formulas.
fn wide(r: Rational) -> (i128, i128) {
    (i128::from(r.numerator()), i128::from(r.denominator()))
}

/// Oracle sum: `(a·d + c·b) / (b·d)`.
fn oracle_add(x: Rational, y: Rational) -> Result<(i64, i64), RationalError> {
    let ((a, b), (c, d)) = (wide(x), wide(y));
    expected(a * d + c * b, b * d)
}

/// Oracle difference: `(a·d − c·b) / (b·d)`.
fn oracle_sub(x: Rational, y: Rational) -> Result<(i64, i64), RationalError> {
    let ((a, b), (c, d)) = (wide(x), wide(y));
    expected(a * d - c * b, b * d)
}

/// Oracle product: `(a·c) / (b·d)`.
fn oracle_mul(x: Rational, y: Rational) -> Result<(i64, i64), RationalError> {
    let ((a, b), (c, d)) = (wide(x), wide(y));
    expected(a * c, b * d)
}

/// Oracle quotient: `(a·d) / (b·c)`; `c = 0` gives a zero denominator, hence `DivisionByZero`.
fn oracle_div(x: Rational, y: Rational) -> Result<(i64, i64), RationalError> {
    let ((a, b), (c, d)) = (wide(x), wide(y));
    expected(a * d, b * c)
}

/// Checks the three invariants of ADR-0015 §2 on a value produced by the library.
fn assert_canonical(r: Rational) {
    let (n, d) = (r.numerator(), r.denominator());
    assert!(d >= 1, "denominator must be ≥ 1: {r:?}");
    assert_ne!(n, i64::MIN, "numerator must not be i64::MIN: {r:?}");
    assert_eq!(
        euclid(i128::from(n), i128::from(d)),
        1,
        "must be in lowest terms: {r:?}"
    );
}

/// Checks a library result against the oracle and, when it is a value, its invariants.
fn check(actual: Result<Rational, RationalError>, oracle: Result<(i64, i64), RationalError>) {
    if let Ok(r) = actual {
        assert_canonical(r);
    }
    assert_eq!(parts(actual), oracle);
}

/// The hash of a value with the standard hasher, to test `Hash`/`Eq` consistency.
fn hash_of(r: Rational) -> u64 {
    let mut h = DefaultHasher::new();
    r.hash(&mut h);
    h.finish()
}

// ------------------------------------------------------------------------------------------------
// Input generators
// ------------------------------------------------------------------------------------------------

/// An `i64` from a mixture of generators that favour the edge cases of ADR-0015.
fn component() -> impl Strategy<Value = i64> {
    prop_oneof![
        // Anything at all.
        any::<i64>(),
        // Small values: real musical numbers (bars, tuplet ratios).
        -1_000_i64..=1_000,
        // Just below the top and just above the bottom of the range.
        (0_i64..=64).prop_map(|k| i64::MAX - k),
        (0_i64..=64).prop_map(|k| i64::MIN + k),
        // Signed powers of two, up to 2⁶²: lots of common factors with musical denominators.
        (0_u32..63, any::<bool>()).prop_map(|(e, neg)| {
            let v = 1_i64 << e;
            if neg { -v } else { v }
        }),
        // "Smooth" numbers: products of small primes, as in tuplets of tuplets. They share large
        // factors with each other, so reductions are big.
        prop::collection::vec(prop::sample::select(vec![2_i64, 3, 5, 7, 11, 13]), 0..40).prop_map(
            |factors| {
                factors
                    .into_iter()
                    .try_fold(1_i64, i64::checked_mul)
                    .unwrap_or(i64::MAX)
            }
        ),
    ]
}

/// Any representable [`Rational`], built from two [`component`]s (pairs that do not fit after
/// reduction, or with a zero denominator, are discarded and redrawn).
fn rational() -> impl Strategy<Value = Rational> {
    (component(), component())
        .prop_filter_map("not representable", |(n, d)| Rational::new(n, d).ok())
}

/// Two values that are often **related**, so that intermediate results are large while reduced
/// results often fit: the cases where ADR-0015 must succeed and `num-rational` would fail.
fn pair() -> impl Strategy<Value = (Rational, Rational)> {
    prop_oneof![
        // Independent values.
        (rational(), rational()),
        // Equal and opposite values: `x − x = 0`, `x + (−x) = 0`, `x / x = 1`.
        rational().prop_map(|x| (x, x)),
        rational().prop_map(|x| (x, -x)),
        // A value and a small multiple of its reciprocal: the product is small.
        (rational(), -8_i64..=8).prop_filter_map("zero or not representable", |(x, k)| {
            let r = x.checked_recip().ok()?;
            Some((x, r.checked_mul(Rational::from_integer(k).ok()?).ok()?))
        }),
        // Two values with the same large denominator: the equal-denominator fast path.
        (component(), component(), component())
            .prop_filter_map("not representable", |(a, c, d)| {
                Some((Rational::new(a, d).ok()?, Rational::new(c, d).ok()?))
            }),
    ]
}

// ------------------------------------------------------------------------------------------------
// Properties
// ------------------------------------------------------------------------------------------------

proptest! {
    // 2048 cases per property instead of the default 256: each case costs microseconds, and the
    // edge cases are rare enough that more draws are worth it.
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// `new` agrees with the oracle on every pair of `i64`, zero denominators and `i64::MIN`
    /// included.
    #[test]
    fn new_matches_oracle(n in component(), d in component()) {
        check(Rational::new(n, d), expected(i128::from(n), i128::from(d)));
    }

    /// `new` on a zero denominator is always `DivisionByZero`, whatever the numerator.
    #[test]
    fn new_with_zero_denominator(n in component()) {
        prop_assert_eq!(Rational::new(n, 0), Err(RationalError::DivisionByZero));
    }

    /// `from_integer` is `new(n, 1)`.
    #[test]
    fn from_integer_matches_new(n in component()) {
        prop_assert_eq!(Rational::from_integer(n), Rational::new(n, 1));
    }

    /// Sum against the oracle.
    #[test]
    fn add_matches_oracle((x, y) in pair()) {
        check(x.checked_add(y), oracle_add(x, y));
    }

    /// Difference against the oracle.
    #[test]
    fn sub_matches_oracle((x, y) in pair()) {
        check(x.checked_sub(y), oracle_sub(x, y));
    }

    /// Product against the oracle.
    #[test]
    fn mul_matches_oracle((x, y) in pair()) {
        check(x.checked_mul(y), oracle_mul(x, y));
    }

    /// Quotient against the oracle (dividing by zero included, when `y` is zero).
    #[test]
    fn div_matches_oracle((x, y) in pair()) {
        check(x.checked_div(y), oracle_div(x, y));
    }

    /// Reciprocal against the oracle: `1/x = b/a`.
    #[test]
    fn recip_matches_oracle(x in rational()) {
        let (a, b) = wide(x);
        check(x.checked_recip(), expected(b, a));
    }

    /// Negation and absolute value never fail and agree with the oracle.
    #[test]
    fn neg_and_abs_match_oracle(x in rational()) {
        let (a, b) = wide(x);
        check(Ok(-x), expected(-a, b));
        check(Ok(x.abs()), expected(a.abs(), b));
        prop_assert_eq!(-(-x), x);
    }

    /// Sign predicates agree with the numerator's sign and with comparisons against zero.
    #[test]
    fn sign_predicates(x in rational()) {
        let s = x.numerator().signum();
        prop_assert_eq!(x.signum(), s);
        prop_assert_eq!(x.is_zero(), s == 0);
        prop_assert_eq!(x.is_positive(), s > 0);
        prop_assert_eq!(x.is_negative(), s < 0);
        prop_assert_eq!(x.cmp(&Rational::ZERO), s.cmp(&0));
        prop_assert_eq!(x.is_integer(), x.denominator() == 1);
    }

    /// `floor`, `ceil` and `trunc` against `i128` integer division, plus their defining
    /// inequalities checked with `Rational` comparisons.
    #[test]
    fn rounding_matches_oracle(x in rational()) {
        let (a, b) = wide(x);
        let floor = a.div_euclid(b);
        let ceil = -((-a).div_euclid(b));
        let trunc = a / b;
        prop_assert_eq!(i128::from(x.floor()), floor);
        prop_assert_eq!(i128::from(x.ceil()), ceil);
        prop_assert_eq!(i128::from(x.trunc()), trunc);
        // floor(x) ≤ x ≤ ceil(x), with equality exactly for integers.
        let f = Rational::from_integer(x.floor()).unwrap();
        let c = Rational::from_integer(x.ceil()).unwrap();
        prop_assert!(f <= x && x <= c);
        prop_assert_eq!(f == c, x.is_integer());
    }

    /// Comparison against the oracle's exact cross products, and consistency with `==`.
    #[test]
    fn cmp_matches_oracle((x, y) in pair()) {
        let ((a, b), (c, d)) = (wide(x), wide(y));
        let oracle = (a * d).cmp(&(c * b));
        prop_assert_eq!(x.cmp(&y), oracle);
        prop_assert_eq!(x.partial_cmp(&y), Some(oracle));
        prop_assert_eq!(x == y, oracle == Ordering::Equal);
        // Antisymmetry.
        prop_assert_eq!(y.cmp(&x), oracle.reverse());
    }

    /// Comparison agrees with the sign of the difference whenever the difference fits: the two
    /// go through entirely different code (cross products vs addition and reduction).
    #[test]
    fn cmp_matches_sign_of_difference((x, y) in pair()) {
        if let Ok(diff) = x.checked_sub(y) {
            prop_assert_eq!(x.cmp(&y), diff.signum().cmp(&0));
        }
    }

    /// Transitivity of `≤` on triples, and sorting gives the same order as the oracle.
    #[test]
    fn order_is_total(mut v in prop::collection::vec(rational(), 3..12)) {
        let (x, y, z) = (v[0], v[1], v[2]);
        if x <= y && y <= z {
            prop_assert!(x <= z);
        }
        v.sort();
        for w in v.windows(2) {
            let ((a, b), (c, d)) = (wide(w[0]), wide(w[1]));
            prop_assert!(a * d <= c * b, "{:?} must not be greater than {:?}", w[0], w[1]);
        }
    }

    /// The canonical form makes equal values bit-for-bit identical: scaling numerator and
    /// denominator by the same factor gives an equal value with an equal hash.
    #[test]
    fn scaled_fractions_are_equal_with_equal_hashes(
        n in -1_000_000_i64..=1_000_000,
        d in 1_i64..=1_000_000,
        k in prop_oneof![-1_000_i64..=-1, 1_i64..=1_000],
    ) {
        let plain = Rational::new(n, d).unwrap();
        let scaled = Rational::new(n * k, d * k).unwrap();
        prop_assert_eq!(plain, scaled);
        prop_assert_eq!(hash_of(plain), hash_of(scaled));
    }

    /// Algebraic identities, whenever the operations involved succeed: commutativity, neutral
    /// elements, inverse elements, and that subtraction undoes addition.
    #[test]
    fn algebraic_identities((x, y) in pair()) {
        prop_assert_eq!(x.checked_add(y), y.checked_add(x));
        prop_assert_eq!(x.checked_mul(y), y.checked_mul(x));
        prop_assert_eq!(x.checked_add(Rational::ZERO), Ok(x));
        prop_assert_eq!(x.checked_mul(Rational::ONE), Ok(x));
        prop_assert_eq!(x.checked_sub(x), Ok(Rational::ZERO));
        if !x.is_zero() {
            prop_assert_eq!(x.checked_div(x), Ok(Rational::ONE));
        }
        if let Ok(sum) = x.checked_add(y) {
            prop_assert_eq!(sum.checked_sub(y), Ok(x));
        }
        if let Ok(product) = x.checked_mul(y) && !y.is_zero() {
            prop_assert_eq!(product.checked_div(y), Ok(x));
        }
    }
}
