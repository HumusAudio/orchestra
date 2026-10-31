// SPDX-License-Identifier: GPL-3.0-or-later

//! The exact rational number used for every musical position, duration and offset (ADR-0015).
//!
//! # What lives here
//! - [`Rational`]: a fraction `num/den` of two `i64`, always in lowest terms, `Copy`, 16 bytes.
//! - [`RationalError`]: the typed error returned when a result does not fit, or when something
//!   is divided by zero.
//!
//! # The three invariants
//! Every constructor and every operation establishes them, and every method may assume them:
//! 1. `den ≥ 1`: the sign lives in the numerator only;
//! 2. `gcd(|num|, den) = 1`: lowest terms, so zero is exactly `0/1`;
//! 3. `num ≠ i64::MIN`: hence `|num| ≤ i64::MAX`, so negation and absolute value never overflow.
//!
//! Because of 1 and 2 every value has exactly one representation, which is why `PartialEq`, `Eq`
//! and `Hash` can simply be derived field by field (ADR-0015 §2).
//!
//! # How the arithmetic stays exact without allocating
//! Every operation widens its operands to `i128`, computes the exact result there, reduces it to
//! lowest terms and only then checks that it fits back into `i64` (ADR-0015 §4). The products of
//! two `i64` magnitudes are below 2¹²⁶, and the sums of two such products below 2¹²⁷, so the
//! `i128` intermediates of addition, multiplication and comparison can never overflow. The result
//! is an error **only** when the *reduced* result does not satisfy the invariants: `1/2 · 2/1`
//! never fails, whatever the size of the numbers involved.
//!
//! # Why methods instead of operators?
//! `std::ops::{Add, Sub, Mul, Div}` could only panic, wrap or saturate on overflow, and all three
//! are forbidden for musical time (ADR-0015 §6): a panic crashes the program, a wrapped or
//! saturated position is silently wrong. So the fallible operations are `checked_*` methods that
//! return [`RationalError`]. Only negation, which can never fail thanks to invariant 3, is an
//! operator ([`Neg`]).
//!
//! # Why `const fn`?
//! Constructors and arithmetic are `const fn`, so that fixed values (a quarter note, a triplet
//! ratio) can be computed **at compile time** and checked there: see the example on
//! [`Rational::new`]. The price is a little verbosity: the `?` operator and trait methods such as
//! `TryFrom` are not usable in `const fn` yet, so the code uses explicit `match` (through the two
//! small macros below) and a few documented `as` casts.
//!
//! # Lints
//! The crate denies `clippy::arithmetic_side_effects`: plain `+ - * / % << >>` on integers do not
//! compile here. Every integer operation is a `checked_*` method (whose failure becomes
//! [`RationalError::Overflow`]) or a `wrapping_*` method whose comment proves that it cannot wrap.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::ops::Neg;

/// Unwraps a `Result` inside a `const fn`, returning the error early.
///
/// The `?` operator relies on the `Try` trait, which cannot be called in a `const fn` on stable
/// Rust; this macro is the hand-written equivalent of `?` for our `Result<_, RationalError>`.
macro_rules! const_try {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Err(error),
        }
    };
}

/// Unwraps an `Option` coming from a `checked_*` integer operation inside a `const fn`, turning
/// `None` into an early [`RationalError::Overflow`].
///
/// Some of the operations wrapped by this macro are proven not to overflow (see the comments at
/// each use); they are still checked rather than `wrapping_*` when the check is free compared with
/// the work around it, so that a mistake in the proof becomes an error, never a wrong value.
macro_rules! or_overflow {
    ($option:expr) => {
        match $option {
            Some(value) => value,
            None => return Err(RationalError::Overflow),
        }
    };
}

