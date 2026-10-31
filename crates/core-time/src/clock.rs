// SPDX-License-Identifier: GPL-3.0-or-later

//! Real time: exact seconds, sample rates, frame rates and the rounding rules (ADR-0020 §5, §7).
//!
//! # What lives here
//! - [`Seconds`]: an **exact** time in seconds, a newtype over [`Rational`]. Approximate `f64`
//!   seconds only ever appear as an output, for display.
//! - [`SampleRate`]: a positive whole number of samples per second (48 000 Hz).
//! - [`FrameRate`]: a positive **rational** number of video frames per second (24, 25,
//!   30000/1001), never a float like 29.97 (ADR-0005 §3).
//! - [`ClockError`]: invalid rates.
//!
//! # The two rounding rules (ADR-0020 §5)
//! - A **sample** is an instant: an event at time `t` sounds at the nearest sample,
//!   `n = ⌊t·rate + 1/2⌋`, ties going to the later sample. Error at most half a sample.
//! - A **frame** is an interval: time `t` belongs to the frame that contains it, `⌊t·fps⌋`.
//!
//! Both are computed with exact rational arithmetic, so they give the same index on every platform.
//! The inverses are exact too: sample `n` is at `n/rate` seconds, frame `k` starts at `k/fps`.
//! Indices are `i64`: negative before the start of the timeline (pre-roll).
//!
//! This module only knows seconds. Turning a musical position into seconds is the tempo map's job
//! (F1.05b), which also handles the irrational times inside gradual tempo changes.

use std::error::Error;
use std::fmt;
use std::num::NonZeroU32;

use crate::rational::{Rational, RationalError};

/// Unwraps a `Result` inside a `const fn`, returning the error early (the `?` of stable `const`).
macro_rules! const_try {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Err(error),
        }
    };
}

/// One half, used by the nearest-sample rounding.
const HALF: Rational = match Rational::new(1, 2) {
    Ok(half) => half,
    // Unreachable: 1/2 is a valid rational. Evaluated at compile time, so a failure here would be
    // a compile error, never a run-time panic.
    Err(_) => Rational::ZERO,
};

/// Why a sample rate or a frame rate could not be built.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to
/// a Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClockError {
    /// A sample rate of zero.
    ZeroSampleRate,
    /// A frame rate that is zero or negative.
    NonPositiveFrameRate,
}

impl fmt::Display for ClockError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroSampleRate => "a sample rate must be at least 1 Hz",
            Self::NonPositiveFrameRate => "a frame rate must be positive",
        })
    }
}

impl Error for ClockError {}

/// An exact time in seconds (ADR-0020 §7).
///
/// Times on tempo-change boundaries and inside constant-tempo segments are rational, so they are
/// kept exactly; `Seconds` may be negative (pre-roll, notes played before the start).
///
/// # Examples
/// ```
/// use orchestra_core_time::{Rational, SampleRate, Seconds};
///
/// let rate = SampleRate::new(48_000)?;
/// // 1/3 s is 16 000 samples exactly; 1/96 000 s is half a sample, which rounds up.
/// assert_eq!(rate.sample_at(Seconds::from_rational(Rational::new(1, 3)?))?, 16_000);
/// assert_eq!(rate.sample_at(Seconds::from_rational(Rational::new(1, 96_000)?))?, 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Seconds(Rational);

impl Seconds {
    /// Zero seconds: the start of the timeline.
    pub const ZERO: Self = Self(Rational::ZERO);

    /// Wraps an exact number of seconds. O(1).
    #[must_use]
    pub const fn from_rational(value: Rational) -> Self {
        Self(value)
    }

    /// The exact number of seconds. O(1).
    #[must_use]
    pub const fn as_rational(self) -> Rational {
        self.0
    }

    /// The sum of two times (or of a time and an interval).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    pub const fn checked_add(self, rhs: Self) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_add(rhs.0))))
    }

    /// The difference of two times.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the reduced result does not fit.
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, RationalError> {
        Ok(Self(const_try!(self.0.checked_sub(rhs.0))))
    }

    /// An **approximate** value in seconds, for display only (ADR-0015 §8). Nothing converted with
    /// this method may flow back into the model.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        self.0.to_f64()
    }
}

