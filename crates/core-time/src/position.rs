// SPDX-License-Identifier: GPL-3.0-or-later

//! The two domain types of musical time: [`Position`] and [`Duration`] (ADR-0018 §1).
//!
//! # Why two types over the same number?
//! A position ("bar 3 starts here") and a duration ("this note lasts a quarter") are both a
//! [`Rational`] number of whole notes, but they do not obey the same algebra:
//!
//! | Expression | Meaning | Here |
//! |---|---|---|
//! | position − position | how far apart they are | [`Position::checked_duration_since`] → [`Duration`] |
//! | position ± duration | a later or earlier position | [`Position::checked_add`] / [`Position::checked_sub`] → [`Position`] |
//! | duration ± duration | a longer or shorter duration | [`Duration::checked_add`] / [`Duration::checked_sub`] → [`Duration`] |
//! | duration × number | a scaled duration (a tuplet ratio, a dotted note) | [`Duration::checked_mul`] / [`Duration::checked_div`] → [`Duration`] |
//! | duration ÷ duration | how many times one fits in the other | [`Duration::checked_ratio`] → [`Rational`] |
//! | position + position | **meaningless** | does not compile |
//! | position × number | **meaningless** | does not compile |
//!
//! With plain `Rational` everywhere, "bar start + note position" (two positions added by
//! mistake) would compile and give a wrong value one bar too late; with these types it is a
//! compile error. This is the whole point of the module: it adds no arithmetic of its own, every
//! method forwards to the exact, checked arithmetic of [`Rational`] (ADR-0015).
//!
//! ```compile_fail
//! use orchestra_core_time::Position;
//! // Adding two positions is meaningless, so the method does not exist for that argument type.
//! let p = Position::ZERO.checked_add(Position::ZERO);
//! ```
//!
//! ```compile_fail
//! use orchestra_core_time::{Position, Rational};
//! // Scaling a position is meaningless too: only durations have `checked_mul`.
//! let p = Position::ZERO.checked_mul(Rational::ONE);
//! ```
//!
//! # Signs
//! Both types accept any value, negative included (ADR-0018 §1):
//! - a [`Position`] before zero is a point before the start of the score. The meter map rejects
//!   it, but performed positions may legitimately be there (a first note played early by a
//!   microtiming offset), and intermediate results of a calculation may pass through it;
//! - a [`Duration`] is **signed** (owner's choice, 2026-10-31): `earlier − later` is negative, and
//!   a microtiming offset that anticipates a note is negative. "A note lasts more than zero" is a
//!   rule of the model, checked there.
//!
//! # Errors
//! Every fallible method returns the [`RationalError`] of the underlying operation, unchanged:
//! [`RationalError::Overflow`] if the reduced result does not fit, [`RationalError::DivisionByZero`]
//! when dividing by zero. As with `Rational`, there are no `+ - * /` operators (ADR-0015 §6), and
//! every method is a `const fn`.
//!
//! # Cost
//! Both types are `#[repr(transparent)]` wrappers: same 16 bytes, same `Copy`, same cost as the
//! `Rational` operation they forward to; the wrapping is free after compilation.

use std::fmt;
use std::ops::Neg;

use crate::rational::{Rational, RationalError};

/// Unwraps a `Result<Rational, RationalError>` inside a `const fn`, returning the error early.
///
/// The `?` operator and `Result::map` are not callable in a `const fn` on stable Rust, so each
/// forwarding method uses this hand-written equivalent of `?` before re-wrapping the value.
macro_rules! const_try {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Err(error),
        }
    };
}

/// A point on the written timeline, in whole notes from the start of the score (ADR-0018 §1).
///
/// Zero is the start of bar 0, which is the pickup bar when the score has one. Positions are
/// *written* positions: repeats and jumps are unrolled elsewhere (`orchestra-playback`, ADR-0004),
/// so the same written position may be played several times.
///
/// See the module documentation for the algebra and the reasons behind it.
///
/// # Examples
/// ```
/// use orchestra_core_time::{Duration, Position, Rational, RationalError};
///
/// let quarter = Duration::from_rational(Rational::new(1, 4)?);
/// let beat_two = Position::ZERO.checked_add(quarter)?;
/// let beat_four = beat_two.checked_add(quarter)?.checked_add(quarter)?;
/// // Position − position is a duration: two quarters.
/// assert_eq!(beat_four.checked_duration_since(beat_two)?, quarter.checked_mul(Rational::new(2, 1)?)?);
/// // Backwards it is negative: durations are signed.
/// assert!(beat_two.checked_duration_since(beat_four)?.is_negative());
/// # Ok::<(), RationalError>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Position(Rational);

/// A signed distance between two positions, in whole notes (ADR-0018 §1).
///
/// Used for note lengths, bar lengths, offsets inside a bar or a beat and microtiming offsets
/// (ADR-0004). It may be negative (see the module documentation).
///
/// # Examples
/// ```
/// use orchestra_core_time::{Duration, Rational, RationalError};
///
/// let quarter = Duration::from_rational(Rational::new(1, 4)?);
/// // A dotted quarter is a quarter times 3/2.
/// let dotted = quarter.checked_mul(Rational::new(3, 2)?)?;
/// // A bar of 6/8 holds exactly two dotted quarters.
/// let bar = Duration::from_rational(Rational::new(6, 8)?);
/// assert_eq!(bar.checked_ratio(dotted)?, Rational::new(2, 1)?);
/// # Ok::<(), RationalError>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Duration(Rational);

impl Position {
    /// The start of the score: the start of bar 0.
    pub const ZERO: Self = Self(Rational::ZERO);