/// The largest magnitude allowed for a numerator or a denominator: `i64::MAX` = 2⁶³ − 1, as a
/// `u128` so that it can be compared with the magnitudes of the `i128` intermediates.
///
/// Written as a literal because the obvious `i64::MAX as u128` would trip the `cast_sign_loss`
/// lint and `u128::from` is not callable in a `const` item. A unit test checks that the literal
/// is right.
const MAX_MAGNITUDE: u128 = 0x7FFF_FFFF_FFFF_FFFF;

/// Why a [`Rational`] operation could not produce a value.
///
/// The `Display` texts are English and meant for developers (logs, test failures). They are never
/// shown to the user as they are: the interface maps each variant to a Fluent key, like every
/// other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RationalError {
    /// The exact result, already reduced to lowest terms, does not fit the representation: its
    /// numerator or its denominator is larger than `i64::MAX` in magnitude, or its numerator would
    /// be exactly `i64::MIN` (excluded by invariant 3). With the headroom computed in ADR-0015
    /// (about eight orders of magnitude for real scores) this means a corrupt or absurd input; the
    /// command that caused it fails and is not applied (ADR-0003).
    Overflow,
    /// A zero denominator was given to [`Rational::new`], or something was divided by zero
    /// ([`Rational::checked_div`], [`Rational::checked_recip`]).
    DivisionByZero,
}

impl fmt::Display for RationalError {
    /// Writes a short English description of the error, for developers (see the type's docs).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow => {
                f.write_str("rational number overflow: the reduced result does not fit in 64 bits")
            }
            Self::DivisionByZero => {
                f.write_str("rational number with a zero denominator (division by zero)")
            }
        }
    }
}

impl Error for RationalError {}

/// An exact rational number `num/den`: the type of musical time (ADR-0005, ADR-0015).
///
/// Positions and durations are measured in **whole notes**: a quarter note is `1/4`, an
/// eighth-note triplet `1/12`, bar 3 of a 4/4 piece starts at `2/1`. Microtiming offsets
/// (ADR-0004) use the same unit and may be negative.
///
/// See the module documentation for the invariants, the overflow policy and why there are no
/// arithmetic operators.
///
/// # Cost
/// 16 bytes, `Copy`, never allocates. Arithmetic is O(log n) in the magnitude of the operands
/// (the greatest common divisor), comparison is O(1).
///
/// # Examples
/// ```
/// use orchestra_core_time::{Rational, RationalError};
///
/// let quarter = Rational::new(1, 4)?;
/// let triplet_eighth = Rational::new(1, 12)?;
/// // Three triplet eighths make exactly one quarter note: no rounding, ever.
/// let three = triplet_eighth.checked_add(triplet_eighth)?.checked_add(triplet_eighth)?;
/// assert_eq!(three, quarter);
/// assert_eq!(Rational::new(1, 0), Err(RationalError::DivisionByZero));
/// # Ok::<(), RationalError>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rational {
    /// The numerator: carries the sign; never `i64::MIN` (invariant 3).
    num: i64,
    /// The denominator: always `≥ 1` (invariant 1) and coprime with `num` (invariant 2).
    den: i64,
}

/// Greatest common divisor of two `u64`, with the binary (Stein) algorithm.
///
/// # Why Stein and not Euclid?
/// Euclid needs one division per step, and integer division is among the slowest instructions
/// (tens of cycles on the reference hardware, worse for 128 bits). Stein only uses shifts,
/// subtractions and `trailing_zeros` (a single instruction), so it is usually faster for
/// machine-word integers (ADR-0015 §9). It also has no division-by-zero path to worry about.
///
/// # Contract
/// `gcd(0, b) = b`, `gcd(a, 0) = a`, `gcd(0, 0) = 0`. The result is at most `max(a, b)`.
///
/// # Complexity
/// O(log a + log b): each iteration removes at least one bit from the larger operand.
const fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    // The common power of two: `trailing_zeros` of the OR is the number of factors 2 shared by
    // both. It is below 64 because `a | b ≠ 0`.
    let shift = (a | b).trailing_zeros();
    // Make `a` odd. `trailing_zeros(a) < 64` because `a ≠ 0`, so the shift is exact and
    // `wrapping_shr` never masks its amount.
    a = a.wrapping_shr(a.trailing_zeros());
    loop {
        // Make `b` odd as well (same reasoning: `b ≠ 0` at the top of every iteration).
        b = b.wrapping_shr(b.trailing_zeros());
        // Keep `a ≤ b`, so that the subtraction below cannot underflow.
        if a > b {
            let t = a;
            a = b;
            b = t;
        }
        // `b ≥ a`, so this never wraps. Both are odd, so the difference is even (or zero), and
        // `gcd(a, b) = gcd(a, b − a)`.
        b = b.wrapping_sub(a);
        if b == 0 {
            break;
        }
    }
    // Put back the common factors 2. The result is ≤ the original operands, so it fits and the
    // shift cannot lose bits.
    a.wrapping_shl(shift)
}