impl fmt::Debug for Seconds {
    /// Writes `Seconds(1/3)`, or `Seconds(2)` for integers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_integer() {
            write!(f, "Seconds({})", self.0.numerator())
        } else {
            write!(
                f,
                "Seconds({}/{})",
                self.0.numerator(),
                self.0.denominator()
            )
        }
    }
}

/// An audio sample rate: a positive whole number of samples per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SampleRate(NonZeroU32);

impl SampleRate {
    /// Builds a sample rate from a frequency in Hz (44 100, 48 000, 96 000…).
    ///
    /// # Errors
    /// [`ClockError::ZeroSampleRate`] if `hz` is zero.
    pub const fn new(hz: u32) -> Result<Self, ClockError> {
        match NonZeroU32::new(hz) {
            Some(hz) => Ok(Self(hz)),
            None => Err(ClockError::ZeroSampleRate),
        }
    }

    /// The frequency in Hz. O(1).
    #[must_use]
    pub const fn hz(self) -> u32 {
        self.0.get()
    }

    /// The rate as a rational, for the exact arithmetic below.
    const fn as_rational(self) -> Rational {
        // `from_integer` only fails for `i64::MIN`; a `u32` is far from it, so the fallback is
        // unreachable (it keeps the function infallible without a panic path).
        match Rational::from_integer(self.0.get() as i64) {
            Ok(rate) => rate,
            Err(_) => Rational::ONE,
        }
    }

    /// The nearest sample to time `t`: `⌊t·rate + 1/2⌋`, ties to the later sample (ADR-0020 §5).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if `t·rate` does not fit (a time of about 6·10⁹ years at
    /// 48 kHz).
    ///
    /// # Complexity
    /// O(log) for the rational product; no allocation.
    pub const fn sample_at(self, t: Seconds) -> Result<i64, RationalError> {
        let scaled = const_try!(t.0.checked_mul(self.as_rational()));
        Ok(const_try!(scaled.checked_add(HALF)).floor())
    }

    /// The exact time of sample `n`: `n / rate` seconds.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] only for `n = i64::MIN`.
    pub const fn time_of_sample(self, n: i64) -> Result<Seconds, RationalError> {
        let n = const_try!(Rational::from_integer(n));
        Ok(Seconds(const_try!(n.checked_div(self.as_rational()))))
    }
}

/// A video frame rate: a positive rational number of frames per second (ADR-0005 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameRate(Rational);

impl FrameRate {
    /// Builds a frame rate: `24/1`, `25/1`, `30000/1001` (NTSC).
    ///
    /// # Errors
    /// [`ClockError::NonPositiveFrameRate`] if `fps ≤ 0`.
    pub const fn new(fps: Rational) -> Result<Self, ClockError> {
        if fps.is_positive() {
            Ok(Self(fps))
        } else {
            Err(ClockError::NonPositiveFrameRate)
        }
    }

    /// The frames per second, exact. O(1).
    #[must_use]
    pub const fn fps(self) -> Rational {
        self.0
    }

    /// The frame that contains time `t`: `⌊t·fps⌋` (ADR-0020 §5).
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if `t·fps` does not fit.
    ///
    /// # Complexity
    /// O(log); no allocation.
    pub const fn frame_at(self, t: Seconds) -> Result<i64, RationalError> {
        Ok(const_try!(t.0.checked_mul(self.0)).floor())
    }

    /// The exact time at which frame `k` starts: `k / fps` seconds.
    ///
    /// # Errors
    /// [`RationalError::Overflow`] if the result does not fit (or `k = i64::MIN`).
    pub const fn frame_start(self, k: i64) -> Result<Seconds, RationalError> {
        let k = const_try!(Rational::from_integer(k));
        Ok(Seconds(const_try!(k.checked_div(self.0))))
    }
}
