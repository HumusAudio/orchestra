// SPDX-License-Identifier: GPL-3.0-or-later

//! Tempo values: a beat unit and a rational number of beats per minute (ADR-0020 §2).
//!
//! # Units and speed
//! A tempo is written as "♩ = 120" or "♩. = 80": a **beat unit** (a [`Duration`] in whole notes,
//! 1/4 or 3/8) and a number of **beats per minute**, a positive [`Rational`] so that 92.5 is
//! `185/2` exactly. What actually moves the music is its **speed**, in whole notes per second:
//!
//! `speed = unit × bpm / 60`
//!
//! ♩ = 120 and ♩. = 80 have the same speed (1/2 whole note per second) although they are written
//! differently. Gradual tempo changes are linear in speed (ADR-0020 §1), so a change between two
//! tempi written with different units is well defined. Equality of `Tempo` is **structural** (the
//! written form); compare speeds to know whether two tempi sound the same.
//!
//! This module holds the conversions at a **constant** tempo, which are exact in both directions;
//! gradual changes are handled by the tempo map (F1.05b).

use std::error::Error;
use std::fmt;

use crate::clock::Seconds;
use crate::position::Duration;
use crate::rational::{Rational, RationalError};

/// Why a [`Tempo`] could not be built.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to
/// a Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TempoError {
    /// The beat unit was zero or negative.
    NonPositiveUnit,
    /// The number of beats per minute was zero or negative.
    NonPositiveBpm,
}

impl fmt::Display for TempoError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonPositiveUnit => "the beat unit of a tempo must be positive",
            Self::NonPositiveBpm => "the beats per minute of a tempo must be positive",
        })
    }
}

impl Error for TempoError {}

/// A tempo: a beat unit and a number of beats per minute (ADR-0020 §2).
///
/// # Examples
/// ```
/// use orchestra_core_time::{Duration, Rational, Seconds, Tempo};
///
/// let quarter = Duration::from_rational(Rational::new(1, 4)?);
/// let dotted_quarter = Duration::from_rational(Rational::new(3, 8)?);
/// let a = Tempo::new(quarter, Rational::from_integer(120)?)?;
/// let b = Tempo::new(dotted_quarter, Rational::from_integer(80)?)?;
/// // Written differently, same speed: half a whole note per second.
/// assert_ne!(a, b);
/// assert_eq!(a.speed()?, b.speed()?);
/// assert_eq!(a.speed()?, Rational::new(1, 2)?);
/// // A 4/4 bar lasts exactly 2 seconds at ♩ = 120.
/// let bar = Duration::from_rational(Rational::ONE);
/// assert_eq!(a.seconds_for(bar)?, Seconds::from_rational(Rational::from_integer(2)?));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tempo {
    /// The beat unit, in whole notes; > 0.
    unit: Duration,
    /// Beats per minute; > 0.
    bpm: Rational,
}

/// Sixty, the number of seconds in a minute.
const SIXTY: Rational = match Rational::from_integer(60) {
    Ok(sixty) => sixty,
    // Unreachable: 60 is a valid rational. Evaluated at compile time.
    Err(_) => Rational::ONE,
};

impl Tempo {
    /// Builds a tempo from its beat unit and its beats per minute.
    ///
    /// # Errors
    /// [`TempoError::NonPositiveUnit`] or [`TempoError::NonPositiveBpm`].
    pub const fn new(unit: Duration, bpm: Rational) -> Result<Self, TempoError> {
        if !unit.is_positive() {
            return Err(TempoError::NonPositiveUnit);
        }
        if !bpm.is_positive() {
            return Err(TempoError::NonPositiveBpm);
        }
        Ok(Self { unit, bpm })
    }

    /// The beat unit. O(1).
    #[must_use]
    pub const fn unit(self) -> Duration {
        self.unit
    }

    /// The beats per minute. O(1).
    #[must_use]
    pub const fn bpm(self) -> Rational {
        self.bpm
    }

    /// The speed in whole notes per second: `unit × bpm / 60`. Always positive.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] for absurd values only.
    ///
    /// # Complexity
    /// O(log); no allocation.
    pub const fn speed(self) -> Result<Rational, RationalError> {
        match self.unit.as_rational().checked_mul(self.bpm) {
            Ok(per_minute) => per_minute.checked_div(SIXTY),
            Err(error) => Err(error),
        }
    }

    /// The exact time that `duration` lasts at this constant tempo: `duration / speed`.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the result does not fit.
    pub const fn seconds_for(self, duration: Duration) -> Result<Seconds, RationalError> {
        let speed = match self.speed() {
            Ok(speed) => speed,
            Err(error) => return Err(error),
        };
        match duration.as_rational().checked_div(speed) {
            Ok(seconds) => Ok(Seconds::from_rational(seconds)),
            Err(error) => Err(error),
        }
    }

    /// The exact musical duration covered in `time` at this constant tempo: `time × speed`.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the result does not fit.
    pub const fn duration_in(self, time: Seconds) -> Result<Duration, RationalError> {
        let speed = match self.speed() {
            Ok(speed) => speed,
            Err(error) => return Err(error),
        };
        match time.as_rational().checked_mul(speed) {
            Ok(whole_notes) => Ok(Duration::from_rational(whole_notes)),
            Err(error) => Err(error),
        }
    }
}

impl fmt::Debug for Tempo {
    /// Writes `Tempo(1/4 = 120)` or `Tempo(3/8 = 185/2)`: unit, then beats per minute.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = self.unit.as_rational();
        write!(f, "Tempo({}/{} = ", unit.numerator(), unit.denominator())?;
        if self.bpm.is_integer() {
            write!(f, "{})", self.bpm.numerator())
        } else {
            write!(f, "{}/{})", self.bpm.numerator(), self.bpm.denominator())
        }
    }
}