/// Greatest common divisor of two `u128`: the same Stein algorithm as [`gcd_u64`], on 128 bits.
///
/// When both operands fit in 64 bits (the overwhelmingly common case: musical denominators are
/// small) it delegates to [`gcd_u64`], because 64-bit operations are cheaper than their 128-bit
/// emulation on two registers. This is the "64-bit fast path" foreseen by ADR-0015 §9.
///
/// # Complexity
/// O(log a + log b).
const fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    // Fast path: both operands fit in a machine word. The casts are exact because of the check,
    // and `u128::from`/`u64::try_from` are not callable in a `const fn`.
    if a <= u64::MAX as u128 && b <= u64::MAX as u128 {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "both values were just checked to fit in u64"
        )]
        return gcd_u64(a as u64, b as u64) as u128;
    }
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    // From here on the reasoning is identical to `gcd_u64`, with 128 instead of 64.
    let shift = (a | b).trailing_zeros();
    a = a.wrapping_shr(a.trailing_zeros());
    loop {
        b = b.wrapping_shr(b.trailing_zeros());
        if a > b {
            let t = a;
            a = b;
            b = t;
        }
        b = b.wrapping_sub(a);
        if b == 0 {
            break;
        }
    }
    a.wrapping_shl(shift)
}

/// Turns a magnitude into an `i64`, if it is at most `i64::MAX`.
///
/// This is the single place where the `i128`/`u128` world narrows back to `i64`. It exists
/// because `i64::try_from` is not callable in a `const fn`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "the value was just checked to be at most i64::MAX, so the cast is exact"
)]
const fn narrow(magnitude: u128) -> Option<i64> {
    if magnitude <= MAX_MAGNITUDE {
        Some(magnitude as i64)
    } else {
        None
    }
}

/// Widens an `i64` to `i128` (always exact).
///
/// A `const fn` stand-in for `i128::from`, which is not callable in a `const fn`. Clippy's
/// `cast_lossless` would rather see `From`, hence the local exception.
#[allow(
    clippy::cast_lossless,
    reason = "i128::from is not const; the widening is exact"
)]
const fn wide(value: i64) -> i128 {
    value as i128
}

impl Rational {
    /// Zero, `0/1`: the start of a piece, an empty duration, "no offset".
    pub const ZERO: Self = Self { num: 0, den: 1 };

    /// One, `1/1`: a whole note.
    pub const ONE: Self = Self { num: 1, den: 1 };