    /// Wraps a number of whole notes from the start of the score.
    ///
    /// This is the explicit way in from plain numbers: the conversions of the tempo map (F1.05)
    /// and the file format (F1.15) use it. Its presence in a diff signals that a value enters the
    /// type safety from outside. O(1).
    #[must_use]
    pub const fn from_rational(value: Rational) -> Self {
        Self(value)
    }

    /// The number of whole notes from the start of the score: the explicit way out. O(1).
    #[must_use]
    pub const fn as_rational(self) -> Rational {
        self.0
    }

    /// The position `self + duration`, later if the duration is positive, earlier if negative.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit (ADR-0015 §4).
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_add`].
    pub const fn checked_add(self, duration: Duration) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_add(duration.0))))
    }

    /// The position `self − duration`, earlier if the duration is positive.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_sub`].
    pub const fn checked_sub(self, duration: Duration) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_sub(duration.0))))
    }

    /// The duration from `earlier` to `self`: `self − earlier`.
    ///
    /// The name follows `std::time::Instant::duration_since`, and so does the order of the
    /// operands: the receiver is the later position. Unlike `Instant`, the result is simply
    /// negative when `earlier` is in fact later, because durations are signed.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_sub`].
    pub const fn checked_duration_since(self, earlier: Self) -> Result<Duration, RationalError> {
        Ok(Duration(const_try!(self.0.checked_sub(earlier.0))))
    }
}

impl Duration {
    /// No time at all.
    pub const ZERO: Self = Self(Rational::ZERO);

    /// Wraps a number of whole notes: `1/4` is a quarter note. The explicit way in. O(1).
    #[must_use]
    pub const fn from_rational(value: Rational) -> Self {
        Self(value)
    }

    /// The number of whole notes: the explicit way out. O(1).
    #[must_use]
    pub const fn as_rational(self) -> Rational {
        self.0
    }

    /// The sum of two durations.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_add`].
    pub const fn checked_add(self, rhs: Self) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_add(rhs.0))))
    }

    /// The difference of two durations (negative if `rhs` is longer).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_sub`].
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_sub(rhs.0))))
    }

    /// The duration scaled by a pure number: `factor` copies of `self`.
    ///
    /// A dotted note is `checked_mul(3/2)`, a note inside a 3:2 triplet `checked_mul(2/3)`, the
    /// length of `n` bars `checked_mul(n)`.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_mul`].
    pub const fn checked_mul(self, factor: Rational) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_mul(factor))))
    }

    /// The duration divided by a pure number: one `divisor`-th of `self`.
    ///
    /// # Errors
    /// - [`RationalError::DivisionByZero`] if `divisor` is zero;
    /// - [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_div`].
    pub const fn checked_div(self, divisor: Rational) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_div(divisor))))
    }

    /// How many times `unit` fits in `self`, as an exact pure number: `self / unit`.
    ///
    /// The units cancel out, which is why the result is a [`Rational`] and not a duration. Taking
    /// the [`floor`](Rational::floor) of the result gives the number of *whole* units, e.g. the
    /// bar that contains an offset from the start of a run of equal bars (ADR-0018 §4).
    ///
    /// # Errors
    /// - [`RationalError::DivisionByZero`] if `unit` is zero;
    /// - [`RationalError::Overflow`] if the reduced result does not fit.
    ///
    /// # Complexity
    /// O(log) like [`Rational::checked_div`].
    pub const fn checked_ratio(self, unit: Self) -> Result<Rational, RationalError> {
        self.0.checked_div(unit.0)
    }

    /// The length without its sign. Infallible (ADR-0015 invariant 3). O(1).
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// The sign: `-1` if negative, `0` if zero, `1` if positive. O(1).
    #[must_use]
    pub const fn signum(self) -> i64 {
        self.0.signum()
    }

    /// `true` for a zero duration. O(1).
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    /// `true` for a duration strictly greater than zero (forwards in time). O(1).
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0.is_positive()
    }

    /// `true` for a duration strictly less than zero (backwards in time). O(1).
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.0.is_negative()
    }
}

impl Neg for Duration {
    type Output = Self;

    /// The same distance in the opposite direction. Infallible, like [`Rational`]'s `Neg`. O(1).
    fn neg(self) -> Self {
        // `negated` is the `const` body of `Rational`'s `Neg`: it cannot wrap (ADR-0015
        // invariant 3). It is called directly because clippy's `arithmetic_side_effects` rejects
        // the `-` operator here, unable to see that it is the infallible rational negation.
        Self(self.0.negated())
    }
}

impl fmt::Debug for Position {
    /// Writes `Position(3/8)`, or `Position(2)` for integers.
    ///
    /// Only `Debug`, like `Rational`: the text form for files and the interface comes in F1.15.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_parts(f, "Position", self.0)
    }
}

impl fmt::Debug for Duration {
    /// Writes `Duration(-1/4)`, or `Duration(1)` for integers. Only `Debug` (see [`Position`]).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_parts(f, "Duration", self.0)
    }
}

/// Writes `Name(num/den)`, or `Name(num)` when the value is an integer.
///
/// Shared by the two `Debug` implementations. It does not reuse `Rational`'s `Debug`, which would
/// give the noisier `Position(Rational(3/8))`.
fn write_parts(f: &mut fmt::Formatter<'_>, name: &str, value: Rational) -> fmt::Result {
    if value.is_integer() {
        write!(f, "{name}({})", value.numerator())
    } else {
        write!(f, "{name}({}/{})", value.numerator(), value.denominator())
    }
}