    /// Builds `num/den`, reduced to lowest terms with the sign moved to the numerator.
    ///
    /// Any pair of `i64` is accepted, `i64::MIN` included, as long as the **reduced** value fits
    /// the invariants: `new(i64::MIN, 2)` is `-2⁶²/1`, while `new(i64::MIN, 1)` overflows
    /// (its numerator would be `i64::MIN`) and so does `new(1, i64::MIN)` (its denominator would
    /// be 2⁶³).
    ///
    /// # Errors
    /// - [`RationalError::DivisionByZero`] if `den == 0`;
    /// - [`RationalError::Overflow`] if the reduced value does not fit (see above).
    ///
    /// # Complexity
    /// O(log max(|num|, |den|)) for the reduction.
    ///
    /// # Examples
    /// ```
    /// use orchestra_core_time::Rational;
    ///
    /// let r = Rational::new(6, -8)?;
    /// assert_eq!((r.numerator(), r.denominator()), (-3, 4));
    ///
    /// // `new` is a `const fn`: a fixed value can be checked at compile time. A typo such as a
    /// // zero denominator then stops the build instead of failing at run time.
    /// const QUARTER: Rational = match Rational::new(1, 4) {
    ///     Ok(r) => r,
    ///     Err(_) => panic!("1/4 is a valid rational"),
    /// };
    /// assert_eq!(QUARTER.denominator(), 4);
    /// # Ok::<(), orchestra_core_time::RationalError>(())
    /// ```
    pub const fn new(num: i64, den: i64) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::DivisionByZero);
        }
        // Widening to `i128` lets `from_wide` handle `i64::MIN` (whose magnitude 2⁶³ does not fit
        // in `i64`) exactly like every other value: reduce first, check the fit afterwards.
        Self::from_wide(wide(num), wide(den))
    }

    /// Builds the integer `n/1`.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if `n == i64::MIN` (invariant 3).
    ///
    /// # Complexity
    /// O(1).
    pub const fn from_integer(n: i64) -> Result<Self, RationalError> {
        if n == i64::MIN {
            Err(RationalError::Overflow)
        } else {
            Ok(Self { num: n, den: 1 })
        }
    }

    /// The numerator, in lowest terms: carries the sign, never `i64::MIN`.
    #[must_use]
    pub const fn numerator(self) -> i64 {
        self.num
    }

    /// The denominator, in lowest terms: always `≥ 1`.
    #[must_use]
    pub const fn denominator(self) -> i64 {
        self.den
    }

    /// Reduces an exact `i128` fraction and narrows it to a `Rational`.
    ///
    /// This is the common tail of [`new`](Self::new) and of the arithmetic: it moves the sign to
    /// the numerator, divides both parts by their greatest common divisor and checks that the
    /// result satisfies the invariants.
    ///
    /// # Preconditions
    /// `den ≠ 0` (the callers check it, or it holds by construction).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced magnitudes exceed `i64::MAX`. That single check
    /// also excludes a numerator of `i64::MIN`, whose magnitude is 2⁶³ > `i64::MAX`.
    /// [`RationalError::DivisionByZero`] if the precondition is violated (defensive: never
    /// reached by the callers in this file).
    ///
    /// # Complexity
    /// O(log max(|num|, |den|)).
    const fn from_wide(num: i128, den: i128) -> Result<Self, RationalError> {
        if den == 0 {
            return Err(RationalError::DivisionByZero);
        }
        // Work on magnitudes and a sign flag: `unsigned_abs` is exact even for `i128::MIN`, so
        // no case needs special handling here.
        let negative = (num < 0) != (den < 0);
        let n = num.unsigned_abs();
        let d = den.unsigned_abs();
        // `d ≠ 0`, so `g ≥ 1` and the divisions below cannot fail; `checked_div` keeps that
        // reasoning out of the lint's way at no measurable cost.
        let g = gcd_u128(n, d);
        let n = or_overflow!(narrow(or_overflow!(n.checked_div(g))));
        let d = or_overflow!(narrow(or_overflow!(d.checked_div(g))));
        // `n ≤ i64::MAX`, so `-n ≥ -i64::MAX > i64::MIN`: the negation cannot wrap, and the
        // numerator can never be `i64::MIN` (invariant 3). Zero always gets the positive sign,
        // because `0.wrapping_neg() == 0`; and `gcd(0, d) = d`, so its denominator is 1.
        Ok(Self {
            num: if negative { n.wrapping_neg() } else { n },
            den: d,
        })
    }

    /// Narrows an `i128` fraction that is **already** in lowest terms with `den > 0`.
    ///
    /// Used by multiplication, which reduces before multiplying and therefore does not need the
    /// gcd of [`from_wide`](Self::from_wide) again.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if either part does not fit the invariants.
    ///
    /// # Complexity
    /// O(1).
    const fn from_wide_reduced(num: i128, den: i128) -> Result<Self, RationalError> {
        let n = or_overflow!(narrow(num.unsigned_abs()));
        let d = or_overflow!(narrow(den.unsigned_abs()));
        Ok(Self {
            // Same argument as in `from_wide`: `n ≤ i64::MAX`, so the negation cannot wrap.
            num: if num < 0 { n.wrapping_neg() } else { n },
            den: d,
        })
    }

    /// Exact sum `self + rhs`.
    ///
    /// # Algorithm (ADR-0015 §4)
    /// With `self = a/b`, `rhs = c/d` and `g = gcd(b, d)`:
    /// `a/b + c/d = (a·(d/g) + c·(b/g)) / ((b/g)·d)`, then reduced. Dividing by `g` first keeps
    /// the intermediates as small as possible (the denominator is the least common multiple of
    /// `b` and `d`). When `b == d` the formula collapses to `(a + c)/b`, which skips the first gcd.
    ///
    /// # Why it cannot overflow internally
    /// `|a·(d/g)| ≤ (2⁶³−1)²` < 2¹²⁶, and the same for the other product, so the `i128` sum is
    /// below 2¹²⁷: it fits. The `checked_*` calls are still used (see [`or_overflow!`]).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] only if the reduced sum does not fit.
    ///
    /// # Complexity
    /// O(log) for one or two gcds.
    pub const fn checked_add(self, rhs: Self) -> Result<Self, RationalError> {
        // Fast path: equal denominators (very common: notes in the same tuplet, integer bars).
        if self.den == rhs.den {
            let num = or_overflow!(wide(self.num).checked_add(wide(rhs.num)));
            return Self::from_wide(num, wide(self.den));
        }
        // Both denominators are ≥ 1 and below 2⁶³, so they fit in `u64` exactly. `unsigned_abs`
        // is the const-friendly way to get that `u64`.
        let g = gcd_u64(self.den.unsigned_abs(), rhs.den.unsigned_abs());
        // `g ≥ 1` (both operands are non-zero) and `g ≤ min(b, d) ≤ i64::MAX`.
        let g = wide(or_overflow!(narrow(g as u128)));
        let b_over_g = or_overflow!(wide(self.den).checked_div(g));
        let d_over_g = or_overflow!(wide(rhs.den).checked_div(g));
        let left = or_overflow!(wide(self.num).checked_mul(d_over_g));
        let right = or_overflow!(wide(rhs.num).checked_mul(b_over_g));
        let num = or_overflow!(left.checked_add(right));
        let den = or_overflow!(b_over_g.checked_mul(wide(rhs.den)));
        Self::from_wide(num, den)
    }

    /// Exact difference `self − rhs`.
    ///
    /// Computed as `self + (−rhs)`: negation is exact and infallible (invariant 3), so this has
    /// the same guarantees and cost as [`checked_add`](Self::checked_add).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] only if the reduced difference does not fit. Note that
    /// `-i64::MAX − 1` is an overflow, because its numerator would be `i64::MIN`.
    ///
    /// # Complexity
    /// O(log), like the sum.
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, RationalError> {
        self.checked_add(rhs.negated())
    }

    /// Exact product `self · rhs`.
    ///
    /// # Algorithm (ADR-0015 §4)
    /// With `self = a/b` and `rhs = c/d`, first cross-reduce: `g1 = gcd(|a|, d)`,
    /// `g2 = gcd(|c|, b)`; then `(a/g1 · c/g2) / (b/g2 · d/g1)`. Since `a/b` and `c/d` are already
    /// in lowest terms, the cross-reduced product is in lowest terms too, so no final gcd is
    /// needed. This is why `(huge/3) · (3/huge)` gives `1/1` without any large intermediate.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] only if the reduced product does not fit.
    ///
    /// # Complexity
    /// O(log) for two gcds.
    pub const fn checked_mul(self, rhs: Self) -> Result<Self, RationalError> {
        // Zero must be handled apart: with `a = 0`, `g1 = gcd(0, d) = d` and the formula would
        // give `0/(b/g2)`, which is not the canonical `0/1`.
        if self.num == 0 || rhs.num == 0 {
            return Ok(Self::ZERO);
        }
        // Magnitudes fit in `u64` (invariant 3 for numerators, invariant 1 for denominators).
        let g1 = gcd_u64(self.num.unsigned_abs(), rhs.den.unsigned_abs());
        let g2 = gcd_u64(rhs.num.unsigned_abs(), self.den.unsigned_abs());
        // Each gcd is ≥ 1 (its operands are non-zero) and ≤ i64::MAX (it divides an `i64`
        // magnitude ≤ i64::MAX).
        let g1 = wide(or_overflow!(narrow(g1 as u128)));
        let g2 = wide(or_overflow!(narrow(g2 as u128)));
        let a = or_overflow!(wide(self.num).checked_div(g1));
        let d = or_overflow!(wide(rhs.den).checked_div(g1));
        let c = or_overflow!(wide(rhs.num).checked_div(g2));
        let b = or_overflow!(wide(self.den).checked_div(g2));
        // Each product of two magnitudes ≤ i64::MAX is < 2¹²⁶: it fits in `i128`.
        let num = or_overflow!(a.checked_mul(c));
        let den = or_overflow!(b.checked_mul(d));
        Self::from_wide_reduced(num, den)
    }

    /// Exact reciprocal `1 / self`.
    ///
    /// Swapping numerator and denominator keeps lowest terms, and both magnitudes are already at
    /// most `i64::MAX`, so the only possible failure is zero.
    ///
    /// # Errors
    /// [`RationalError::DivisionByZero`] if `self` is zero.
    ///
    /// # Complexity
    /// O(1).
    pub const fn checked_recip(self) -> Result<Self, RationalError> {
        if self.num == 0 {
            return Err(RationalError::DivisionByZero);
        }
        // The new denominator is |num| (≤ i64::MAX by invariant 3, ≥ 1 because num ≠ 0); the
        // sign moves to the new numerator, the old denominator (≥ 1, so negating it cannot wrap).
        Ok(Self {
            num: if self.num < 0 {
                self.den.wrapping_neg()
            } else {
                self.den
            },
            den: self.num.wrapping_abs(),
        })
    }

    /// Exact quotient `self / rhs`: the product with the reciprocal of `rhs`.
    ///
    /// # Errors
    /// - [`RationalError::DivisionByZero`] if `rhs` is zero;
    /// - [`RationalError::Overflow`] if the reduced quotient does not fit.
    ///
    /// # Complexity
    /// O(log), like the product.
    pub const fn checked_div(self, rhs: Self) -> Result<Self, RationalError> {
        self.checked_mul(const_try!(rhs.checked_recip()))
    }

    /// `-self`: the `const` implementation behind [`Neg`].
    ///
    /// Cannot wrap because `num ≠ i64::MIN` (invariant 3); the result keeps all invariants (`-0`
    /// is `0`, and the gcd does not depend on the sign).
    const fn negated(self) -> Self {
        Self {
            num: self.num.wrapping_neg(),
            den: self.den,
        }
    }

    /// The absolute value `|self|`. Infallible thanks to invariant 3. O(1).
    #[must_use]
    pub const fn abs(self) -> Self {
        Self {
            // `wrapping_abs` only wraps for `i64::MIN`, which invariant 3 excludes.
            num: self.num.wrapping_abs(),
            den: self.den,
        }
    }

    /// The sign: `-1` if negative, `0` if zero, `1` if positive. O(1).
    #[must_use]
    pub const fn signum(self) -> i64 {
        // The denominator is positive, so the sign of the value is the sign of the numerator.
        self.num.signum()
    }

    /// `true` if the value is zero. O(1).
    #[must_use]
    pub const fn is_zero(self) -> bool {
        // Canonical form: zero is exactly `0/1`, so the numerator alone decides.
        self.num == 0
    }

    /// `true` if the value is strictly greater than zero. O(1).
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.num > 0
    }

    /// `true` if the value is strictly less than zero. O(1).
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.num < 0
    }

    /// `true` if the value is an integer (a whole number of whole notes). O(1).
    #[must_use]
    pub const fn is_integer(self) -> bool {
        // In lowest terms, `num/den` is an integer exactly when `den == 1`.
        self.den == 1
    }

    /// The largest integer `≤ self` (rounds towards −∞). O(1).
    ///
    /// Example: `floor(7/2) = 3`, `floor(-7/2) = -4`. Used to find, say, the bar that contains a
    /// position; never fails, because `|floor(x)| ≤ |num| ≤ i64::MAX`.
    #[must_use]
    pub const fn floor(self) -> i64 {
        // With a positive divisor, Euclidean division rounds towards −∞, which is exactly the
        // floor. `wrapping_div_euclid` only wraps for `i64::MIN / -1` (impossible: `den ≥ 1`) and
        // panics only for a zero divisor (impossible: `den ≥ 1`).
        self.num.wrapping_div_euclid(self.den)
    }

    /// The smallest integer `≥ self` (rounds towards +∞). O(1).
    ///
    /// Example: `ceil(7/2) = 4`, `ceil(-7/2) = -3`.
    #[must_use]
    pub const fn ceil(self) -> i64 {
        let floor = self.floor();
        if self.den == 1 {
            // An integer is its own ceiling.
            floor
        } else {
            // Not an integer, so the ceiling is the floor plus one. It cannot wrap: with
            // `den ≥ 2`, `floor ≤ num / 2 < i64::MAX`.
            floor.wrapping_add(1)
        }
    }

    /// The integer part, rounding towards zero. O(1).
    ///
    /// Example: `trunc(7/2) = 3`, `trunc(-7/2) = -3`.
    #[must_use]
    pub const fn trunc(self) -> i64 {
        // Truncating is rounding towards zero: the floor for non-negative values, the ceiling for
        // negative ones. Reusing them avoids a second division (and `wrapping_div`, which clippy's
        // `arithmetic_side_effects` rejects because it may panic on a zero divisor).
        if self.num < 0 {
            self.ceil()
        } else {
            self.floor()
        }
    }

    /// An **approximate** `f64` value, for display and drawing only (ADR-0015 §8).
    ///
    /// Each part is rounded to the nearest `f64` and then divided, so the result may differ from
    /// the exact value in the last bits; for musical values (small denominators) it is the
    /// nearest `f64` in practice. Nothing converted with this method may ever flow back into the
    /// model: conversions to seconds, samples and frames have their own exact functions (F1.05).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "approximation for display only, documented above; never fed back into the model"
    )]
    pub fn to_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

impl Neg for Rational {
    type Output = Self;

    /// `-self`. Infallible: invariant 3 guarantees that the numerator can be negated. O(1).
    fn neg(self) -> Self {
        self.negated()
    }
}

impl Ord for Rational {
    /// Exact comparison (ADR-0015 §5). O(1).
    ///
    /// With equal denominators the numerators decide. Otherwise `a/b` vs `c/d` (with `b, d > 0`)
    /// is the same as `a·d` vs `c·b`, computed in `i128`: each product is below 2¹²⁶ in magnitude,
    /// so it cannot overflow, and the comparison is never approximate and never fails.
    fn cmp(&self, other: &Self) -> Ordering {
        if self.den == other.den {
            return self.num.cmp(&other.num);
        }
        // `wrapping_mul` cannot wrap here: see the bound above.
        let left = wide(self.num).wrapping_mul(wide(other.den));
        let right = wide(other.num).wrapping_mul(wide(self.den));
        left.cmp(&right)
    }
}

impl PartialOrd for Rational {
    /// Always `Some`: rational numbers are totally ordered. Delegates to [`Ord::cmp`].
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for Rational {
    /// Writes `Rational(3/8)`, or `Rational(2)` for integers.
    ///
    /// Deliberately only `Debug`: the canonical text form for files and the interface is decided
    /// in F1.15 (ADR-0015 §10), so there is no `Display` and no `FromStr` yet.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "Rational({})", self.num)
        } else {
            write!(f, "Rational({}/{})", self.num, self.den)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    reason = "test inputs are small hand-written constants; an overflow would fail the test loudly"
)]
mod tests {
    //! Unit tests of the private helpers. The public behaviour is tested in `tests/`, against an
    //! independent oracle.

    use super::{MAX_MAGNITUDE, gcd_u64, gcd_u128, narrow};

    /// The literal `MAX_MAGNITUDE` is really `i64::MAX`.
    #[test]
    fn max_magnitude_is_i64_max() {
        assert_eq!(MAX_MAGNITUDE, u128::try_from(i64::MAX).unwrap());
    }

    /// Stein's algorithm on hand-checked cases, including zeros, powers of two and the extremes.
    #[test]
    fn gcd_known_values() {
        assert_eq!(gcd_u64(0, 0), 0);
        assert_eq!(gcd_u64(0, 7), 7);
        assert_eq!(gcd_u64(7, 0), 7);
        assert_eq!(gcd_u64(12, 18), 6);
        assert_eq!(gcd_u64(1024, 96), 32);
        assert_eq!(gcd_u64(17, 13), 1);
        assert_eq!(gcd_u64(u64::MAX, u64::MAX), u64::MAX);
        // 2⁶³ and 2⁶² share 2⁶².
        assert_eq!(gcd_u64(1 << 63, 1 << 62), 1 << 62);
        assert_eq!(gcd_u128(1 << 127, 1 << 100), 1 << 100);
        assert_eq!(gcd_u128(u128::MAX, 3), 3);
        assert_eq!(gcd_u128(0, 1 << 90), 1 << 90);
    }

    /// Stein agrees with Euclid (written here with `%`, a different algorithm) on many values,
    /// both in the 64-bit fast path and in the 128-bit loop.
    #[test]
    fn gcd_agrees_with_euclid() {
        /// Euclid's algorithm, the reference.
        fn euclid(mut a: u128, mut b: u128) -> u128 {
            while b != 0 {
                let t = a.checked_rem(b).unwrap();
                a = b;
                b = t;
            }
            a
        }
        // A deterministic spread of values: small numbers, multiples of a big common factor
        // (so that the gcd is large), and values above 2⁶⁴ (so that the 128-bit loop runs).
        let factor: u128 = 1_000_000_007;
        let mut cases = Vec::new();
        for i in 0..40_u128 {
            for j in 0..40_u128 {
                cases.push((i, j));
                cases.push((
                    i.checked_mul(factor).unwrap(),
                    j.checked_mul(factor).unwrap(),
                ));
                let big = 1_u128 << 70;
                cases.push((big.checked_mul(i + 1).unwrap(), big.checked_add(j).unwrap()));
            }
        }
        for (a, b) in cases {
            assert_eq!(gcd_u128(a, b), euclid(a, b), "gcd({a}, {b})");
        }
    }

    /// `narrow` accepts exactly the range `0..=i64::MAX`.
    #[test]
    fn narrow_boundaries() {
        assert_eq!(narrow(0), Some(0));
        assert_eq!(narrow(MAX_MAGNITUDE), Some(i64::MAX));
        assert_eq!(narrow(MAX_MAGNITUDE + 1), None);
        assert_eq!(narrow(u128::MAX), None);
    }
}
